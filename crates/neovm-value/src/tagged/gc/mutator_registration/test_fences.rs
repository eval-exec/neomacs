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

/// Peer-signed 18:40Z shape (a): typed scoped seam over the owner-private
/// admission word. Exactly the two states the probe stores.
pub enum AdmissionProbeState {
    InSlice(u32),
    Out,
}

impl AdmissionProbeState {
    fn word(self) -> u64 {
        match self {
            Self::InSlice(epoch) => super::admission::AdmissionWord::in_slice(epoch as u64),
            Self::Out => super::admission::AdmissionWord::OUT,
        }
    }
}

/// Restore-on-drop guard for [`MutatorPermit::hold_admission_for_test`]:
/// stores the PRIOR (quiescent) word back, so the deliberately inconsistent
/// state cannot outlive the tested transition.
#[must_use = "the guard restores the permit's prior admission word on drop"]
pub struct AdmissionProbeGuard<'a> {
    entry: &'a std::sync::Arc<super::MutatorEntry>,
    prior: u64,
}

impl Drop for AdmissionProbeGuard<'_> {
    fn drop(&mut self) {
        self.entry
            .admission
            .store(self.prior, std::sync::atomic::Ordering::Release);
    }
}

impl super::MutatorPermit<'_> {
    /// CAS-install a probe state from the current quiescent OUT word and
    /// return the restore-on-drop guard.
    ///
    /// # Safety
    /// No collector or catalog transition other than the one under test may
    /// observe this permit while the guard lives.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) unsafe fn hold_admission_for_test(
        &self,
        state: AdmissionProbeState,
    ) -> AdmissionProbeGuard<'_> {
        let prior = self
            .entry
            .admission
            .compare_exchange(
                super::admission::AdmissionWord::OUT,
                state.word(),
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .expect("admission probe requires a quiescent OUT word");
        AdmissionProbeGuard {
            entry: &self.entry,
            prior,
        }
    }
}
