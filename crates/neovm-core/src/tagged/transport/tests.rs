//! Immediate and rooted transport: validation, rooting through collections,
//! retirement from foreign threads, and heap identity.

use std::sync::{Arc, Barrier, mpsc};

use super::root_table::cell_census;
use super::{
    ImmediateValue, NotImmediate, SharedRoot, SharedRootError, collect_shared_root_gc_roots,
};
use crate::emacs_core::intern::{
    intern, intern_uninterned, is_canonical_id, unintern_canonical_id,
};
use crate::heap_types::LispString;
use crate::tagged::gc::TaggedHeap;
use crate::tagged::value::TaggedValue;

/// Collect with this heap's shared roots as the only roots.
fn collect_with_shared_roots(heap: &mut TaggedHeap) {
    let mut roots = Vec::new();
    collect_shared_root_gc_roots(&heap, &mut roots);
    heap.collect_exact(roots.into_iter());
}

/// A root that keeps `heap`'s table alive, so retired cells stay observable:
/// a table is freed together with its last root.
fn table_anchor(heap: &TaggedHeap) -> SharedRoot {
    // SAFETY: this mutator creates a live symbol and immediately admits it.
    // Fixture collections seed the heap's shared-root table, and the anchor
    // is only used while its owning heap remains alive.
    unsafe {
        SharedRoot::new(
            heap,
            TaggedValue::from_sym_id(intern_uninterned("p73-table-anchor")),
        )
    }
}

fn alloc_named_cons(heap: &mut TaggedHeap, name: &str) -> (TaggedValue, TaggedValue) {
    let text = heap.alloc_string(LispString::from_utf8(name));
    (heap.alloc_cons(text, TaggedValue::fixnum(7)), text)
}

#[test]
fn immediate_values_are_fixnums_nil_and_t() {
    assert_eq!(ImmediateValue::NIL.value().bits(), TaggedValue::NIL.bits());
    assert_eq!(ImmediateValue::T.value().bits(), TaggedValue::T.bits());
    let fixnum = ImmediateValue::fixnum(-42).expect("in range");
    assert_eq!(fixnum.value().as_fixnum(), Some(-42));
    assert_eq!(
        ImmediateValue::fixnum(TaggedValue::MOST_POSITIVE_FIXNUM).map(ImmediateValue::value),
        Some(TaggedValue::fixnum(TaggedValue::MOST_POSITIVE_FIXNUM))
    );
    assert_eq!(
        ImmediateValue::fixnum(TaggedValue::MOST_POSITIVE_FIXNUM + 1),
        None
    );
    assert_eq!(
        ImmediateValue::fixnum(TaggedValue::MOST_NEGATIVE_FIXNUM - 1),
        None
    );
    assert_eq!(
        ImmediateValue::try_from(TaggedValue::NIL),
        Ok(ImmediateValue::NIL)
    );
    assert_eq!(
        ImmediateValue::try_from(TaggedValue::T),
        Ok(ImmediateValue::T)
    );
    assert_eq!(TaggedValue::from(fixnum).bits(), fixnum.value().bits());
}

#[test]
fn immediate_values_reject_heap_objects_and_uninterned_symbols() {
    let mut heap = TaggedHeap::new();
    let (cons, text) = alloc_named_cons(&mut heap, "not-immediate");
    assert_eq!(
        ImmediateValue::try_from(cons),
        Err(NotImmediate::HeapObject)
    );
    assert_eq!(
        ImmediateValue::try_from(text),
        Err(NotImmediate::HeapObject)
    );
    let uninterned = TaggedValue::from_sym_id(intern_uninterned("p73-uninterned"));
    assert_eq!(
        ImmediateValue::try_from(uninterned),
        Err(NotImmediate::SymbolNeedsRoot)
    );
}

#[test]
fn immediate_values_reject_a_canonical_symbol_before_and_after_unintern() {
    let id = intern("p73-immediate-canonical-rejection");
    let symbol = TaggedValue::from_sym_id(id);
    assert!(is_canonical_id(id));
    assert_eq!(
        ImmediateValue::try_from(symbol),
        Err(NotImmediate::SymbolNeedsRoot)
    );

    assert!(unintern_canonical_id(id));
    assert!(!is_canonical_id(id));
    assert_eq!(
        ImmediateValue::try_from(symbol),
        Err(NotImmediate::SymbolNeedsRoot)
    );
}

