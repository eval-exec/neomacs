//! Closed, static completion choices for the one source collector frame.
//! Types here are eval-side. Observation effects stay in neovm-value.

use super::super::super::FrozenLegacyRoots;
use super::super::OwnedParallelError;
use super::QuiescentCollector;
use neovm_value::tagged::collection_reads::{SourceObservationError, SourceObservationsDetached};
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::rc::Rc;

// Private module and private implementors seal this vocabulary. No user
// callback or Send result can be inserted at the source loan boundary.
pub(super) trait FinishSource {
    type Output: 'static;
    type Failure: std::fmt::Debug + 'static;
    fn preparation_failure(reason: OwnedParallelError) -> Self::Failure;
    fn finish(
        collector: QuiescentCollector<'_>,
        census: FrozenLegacyRoots,
    ) -> Result<Self::Output, Self::Failure>;
}

pub(super) struct CensusOnly;

impl FinishSource for CensusOnly {
    type Output = FrozenLegacyRoots;
    type Failure = OwnedParallelError;

    fn preparation_failure(reason: OwnedParallelError) -> Self::Failure {
        reason
    }

    fn finish(
        collector: QuiescentCollector<'_>,
        census: FrozenLegacyRoots,
    ) -> Result<Self::Output, Self::Failure> {
        drop(collector);
        Ok(census)
    }
}

pub(super) struct WithdrawObservations;

/// Local refusal of observation withdrawal. The census stays retained. This
/// value cannot cross a thread: the armed shell still owns the Context.
#[must_use]
pub(crate) enum ObservationFailure {
    Preparation {
        reason: OwnedParallelError,
        _thread: PhantomData<Rc<()>>,
    },
    Observations {
        reason: SourceObservationError,
        _census: ManuallyDrop<FrozenLegacyRoots>,
        _thread: PhantomData<Rc<()>>,
    },
}

impl std::fmt::Debug for ObservationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preparation { reason, .. } => {
                formatter.debug_tuple("Preparation").field(reason).finish()
            }
            Self::Observations { reason, .. } => {
                formatter.debug_tuple("Observations").field(reason).finish()
            }
        }
    }
}

static_assertions::assert_not_impl_any!(ObservationFailure: Send, Sync, Clone, Copy);

impl FinishSource for WithdrawObservations {
    type Output = (FrozenLegacyRoots, SourceObservationsDetached);
    type Failure = ObservationFailure;

    fn preparation_failure(reason: OwnedParallelError) -> Self::Failure {
        ObservationFailure::Preparation {
            reason,
            _thread: PhantomData,
        }
    }

    fn finish(
        mut collector: QuiescentCollector<'_>,
        census: FrozenLegacyRoots,
    ) -> Result<Self::Output, Self::Failure> {
        match collector.detach_source_observations() {
            Ok(receipt) => {
                drop(collector);
                Ok((census, receipt))
            }
            Err(reason) => {
                // End the local loan and retain the original census. The armed
                // shell still owns the Context and the keeper.
                drop(collector);
                Err(ObservationFailure::Observations {
                    reason,
                    _census: ManuallyDrop::new(census),
                    _thread: PhantomData,
                })
            }
        }
    }
}
