//! Cold collector ownership and sealed production/proof construction inputs.
//!
//! A production capability owns the actual exclusive heap loan and keeper.
//! Checking an environment string or carrying a heap ID alone cannot mint it.
use super::{Baton, EraAdmissionFailure, QuiescentCollector, RegistrationError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gen1AdmissionError {
    GenerationalRequired,
    Registration(RegistrationError),
}

#[must_use = "recover or drop the complete collector and keeper input"]
pub(crate) struct Gen1AdmissionFailure<'heap> {
    pub(crate) reason: Gen1AdmissionError,
    pub(crate) collector: QuiescentCollector<'heap>,
    pub(crate) keeper: Baton,
}
impl std::fmt::Debug for Gen1AdmissionFailure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Gen1AdmissionFailure")
            .field(&self.reason)
            .finish()
    }
}
impl<'heap> From<EraAdmissionFailure<'heap>> for Gen1AdmissionFailure<'heap> {
    fn from(failure: EraAdmissionFailure<'heap>) -> Self {
        Self {
            reason: Gen1AdmissionError::Registration(failure.reason),
            collector: failure.collector,
            keeper: failure.keeper,
        }
    }
}

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

/// Only the one checked exclusive-collector conversion can mint this type.
/// Owning the collector freezes the actual mode for its complete lifetime.
#[must_use = "consume this capability into production era admission"]
pub struct Gen1Admission<'heap> {
    checked: CheckedCollector<'heap>,
}
impl std::fmt::Debug for Gen1Admission<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gen1Admission")
            .field("heap", &self.checked.collector.heap_identity())
            .finish_non_exhaustive()
    }
}
impl<'heap> Gen1Admission<'heap> {
    #[cold]
    #[inline(never)]
    pub(crate) fn check(
        collector: QuiescentCollector<'heap>,
        keeper: Baton,
    ) -> Result<Self, Gen1AdmissionFailure<'heap>> {
        let mut checked = CheckedCollector::check(collector, keeper)?;
        // This is the only production mode read for N-mutator admission. It
        // borrows the actual retained collector; no TLS or per-call test.
        if !checked
            .collector
            .registration_heap_mut()
            .generational_enabled()
        {
            let (collector, keeper) = checked.into_parts();
            return Err(Gen1AdmissionFailure {
                reason: Gen1AdmissionError::GenerationalRequired,
                collector,
                keeper,
            });
        }
        Ok(Self { checked })
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
impl sealed::Sealed for Gen1Admission<'_> {}
impl<'heap> CoreAdmission<'heap> for Gen1Admission<'heap> {
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

static_assertions::assert_not_impl_any!(Gen1Admission<'static>: Send, Sync, Clone, Copy);
#[cfg(test)]
static_assertions::assert_not_impl_any!(ProofAdmission<'static>: Send, Sync, Clone, Copy);

#[cfg(test)]
mod tests;
