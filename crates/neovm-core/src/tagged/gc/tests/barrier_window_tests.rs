//! The write barrier's owner window (`barrier_window.rs`) against the gate it
//! replaced: in every collector state and for every kind of owner, a store
//! reaches `record_heap_write` exactly when the old three-flag gate sent it
//! there — except a write by a tenured owner its header already marks
//! remembered, whose heap call was a no-op insert — and leaves the
//! remembered set exactly as the old one did.

use super::*;
use crate::tagged::header::{ConsCdrOrNext, LispValueVec, VecLikeHeader, VecLikeType, VectorObj};

/// The gate this lever replaced, kept verbatim as the reference model: its
/// three thread-local flags, the partition-only cons span test, then the
/// (unchanged) outlined rejects. `true` means the write reaches the heap.
fn old_gate_reaches_heap(owner: TaggedValue) -> bool {
    if !owner.is_heap_object() {
        return false;
    }
    let disabled =
        TAGGED_HEAP_WRITE_TRACKING_MODE.with(|mode| mode.get()) == WriteTrackingMode::Disabled;
    let partition = TAGGED_HEAP_PARTITION_ACTIVE.with(|p| p.get());
    let concurrent = TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.get());
    if disabled && !partition && !concurrent {
        return false;
    }
    if disabled && !concurrent && owner.is_cons() {
        let addr = owner.bits() & !crate::tagged::value::TAG_MASK;
        let (lo, hi) = TAGGED_HEAP_DUMP_SPAN.with(|s| s.get());
        if addr < lo || addr >= hi {
            return false;
        }
    }
    barrier_needs_heap(
        HeapWriteRecord::bulk(owner, HeapWriteKind::VectorSlot),
        disabled,
        concurrent,
    )
}

fn record_calls() -> usize {
    RECORD_HEAP_WRITE_CALLS.with(|c| c.get())
}

fn slow_calls() -> usize {
    BARRIER_SLOW_CALLS.with(|c| c.get())
}

/// The header's `remembered` byte of a non-cons owner.
fn header_remembered(owner: TaggedValue) -> bool {
    let addr = owner.bits() & !crate::tagged::value::TAG_MASK;
    unsafe {
        (*(addr as *const GcHeader))
            .remembered
            .load(Ordering::Relaxed)
    }
}

/// A tenured owner outside the window that its header already marks
/// remembered: the old gate sent its writes to the heap only to re-insert an
/// owner the set already holds; the inline gate stops them.
fn remembered_outside_window(heap: &TaggedHeap, owner: TaggedValue) -> bool {
    !owner.is_cons()
        && !published_barrier_window().covers(owner.bits() & !7)
        && header_remembered(owner)
        && heap.mapped_remembered.contains(&owner.bits())
}

/// A fake image cons, registered the way the pdump loader registers one
/// (extends the dump span and turns the partition on).
fn mapped_cons(heap: &mut TaggedHeap) -> TaggedValue {
    let cell = Box::into_raw(Box::new([ConsCell {
        car: TaggedValue::fixnum(1),
        cdr_or_next: ConsCdrOrNext {
            cdr: TaggedValue::NIL,
        },
    }])) as *mut ConsCell;
    unsafe { heap.register_mapped_cons_range(cell, 1) };
    unsafe { TaggedValue::from_cons_ptr(cell) }
}

/// A fake image vector (owned slots, so a store needs no copy).
fn mapped_vector(heap: &mut TaggedHeap) -> TaggedValue {
    let obj = Box::into_raw(Box::new(VectorObj {
        header: VecLikeHeader::new(VecLikeType::Vector),
        data: LispValueVec::owned(vec![TaggedValue::NIL; 2]),
    }));
    unsafe {
        heap.register_mapped_veclike_object(obj as *mut VecLikeHeader, size_of::<VectorObj>())
    };
    unsafe { TaggedValue::from_veclike_ptr(obj as *const VecLikeHeader) }
}

/// Store a fresh young cons into `owner` through the mutation wrapper its
/// kind uses, asserting the wrapper accepted it.
fn store_into(heap: &mut TaggedHeap, owner: TaggedValue) {
    let child = heap.alloc_cons(TaggedValue::fixnum(7), TaggedValue::NIL);
    let stored = if owner.is_cons() {
        crate::tagged::mutate::set_cons_car(owner, child)
    } else {
        crate::tagged::mutate::set_vector_slot(owner, 0, child)
    };
    assert!(stored, "the wrapper must accept the store");
}