#[test]
fn shared_root_alone_keeps_an_object_alive_until_it_retires() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    let (cons, text) = alloc_named_cons(&mut heap, "shared-root-payload");
    let (dead_cons, dead_text) = alloc_named_cons(&mut heap, "unrooted-control");
    let _anchor = table_anchor(&heap);
    // SAFETY: `cons` was just allocated by this heap, with no intervening
    // collection. All collections below seed shared roots, and the heap
    // remains alive until its returned root has retired on the holder.
    let root = unsafe { SharedRoot::new(&heap, cons) };
    assert_eq!(cell_census(heap.heap_identity()), (2, 0));

    for _ in 0..3 {
        collect_with_shared_roots(&mut heap);
        assert!(!heap.owns_heap_value_for_test(dead_cons));
        assert!(!heap.owns_heap_value_for_test(dead_text));
        assert!(heap.owns_heap_value_for_test(cons));
        assert!(heap.owns_heap_value_for_test(text));
        let local = root.materialize(&heap).expect("same heap");
        assert_eq!(local.value().bits(), cons.bits());
        assert_eq!(
            local.value().cons_car().as_str_owned().as_deref(),
            Some("shared-root-payload")
        );
    }

    // The root is the only holder while another thread owns it across a
    // collection; it retires there, on a thread that is not a mutator.
    let collected = Arc::new(Barrier::new(2));
    let (to_holder, holder_rx) = mpsc::channel::<SharedRoot>();
    let holder_collected = Arc::clone(&collected);
    let holder = std::thread::spawn(move || {
        let root = holder_rx.recv().expect("root arrives");
        holder_collected.wait();
        holder_collected.wait();
        drop(root);
    });
    to_holder.send(root).expect("holder alive");
    collected.wait();
    collect_with_shared_roots(&mut heap);
    assert!(heap.owns_heap_value_for_test(cons));
    collected.wait();
    holder.join().expect("holder exits");

    assert_eq!(cell_census(heap.heap_identity()), (1, 1));
    collect_with_shared_roots(&mut heap);
    assert!(!heap.owns_heap_value_for_test(cons));
    assert!(!heap.owns_heap_value_for_test(text));
}

#[test]
fn shared_root_materializes_only_on_its_own_heap() {
    let mut first = TaggedHeap::new();
    let second = TaggedHeap::new();
    let (cons, _) = alloc_named_cons(&mut first, "first-heap");
    // SAFETY: `cons` is live in `first`, allocated immediately above. No
    // collection runs and both heaps outlive these materialization attempts.
    let root = unsafe { SharedRoot::new(&first, cons) };
    assert_eq!(root.heap_identity(), first.heap_identity());
    assert_eq!(
        root.materialize(&second).map(|local| local.value().bits()),
        Err(SharedRootError::ForeignHeap {
            owner: first.heap_identity(),
            mutator: second.heap_identity(),
        })
    );
    assert_ne!(first.heap_identity(), second.heap_identity());
    assert_eq!(
        root.materialize(&first).map(|local| local.value().bits()),
        Ok(cons.bits())
    );
}

#[test]
fn untraced_values_take_no_root_cell() {
    let heap = TaggedHeap::new();
    let _anchor = table_anchor(&heap);
    // SAFETY: these immediate constants have no reclaimable backing object.
    // The heap remains alive throughout materialization, and no collection
    // runs in this fixture.
    let roots = unsafe {
        [
            SharedRoot::new(&heap, TaggedValue::NIL),
            SharedRoot::new(&heap, TaggedValue::T),
            SharedRoot::new(&heap, TaggedValue::fixnum(99)),
        ]
    };
    assert_eq!(cell_census(heap.heap_identity()), (1, 0));
    for (root, expected) in
        roots
            .iter()
            .zip([TaggedValue::NIL, TaggedValue::T, TaggedValue::fixnum(99)])
    {
        assert_eq!(
            root.materialize(&heap).expect("same heap").value().bits(),
            expected.bits()
        );
    }
    // Symbols other than nil and t are traced: an uninterned symbol's cells
    // survive only while something marks it.
    // SAFETY: the symbol is created immediately before admission on this
    // heap's mutator; no collection runs and the heap outlives the root.
    let uninterned = unsafe {
        SharedRoot::new(
            &heap,
            TaggedValue::from_sym_id(intern_uninterned("p73-rooted-symbol")),
        )
    };
    assert_eq!(cell_census(heap.heap_identity()), (2, 0));
    drop(uninterned);
    assert_eq!(cell_census(heap.heap_identity()), (1, 1));
}

