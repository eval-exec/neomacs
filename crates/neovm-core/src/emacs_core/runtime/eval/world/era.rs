//! The sole concrete World keeper specialization of the lower collection era.
use super::*;
use crate::tagged::gc::{self, RegistrationError};
#[cfg(test)]
use crate::tagged::value::TaggedValue;

pub type WorldEra<'heap> = gc::ParallelHeapEra<'heap, Baton>;
#[cfg(test)]
pub(crate) type WorldProofEra<'heap> = gc::GenericProofEra<'heap, Baton>;
#[cfg(test)]
pub(crate) type WorldProofEraFinishFailure<'heap> = gc::GenericProofEraFinishFailure<'heap, Baton>;

/// Retain the original epoch and derive the original backing only at the
/// lower admission's existing preparation boundary, after generation issuance.
pub(crate) fn prepare_keeper_backing(
    keeper: &Baton,
) -> Result<
    (
        gc::FacadeEpochRetention,
        crate::emacs_core::symbol::parallel::SymbolBacking,
    ),
    RegistrationError,
> {
    let epoch = keeper
        .parallel_epoch_retention()
        .map_err(|_| RegistrationError::Exhausted)?;
    let backing = keeper.parallel_symbol_backing();
    Ok((epoch, backing))
}

impl<'heap> gc::ParallelHeapEra<'heap, Baton> {
    #[cold]
    pub(crate) fn admit(
        admission: gc::Gen1Admission<'heap>,
        keeper: Baton,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>> {
        // SAFETY: the static adapter derives the original keeper's retained
        // epoch/backing, at the lower collector's unchanged preparation point.
        unsafe { Self::admit_with(admission, keeper, prepare_keeper_backing) }
    }

    #[cold]
    pub(crate) fn admit_rooted(
        admission: gc::Gen1Admission<'heap>,
        keeper: Baton,
        roots: gc::FrozenLegacyRoots,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>> {
        // SAFETY: same retained keeper preparation; the roots are the sealed
        // suspended original Context census checked by the lower collector.
        unsafe { Self::admit_rooted_with(admission, keeper, roots, prepare_keeper_backing) }
    }
}

#[cfg(test)]
impl<'heap> gc::GenericProofEra<'heap, Baton> {
    #[cold]
    pub(crate) fn admit(
        collector: gc::QuiescentCollector<'heap>,
        keeper: Baton,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>> {
        // SAFETY: fixture retains the same original World and checked keeper.
        unsafe { Self::admit_with(collector, keeper, prepare_keeper_backing) }
    }
    #[cold]
    pub(crate) fn admit_rooted(
        collector: gc::QuiescentCollector<'heap>,
        keeper: Baton,
        roots: gc::FrozenLegacyRoots,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>> {
        // SAFETY: same original keeper and sealed suspended private census.
        unsafe { Self::admit_rooted_with(collector, keeper, roots, prepare_keeper_backing) }
    }
    #[cold]
    pub(crate) fn admit_rooted_with_weak_tables_for_test(
        collector: gc::QuiescentCollector<'heap>,
        keeper: Baton,
        roots: gc::FrozenLegacyRoots,
        tables: &[TaggedValue],
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>> {
        // SAFETY: same original keeper; lower admission validates actual table
        // catalog membership and exclusive stopped weak-table authority.
        unsafe {
            Self::admit_rooted_with_weak_tables_for_test_inner(
                collector,
                keeper,
                roots,
                tables,
                prepare_keeper_backing,
            )
        }
    }
}

// Each concrete entry invokes the EXISTING canonical World walker. No second
// child schema, cached census, stored callback or virtual dispatch is added.
macro_rules! world_collection {
    ($era:ident) => {
        impl $era<'_> {
            #[cold]
            pub(crate) fn collect_stopped(
                &self,
                stopped: &mut gc::AllStopped<'_>,
            ) -> Result<gc::CollectionReport, gc::CollectionError> {
                // SAFETY: this exact static method traces the CURRENT World of
                // the retained Baton and checks the actual stopped authority.
                unsafe { self.collect_stopped_with(stopped, Baton::collect_parallel_world_roots) }
            }
            #[cold]
            pub(crate) fn collect_stopped_minor(
                &self,
                stopped: &mut gc::AllStopped<'_>,
            ) -> Result<gc::CollectionReport, gc::MinorCollectionError> {
                // SAFETY: same complete canonical walker; the lower GEN0
                // refusal remains before this callback and before mutation.
                unsafe {
                    self.collect_stopped_minor_with(stopped, Baton::collect_parallel_world_roots)
                }
            }
        }
    };
}
world_collection!(WorldEra);
#[cfg(test)]
world_collection!(WorldProofEra);

static_assertions::assert_not_impl_any!(WorldEra<'static>: Send, Sync, Clone, Copy);
#[cfg(test)]
static_assertions::assert_not_impl_any!(WorldProofEra<'static>: Send, Sync, Clone, Copy);