/// Every write by `owner`, twice (the second is the repeat-owner case):
/// the heap is reached exactly when the old gate said so.
fn assert_matches_old_gate(heap: &mut TaggedHeap, what: &str, owner: TaggedValue) {
    for round in 0..2 {
        let remembered = remembered_outside_window(heap, owner);
        let expected = old_gate_reaches_heap(owner) && !remembered;
        let before = record_calls();
        let set_before = heap.mapped_remembered.len();
        store_into(heap, owner);
        assert_eq!(
            record_calls() - before,
            usize::from(expected),
            "{what}, write {round}: the barrier must reach the heap iff the old gate did \
             (window {:?})",
            published_barrier_window(),
        );
        if remembered {
            assert_eq!(heap.mapped_remembered.len(), set_before);
        }
    }
}

/// The owners of a partitioned heap after its first cycle, one per class.
struct Owners {
    heap_cons: TaggedValue,
    image_cons: TaggedValue,
    young_vector: TaggedValue,
    tenured_vector: TaggedValue,
    remembered_vector: TaggedValue,
    image_vector: TaggedValue,
}

fn partitioned_heap_owners(heap: &mut TaggedHeap) -> Owners {
    let image_cons = mapped_cons(heap);
    let image_vector = mapped_vector(heap);
    let tenured_vector = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    let remembered_vector = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    let root = heap.alloc_cons(tenured_vector, TaggedValue::NIL);
    let root = heap.alloc_cons(remembered_vector, root);
    // The first partition cycle tenures every survivor.
    heap.collect_exact(std::iter::once(root));
    assert!(heap.value_is_tenured(tenured_vector));
    assert!(heap.value_is_tenured(remembered_vector));
    // One write remembers this owner.
    store_into(heap, remembered_vector);
    assert!(heap.mapped_remembered.contains(&remembered_vector.bits()));
    assert!(!heap.mapped_remembered.contains(&tenured_vector.bits()));
    Owners {
        heap_cons: heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL),
        image_cons,
        young_vector: heap.alloc_vector(vec![TaggedValue::NIL; 2]),
        tenured_vector,
        remembered_vector,
        image_vector,
    }
}

fn check_every_owner(heap: &mut TaggedHeap, owners: &Owners) {
    assert_matches_old_gate(heap, "heap cons", owners.heap_cons);
    assert_matches_old_gate(heap, "image cons", owners.image_cons);
    assert_matches_old_gate(heap, "young vector", owners.young_vector);
    assert_matches_old_gate(heap, "tenured vector", owners.tenured_vector);
    assert_matches_old_gate(heap, "remembered vector", owners.remembered_vector);
    assert_matches_old_gate(heap, "image vector", owners.image_vector);
}

/// No partition, no mark, no tracking: nothing reaches the heap.
#[test]
fn a_bare_heap_records_nothing() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    assert_eq!(published_barrier_window(), BarrierWindow::NONE);
    let cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let vector = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    assert_matches_old_gate(&mut heap, "cons", cons);
    assert_matches_old_gate(&mut heap, "vector", vector);
    assert_eq!(heap.mapped_remembered.len(), 0);
}

/// Partition only (a session's steady state): image owners and tenured
/// owners not yet remembered reach the heap, nothing else does, and the
/// remembered set ends up holding exactly the image and tenured owners.
#[test]
fn partition_only_matches_the_old_gate_for_every_owner() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let owners = partitioned_heap_owners(&mut heap);
    let window = published_barrier_window();
    assert_eq!(
        window,
        BarrierWindow::span(heap.dump_addr_lo, heap.dump_addr_hi)
    );
    check_every_owner(&mut heap, &owners);
    for (what, owner, remembered) in [
        ("heap cons", owners.heap_cons, false),
        ("image cons", owners.image_cons, true),
        ("young vector", owners.young_vector, false),
        ("tenured vector", owners.tenured_vector, true),
        ("remembered vector", owners.remembered_vector, true),
        ("image vector", owners.image_vector, true),
    ] {
        assert_eq!(
            heap.mapped_remembered.contains(&owner.bits()),
            remembered,
            "{what}: remembered-set membership"
        );
    }
}

