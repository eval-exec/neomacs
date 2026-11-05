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
