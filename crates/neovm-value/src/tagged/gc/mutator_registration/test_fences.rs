//! Fence delegates that must be written inside the `crate::tagged::gc` module
//! tree because their signatures name crate-visible registry/resume types.
//!
//! The module is `pub(crate)`; `neovm_value::test_support::authority` is the
//! only public re-export path, so a compile-fail fixture that names an
//! original item keeps failing. Each item has one AUTHORITY-ITEMS row.

use super::Mutator;
use super::rendezvous::{RendezvousError, ResumeGcImage};
use crate::tagged::gc::ProofEra;

/// Deliberate registry poisoning for the re-homed root-slot panic-path test:
/// the test proves that an abandoned era is conservatively retained, so it
/// injects the same poisoned state the production Drop path publishes. The
/// original `Registry::poison` stays `pub(super)`.
#[allow(private_interfaces)]
pub fn poison_registry_for_test<'heap, K: crate::tagged::gc::EraKeeper>(era: &ProofEra<'heap, K>) {
    era.registry().poison();
}

/// REGWAIT-GAP fence: begin the retrying restore image for the re-homed
/// root-slot restore loop. The original stays
/// `pub(in crate::tagged::gc::mutator_registration)`; the caller cannot name
/// the returned image type, only drive it.
#[allow(private_interfaces)]
pub fn begin_restore_retry_for_test<'owner, 'era>(
    owner: &'owner mut Mutator<'era>,
    native: bool,
) -> Result<ResumeGcImage<'owner, 'era>, RendezvousError> {
    owner.begin_restore_retry(native)
}
