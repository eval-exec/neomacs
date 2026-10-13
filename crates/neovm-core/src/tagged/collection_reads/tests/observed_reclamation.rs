//! A real clear in a different heap must not reconstruct surviving geometry.
//!
//! Both heaps execute on this test's mutator. The certificate stays !Send, and
//! its owner remains allocated in the live heap while only the other heap is
//! collected. The counter is test-only metadata inside the existing State.

use super::super::{CompiledJournalMode, STATE, capture, force_compiled_journal_for_test};
use crate::tagged::gc::{
    TaggedHeap, collection_observation_epoch, collection_observed_metadata,
    mark_collection_observed, set_tagged_heap,
};
use crate::tagged::value::TaggedValue;

struct ObservedMode;

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

fn geometry_rebuilds() -> usize {
    STATE.with(|state| state.borrow().observed.geometry_rebuilds_for_test())
}

#[test]
fn unrelated_heap_reclamation_preserves_live_envelope_geometry() {
    crate::test_utils::init_test_tracing();
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let _mode = ObservedMode;
    let mut live_heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut live_heap);
    let owner = live_heap.alloc_cons(TaggedValue::make_int(97), TaggedValue::NIL);
    let (_, reads) = capture(|| owner.cons_car());
    let reads = reads.expect("the retained owner supplies a real certificate");
    assert!(collection_observed_metadata(owner.bits()));
    let bounds = STATE.with(|state| state.borrow().observed.full_bounds());
    let window = live_heap.jit_barrier_window_for_test();
    let rebuilds = geometry_rebuilds();

    let mut other_heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut other_heap);
    let garbage = other_heap.alloc_cons(TaggedValue::make_int(7), TaggedValue::NIL);
    // A shared mark does not insert a dependency into this mutator's ledger.
    // Its actual reclamation must advance the shared epoch, not merely invoke
    // a sweep publisher with no marks removed.
    assert!(mark_collection_observed(garbage.bits()));
    let epoch = collection_observation_epoch();
    other_heap.collect_exact(std::iter::empty());
    assert_ne!(collection_observation_epoch(), epoch);
    assert!(!collection_observed_metadata(garbage.bits()));
    set_tagged_heap(&mut live_heap);

    assert_eq!(
        geometry_rebuilds(),
        rebuilds,
        "an unrelated real mark clear must not reconstruct the live address tree"
    );
    assert_eq!(
        STATE.with(|state| state.borrow().observed.full_bounds()),
        bounds
    );
    assert_eq!(live_heap.jit_barrier_window_for_test(), window);
    assert!(live_heap.owns_heap_value_for_test(owner));
    assert!(reads.unchanged());
    owner.set_car(TaggedValue::make_int(23));
    assert!(
        !reads.unchanged(),
        "the retained dependency still invalidates"
    );
}
