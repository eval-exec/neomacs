//! Signed test-support probes for the re-homed Tier-H user-guard test.
//!
//! Every function is a plain-data copy or a scoped policy write. The public
//! routes are the `test_support::authority` delegates, one AUTHORITY row
//! each. This child is the domain module for those probes, so `gc.rs` stays
//! under the facade ceiling. Child modules of `tagged::gc` already name the
//! `pub(super)` mutator accessors and the heap's private start-slot field.

use super::TaggedHeap;
use super::concurrent_hash::{self, HashTableScanPolicy};
use super::knobs;

#[cfg(feature = "test-support")]
pub(crate) fn set_concurrent_claims_for_test(on: Option<bool>) {
    knobs::set_concurrent_claims_for_test(on);
}

#[cfg(feature = "test-support")]
pub(crate) fn concurrent_hash_scan_policy_is_always_clone_for_test() -> bool {
    knobs::concurrent_hash_scan_policy() == HashTableScanPolicy::AlwaysClone
}

#[cfg(feature = "test-support")]
pub(crate) fn hash_snapshot_untouched_for_test() -> u8 {
    concurrent_hash::untouched_for_test()
}

#[cfg(feature = "test-support")]
pub(crate) fn hash_snapshot_entry_state_for_test(
    heap: &TaggedHeap,
    owner: usize,
) -> (u8, bool, bool) {
    heap.concurrent_hash_snapshot()
        .unwrap()
        .entry_state_for_test(owner)
}

#[cfg(feature = "test-support")]
pub(crate) fn hash_snapshot_present_for_test(heap: &TaggedHeap) -> bool {
    heap.concurrent_hash_snapshot().is_some()
}

#[cfg(feature = "test-support")]
pub(crate) fn concurrent_hash_mutator_state_for_test(
    heap: &TaggedHeap,
) -> (usize, Vec<crate::tagged::value::TaggedValue>) {
    let state = heap.current_concurrent_hash_mutator();
    let state = state.lock().unwrap();
    (
        state.retired_hash_buffers.len(),
        state.written_hash_owners.clone(),
    )
}

#[cfg(feature = "test-support")]
pub(crate) fn set_minors_since_major_for_test(heap: &mut TaggedHeap, value: usize) {
    heap.current_mutator_gc_mut().pacing.minors_since_major = value;
}

#[cfg(feature = "test-support")]
pub(crate) fn concurrent_obarray_start_slots_present_for_test(heap: &TaggedHeap) -> bool {
    heap.concurrent_obarray_start_slots.is_some()
}
