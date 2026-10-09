//! Major-only symbol preimages through the production producers and heap TLS.

use super::*;
use crate::buffer::text_props::{PropertyInterval, TextPropertyTable};
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::intern::{intern, intern_uninterned};
use crate::emacs_core::symbol::Obarray;
use crate::heap_types::LispString;

struct ScratchRoots(usize);
impl ScratchRoots {
    fn new() -> Self {
        Self(save_scratch_gc_roots())
    }
    fn keep(&self, value: TaggedValue) -> TaggedValue {
        push_scratch_gc_root(value);
        value
    }
}
impl Drop for ScratchRoots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn heap(enabled: bool) -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    heap.generational.enabled = enabled;
    heap.publish_barrier_window();
    heap
}

fn symbol(name: &str) -> TaggedValue {
    TaggedValue::from_sym_id(intern_uninterned(name))
}

fn id(value: TaggedValue) -> SymId {
    let crate::tagged::value::ValueKind::Symbol(id) = value.kind() else {
        panic!("the fixture needs an ordinary bare symbol");
    };
    id
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PhaseKind {
    Synthetic,
    Worker,
}

/// Synthetic phases install the real public TLS gate but launch no worker.
/// Cleanup must clear their flags before heap Drop expects a receiver. Real
/// phases instead join their actual worker if an assertion unwinds first.
struct MarkPhase {
    heap: *mut TaggedHeap,
    kind: PhaseKind,
}
impl MarkPhase {
    fn synthetic(heap: &mut TaggedHeap) -> Self {
        assert!(!heap.concurrent_mark_running);
        heap.concurrent_begin();
        // No worker owns gray in this fixture; make the owner's SATB scratch
        // satisfy its actual concurrent contract without fabricating a job.
        heap.gray_queue.clear();
        heap.concurrent_mark_running = true;
        set_tagged_heap(heap);
        Self {
            heap,
            kind: PhaseKind::Synthetic,
        }
    }

    fn worker(heap: &mut TaggedHeap, roots: &[TaggedValue], first_partition: bool) -> Self {
        assert!(!heap.concurrent_mark_running);
        if first_partition {
            heap.arm_first_cycle_concurrent();
        }
        heap.concurrent_begin();
        for &root in roots {
            heap.seed_root(root);
        }
        set_tagged_heap(heap);
        heap.launch_concurrent_mark();
        Self {
            heap,
            kind: PhaseKind::Worker,
        }
    }

    fn stop_synthetic(&self, heap: &mut TaggedHeap) {
        assert!(self.kind == PhaseKind::Synthetic);
        assert_eq!(self.heap, heap as *mut TaggedHeap);
        heap.close_alloc_regions();
        heap.concurrent_mark_running = false;
        set_tagged_heap(heap);
    }
}
impl Drop for MarkPhase {
    fn drop(&mut self) {
        // SAFETY: the heap is declared before this guard and remains at its
        // stable stack address until the guard is dropped on either path.
        let heap = unsafe { &mut *self.heap };
        if self.kind == PhaseKind::Worker && heap.concurrent_mark_running {
            heap.join_concurrent_mark();
        }
        heap.close_alloc_regions();
        heap.concurrent_mark_running = false;
        heap.mark_in_progress = false;
        heap.generational.major_in_progress = false;
        set_tagged_heap(heap);
        clear_tagged_heap_if_installed(heap);
    }
}

fn satb(heap: &TaggedHeap) -> Vec<TaggedValue> {
    heap.satb_shared
        .lock()
        .unwrap()
        .iter()
        .map(|word| word.value())
        .collect()
}

fn special_immediates() -> [TaggedValue; 4] {
    [
        TaggedValue::NIL,
        TaggedValue::T,
        TaggedValue::UNBOUND,
        TaggedValue::fixnum(65),
    ]
}

fn interval_string(
    heap: &mut TaggedHeap,
    roots: &ScratchRoots,
    value: TaggedValue,
) -> (TaggedValue, TaggedValue) {
    let string = roots.keep(heap.alloc_string(LispString::from_utf8("gap then property")));
    let name = TaggedValue::fixnum(1);
    let mut properties = std::collections::HashMap::new();
    properties.insert(name, value);
    // A real leading gap yields NIL as a table root, alongside the property's
    // heap plist. NIL must never be mistaken for an ordinary Symbol kind.
    let table = TextPropertyTable::from_dump(vec![PropertyInterval {
        start: 1,
        end: 2,
        properties,
        key_order: vec![name],
    }]);
    let mut table_roots = Vec::new();
    table.for_each_root(|value| {
        roots.keep(value);
        table_roots.push(value);
    });
    assert!(table_roots.contains(&TaggedValue::NIL));
    let plist = *table_roots.iter().find(|value| value.is_cons()).unwrap();
    // No Lisp allocation occurs between table construction/rooting and its
    // installation in the rooted string; the closure only transfers ownership.
    crate::tagged::mutate::with_lisp_string_mut(string, |data| *data.intervals_mut() = table)
        .unwrap();
    (string, plist)
}

#[test]
fn major_symbol_public_root_producers_are_idle_without_concurrent_tls() {
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        set_tagged_heap(&mut heap);
        let roots = ScratchRoots::new();
        let key = symbol("major-preimage-idle");
        let positioned = roots.keep(heap.alloc_symbol_with_pos(key, TaggedValue::fixnum(3)));
        note_root_overwrite(key);
        note_root_overwrite(positioned);
        for value in special_immediates() {
            note_root_overwrite(value);
        }
        feed_concurrent_roots(&[key, positioned, TaggedValue::fixnum(7)]);
        assert!(heap.current_mutator_gc().major_symbol_preimages.is_empty());
        assert!(satb(&heap).is_empty());
        assert!(!heap.marked_symbols.contains(id(key)));
    }
}

