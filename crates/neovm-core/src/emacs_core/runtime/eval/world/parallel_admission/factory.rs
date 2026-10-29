//! Static nominal constructors for the single owned-Context transaction.
//! Only this child module implements a plan; proof plans do not exist in a
//! product build and cannot manufacture the production projection.
use super::super::{Baton, FrozenLegacyRoots};
#[cfg(test)]
use super::OwnedProofProjection;
use super::{OwnedParallelError, OwnedParallelProjection, map_registration};
#[cfg(test)]
use crate::tagged::gc::ProofEra;
use crate::tagged::gc::{Gen1Admission, Gen1AdmissionError, ParallelHeapEra, QuiescentCollector};
#[cfg(test)]
use crate::tagged::value::TaggedValue;

mod sealed {
    pub trait Sealed {}
}

/// A refused factory still owns both serial return capabilities. No plan may
/// discard them before the shared caller performs its recovery transaction.
#[must_use]
pub(super) struct AdmissionFailure<'heap> {
    pub(super) reason: OwnedParallelError,
    pub(super) collector: QuiescentCollector<'heap>,
    pub(super) keeper: Baton,
}

pub(super) trait AdmissionPlan: sealed::Sealed {
    type Projection<'heap>;

    fn admit<'heap>(
        self,
        collector: QuiescentCollector<'heap>,
        keeper: Baton,
        roots: FrozenLegacyRoots,
    ) -> Result<Self::Projection<'heap>, AdmissionFailure<'heap>>;
}

pub(super) struct ProductionPlan;
impl sealed::Sealed for ProductionPlan {}
impl AdmissionPlan for ProductionPlan {
    type Projection<'heap> = OwnedParallelProjection<'heap>;

    fn admit<'heap>(
        self,
        collector: QuiescentCollector<'heap>,
        keeper: Baton,
        roots: FrozenLegacyRoots,
    ) -> Result<Self::Projection<'heap>, AdmissionFailure<'heap>> {
        let admission = Gen1Admission::check(collector, keeper).map_err(|failure| {
            let reason = match failure.reason {
                Gen1AdmissionError::GenerationalRequired => {
                    OwnedParallelError::GenerationalRequired
                }
                Gen1AdmissionError::Registration(error) => map_registration(error),
            };
            AdmissionFailure {
                reason,
                collector: failure.collector,
                keeper: failure.keeper,
            }
        })?;
        ParallelHeapEra::admit_rooted(admission, roots)
            .map(|era| OwnedParallelProjection { era })
            .map_err(|failure| AdmissionFailure {
                reason: map_registration(failure.reason),
                collector: failure.collector,
                keeper: failure.keeper,
            })
    }
}

#[cfg(test)]
pub(super) struct ProofPlan;
#[cfg(test)]
impl sealed::Sealed for ProofPlan {}
#[cfg(test)]
impl AdmissionPlan for ProofPlan {
    type Projection<'heap> = OwnedProofProjection<'heap>;

    fn admit<'heap>(
        self,
        collector: QuiescentCollector<'heap>,
        keeper: Baton,
        roots: FrozenLegacyRoots,
    ) -> Result<Self::Projection<'heap>, AdmissionFailure<'heap>> {
        ProofEra::admit_rooted(collector, keeper, roots)
            .map(|era| OwnedProofProjection { era })
            .map_err(|failure| AdmissionFailure {
                reason: map_registration(failure.reason),
                collector: failure.collector,
                keeper: failure.keeper,
            })
    }
}

#[cfg(test)]
pub(super) struct WeakProofPlan<'tables> {
    pub(super) tables: &'tables [TaggedValue],
}
#[cfg(test)]
impl sealed::Sealed for WeakProofPlan<'_> {}
#[cfg(test)]
impl AdmissionPlan for WeakProofPlan<'_> {
    type Projection<'heap> = OwnedProofProjection<'heap>;

    fn admit<'heap>(
        self,
        collector: QuiescentCollector<'heap>,
        keeper: Baton,
        roots: FrozenLegacyRoots,
    ) -> Result<Self::Projection<'heap>, AdmissionFailure<'heap>> {
        ProofEra::admit_rooted_with_weak_tables_for_test(collector, keeper, roots, self.tables)
            .map(|era| OwnedProofProjection { era })
            .map_err(|failure| AdmissionFailure {
                reason: map_registration(failure.reason),
                collector: failure.collector,
                keeper: failure.keeper,
            })
    }
}