#[test]
fn clones_share_one_root_and_retired_cells_are_recycled() {
    let mut heap = TaggedHeap::new();
    let identity = heap.heap_identity();
    let (cons, _) = alloc_named_cons(&mut heap, "cloned");
    let _anchor = table_anchor(&heap);
    // SAFETY: `cons` was allocated by this heap and remains live at admission.
    // The fixture seeds shared roots for its only collection, and the heap
    // outlives materialization and retirement of every lease.
    let root = unsafe { SharedRoot::new(&heap, cons) };
    let clone = root.clone();
    assert!(clone.is_same_object(&root));
    assert_eq!(cell_census(identity), (2, 0));
    drop(root);
    collect_with_shared_roots(&mut heap);
    assert!(heap.owns_heap_value_for_test(cons));
    drop(clone);
    assert_eq!(cell_census(identity), (1, 1));

    // The next registration recycles the retired cell instead of growing.
    let (other, _) = alloc_named_cons(&mut heap, "recycled");
    // SAFETY: `other` was just allocated by this heap. No further collection
    // occurs, and the heap remains alive until this root is dropped.
    let recycled = unsafe { SharedRoot::new(&heap, other) };
    assert_eq!(cell_census(identity), (2, 0));
    // SAFETY: the preceding collection marked `cons` through `clone`.
    // Dropping `clone` retires its root but does not reclaim its object, and
    // no collection occurs before or after this new admission.
    let previous = unsafe { SharedRoot::new(&heap, cons) };
    assert!(!recycled.is_same_object(&previous));
}

#[test]
fn a_table_is_freed_with_its_last_root() {
    let heap = TaggedHeap::new();
    let root = table_anchor(&heap);
    assert_eq!(cell_census(heap.heap_identity()), (1, 0));
    std::thread::spawn(move || drop(root))
        .join()
        .expect("dropping thread exits");
    assert_eq!(cell_census(heap.heap_identity()), (0, 0));
}

#[test]
fn shared_roots_cross_threads_while_the_mutator_collects() {
    let mut heap = TaggedHeap::new();
    let identity = heap.heap_identity();
    let mut payloads = Vec::new();
    let mut roots = Vec::new();
    for index in 0..32 {
        let (cons, text) = alloc_named_cons(&mut heap, &format!("concurrent-{index}"));
        payloads.push((cons, text));
        // SAFETY: this mutator has just allocated `cons` in this heap, with
        // no intervening collection. All later collections seed shared roots
        // and the heap outlives the workers and returned-root materialization.
        roots.push(unsafe { SharedRoot::new(&heap, cons) });
    }

    // Workers clone, hold and drop roots on their own threads while the
    // mutator collects; each returns one clone per root it was given.
    let (returned_tx, returned_rx) = mpsc::channel::<Vec<SharedRoot>>();
    let workers: Vec<_> = roots
        .chunks(8)
        .map(|chunk| {
            let chunk = chunk.to_vec();
            let returned_tx = returned_tx.clone();
            std::thread::spawn(move || {
                for _ in 0..200 {
                    let clones: Vec<SharedRoot> = chunk.iter().map(SharedRoot::clone).collect();
                    drop(clones);
                }
                returned_tx.send(chunk).expect("mutator alive");
            })
        })
        .collect();
    drop(returned_tx);
    drop(roots);
    for _ in 0..4 {
        collect_with_shared_roots(&mut heap);
    }
    let returned: Vec<SharedRoot> = returned_rx.into_iter().flatten().collect();
    for worker in workers {
        worker.join().expect("worker exits");
    }

    collect_with_shared_roots(&mut heap);
    assert_eq!(returned.len(), payloads.len());
    for (cons, text) in &payloads {
        assert!(heap.owns_heap_value_for_test(*cons));
        assert!(heap.owns_heap_value_for_test(*text));
    }
    for root in &returned {
        let local = root.materialize(&heap).expect("same heap");
        assert!(
            payloads
                .iter()
                .any(|(cons, _)| cons.bits() == local.value().bits())
        );
    }
    assert_eq!(cell_census(identity).0, payloads.len());
    drop(returned);
    collect_with_shared_roots(&mut heap);
    for (cons, text) in &payloads {
        assert!(!heap.owns_heap_value_for_test(*cons));
        assert!(!heap.owns_heap_value_for_test(*text));
    }
}

