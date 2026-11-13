//! The sole concrete World keeper specialization of the lower collection era.
use super::*;
use neovm_value::tagged::gc::{self, RegistrationError};
#[cfg(test)]
use neovm_value::tagged::value::TaggedValue;

pub type WorldEra<'heap> = gc::ParallelHeapEra<'heap, Baton>;
#[cfg(any(test, feature = "test-support"))]
pub(crate) type WorldProofEra<'heap> = neovm_value::test_support::authority::ProofEra<'heap, Baton>;
#[cfg(test)]
pub(crate) type WorldProofEraFinishFailure<'heap> =
    neovm_value::test_support::authority::ProofEraFinishFailure<'heap, Baton>;

/// Retain the original epoch and derive the original backing only at the
/// lower admission's existing preparation boundary, after generation issuance.
pub(crate) fn prepare_keeper_backing(
    keeper: &Baton,
) -> Result<
    (
        gc::FacadeEpochRetention,
        neovm_value::symbol::parallel::SymbolBacking,
    ),
    RegistrationError,
> {
    let epoch = keeper
        .parallel_epoch_retention()
        .map_err(|_| RegistrationError::Exhausted)?;
    let backing = keeper.parallel_symbol_backing();
    Ok((epoch, backing))
}

/// World-keeper entry points for the production multi-mutator era.
///
/// The era type is value-layer data; admitting it needs this World's `Baton`
/// keeper and its retained-epoch adapter, so the two constructors stay upper
/// as an extension trait on the moved type.
pub(crate) trait ParallelHeapEraWorldExt<'heap>: Sized {
    fn admit(
        admission: gc::Gen1Admission<'heap>,
        keeper: Baton,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>>;

    fn admit_rooted(
        admission: gc::Gen1Admission<'heap>,
        keeper: Baton,
        roots: gc::FrozenLegacyRoots,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>>;
}

impl<'heap> ParallelHeapEraWorldExt<'heap> for gc::ParallelHeapEra<'heap, Baton> {
    #[cold]
    fn admit(
        admission: gc::Gen1Admission<'heap>,
        keeper: Baton,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>> {
        // SAFETY: the static adapter derives the original keeper's retained
        // epoch/backing, at the lower collector's unchanged preparation point.
        unsafe { Self::admit_with(admission, keeper, prepare_keeper_backing) }
    }

    #[cold]
    fn admit_rooted(
        admission: gc::Gen1Admission<'heap>,
        keeper: Baton,
        roots: gc::FrozenLegacyRoots,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>> {
        // SAFETY: same retained keeper preparation; the roots are the sealed
        // suspended original Context census checked by the lower collector.
        unsafe { Self::admit_rooted_with(admission, keeper, roots, prepare_keeper_backing) }
    }
}

/// World-keeper test entry points for the restricted proof era.
#[cfg(test)]
pub(crate) trait WorldProofEraExt<'heap>: Sized {
    fn admit(
        collector: gc::QuiescentCollector<'heap>,
        keeper: Baton,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>>;
    fn admit_rooted(
        collector: gc::QuiescentCollector<'heap>,
        keeper: Baton,
        roots: gc::FrozenLegacyRoots,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>>;
    fn admit_rooted_with_weak_tables_for_test(
        collector: gc::QuiescentCollector<'heap>,
        keeper: Baton,
        roots: gc::FrozenLegacyRoots,
        tables: &[TaggedValue],
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>>;
}

#[cfg(test)]
impl<'heap> WorldProofEraExt<'heap>
    for neovm_value::test_support::authority::ProofEra<'heap, Baton>
{
    #[cold]
    fn admit(
        collector: gc::QuiescentCollector<'heap>,
        keeper: Baton,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>> {
        // SAFETY: fixture retains the same original World and checked keeper.
        unsafe { Self::admit_with(collector, keeper, prepare_keeper_backing) }
    }
    #[cold]
    fn admit_rooted(
        collector: gc::QuiescentCollector<'heap>,
        keeper: Baton,
        roots: gc::FrozenLegacyRoots,
    ) -> Result<Self, gc::EraAdmissionFailure<'heap, Baton>> {
        // SAFETY: same original keeper and sealed suspended private census.
        unsafe { Self::admit_rooted_with(collector, keeper, roots, prepare_keeper_backing) }
    }
    #[cold]
    fn admit_rooted_with_weak_tables_for_test(
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

/// World-collection entry points for the production multi-mutator era.
///
/// The era type is value-layer data and each entry invokes this World's
/// canonical `Baton` walker, so the two entry points stay upper as an
/// extension trait on the moved type.  Test-only: the entry is exercised by
/// the re-homed root/threading oracles, not by the product build.
#[cfg(test)]
pub(crate) trait ParallelHeapEraWorldCollectExt {
    fn collect_stopped(
        &self,
        stopped: &mut gc::AllStopped<'_>,
    ) -> Result<gc::CollectionReport, gc::CollectionError>;
    fn collect_stopped_minor(
        &self,
        stopped: &mut gc::AllStopped<'_>,
    ) -> Result<gc::CollectionReport, gc::MinorCollectionError>;
}

// Each concrete entry invokes the EXISTING canonical World walker. No second
// child schema, cached census, stored callback or virtual dispatch is added.
#[cfg(test)]
macro_rules! world_collection {
    ($era:ident) => {
        impl<'heap> ParallelHeapEraWorldCollectExt for $era<'heap> {
            #[cold]
            fn collect_stopped(
                &self,
                stopped: &mut gc::AllStopped<'_>,
            ) -> Result<gc::CollectionReport, gc::CollectionError> {
                // SAFETY: this exact static method traces the CURRENT World of
                // the retained Baton and checks the actual stopped authority.
                unsafe { self.collect_stopped_with(stopped, Baton::collect_parallel_world_roots) }
            }
            #[cold]
            fn collect_stopped_minor(
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
#[cfg(test)]
world_collection!(WorldEra);
#[cfg(test)]
world_collection!(WorldProofEra);

static_assertions::assert_not_impl_any!(WorldEra<'static>: Send, Sync, Clone, Copy);
// R4-4: the evaluator-keeper auto-trait assertions re-homed from neovm-value.
// `WorldEra`/`WorldProofEra` are literally value's `ParallelHeapEra`/
// `ProofEra` with this crate's `Baton`, and `EraAdmissionFailure` is the
// public failure wrapper returned by every admission path: it carries the
// same `QuiescentCollector` + `K` composition as value's internal
// `ProductionAdmission`/`ProofAdmission` witnesses.  Those two witnesses are
// `pub(super)` construction types that never cross the crate boundary, so
// naming them here would require widening value's signed TEST-SUPPORT-VIS
// surface for a type no upper test can construct or observe; the keeper
// propagation invariant is pinned through the public wrapper instead.
// Compiled in the cross-crate test-support build too (not only cfg(test)), so
// core's test graph checks it.
#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_not_impl_any!(WorldProofEra<'static>: Send, Sync, Clone, Copy);
#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_not_impl_any!(
    neovm_value::tagged::gc::mutator_registration::EraAdmissionFailure<'static, Baton>:
        Send,
        Sync,
        Clone,
        Copy
);
