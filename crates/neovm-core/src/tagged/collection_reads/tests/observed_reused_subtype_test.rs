//! Cached native eligibility promotion, modeled with an actual live Record.
//!
//! The local ledger is seeded with a previous owner's `Other` geometry at the
//! live Record's identity. This models the stale eligibility state without
//! moving a Context or a Lisp value between threads. Every header read and
//! publication below belongs to this thread's retained heap. Existing
//! reclamation tests cover actual sweep/reuse; this is not a migration test.

use super::{
    CompiledJournalMode, NativeWindowCollectionOwner, OBSERVED_OWNER_PUBLICATIONS, State,
    force_compiled_journal_for_test, native_window_collection_owner,
    publish_observed_owner_in_state,
};
use crate::tagged::gc::{
    TaggedHeap, collection_observed_metadata, mark_collection_observed, set_tagged_heap,
};
use crate::tagged::value::{TAG_MASK, TaggedValue};

#[must_use = "dropping restores this mutator's journal policy"]
#[derive(Debug)]
struct ObservedMode;

impl ObservedMode {
    fn enter() -> Self {
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        Self
    }
}

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

#[test]
fn observed_cached_other_eligibility_promotes_from_a_live_record_projection() {
    crate::test_utils::init_test_tracing();
    let _mode = ObservedMode::enter();
    let mut heap = Box::new(TaggedHeap::new());
    heap.set_gc_threshold(usize::MAX);
    set_tagged_heap(&mut heap);
    let record = heap.alloc_record(vec![TaggedValue::make_int(19)]);
    let bits = record.bits();
    let address = bits & !TAG_MASK;
    assert_eq!(
        native_window_collection_owner(bits),
        NativeWindowCollectionOwner::Slots
    );
    assert!(mark_collection_observed(bits));
    assert!(collection_observed_metadata(bits));

    let mut state = State::default();
    assert!(
        state
            .observed
            .insert(bits, NativeWindowCollectionOwner::Other)
    );
    assert!(state.observed.already_published(bits));
    assert!(!state.observed.has_slot_geometry(bits));
    assert!(
        state
            .observed
            .gate()
            .slots
            .iter()
            .all(|range| !range.covers(address))
    );
    let publications = OBSERVED_OWNER_PUBLICATIONS.with(std::cell::Cell::get);

    // The shared mark is already set: the return value describes whether that
    // bit was newly set, not whether this ledger's gate was republished.
    assert!(!publish_observed_owner_in_state(&mut state, bits));
    assert!(
        state.observed.has_slot_geometry(bits),
        "cached identity membership must not suppress live Record slot eligibility"
    );
    assert!(
        state
            .observed
            .gate()
            .slots
            .iter()
            .any(|range| range.covers(address))
    );
    assert!(
        state
            .observed
            .gate()
            .observed
            .iter()
            .any(|range| range.covers(address))
    );
    assert_eq!(
        OBSERVED_OWNER_PUBLICATIONS.with(std::cell::Cell::get),
        publications + 1,
        "promoted geometry must follow the ordinary owner/gate publication path"
    );
    assert!(collection_observed_metadata(bits));

    // Once positive geometry has been retained, the ordinary cached hit needs
    // no repeated publication. Native Record journaling is covered separately
    // by the gen0_inline_record_aset_invalidates_collection_read_certificates test.
    assert!(!publish_observed_owner_in_state(&mut state, bits));
    assert_eq!(
        OBSERVED_OWNER_PUBLICATIONS.with(std::cell::Cell::get),
        publications + 1
    );
}
