//! Real concurrent SATB delivery, including words invisible to root reseeding.
//! Payloads predate launch and are held only in unregistered Rust fixture lists;
//! a heap vector would let the blanket Tier-B scan mask a missing pre-image.

use super::*;
use crate::heap_types::LispString;
use crate::intern::intern;
use crate::symbol::Obarray;
use crate::tagged::mutate::{set_cons_car, with_vector_data_mut};
use crate::value::text_props::{PropertyInterval, TextPropertyTable};
use std::time::{Duration, Instant};

const WRITES: usize = 10_000;

fn heap() -> TestHeap {
    let mut heap = TestHeap::new();
    // GEN1's independent dirty-cons insertion retrace would repair a missing
    // SATB log and invalidate these transport mutation witnesses.
    heap.generational.enabled = false;
    // SAFETY: serial fixture; the heap is exclusively borrowed by the test
    // thread and no other mutator or compiled leaf runs across the publication.
    unsafe { heap.publish_barrier_window() };
    heap
}

fn begin(heap: &mut TestHeap, roots: &[TaggedValue], payloads: &[TaggedValue]) {
    heap.concurrent_begin();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.install();
    heap.launch_concurrent_mark();
    let deadline = Instant::now() + Duration::from_secs(60);
    while !heap.concurrent_mark_done() {
        assert!(
            Instant::now() < deadline,
            "real marker did not drain start graph"
        );
        std::thread::yield_now();
    }
    for &root in roots {
        assert!(
            heap.is_value_marked(root),
            "destination must already be black"
        );
    }
    assert!(
        payloads.iter().all(|&x| !heap.is_value_marked(x)),
        "orphan witnesses must start white and predate launch"
    );
}

fn payloads(heap: &mut TestHeap) -> Vec<TaggedValue> {
    (0..WRITES)
        .map(|i| heap.alloc_cons(TaggedValue::fixnum(i as i64), TaggedValue::NIL))
        .collect()
}

fn check_survivors(heap: &TestHeap, payloads: &[TaggedValue]) {
    for (i, &value) in payloads.iter().enumerate() {
        let ptr = value.xcons_ptr();
        let base = ConsBlock::block_base_for_ptr(ptr);
        let block = heap
            .cons_blocks
            .iter()
            .find(|block| block.base_addr() == base)
            .expect("survivor's owned block must remain allocated after sweep");
        assert!(ConsBlock::ptr_is_cell_aligned(ptr));
        assert!(ConsBlock::index_of_ptr(ptr) < block.next_index as usize);
        assert!(
            block.is_marked_ptr(ptr),
            "unmarked cons must never be dereferenced"
        );
        // SAFETY: the completed collector retained this owned, marked cell;
        // no allocation or next collection occurs between that proof and read.
        assert_eq!(unsafe { (*ptr).load_car() }, TaggedValue::fixnum(i as i64));
    }
}

fn finish(heap: &mut TestHeap, payloads: &[TaggedValue]) {
    heap.finish_concurrent_mark().unwrap();
    heap.incremental_drain_all();
    // Every orphan has its own already-black destination. Missing full or
    // partial chunks therefore turn the actual tricolor verifier red, before
    // root reseeding or sweep can hide the bug or free any referent.
    heap.close_alloc_regions();
    heap.verify_incremental_tricolor();
    for (i, &value) in payloads.iter().enumerate() {
        assert!(
            heap.is_value_marked(value),
            "SATB lost preexisting payload {i}"
        );
    }
    let bytes = heap.live_bytes();
    heap.incremental_finish(bytes, Instant::now());
    heap.finish_incremental_sweep_now();
    check_survivors(heap, payloads);
}

