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

/// P1-HOOKS-2 FENCE (peer 16:4xZ): issue a thread projection. Identical
/// signature to `ThreadProjectionTable::project_for_test`; the method stays
/// `pub(crate)` and `cfg(any(test, feature = "test-support"))`, and the table
/// is the only authority that maps a header back to its descriptor.
pub fn project_for_test(
    table: &mut crate::forward::ThreadProjectionTable,
    arm: crate::forward::CanonicalArm,
    plain: crate::value::Value,
    host_projected: bool,
) -> &'static crate::forward::ThreadProjectionDescriptor {
    table.project_for_test(arm, plain, host_projected)
}

/// P1-HOOKS-2 FENCE (peer 16:4xZ): write a descriptor's canonical plain word.
/// Same body as `ThreadProjectionDescriptor::set_plain_value`, whose note-the-
/// overwritten-word contract is unchanged; the method stays `pub(crate)`.
pub fn set_plain_value(
    descriptor: &crate::forward::ThreadProjectionDescriptor,
    value: crate::value::Value,
) {
    descriptor.set_plain_value(value);
}

/// P1-HOOKS-2 unsafe FENCE (peer 16:4xZ): rebuild a cell handle from a raw
/// header address, exactly as `FwdRef::from_address`.
///
/// # Safety
/// The caller must pass an address that names a live descriptor header (the
/// cell only stores addresses that came from `FwdRef::of`); the conversion is
/// infallible by that invariant.
pub unsafe fn from_address(address: usize) -> crate::forward::FwdRef {
    // SAFETY: the caller contract is identical to `FwdRef::from_address`'s.
    unsafe { crate::forward::FwdRef::from_address(address) }
}

/// P1-HOOKS-2 unsafe FENCE (peer 16:4xZ): promote a header to the permanent
/// generation through the `ImageResident` capability.
///
/// # Safety
/// `header` must belong to an object live in this heap, and the caller must
/// hold the world-stopped guarantee: no collection is in progress and no
/// mutator is running. The permanent generation stays reserved for the dump
/// image; `resident` must be the real image-row proof (or its `for_test`
/// capability in a fixture).
pub unsafe fn make_permanent_for_test(
    header: &mut crate::tagged::header::GcHeader,
    resident: &crate::tagged::header::ImageResident,
) {
    header.make_permanent(resident);
}

/// P1-HOOKS-2 FENCE (peer 16:4xZ): opaque copy of an obarray's function-cell
/// publication clock. The value can only be minted by
/// [`snapshot_function_epoch_for_test`] from a live obarray and restored into
/// the same obarray; it is not a general epoch setter (forging the clock can
/// make stale dispatch caches look valid).
#[derive(Clone, Copy, Debug)]
pub struct FunctionEpochSnapshot(u64);

/// The snapshot half of the signed snapshot/restore fence.
pub fn snapshot_function_epoch_for_test(obarray: &crate::symbol::Obarray) -> FunctionEpochSnapshot {
    FunctionEpochSnapshot(obarray.function_epoch_snapshot_raw())
}

/// The restore half of the signed snapshot/restore fence: store back a clock
/// this same obarray read earlier.
pub fn restore_function_epoch_for_test(
    obarray: &mut crate::symbol::Obarray,
    snapshot: FunctionEpochSnapshot,
) {
    obarray.restore_function_epoch_raw(snapshot.0);
}
/// Value-FENCE row (peer 16:44:46Z): typed serial-fixture hook arming or
/// disarming the concurrent SATB barrier without a GC thread. The original
/// method is `pub(crate) unsafe fn`; this delegate keeps its contract.
///
/// # Safety
/// No concurrent marker has been launched on this heap and none can run
/// during the call (serial synthetic fixture only). `on = false` disables the
/// SATB barrier, so a live marker would miss and free reachable objects.
#[cfg(any(test, feature = "test-support"))]
pub unsafe fn set_concurrent_active_for_test(heap: &mut crate::tagged::gc::TaggedHeap, on: bool) {
    // SAFETY: the caller upholds the serial-fixture contract above.
    unsafe { heap.set_concurrent_active_for_test(on) }
}

/// Value-FENCE row (peer 16:44:46Z): typed serial-fixture hook consuming the
/// published SATB handoff. The original method is `pub(crate) unsafe fn`.
///
/// # Safety
/// No concurrent marker has been launched on this heap and none can run
/// during the call; the published SATB handoff belongs to this serial
/// synthetic fixture and is consumed exactly once.
#[cfg(any(test, feature = "test-support"))]
pub unsafe fn take_satb_shared_for_test(
    heap: &mut crate::tagged::gc::TaggedHeap,
) -> Vec<crate::tagged::value::TaggedValue> {
    // SAFETY: the caller upholds the serial-fixture contract above.
    unsafe { heap.take_satb_shared_for_test() }
}
/// Value-FENCE row (Ruling P1-VALUE-CLASSES C1): owner-side admission witness
/// constructor for the relocated parallel transport tests. The witness is
/// metadata only and the original method stays crate-internal.
pub fn owner_root_admission_for_test<'a>(
    identity: crate::tagged::transport::ParallelRootIdentity,
    roots: &'a [crate::tagged::transport::SharedRoot],
) -> crate::tagged::gc::mutator_registration::OwnerRootAdmission<'a> {
    crate::tagged::gc::mutator_registration::OwnerRootAdmission::for_test(identity, roots)
}

/// Value-FENCE row (Ruling P1-VALUE-CLASSES C1): stopped-side witness
/// constructor. The original method stays crate-internal.
///
/// # Safety
/// The returned witness gates stopped-world root reads: the caller must hold
/// the stopped `AllStopped` world for this exact identity and epoch, and no
/// other owner may resume while the witness lives.
pub unsafe fn stopped_root_admission_for_test<'a>(
    identity: crate::tagged::transport::ParallelRootIdentity,
    epoch: u64,
) -> crate::tagged::gc::mutator_registration::StoppedRootAdmission<'a> {
    // SAFETY: the caller upholds the identical stopped-world contract above.
    unsafe {
        crate::tagged::gc::mutator_registration::StoppedRootAdmission::for_test(identity, epoch)
    }
}

/// Value-FENCE row (peer 17:27:27Z C1 TOKEN): the epoch-retention mints a
/// `FacadeEpochRetention` token (it retains exclusion through the final
/// publication or reader obligation), so it is fenced rather than VIS. Safe:
/// a retention only delays; it cannot unsound anything.
pub fn test_root_epoch_retention<'heap, K: crate::tagged::gc::EraKeeper>(
    era: &ProofEra<'heap, K>,
) -> crate::tagged::gc::FacadeEpochRetention {
    era.test_root_epoch_retention()
}

/// P1-HOOKS-2 FENCE (peer 17:04:43Z pre-signed): record a change of a thread
/// projection's logical redirect. Identical signature to
/// `ThreadProjectionDescriptor::set_arm`; the method stays `pub(crate)` and
/// the `Cell` write stays inside this crate.
pub fn set_arm(
    descriptor: &crate::forward::ThreadProjectionDescriptor,
    arm: crate::forward::CanonicalArm,
) {
    descriptor.set_arm(arm);
}