#[test]
fn major_symbol_root_overwrite_gates_keep_exact_kind_and_symbol_with_pos_paths() {
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        set_tagged_heap(&mut heap);
        let roots = ScratchRoots::new();
        let public = symbol("major-preimage-public");
        let checked = symbol("major-preimage-checked");
        let positioned = roots.keep(heap.alloc_symbol_with_pos(public, TaggedValue::fixnum(9)));
        let _phase = MarkPhase::synthetic(&mut heap);
        assert!(concurrent_mark_active());
        note_root_overwrite(public);
        note_root_overwrite_while_marking(checked);
        note_root_overwrite(positioned);
        for value in special_immediates() {
            note_root_overwrite(value);
            note_root_overwrite_while_marking(value);
        }
        assert_eq!(satb(&heap), [positioned]);
        let expected = if enabled {
            vec![id(public), id(checked)]
        } else {
            vec![]
        };
        assert_eq!(heap.current_mutator_gc().major_symbol_preimages, expected);
        assert!(!heap.marked_symbols.contains(id(public)));
        assert!(!heap.marked_symbols.contains(id(checked)));
    }
}

#[test]
fn major_symbol_root_sink_defensively_rejects_special_immediates() {
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        set_tagged_heap(&mut heap);
        let _phase = MarkPhase::synthetic(&mut heap);
        for value in special_immediates() {
            heap.note_root_overwrite_value(value);
        }
        assert!(satb(&heap).is_empty());
        assert!(heap.current_mutator_gc().major_symbol_preimages.is_empty());
    }
}

#[test]
fn major_symbol_obarray_value_function_and_plist_writers_log_their_old_cells() {
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        set_tagged_heap(&mut heap);
        let mut obarray = Obarray::new();
        let owner = intern("major-preimage-obarray-owner");
        let value = symbol("major-preimage-obarray-value");
        let function = symbol("major-preimage-obarray-function");
        let plist = symbol("major-preimage-obarray-plist");
        obarray.set_symbol_value_id(owner, value);
        obarray.set_symbol_function_id(owner, function);
        obarray.set_symbol_plist_id(owner, plist);
        let _phase = MarkPhase::synthetic(&mut heap);
        assert!(obarray.set_plain_untrapped_value_id(owner, TaggedValue::fixnum(4)));
        obarray.set_symbol_function_id(owner, TaggedValue::NIL);
        obarray.set_symbol_plist_id(owner, TaggedValue::NIL);
        let expected = if enabled {
            vec![id(value), id(function), id(plist)]
        } else {
            vec![]
        };
        assert_eq!(heap.current_mutator_gc().major_symbol_preimages, expected);
        assert!(satb(&heap).is_empty());
        for key in [value, function, plist] {
            assert!(!heap.marked_symbols.contains(id(key)));
        }
    }
}