#[test]
fn context_root_walk_includes_shared_roots() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::emacs_core::eval::Context::new();
    let text = TaggedValue::string("context-shared-root");
    // SAFETY: `text` was just allocated through this Context's installed
    // heap. Context collections include its shared-root table, and the
    // Context stays alive until the root has retired and its value is unused.
    let root = unsafe { SharedRoot::from_current_heap(text) }.expect("Context installs its heap");
    assert_eq!(root.heap_identity(), context.tagged_heap.heap_identity());
    let holder = std::thread::spawn(move || root);
    let root = holder.join().expect("holder returns the root");
    for _ in 0..3 {
        context.gc_collect_exact();
        assert!(context.tagged_heap.owns_heap_value_for_test(text));
    }
    let local = root
        .materialize(&context.tagged_heap)
        .expect("same heap")
        .value();
    assert_eq!(local.as_str_owned().as_deref(), Some("context-shared-root"));
    drop(root);
    context.gc_collect_exact();
    assert!(!context.tagged_heap.owns_heap_value_for_test(text));
}

#[test]
fn one_batch_lease_keeps_all_children_until_foreign_thread_retirement() {
    let mut context = Box::new(crate::emacs_core::Context::new());
    context.setup_thread_locals();
    let left = TaggedValue::string("batch-left");
    let right = TaggedValue::string("batch-right");
    let identity = context.tagged_heap.heap_identity();
    let before = cell_census(identity).0;
    // SAFETY: both children were just allocated by this installed evaluator.
    // Context collections trace shared roots, and it outlives every lease.
    let mut roots = unsafe { context.share_values(&[left, right]) }.unwrap();
    assert_eq!(cell_census(identity).0, before + 1);
    assert!(roots[0].shares_backing_root(&roots[1]));
    assert!(!roots[0].is_same_object(&roots[1]));
    assert_eq!(
        context.materialize(&roots[0]).unwrap().value().bits(),
        left.bits()
    );
    assert_eq!(
        context
            .materialize(&roots[1])
            .unwrap()
            .value()
            .as_str_owned()
            .as_deref(),
        Some("batch-right")
    );

    // Holding only the right child's handle still retains the entire private
    // vector, including left. No public handle can materialize that vector.
    let held = roots.pop().unwrap();
    drop(roots);
    let barrier = Arc::new(Barrier::new(2));
    let holder_barrier = Arc::clone(&barrier);
    let holder = std::thread::spawn(move || {
        holder_barrier.wait();
        holder_barrier.wait();
        drop(held);
    });
    barrier.wait();
    for _ in 0..3 {
        context.gc_collect_exact();
        assert!(context.tagged_heap.owns_heap_value_for_test(left));
        assert!(context.tagged_heap.owns_heap_value_for_test(right));
    }
    barrier.wait();
    holder.join().unwrap();
    context.gc_collect_exact();
    assert!(!context.tagged_heap.owns_heap_value_for_test(left));
    assert!(!context.tagged_heap.owns_heap_value_for_test(right));
}

#[test]
fn batch_child_identity_and_structural_equality_survive_different_batches() {
    use crate::window::{PresentedWindowChromeArea, PresentedWindowChromeString};
    use neomacs_display_protocol::GlyphStringId;

    let mut context = Box::new(crate::emacs_core::Context::new());
    context.setup_thread_locals();
    let first = TaggedValue::string("equal batch children");
    let equal = TaggedValue::string("equal batch children");
    assert_ne!(first.bits(), equal.bits());
    // SAFETY: these freshly allocated children belong to this installed
    // evaluator; no collection intervenes and its heap outlives the handles.
    let first_batch = unsafe { context.share_values(&[first, first]) }.unwrap();
    // SAFETY: the existing first batch keeps `first` live; `equal` was just
    // allocated on this same mutator. Both heaps/roots remain alive here.
    let second_batch = unsafe { context.share_values(&[first, equal]) }.unwrap();
    assert!(first_batch[0].is_same_object(&first_batch[1]));
    assert!(first_batch[0].is_same_object(&second_batch[0]));
    assert!(!first_batch[0].shares_backing_root(&second_batch[0]));
    assert!(!first_batch[0].is_same_object(&second_batch[1]));
    let left = PresentedWindowChromeString::new(
        PresentedWindowChromeArea::ModeLine,
        GlyphStringId::new(1),
        first_batch[0].clone(),
    );
    let right = PresentedWindowChromeString::new(
        PresentedWindowChromeArea::ModeLine,
        GlyphStringId::new(1),
        second_batch[1].clone(),
    );
    assert_eq!(left, right);
    assert!(!std::thread::spawn(move || left == right).join().unwrap());

    let mut second_context = Box::new(crate::emacs_core::Context::new());
    second_context.setup_thread_locals();
    assert!(matches!(
        second_context.materialize(&first_batch[0]),
        Err(SharedRootError::ForeignHeap { .. })
    ));
}

