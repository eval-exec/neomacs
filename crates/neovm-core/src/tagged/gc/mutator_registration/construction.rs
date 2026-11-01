//! Cold collector ownership and sealed production/proof construction inputs.
//!
//! Collector-mode evidence lives below this legacy scheduling adapter. Both
//! nominal routes share one backing identity/thread validation before minting.
use super::super::Gen1Admission;
use super::{EraAdmissionFailure, EraKeeper, QuiescentCollector, RegistrationError};

/// Shared identity/thread check for both nominal construction paths. Fields
/// and construction stay in this module; no caller can substitute another loan.
#[must_use]
pub(super) struct CheckedCollector<'heap, K: EraKeeper> {
    collector: QuiescentCollector<'heap>,
    keeper: K,
}
impl<'heap, K: EraKeeper> CheckedCollector<'heap, K> {
    fn check(
        collector: QuiescentCollector<'heap>,
        keeper: K,
    ) -> Result<Self, EraAdmissionFailure<'heap, K>> {
        let validation = (|| {
            if collector.heap_identity() != keeper.parallel_heap_identity() {
                return Err(RegistrationError::WrongHeap);
            }
            keeper.keeper_designator()?;
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
    pub(super) fn parts_mut(&mut self) -> (&mut QuiescentCollector<'heap>, &K) {
        (&mut self.collector, &self.keeper)
    }
    pub(super) fn refuse(self, reason: RegistrationError) -> EraAdmissionFailure<'heap, K> {
        let (collector, keeper) = self.into_parts();
        EraAdmissionFailure {
            reason,
            collector,
            keeper,
        }
    }
    pub(super) fn into_parts(self) -> (QuiescentCollector<'heap>, K) {
        (self.collector, self.keeper)
    }
}

/// Legacy adapter: combines value-layer collector evidence with the existing
/// scheduling/backing authority, without adding those types to the capability.
#[must_use]
pub(super) struct ProductionAdmission<'heap, K: EraKeeper> {
    checked: CheckedCollector<'heap, K>,
}
impl<'heap, K: EraKeeper> ProductionAdmission<'heap, K> {
    pub(super) fn check(
        admission: Gen1Admission<'heap>,
        keeper: K,
    ) -> Result<Self, EraAdmissionFailure<'heap, K>> {
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

pub(super) trait CoreAdmission<'heap, K: EraKeeper>: sealed::Sealed {
    type Evidence: ConstructionEvidence;
    fn into_checked(self) -> (CheckedCollector<'heap, K>, Self::Evidence);
}
impl<K: EraKeeper> sealed::Sealed for ProductionAdmission<'_, K> {}
impl<'heap, K: EraKeeper> CoreAdmission<'heap, K> for ProductionAdmission<'heap, K> {
    type Evidence = Gen1Construction;
    fn into_checked(self) -> (CheckedCollector<'heap, K>, Self::Evidence) {
        (self.checked, Gen1Construction { _private: () })
    }
}

#[cfg(test)]
#[must_use]
pub(super) struct ProofAdmission<'heap, K: EraKeeper> {
    checked: CheckedCollector<'heap, K>,
}
#[cfg(test)]
impl<'heap, K: EraKeeper> ProofAdmission<'heap, K> {
    pub(super) fn check(
        collector: QuiescentCollector<'heap>,
        keeper: K,
    ) -> Result<Self, EraAdmissionFailure<'heap, K>> {
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
impl<K: EraKeeper> sealed::Sealed for ProofAdmission<'_, K> {}
#[cfg(test)]
impl<'heap, K: EraKeeper> CoreAdmission<'heap, K> for ProofAdmission<'heap, K> {
    type Evidence = ProofConstruction;
    fn into_checked(self) -> (CheckedCollector<'heap, K>, Self::Evidence) {
        (self.checked, ProofConstruction { _private: () })
    }
}

#[cfg(test)]
static_assertions::assert_not_impl_any!(ProductionAdmission<'static, super::Baton>: Send, Sync, Clone, Copy);
#[cfg(test)]
static_assertions::assert_not_impl_any!(ProofAdmission<'static, super::Baton>: Send, Sync, Clone, Copy);

#[cfg(test)]
mod tests;