#[test]
fn satb_gc_orphan_cons_preimages_survive_10000_writes() {
    let mut heap = heap();
    heap.install();
    let payloads = payloads(&mut heap);
    let sources: Vec<_> = payloads
        .iter()
        .map(|&x| heap.alloc_cons(x, TaggedValue::NIL))
        .collect();
    let black: Vec<_> = (0..WRITES)
        .map(|_| heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL))
        .collect();
    begin(&mut heap, &black, &payloads);
    for (i, source) in sources.into_iter().enumerate() {
        // SAFETY: source predates launch, is retained through the epoch, and
        // is never seeded or otherwise visited by the marker's initial graph.
        let x = unsafe { (*source.xcons_ptr()).load_car() };
        assert!(set_cons_car(source, TaggedValue::NIL));
        assert!(set_cons_car(black[i], x));
    }
    finish(&mut heap, &payloads);
}

#[test]
fn satb_gc_orphan_vector_preimages_survive_10000_writes() {
    let mut heap = heap();
    heap.install();
    let payloads = payloads(&mut heap);
    let black: Vec<_> = (0..WRITES)
        .map(|_| heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL))
        .collect();
    begin(&mut heap, &black, &payloads);
    for (i, &x) in payloads.iter().enumerate() {
        // This vector is deliberately created AFTER the initial Tier-B
        // snapshot; it must not preserve X via an unrelated blanket scan.
        let source = heap.alloc_vector(vec![x]);
        with_vector_data_mut(source, |slots| slots[0] = TaggedValue::NIL).unwrap();
        assert!(set_cons_car(black[i], x));
    }
    finish(&mut heap, &payloads);
}

#[test]
fn satb_gc_orphan_symbol_cell_preimages_survive_10000_writes() {
    let mut heap = heap();
    heap.install();
    let payloads = payloads(&mut heap);
    let black: Vec<_> = (0..WRITES)
        .map(|_| heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL))
        .collect();
    let mut obarray = Obarray::new();
    let slot = intern("satb-orphan-root-cell");
    obarray.set_symbol_value_id(slot, TaggedValue::NIL);
    begin(&mut heap, &black, &payloads);
    for (i, &x) in payloads.iter().enumerate() {
        obarray.set_symbol_value_id(slot, x);
        obarray.set_symbol_value_id(slot, TaggedValue::NIL);
        assert!(set_cons_car(black[i], x));
    }
    finish(&mut heap, &payloads);
}

#[test]
fn satb_gc_orphan_string_interval_preimages_survive_10000_writes() {
    let mut heap = heap();
    heap.install();
    let payloads = payloads(&mut heap);
    let mut sources = Vec::with_capacity(WRITES);
    for &x in &payloads {
        let mut properties = std::collections::HashMap::new();
        let name = TaggedValue::fixnum(1);
        properties.insert(name, x);
        let table = TextPropertyTable::from_dump(vec![PropertyInterval {
            start: 0,
            end: 1,
            properties,
            key_order: vec![name],
        }]);
        let mut string = LispString::from_utf8("x");
        *string.intervals_mut() = table;
        sources.push(heap.alloc_string(string));
    }
    let black: Vec<_> = (0..WRITES)
        .map(|_| heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL))
        .collect();
    begin(&mut heap, &black, &payloads);
    for (i, (source, &x)) in sources.into_iter().zip(&payloads).enumerate() {
        // SAFETY: only this fixture owns the unrooted string. The marker has
        // no reference to it; the enforced raw interval choke point records
        // its pre-image before clearing, without an independent owner retrace.
        unsafe {
            (*(source.as_string_ptr().unwrap() as *mut StringObj))
                .data
                .clear_intervals()
        };
        assert!(set_cons_car(black[i], x));
    }
    finish(&mut heap, &payloads);
}

