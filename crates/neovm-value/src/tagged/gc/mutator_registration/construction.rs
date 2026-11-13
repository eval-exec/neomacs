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

#[cfg(any(test, feature = "test-support"))]
#[must_use]
pub(super) struct ProofAdmission<'heap, K: EraKeeper> {
    checked: CheckedCollector<'heap, K>,
}
#[cfg(any(test, feature = "test-support"))]
impl<'heap, K: EraKeeper> ProofAdmission<'heap, K> {
    pub(super) fn check(
        collector: QuiescentCollector<'heap>,
        keeper: K,
    ) -> Result<Self, EraAdmissionFailure<'heap, K>> {
        CheckedCollector::check(collector, keeper).map(|checked| Self { checked })
    }
}
#[cfg(any(test, feature = "test-support"))]
#[must_use]
pub(super) struct ProofConstruction {
    _private: (),
}
#[cfg(any(test, feature = "test-support"))]
impl sealed::Sealed for ProofConstruction {}
#[cfg(any(test, feature = "test-support"))]
impl ConstructionEvidence for ProofConstruction {}
#[cfg(any(test, feature = "test-support"))]
impl<K: EraKeeper> sealed::Sealed for ProofAdmission<'_, K> {}
#[cfg(any(test, feature = "test-support"))]
impl<'heap, K: EraKeeper> CoreAdmission<'heap, K> for ProofAdmission<'heap, K> {
    type Evidence = ProofConstruction;
    fn into_checked(self) -> (CheckedCollector<'heap, K>, Self::Evidence) {
        (self.checked, ProofConstruction { _private: () })
    }
}

/// R4-F1: pin the admission witnesses' auto traits INSIDE neovm-value with a
/// local `!Send`/`!Sync` stand-in keeper.
///
/// The witnesses are `pub(super)` and the real evaluator keeper (`Baton`) is
/// an upper type this crate cannot name; the eval-side assertion pins the
/// public `EraAdmissionFailure<'static, Baton>` wrapper (R4-4).  This arm is
/// the complementary in-crate pin: an unconditional `unsafe impl Send`/
/// `Sync` (or a plain `Clone`/`Copy`) added to a witness would make the
/// property hold for `LocalKeeper` too and fail the build here, with no
/// TEST-SUPPORT-VIS widening and no upper-crate path.
#[cfg(any(test, feature = "test-support"))]
mod admission_auto_traits {
    use super::{EraKeeper, ProductionAdmission, ProofAdmission};
    use crate::intern::SymId;
    use crate::symbol::parallel::KeeperWorldDesignator;
    use crate::tagged::gc::function_write_hooks::FunctionWriteHooks;
    use crate::tagged::gc::{
        CanonicalHofSeed, CompilerActivity, HeapIdentity, HofOperation, HofPreparationError,
        InitialDefinition, KeeperSymbols, RegistrationError, SymbolWriteToken,
    };
    use crate::tagged::value::TaggedValue;

    /// Not shareable across threads (`Rc` makes it `!Send`/`!Sync`) and
    /// entirely local to this crate.  Type-level only: never constructed, so
    /// no hook below is ever called.
    struct LocalKeeper(std::marker::PhantomData<std::rc::Rc<()>>);

    impl FunctionWriteHooks for LocalKeeper {
        unsafe fn prepare_hof_builtin(
            &self,
            _operation: HofOperation,
            _authority: &SymbolWriteToken<'_>,
        ) -> Result<CanonicalHofSeed, HofPreparationError> {
            unimplemented!("type-level witness; never constructed")
        }
        unsafe fn classify_initial_definition(
            &self,
            _value: TaggedValue,
            _authority: &SymbolWriteToken<'_>,
        ) -> InitialDefinition {
            unimplemented!("type-level witness; never constructed")
        }
        fn check_compiler_quiescent(&self) -> Result<(), CompilerActivity> {
            unimplemented!("type-level witness; never constructed")
        }
        unsafe fn reconcile_function_write(&self, _symbol: SymId, _keeper: &KeeperSymbols<'_>) {
            unimplemented!("type-level witness; never constructed")
        }
    }

    // SAFETY: type-level witness only. The two hooks return the keeper's
    // identity/designator and are never called because no value of this type
    // exists; the admission wrappers below are never constructed with it.
    unsafe impl EraKeeper for LocalKeeper {
        fn keeper_designator(&self) -> Result<KeeperWorldDesignator<'_>, RegistrationError> {
            unimplemented!("type-level witness; never constructed")
        }
        fn parallel_heap_identity(&self) -> HeapIdentity {
            unimplemented!("type-level witness; never constructed")
        }
    }

    static_assertions::assert_not_impl_any!(
        ProductionAdmission<'static, LocalKeeper>:
            Send,
            Sync,
            Clone,
            Copy
    );
    static_assertions::assert_not_impl_any!(
        ProofAdmission<'static, LocalKeeper>:
            Send,
            Sync,
            Clone,
            Copy
    );
}

#[cfg(feature = "test-support")]
mod tests;
#[cfg(feature = "test-support")]
pub(crate) use tests::run_construction_nmjit_test;