#[test]
fn major_symbol_owner_store_preimages_use_mutator_logs_and_preserve_legacy_repair() {
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        set_tagged_heap(&mut heap);
        let roots = ScratchRoots::new();
        let car = symbol("major-preimage-cons-car");
        let slot = symbol("major-preimage-vector-slot");
        let cons = roots.keep(heap.alloc_cons(car, TaggedValue::fixnum(8)));
        let vector = roots.keep(heap.alloc_vector(vec![slot, TaggedValue::fixnum(9)]));
        let _phase = MarkPhase::synthetic(&mut heap);
        assert!(crate::tagged::mutate::set_cons_car(
            cons,
            TaggedValue::fixnum(10)
        ));
        assert!(crate::tagged::mutate::set_vector_slot(
            vector,
            0,
            TaggedValue::fixnum(11)
        ));
        let expected = if enabled {
            vec![id(car), id(slot)]
        } else {
            vec![]
        };
        assert_eq!(heap.current_mutator_gc().major_symbol_preimages, expected);
        assert!(satb(&heap).is_empty());
        assert!(heap.gray_queue.is_empty());
        // Existing OFF owner-child enumeration repairs collector symbol marks.
        // Keep that behavior; only the enabled major moves this sink per-M.
        assert_eq!(heap.marked_symbols.contains(id(car)), !enabled);
        assert_eq!(heap.marked_symbols.contains(id(slot)), !enabled);
    }
}

#[test]
fn major_symbol_live_root_batches_filter_special_immediates_and_keep_heap_values() {
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        set_tagged_heap(&mut heap);
        let roots = ScratchRoots::new();
        let key = symbol("major-preimage-live-batch");
        let positioned = roots.keep(heap.alloc_symbol_with_pos(key, TaggedValue::fixnum(2)));
        let _phase = MarkPhase::synthetic(&mut heap);
        feed_concurrent_roots(&[
            key,
            TaggedValue::NIL,
            TaggedValue::T,
            TaggedValue::UNBOUND,
            TaggedValue::fixnum(65),
            positioned,
            key,
        ]);
        let expected = if enabled {
            vec![key, positioned, key]
        } else {
            vec![positioned]
        };
        assert_eq!(satb(&heap), expected);
        assert!(heap.current_mutator_gc().major_symbol_preimages.is_empty());
        assert!(!heap.marked_symbols.contains(id(key)));
    }
}

#[test]
fn major_symbol_raw_interval_choke_points_retain_plists_once_and_ignore_nil_gaps() {
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        set_tagged_heap(&mut heap);
        let roots = ScratchRoots::new();
        let key = symbol("major-preimage-interval-value");
        let (string, plist) = interval_string(&mut heap, &roots, key);
        let phase = MarkPhase::synthetic(&mut heap);
        let ptr = string.as_string_ptr().unwrap() as *mut StringObj;
        // Exercise LispString's real enforced producers, independently of the
        // outer owner wrapper and its separate written-owner retrace.
        unsafe {
            (*ptr).data.intervals_mut();
        }
        assert_eq!(satb(&heap), [plist]);
        unsafe {
            (*ptr).data.clear_intervals();
        }
        assert_eq!(satb(&heap), [plist], "the same string is deduplicated");
        assert_eq!(heap.satb_string_preimage_addrs.len(), 1);
        assert!(heap.current_mutator_gc().major_symbol_preimages.is_empty());
        phase.stop_synthetic(&mut heap);
        let retained = std::mem::take(&mut *heap.satb_shared.lock().unwrap());
        heap.gray_queue
            .extend(retained.into_iter().map(MarkWord::value));
        heap.incremental_drain_all();
        assert!(heap.marked_symbols.contains(id(key)));
        assert!(heap.current_mutator_gc().major_symbol_preimages.is_empty());
    }
}

