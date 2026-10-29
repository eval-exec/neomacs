//! Cold collector ownership and sealed production/proof construction inputs.
//!
//! Collector-mode evidence lives below this legacy scheduling adapter. Both
//! nominal routes share one backing identity/thread validation before minting.
use super::super::Gen1Admission;
use super::{Baton, EraAdmissionFailure, QuiescentCollector, RegistrationError};

/// Shared identity/thread check for both nominal construction paths. Fields
/// and construction stay in this module; no caller can substitute another loan.
#[must_use]
pub(super) struct CheckedCollector<'heap> {
    collector: QuiescentCollector<'heap>,
    keeper: Baton,
}
impl<'heap> CheckedCollector<'heap> {
    fn check(
        collector: QuiescentCollector<'heap>,
        keeper: Baton,
    ) -> Result<Self, EraAdmissionFailure<'heap>> {
        let validation = (|| {
            if collector.heap_identity() != keeper.parallel_heap_identity() {
                return Err(RegistrationError::WrongHeap);
            }
            if !keeper.held_here() {
                return Err(RegistrationError::WrongThread);
            }
            Ok(())
        })();
        match validation {
            Ok(()) => Ok(Self { collector, keeper }),
            Err(reason) => Err(EraAdmissionFailure {
                reason,
                collector,
                keeper,
            }),
        }
    }
    pub(super) fn parts_mut(&mut self) -> (&mut QuiescentCollector<'heap>, &Baton) {
        (&mut self.collector, &self.keeper)
    }
    pub(super) fn refuse(self, reason: RegistrationError) -> EraAdmissionFailure<'heap> {
        let (collector, keeper) = self.into_parts();
        EraAdmissionFailure {
            reason,
            collector,
            keeper,
        }
    }
    pub(super) fn into_parts(self) -> (QuiescentCollector<'heap>, Baton) {
        (self.collector, self.keeper)
    }
}

/// Legacy adapter: combines value-layer collector evidence with the existing
/// scheduling/backing authority, without adding those types to the capability.
#[must_use]
pub(super) struct ProductionAdmission<'heap> {
    checked: CheckedCollector<'heap>,
}
impl<'heap> ProductionAdmission<'heap> {
    pub(super) fn check(
        admission: Gen1Admission<'heap>,
        keeper: Baton,
    ) -> Result<Self, EraAdmissionFailure<'heap>> {
        CheckedCollector::check(admission.into_collector(), keeper).map(|checked| Self { checked })
    }
}

mod sealed {
    pub trait Sealed {}
}
/// Only nominal construction witnesses can authorize the private core's mint.
pub(super) trait ConstructionEvidence: sealed::Sealed {}
#[must_use]
pub(super) struct Gen1Construction {
    _private: (),
}
impl sealed::Sealed for Gen1Construction {}
impl ConstructionEvidence for Gen1Construction {}

pub(super) trait CoreAdmission<'heap>: sealed::Sealed {
    type Evidence: ConstructionEvidence;
    fn into_checked(self) -> (CheckedCollector<'heap>, Self::Evidence);
}
impl sealed::Sealed for ProductionAdmission<'_> {}
impl<'heap> CoreAdmission<'heap> for ProductionAdmission<'heap> {
    type Evidence = Gen1Construction;
    fn into_checked(self) -> (CheckedCollector<'heap>, Self::Evidence) {
        (self.checked, Gen1Construction { _private: () })
    }
}

#[cfg(test)]
#[must_use]
pub(super) struct ProofAdmission<'heap> {
    checked: CheckedCollector<'heap>,
}
#[cfg(test)]
impl<'heap> ProofAdmission<'heap> {
    pub(super) fn check(
        collector: QuiescentCollector<'heap>,
        keeper: Baton,
    ) -> Result<Self, EraAdmissionFailure<'heap>> {
        CheckedCollector::check(collector, keeper).map(|checked| Self { checked })
    }
}
#[cfg(test)]
#[must_use]
pub(super) struct ProofConstruction {
    _private: (),
}
#[cfg(test)]
impl sealed::Sealed for ProofConstruction {}
#[cfg(test)]
impl ConstructionEvidence for ProofConstruction {}
#[cfg(test)]
impl sealed::Sealed for ProofAdmission<'_> {}
#[cfg(test)]
impl<'heap> CoreAdmission<'heap> for ProofAdmission<'heap> {
    type Evidence = ProofConstruction;
    fn into_checked(self) -> (CheckedCollector<'heap>, Self::Evidence) {
        (self.checked, ProofConstruction { _private: () })
    }
}

static_assertions::assert_not_impl_any!(ProductionAdmission<'static>: Send, Sync, Clone, Copy);
#[cfg(test)]
static_assertions::assert_not_impl_any!(ProofAdmission<'static>: Send, Sync, Clone, Copy);

#[cfg(test)]
mod tests;
