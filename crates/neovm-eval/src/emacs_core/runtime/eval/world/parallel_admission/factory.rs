//! Static nominal constructors for the single owned-Context transaction.
//! Only this child module implements a plan; proof plans do not exist in a
//! product build and cannot manufacture the production projection.
use super::super::{Baton, FrozenLegacyRoots};
#[cfg(any(test, feature = "test-support"))]
use super::OwnedProofProjection;
use super::{OwnedParallelError, OwnedParallelProjection, map_registration};
use crate::emacs_core::eval::world::ParallelHeapEraWorldExt;
#[cfg(test)]
use crate::emacs_core::eval::world::WorldProofEra as ProofEra;
#[cfg(test)]
use crate::emacs_core::eval::world::WorldProofEraExt;
use neovm_value::tagged::gc::{
    Gen1Admission, Gen1AdmissionError, ParallelHeapEra, QuiescentCollector,
};
#[cfg(test)]
use neovm_value::tagged::value::TaggedValue;

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
        let admission = match Gen1Admission::check(collector) {
            Ok(admission) => admission,
            Err(failure) => {
                let (gen1_reason, collector) = failure.into_parts();
                let reason = match gen1_reason {
                    Gen1AdmissionError::GenerationalRequired => {
                        OwnedParallelError::GenerationalRequired
                    }
                };
                return Err(AdmissionFailure {
                    reason,
                    collector,
                    keeper,
                });
            }
        };
        ParallelHeapEra::admit_rooted(admission, keeper, roots)
            .map(|era| OwnedParallelProjection { era })
            .map_err(|failure| {
                let (reason, collector, keeper) = failure.into_parts();
                AdmissionFailure {
                    reason: map_registration(reason),
                    collector,
                    keeper,
                }
            })
    }
}

#[cfg(any(test, feature = "test-support"))]
pub(super) struct ProofPlan;
#[cfg(any(test, feature = "test-support"))]
impl sealed::Sealed for ProofPlan {}
#[cfg(any(test, feature = "test-support"))]
impl AdmissionPlan for ProofPlan {
    type Projection<'heap> = OwnedProofProjection<'heap>;

    fn admit<'heap>(
        self,
        collector: QuiescentCollector<'heap>,
        keeper: Baton,
        roots: FrozenLegacyRoots,
    ) -> Result<Self::Projection<'heap>, AdmissionFailure<'heap>> {
        // SAFETY: identical to the value fence's admission contract; this
        // transaction holds the sealed collector and keeper, and the prepare
        // callback derives the original retained epoch/backing at the lower
        // admission's unchanged preparation point.
        unsafe {
            neovm_value::test_support::authority::admit_rooted_for_test(
                collector,
                keeper,
                roots,
                crate::emacs_core::eval::world::prepare_keeper_backing,
            )
        }
        .map(|era| OwnedProofProjection { era })
        .map_err(|failure| {
            let (reason, collector, keeper) = failure.into_parts();
            AdmissionFailure {
                reason: map_registration(reason),
                collector,
                keeper,
            }
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
            .map_err(|failure| {
                let (reason, collector, keeper) = failure.into_parts();
                AdmissionFailure {
                    reason: map_registration(reason),
                    collector,
                    keeper,
                }
            })
    }
}
