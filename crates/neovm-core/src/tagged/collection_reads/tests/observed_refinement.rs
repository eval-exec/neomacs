//! Exact singleton and slot geometry use real live publication identities.
//!
//! The envelope belongs to this mutator's existing journal State. The tests
//! keep every dependency alive, and inspect the prepared gate before the
//! caller snapshots a newly observed owner's collection revision.

use super::super::{
    CompiledJournalMode, STATE, capture, compiled_observation_gate, force_compiled_journal_for_test,
};
use crate::buffer::BufferId;
use crate::heap_types::LispString;
use crate::tagged::gc::{BarrierWindow, TaggedHeap, collection_observed_metadata, set_tagged_heap};
use crate::tagged::header::{ConsCell, VecLikeType};
use crate::tagged::value::{TAG_MASK, TaggedValue};

struct ObservedMode;

impl ObservedMode {
    fn begin() -> Self {
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        Self
    }
}

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

fn address(owner: TaggedValue) -> usize {
    owner.bits() & !TAG_MASK
}

#[test]
fn observed_small_cons_exemption_survives_large_gap_and_closes_before_publication() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let mut cells: [TaggedValue; 9] =
        std::array::from_fn(|_| heap.alloc_cons(TaggedValue::make_int(97), TaggedValue::NIL));
    cells.sort_unstable_by_key(|owner| address(*owner));
    for pair in cells.windows(2) {
        assert_eq!(address(pair[1]) - address(pair[0]), size_of::<ConsCell>());
    }
    let (_, endpoints) = capture(|| {
        (
            cells[0].cons_car(),
            cells[2].cons_car(),
            cells[8].cons_car(),
        )
    });
    let endpoints = endpoints.expect("the retained endpoints form coherent real dependencies");
    assert!(!collection_observed_metadata(cells[1].bits()));
    assert!(!collection_observed_metadata(cells[5].bits()));

    assert!(STATE.with(|state| {
        state
            .borrow_mut()
            .observed
            .exclude_unobserved(cells[1].bits())
    }));
    let small = compiled_observation_gate(BarrierWindow::NONE);
    assert_eq!(
        small.single_cons,
        address(cells[1]),
        "a two-cell interval needs a retained exact cons exemption",
    );
    assert!(STATE.with(|state| {
        state
            .borrow_mut()
            .observed
            .exclude_unobserved(cells[5].bits())
    }));
    let large = compiled_observation_gate(BarrierWindow::NONE);
    assert_eq!(
        large.single_cons,
        address(cells[1]),
        "learning a larger interval must preserve the independent exact cons exemption",
    );
    let gap = large
        .excluded
        .expect("the larger locally empty interval is retained");
    assert!(gap.covers(address(cells[5])));
    assert!(gap.len() > 2 * size_of::<ConsCell>());
    assert!(!gap.covers(address(cells[1])));

    let (_, singleton) = capture(|| {
        let value = cells[1].cons_car();
        let published = STATE.with(|state| state.borrow().observed.gate());
        assert_eq!(
            published.single_cons, 0,
            "the newly observed exact owner closes its exemption"
        );
        assert_eq!(
            published.excluded,
            Some(gap),
            "an unrelated larger gap remains sound"
        );
        assert!(collection_observed_metadata(cells[1].bits()));
        value
    });
    let singleton = singleton.expect("the new owner is published before its dependency snapshot");
    assert!(endpoints.unchanged());
    assert!(singleton.unchanged());
    cells[1].set_car(TaggedValue::make_int(23));
    assert!(!singleton.unchanged());
    assert!(endpoints.unchanged());
}

#[test]
fn observed_slot_geometry_only_includes_vector_and_record_owners() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::begin();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let cons = heap.alloc_cons(TaggedValue::make_int(97), TaggedValue::NIL);
    let string = heap.alloc_string(LispString::from_utf8("abc"));
    let reference = heap.alloc_buffer(BufferId(7201));
    let vector = heap.alloc_vector(vec![TaggedValue::make_int(97)]);
    let record = heap.alloc_record(vec![TaggedValue::make_int(97)]);
    let (_, reads) = capture(|| {
        assert_eq!(cons.cons_car(), TaggedValue::make_int(97));
        assert_eq!(string.as_str_owned().unwrap(), "abc");
        assert_eq!(reference.veclike_type(), Some(VecLikeType::Buffer));
        assert_eq!(
            vector.as_vector_data().unwrap()[0],
            TaggedValue::make_int(97)
        );
        assert_eq!(record.veclike_type(), Some(VecLikeType::Record));
    });
    let reads = reads.expect("all retained object classes supply real dependencies");
    let lo = address(vector).min(address(record));
    let hi = address(vector).max(address(record)) + 1;
    let gate = compiled_observation_gate(BarrierWindow::NONE);
    assert_eq!(
        gate.slots,
        [BarrierWindow::span(lo, hi), BarrierWindow::NONE],
        "slot geometry must be built only from live Vector and Record publication facts",
    );
    assert!(
        gate.observed
            .iter()
            .any(|range| range.covers(address(cons)))
    );
    for owner in [cons, string, reference, vector, record] {
        assert!(collection_observed_metadata(owner.bits()));
        assert!(STATE.with(|state| state.borrow().observed.already_published(owner.bits())));
    }
    assert!(reads.unchanged());
    assert!(crate::tagged::mutate::set_vector_slot(
        vector,
        0,
        TaggedValue::make_int(23)
    ));
    assert!(!reads.unchanged());
}