#[test]
fn satb_gc_k_registered_mutators_and_midmark_retirement_preserve_10000_preimages() {
    let mut heap = heap();
    heap.install();
    let payloads = payloads(&mut heap);
    let sources: Vec<_> = payloads
        .iter()
        .map(|&x| heap.alloc_cons(x, TaggedValue::NIL))
        .collect();
    let black: Vec<_> = (0..WRITES)
        .map(|_| heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL))
        .collect();
    let ids = {
        // SAFETY: registration precedes launch; the fixture owns all four
        // actual MutatorGcState records on this one deterministic thread.
        let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
        (0..3)
            .map(|_| world.register_mutator_gc_for_test())
            .collect::<Vec<_>>()
    };
    begin(&mut heap, &black, &payloads);
    heap.install();
    for (i, source) in sources.into_iter().enumerate() {
        if i == WRITES / 2 {
            // Other mutators are parked on this physical thread; the marker
            // may consume published chunks while this owner retires.
            let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
            assert!(!world.mutator_gc_for_test(ids[1]).satb.satb.is_empty());
            world
                .retire_mutator_gc_for_test(ids[1])
                .unwrap()
                .acknowledge();
        }
        let mut owner = i % 4;
        if i >= WRITES / 2 && owner == 2 {
            owner = 0;
        }
        let write = |_| {
            // SAFETY: source remains allocated until this joined epoch ends;
            // no allocation, collection, or registry mutation occurs here.
            let x = unsafe { (*source.xcons_ptr()).load_car() };
            assert!(set_cons_car(source, TaggedValue::NIL));
            assert!(set_cons_car(black[i], x));
        };
        if owner == 0 {
            write(heap.root());
        } else {
            // SAFETY: the owner-switch guard installs both real hot and cold
            // state, clears the TLS SATB filter, and restores them on unwind.
            heap.with_mutator_gc_for_test(ids[owner - 1], write)
                .unwrap();
        }
    }
    assert_eq!(heap.mutators().count(), 4);
    finish(&mut heap, &payloads);
}

#[test]
fn satb_gc_unpublished_facade_folds_orphan_partial() {
    let mut heap = heap();
    heap.install();
    let x = heap.alloc_cons(TaggedValue::fixnum(0), TaggedValue::NIL);
    heap.concurrent_begin();
    heap.note_root_overwrite_value(x);
    assert!(!heap.current_mutator_satb().satb.is_empty());
    // SAFETY: this is a closed unpublished epoch with no worker, callback or
    // other root. Its orphan pre-image must float for this collection.
    let result = unsafe { heap.drain_to_quiescent(|_| {}) };
    drop(result.expect("facade must fold every unpublished partial before sweep"));
    check_survivors(&heap, &[x]);
}

#[test]
fn satb_gc_begin_rejects_and_preserves_orphan_residual() {
    let mut heap = heap();
    heap.install();
    let x = heap.alloc_cons(TaggedValue::fixnum(0), TaggedValue::NIL);
    heap.note_root_overwrite_value(x);
    let rejected =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| heap.concurrent_begin()));
    let retained = heap.current_mutator_satb().satb.storage_usage().words;
    // Tear down safely before asserting on a rejected mutant's behavior.
    heap.current_mutator_satb_mut().satb.abandon();
    assert!(
        rejected.is_err(),
        "begin must reject residual rather than silently clear it"
    );
    assert_eq!(
        retained, 1,
        "failed begin must conserve the orphan pre-image"
    );
}

#[test]
fn satb_gc_root_feed_publish_is_only_a_pacing_optimization() {
    let mut heap = heap();
    heap.install();
    let x = heap.alloc_cons(TaggedValue::fixnum(0), TaggedValue::NIL);
    let black = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    begin(&mut heap, &[black], &[x]);
    heap.feed_satb_roots(&[x]);
    heap.finish_concurrent_mark().unwrap();
    // The actual termination root reseed suffices even if the feed's final
    // publish is disabled. Only one word is fed, below the test CAP of4.
    heap.seed_root(x);
    heap.incremental_drain_all();
    assert!(heap.is_value_marked(x));
    heap.verify_incremental_tricolor();
    let bytes = heap.live_bytes();
    heap.incremental_finish(bytes, Instant::now());
    heap.finish_incremental_sweep_now();
    check_survivors(&heap, &[x]);
}