#[test]
fn major_symbol_join_merges_worker_and_mutator_results_after_publication_stops() {
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        // U3.5 full cycles preserve worker-discovered Symbols independently
        // of the still-major-only mutator preimage producers below.
        let worker_preserves_symbols = enabled || heap.concurrent_claims();
        set_tagged_heap(&mut heap);
        let roots = ScratchRoots::new();
        let worker = symbol("major-preimage-worker-snapshot");
        let root = symbol("major-preimage-join-root");
        let owner = symbol("major-preimage-join-owner");
        let live = symbol("major-preimage-join-live");
        let positioned_key = symbol("major-preimage-join-positioned");
        let after = symbol("major-preimage-join-after");
        let cons = roots.keep(heap.alloc_cons(owner, TaggedValue::fixnum(3)));
        let positioned =
            roots.keep(heap.alloc_symbol_with_pos(positioned_key, TaggedValue::fixnum(4)));
        let mut obarray = Obarray::new();
        obarray.set_symbol_value("major-preimage-worker-owner", worker);
        let snapshot = {
            // SAFETY: capture occurs on this test's sole heap/obarray writer.
            let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
            obarray.scan_snapshot(&world)
        };
        heap.set_pending_obarray_scan(snapshot);
        let _phase = MarkPhase::worker(&mut heap, &[], false);
        note_root_overwrite(root);
        assert!(crate::tagged::mutate::set_cons_car(
            cons,
            TaggedValue::fixnum(5)
        ));
        feed_concurrent_roots(&[live, positioned]);
        if enabled {
            assert_eq!(
                heap.current_mutator_gc().major_symbol_preimages,
                [id(root), id(owner)]
            );
            assert!(!heap.marked_symbols.contains(id(root)));
        } else {
            assert!(heap.current_mutator_gc().major_symbol_preimages.is_empty());
            assert!(heap.marked_symbols.contains(id(owner)));
        }
        heap.join_concurrent_mark();
        assert!(!heap.concurrent_mark_running);
        assert!(!concurrent_mark_active());
        assert!(
            heap.current_mutator_gc().major_symbol_preimages.is_empty(),
            "merge must insert into collector marks instead of re-appending"
        );
        assert_eq!(
            heap.marked_symbols.contains(id(worker)),
            worker_preserves_symbols,
            "start scans finish even if join requests an immediate stop"
        );
        assert_eq!(heap.marked_symbols.contains(id(root)), enabled);
        assert!(heap.marked_symbols.contains(id(owner)));
        heap.incremental_drain_all();
        assert_eq!(heap.marked_symbols.contains(id(live)), enabled);
        assert!(heap.marked_symbols.contains(id(positioned_key)));
        assert!(heap.current_mutator_gc().major_symbol_preimages.is_empty());
        note_root_overwrite(after);
        assert!(
            !heap.marked_symbols.contains(id(after)),
            "public root gate is now idle"
        );
        heap.mark_or_push_child(after, "major-symbol-post-join");
        assert!(heap.marked_symbols.contains(id(after)));
        assert!(heap.current_mutator_gc().major_symbol_preimages.is_empty());
    }
}

#[test]
fn major_symbol_first_partition_join_keeps_symbols_and_discards_worker_promo() {
    let mut heap = heap(true);
    // Registering the image below activates the partition and its real span;
    // do not publish a partition window before a mapped object exists.
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let image = fake_image::FakeImage::leak(false);
    let mapped = roots.keep(image.register_vector(&mut heap));
    let key = symbol("major-preimage-first-partition-symbol");
    assert!(crate::tagged::mutate::set_vector_slot(mapped, 0, key));
    let float = roots.keep(heap.alloc_float(2.25));
    let _phase = MarkPhase::worker(&mut heap, &[float], true);
    heap.join_concurrent_mark();
    assert!(heap.marked_symbols.contains(id(key)));
    assert!(
        heap.generational.promo.is_empty(),
        "the permanent splice owns first promotion"
    );
    assert!(!heap.value_is_old_for_test(float));
    assert!(
        heap.is_value_marked(float),
        "the claim's live mark must survive join"
    );
    assert!(heap.concurrent_float_claimed.load(Ordering::Relaxed) > 0);
    assert!(heap.first_cycle_concurrent);
    assert!(heap.current_mutator_gc().major_symbol_preimages.is_empty());
    assert!(!concurrent_mark_active());
}