#[test]
fn window_snapshot_coalesces_all_areas_and_reuses_its_single_lease() {
    use crate::window::{PresentedWindowChromeArea, PresentedWindowChromeString};
    use neomacs_display_protocol::GlyphStringId;

    let mut context = Box::new(crate::emacs_core::Context::new());
    context.setup_thread_locals();
    let mode = TaggedValue::string("mode flattened");
    let mode_leaf = TaggedValue::string("mode leaf");
    let header = TaggedValue::string("header flattened");
    let tab = TaggedValue::string("tab flattened");
    let identity = context.tagged_heap.heap_identity();
    let before = cell_census(identity).0;
    // SAFETY: all four values were allocated by this installed evaluator.
    // Its collections trace shared roots, and it outlives these snapshots.
    let mode_roots = unsafe { context.share_values(&[mode, mode_leaf]) }.unwrap();
    // SAFETY: `header` is still live in the same heap; allocation invokes no
    // Lisp/collection and the evaluator's root walk includes the new lease.
    let header_roots = unsafe { context.share_values(&[header]) }.unwrap();
    // SAFETY: `tab` has the same live installed-heap provenance and lifetime.
    let tab_roots = unsafe { context.share_values(&[tab]) }.unwrap();
    let mut sources: Vec<_> = mode_roots
        .into_iter()
        .enumerate()
        .map(|(index, root)| {
            PresentedWindowChromeString::new(
                PresentedWindowChromeArea::ModeLine,
                GlyphStringId::new(index as u64 + 1),
                root,
            )
        })
        .chain(header_roots.into_iter().map(|root| {
            PresentedWindowChromeString::new(
                PresentedWindowChromeArea::HeaderLine,
                GlyphStringId::new(1),
                root,
            )
        }))
        .chain(tab_roots.into_iter().map(|root| {
            PresentedWindowChromeString::new(
                PresentedWindowChromeArea::TabLine,
                GlyphStringId::new(1),
                root,
            )
        }))
        .collect();
    assert_eq!(cell_census(identity).0, before + 3);
    PresentedWindowChromeString::coalesce_roots(&mut sources, &context).unwrap();
    assert_eq!(cell_census(identity).0, before + 1);
    assert!(
        sources
            .iter()
            .all(|source| source.object().shares_backing_root(sources[0].object()))
    );
    for (source, expected) in sources.iter().zip([mode, mode_leaf, header, tab]) {
        assert_eq!(
            context.materialize(source.object()).unwrap().value().bits(),
            expected.bits()
        );
    }
    let retained_lease = sources[0].object().clone();
    let census = cell_census(identity);
    PresentedWindowChromeString::coalesce_roots(&mut sources, &context).unwrap();
    assert_eq!(cell_census(identity), census);
    assert!(sources[0].object().shares_backing_root(&retained_lease));
    context.gc_collect_exact();
    for child in [mode, mode_leaf, header, tab] {
        assert!(context.tagged_heap.owns_heap_value_for_test(child));
    }
}

#[test]
fn coalescing_rejects_foreign_heap_without_replacing_existing_handles() {
    use crate::window::{PresentedWindowChromeArea, PresentedWindowChromeString};
    use neomacs_display_protocol::GlyphStringId;

    let mut first = Box::new(crate::emacs_core::Context::new());
    first.setup_thread_locals();
    let text = TaggedValue::string("foreign batch");
    // SAFETY: this fresh child belongs to the installed first evaluator,
    // which stays alive throughout the foreign-heap rejection attempt.
    let root = unsafe { first.share_values(&[text]) }
        .unwrap()
        .pop()
        .unwrap();
    let retained = root.clone();
    let mut sources = vec![PresentedWindowChromeString::new(
        PresentedWindowChromeArea::ModeLine,
        GlyphStringId::new(1),
        root,
    )];
    let mut second = Box::new(crate::emacs_core::Context::new());
    second.setup_thread_locals();
    assert!(matches!(
        PresentedWindowChromeString::coalesce_roots(&mut sources, &second),
        Err(SharedRootError::ForeignHeap { .. })
    ));
    assert!(sources[0].object().shares_backing_root(&retained));
    assert!(matches!(
        SharedRoot::coalesce_on_current_mutator(&[&retained]),
        Err(SharedRootError::ForeignHeap { .. })
    ));
    assert!(sources[0].object().is_same_object(&retained));
}