/// A concurrent mark on a partitioned heap: the window is ALL, and the
/// outlined rejects (the SATB dedup) decide exactly as before.
#[test]
fn a_concurrent_mark_matches_the_old_gate_for_every_owner() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let owners = partitioned_heap_owners(&mut heap);
    heap.set_concurrent_active_for_test(true);
    assert_eq!(published_barrier_window(), BarrierWindow::ALL);
    check_every_owner(&mut heap, &owners);
    heap.set_concurrent_active_for_test(false);
    assert_eq!(
        published_barrier_window(),
        BarrierWindow::span(heap.dump_addr_lo, heap.dump_addr_hi)
    );
    heap.satb_shared.lock().unwrap().clear();
}

/// Owner tracking on: every heap write is recorded, owner and all.
#[test]
fn owner_tracking_matches_the_old_gate_for_every_owner() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let owners = partitioned_heap_owners(&mut heap);
    heap.set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
    assert_eq!(published_barrier_window(), BarrierWindow::ALL);
    check_every_owner(&mut heap, &owners);
    for owner in [
        owners.heap_cons,
        owners.young_vector,
        owners.tenured_vector,
        owners.image_vector,
    ] {
        assert!(heap.is_dirty_owner(owner), "{owner:?} must be tracked");
    }
    heap.set_write_tracking_mode(WriteTrackingMode::Disabled);
    assert_eq!(
        published_barrier_window(),
        BarrierWindow::span(heap.dump_addr_lo, heap.dump_addr_hi)
    );
}

/// Owner tracking on a bare heap (no partition): ALL, then back to NONE.
#[test]
fn owner_tracking_on_a_bare_heap_records_every_owner() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    heap.set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
    let cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let vector = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    assert_matches_old_gate(&mut heap, "cons", cons);
    assert_matches_old_gate(&mut heap, "vector", vector);
    assert!(heap.is_dirty_owner(cons) && heap.is_dirty_owner(vector));
    heap.set_write_tracking_mode(WriteTrackingMode::Disabled);
    assert_eq!(published_barrier_window(), BarrierWindow::NONE);
}

/// The window is republished at each writer of its inputs: the dump span,
/// a real concurrent mark's launch and join, the tracking mode, and every
/// heap (re)installation.
#[test]
fn the_window_is_republished_at_every_writer_of_its_inputs() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    assert_eq!(published_barrier_window(), BarrierWindow::NONE);

    let image = mapped_cons(&mut heap);
    let span = BarrierWindow::span(heap.dump_addr_lo, heap.dump_addr_hi);
    assert!(span.covers(image.bits() & !7));
    assert_eq!(published_barrier_window(), span, "extend_dump_span");
    let second = mapped_vector(&mut heap);
    let wider = BarrierWindow::span(heap.dump_addr_lo, heap.dump_addr_hi);
    assert!(wider.covers(image.bits() & !7) && wider.covers(second.bits() & !7));
    assert_eq!(published_barrier_window(), wider, "a second image object");

    heap.set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
    assert_eq!(
        published_barrier_window(),
        BarrierWindow::ALL,
        "tracking on"
    );
    heap.set_write_tracking_mode(WriteTrackingMode::Disabled);
    assert_eq!(published_barrier_window(), wider, "tracking off");

    let root = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    // The first partition cycle is stop-the-world; the second is concurrent.
    heap.collect_exact(std::iter::once(root));
    heap.concurrent_begin();
    heap.seed_root(root);
    assert_eq!(published_barrier_window(), wider, "before the launch");
    heap.launch_concurrent_mark();
    assert_eq!(published_barrier_window(), BarrierWindow::ALL, "launch");
    while !heap.concurrent_mark_done() {
        std::thread::yield_now();
    }
    heap.join_concurrent_mark();
    assert_eq!(published_barrier_window(), wider, "join");
    heap.reseed_runtime_and_remembered_roots();
    heap.seed_root(root);
    let bytes_before = heap.live_bytes();
    heap.incremental_drain_all();
    heap.incremental_finish(bytes_before, std::time::Instant::now());
    heap.finish_incremental_sweep_now();
    assert_eq!(published_barrier_window(), wider, "after the sweep");

    // Installing another heap re-derives its window; reinstalling this one
    // re-derives this one's; uninstalling clears it.
    let mut other = TaggedHeap::new();
    set_tagged_heap(&mut other);
    assert_eq!(
        published_barrier_window(),
        BarrierWindow::NONE,
        "other heap"
    );
    set_tagged_heap(&mut heap);
    assert_eq!(published_barrier_window(), wider, "reinstalled");
    clear_tagged_heap_if_installed(&heap);
    assert_eq!(
        published_barrier_window(),
        BarrierWindow::NONE,
        "uninstalled"
    );
    set_tagged_heap(&mut heap);
}