#[test]
fn major_symbol_generic_cons_write_retraces_current_children_before_sweep() {
    for enabled in [false, true] {
        for is_cdr in [false, true] {
            let mut heap = heap(enabled);
            set_tagged_heap(&mut heap);
            let roots = ScratchRoots::new();
            let owner = roots.keep(heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL));
            // A real completed collection establishes ordinary-old status for
            // GEN1. GEN0 preserves its existing cons tracing behavior.
            heap.collect_exact([owner].into_iter());
            if enabled {
                assert!(heap.value_is_old_for_test(owner));
            }
            let _phase = MarkPhase::worker(&mut heap, &[owner], false);
            while !heap.concurrent_mark_done() {
                std::thread::yield_now();
            }
            // The worker has already claimed this owner. The new bare symbol
            // is neither a snapshot root nor a heap object with a birth log.
            let key = roots.keep(symbol("major-generic-cons-inserted-symbol"));
            let kind = if is_cdr {
                HeapWriteKind::ConsCdr
            } else {
                HeapWriteKind::ConsCar
            };
            note_heap_write(owner, kind);
            // Exercise the public generic pre-store hook with its documented
            // None value record. Existing production car/cdr setters expose
            // Some(value), so this is defensive coverage of that public API.
            unsafe {
                if is_cdr {
                    (*(owner.xcons_ptr() as *mut ConsCell)).set_cdr(key);
                } else {
                    (*(owner.xcons_ptr() as *mut ConsCell)).set_car(key);
                }
            }
            restore_scratch_gc_roots(roots.0);
            assert!(heap.current_mutator_gc().major_symbol_preimages.is_empty());
            assert_eq!(
                heap.current_mutator_gc().major_cons_writes,
                if enabled { vec![owner] } else { vec![] },
            );
            assert!(!heap.marked_symbols.contains(id(key)));
            heap.join_concurrent_mark();
            assert!(!concurrent_mark_active());
            assert!(heap.current_mutator_gc().major_cons_writes.is_empty());
            assert!(heap.current_mutator_gc().major_symbol_preimages.is_empty());
            assert_eq!(heap.marked_symbols.contains(id(key)), enabled);
            // The normal termination proceeds only after this liveness fact
            // is installed. No Lisp allocation occurs before the join drain.
            heap.reseed_runtime_and_remembered_roots();
            heap.seed_root(owner);
            heap.incremental_drain_all();
            heap.incremental_finish(heap.live_bytes(), std::time::Instant::now());
            heap.finish_incremental_sweep_now();
            assert!(heap.owns_heap_value_for_test(owner));
            assert_eq!(heap.marked_symbols.contains(id(key)), enabled);
        }
    }
}

// Append inside tagged/gc/tests/major_symbol_preimage_test.rs. This reuses
// that module's existing heap, MarkPhase and satb fixtures; no new GC seam.

#[test]
fn atomic_forwarder_stores_keep_root_preimages_before_publication() {
    use crate::emacs_core::forward::{
        LispInteger, alloc_intfwd, alloc_kboard_objfwd, alloc_objfwd,
    };
    for enabled in [false, true] {
        let mut heap = heap(enabled);
        set_tagged_heap(&mut heap);
        let first = heap.alloc_cons(TaggedValue::fixnum(11), TaggedValue::NIL);
        let second = heap.alloc_cons(TaggedValue::fixnum(22), TaggedValue::NIL);
        let object = alloc_objfwd(first);
        let keyboard = alloc_kboard_objfwd(second);
        let integer = alloc_intfwd(LispInteger::from_i64(i64::MAX));
        let integer_old = integer.get();
        assert!(integer_old.is_bignum());
        let phase = MarkPhase::synthetic(&mut heap);
        object.set(TaggedValue::NIL);
        keyboard.set(TaggedValue::T);
        integer.set(LispInteger::from_i64(0));
        assert_eq!(integer.get_i64(), 0);
        assert!(object.get().is_nil());
        assert_eq!(keyboard.get().bits(), TaggedValue::T.bits());
        let old_roots = satb(&heap);
        assert!(old_roots.iter().any(|value| value.bits() == first.bits()));
        assert!(old_roots.iter().any(|value| value.bits() == second.bits()));
        assert!(
            old_roots
                .iter()
                .any(|value| value.bits() == integer_old.bits())
        );
        phase.stop_synthetic(&mut heap);
        drop(phase);
    }
}
