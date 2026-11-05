//! Signed Prep-D R6 TA2 authority fence (peer rulings 05:5xZ-b and 06:0xZ).
//!
//! This module may be named only from `crates/neovm-core/core_tests/**`; the
//! architecture TA2 guard rejects every `src/**` or `tests/**` reference.
//! Each item here is one row of `AUTHORITY-ITEMS.tsv`, signed before code.
//! The fence re-exports existing drivers and hooks with their bodies and
//! contracts unchanged, and never widens an original path, so a compile-fail
//! fixture that names the original item keeps failing.
//!
//! Row 1: `mark_collection_observed` (existing GC observation driver).

/// Publish an exact sticky collection-observation mark for a live heap owner,
/// without observing a pointer projection or consulting the active heap. The
/// caller gates this API on observed mode and publishes its own JIT address
/// envelope before any subsequent compiled mutation. Returns whether the
/// shared mark is new.
///
/// This is the existing `pub(crate)` driver under the signed fence spelling;
/// body and contract are unchanged.
pub fn mark_collection_observed(bits: usize) -> bool {
    crate::tagged::gc::mark_collection_observed(bits)
}

/// The restricted proof-era type used by the re-homed era-fixture tests.
/// Same item as `tagged::gc::GenericProofEra`; the original path stays
/// `cfg(any(test, feature = "test-support"))` + `pub(crate)`.
pub type ProofEra<'heap, K> = crate::tagged::gc::ProofEra<'heap, K>;

/// Finish refusal of [`ProofEra`]; same item as
/// `tagged::gc::GenericProofEraFinishFailure`.
pub type ProofEraFinishFailure<'heap, K> = crate::tagged::gc::ProofEraFinishFailure<'heap, K>;

/// Fence-only symbol-authority word occupancy (peer 09:2xZ): the word itself
/// never leaves value. Callers get a copied u64 reading or an RAII guard whose
/// Drop restores quiescence; the reader/writer sentinel stays private.
#[doc(hidden)]
pub use crate::tagged::gc::mutator_registration::symbol_authority::occupancy::{
    OccupiedSymbolWord, SymbolWordOccupancy,
};

/// Fence-only registration-wait helpers (REGWAIT-GAP): test timing and retry
/// classification only; they add no authority path.
#[doc(hidden)]
pub use crate::tagged::gc::mutator_registration::retry::{
    Deadline, Retry, SPIN_WAIT, retry, retry_owned,
};
#[doc(hidden)]
pub use crate::tagged::gc::mutator_registration::symbol_functions::retry::rendezvous;

/// Fence delegates for the re-homed root-slot tests (peer-signed rows): the
/// registry poison hook and the REGWAIT-GAP restore-retry begin. The originals
/// stay restricted; the module is crate-visible, so this re-export is their
/// only public path.
pub use crate::tagged::gc::mutator_registration::test_fences::{
    begin_restore_retry_for_test, poison_registry_for_test,
};

/// Fence delegate for the deterministic host-side root-table contention
/// scope. The original stays `cfg(any(test, feature = "test-support"))`
/// `pub(crate)`, so this delegate is its only cross-crate route.
pub fn with_root_table_lock_for_test(heap: crate::tagged::gc::HeapIdentity, op: impl FnOnce()) {
    crate::tagged::transport::with_root_table_lock_for_test(heap, op)
}

/// Typed fence hook (AUTHORITY row): exhaust the owner's root-slot
/// generation to force the overflow refusal path. There is deliberately no
/// general generation setter; the state field stays private.
#[cfg(any(test, feature = "test-support"))]
pub fn exhaust_root_slot_generation_for_test(run: &mut crate::tagged::gc::RunAccess<'_, '_>) {
    run.exhaust_root_slot_generation_for_test();
}

/// # Safety
/// prepare must return the original retained keeper's epoch and backing,
/// without executing Lisp, resuming owners, or leaking a manager reference.
#[cold]
pub unsafe fn admit_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    collector: crate::tagged::gc::QuiescentCollector<'heap>,
    keeper: K,
    prepare: impl FnOnce(
        &K,
    ) -> Result<
        (
            crate::tagged::gc::FacadeEpochRetention,
            crate::symbol::parallel::SymbolBacking,
        ),
        crate::tagged::gc::RegistrationError,
    >,
) -> Result<ProofEra<'heap, K>, crate::tagged::gc::EraAdmissionFailure<'heap, K>> {
    // SAFETY: the caller contract is identical to `ProofEra::admit_with`.
    unsafe { ProofEra::admit_with(collector, keeper, prepare) }
}

/// # Safety
/// prepare must return the original retained keeper's epoch and backing,
/// without executing Lisp, resuming owners, or leaking a manager reference.
#[cold]
pub unsafe fn admit_rooted_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    collector: crate::tagged::gc::QuiescentCollector<'heap>,
    keeper: K,
    roots: crate::tagged::gc::FrozenLegacyRoots,
    prepare: impl FnOnce(
        &K,
    ) -> Result<
        (
            crate::tagged::gc::FacadeEpochRetention,
            crate::symbol::parallel::SymbolBacking,
        ),
        crate::tagged::gc::RegistrationError,
    >,
) -> Result<ProofEra<'heap, K>, crate::tagged::gc::EraAdmissionFailure<'heap, K>> {
    // SAFETY: the caller contract is identical to `ProofEra::admit_rooted_with`.
    unsafe { ProofEra::admit_rooted_with(collector, keeper, roots, prepare) }
}