/// The window's own arithmetic: empty, all, and a span's two edges.
#[test]
fn the_window_covers_exactly_its_span() {
    assert!(!BarrierWindow::NONE.covers(0));
    assert!(!BarrierWindow::NONE.covers(0x1000));
    assert!(BarrierWindow::ALL.covers(0));
    assert!(BarrierWindow::ALL.covers(usize::MAX - 7));
    let w = BarrierWindow::span(0x1000, 0x2000);
    assert!(!w.covers(0x0ff8));
    assert!(w.covers(0x1000));
    assert!(w.covers(0x1ff8));
    assert!(!w.covers(0x2000));
    assert_eq!(BarrierWindow::span(0x2000, 0x1000), BarrierWindow::NONE);
}

/// The header bit follows the remembered set: set by the barrier's first
/// write of a tenured owner and by the promotion scan for a tenured owner
/// born with a young child, never for an image owner, never for a young
/// one. From then on the owner's writes never leave the inline gate.
#[test]
fn the_remembered_bit_is_set_exactly_when_the_owner_is_remembered() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let image = mapped_vector(&mut heap);
    let quiet = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    let young_child = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    let parent = heap.alloc_vector(vec![young_child, TaggedValue::NIL]);
    let root = heap.alloc_cons(quiet, TaggedValue::NIL);
    let root = heap.alloc_cons(parent, root);
    for owner in [image, quiet, parent] {
        assert!(!header_remembered(owner), "{owner:?} starts unremembered");
    }
    heap.collect_exact(std::iter::once(root));

    // Promotion: the scan remembers the tenured owner holding a young cons.
    assert!(heap.value_is_tenured(parent));
    assert!(heap.mapped_remembered.contains(&parent.bits()));
    assert!(header_remembered(parent), "the promotion scan sets the bit");
    // A tenured owner with no young child is tenured but not remembered.
    assert!(heap.value_is_tenured(quiet));
    assert!(!heap.mapped_remembered.contains(&quiet.bits()));
    assert!(!header_remembered(quiet));

    // Its first write leaves the gate once and remembers it; the rest stay
    // inline.
    let before = slow_calls();
    store_into(&mut heap, quiet);
    assert_eq!(
        slow_calls() - before,
        1,
        "the first write takes the slow path"
    );
    assert!(heap.mapped_remembered.contains(&quiet.bits()));
    assert!(
        header_remembered(quiet),
        "the barrier's insert sets the bit"
    );
    // Even with the thread-local remembered cache gone, a remembered owner
    // never leaves the inline gate again.
    set_tagged_heap(&mut heap);
    let before = slow_calls();
    for _ in 0..8 {
        store_into(&mut heap, quiet);
        store_into(&mut heap, parent);
    }
    assert_eq!(slow_calls(), before, "remembered owners stay inline");

    // An image owner is remembered by its writes but its header is left
    // alone: it sits in the window, so the byte is never read.
    let before = slow_calls();
    store_into(&mut heap, image);
    assert_eq!(slow_calls() - before, 1, "an image owner is in the window");
    assert!(heap.mapped_remembered.contains(&image.bits()));
    assert!(!header_remembered(image));

    // A young owner is never remembered.
    let young = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    let before = slow_calls();
    store_into(&mut heap, young);
    assert_eq!(slow_calls(), before);
    assert!(!header_remembered(young));
    assert!(!heap.mapped_remembered.contains(&young.bits()));
}
