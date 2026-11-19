//! Eval owns the concrete scheduling authority. The value layer retains only
//! the cold loan adapter. No manager reference crosses this seam.

use super::Baton;
use neovm_value::tagged::gc::heap_transfer::HeapRootDesignator;
use neovm_value::tagged::gc::{
    AllStopped, EraWorldLoan, HeapIdentity, RegistrationError, RendezvousError,
    SymbolRootDesignators, WorldLoan,
};
use neovm_value::tagged::header::MarkerObj;
use neovm_value::tagged::value::TaggedValue;

// SAFETY: the Baton is the unique serial authority of its retained World.
// Every operation delegates to the holder, designator, and retention checks
// the Baton already performs. The final transfer moves that same Baton.
unsafe impl EraWorldLoan for Baton {
    fn held_here(&self) -> bool {
        Baton::held_here(self)
    }

    fn parallel_heap_identity(&self) -> HeapIdentity {
        Baton::parallel_heap_identity(self)
    }

    fn parallel_epoch_retention(
        &self,
    ) -> Result<
        neovm_value::tagged::gc::FacadeEpochRetention,
        neovm_value::tagged::gc::FacadeMarkExclusionError,
    > {
        Baton::parallel_epoch_retention(self)
    }

    fn symbol_root_designators(&self) -> SymbolRootDesignators {
        Baton::parallel_symbol_root_designators(self)
    }

    fn check_symbol_root_designators(
        &self,
        roots: &SymbolRootDesignators,
    ) -> Result<(), RegistrationError> {
        Baton::check_era_symbol_root_designators(self, roots)
    }

    fn collect_parallel_world_roots(
        &self,
        stopped: &mut AllStopped<'_>,
        out: &mut Vec<TaggedValue>,
    ) -> Result<Vec<*mut *mut MarkerObj>, RendezvousError> {
        Baton::collect_parallel_world_roots(self, stopped, out)
    }

    fn into_retained_owner(self: Box<Self>) -> Box<dyn std::any::Any + Send> {
        self
    }

    fn adopt_current_thread(&self) {
        self.enter();
    }

    fn heap_root_designator(&self) -> HeapRootDesignator {
        self.parallel_heap_root_designator()
    }
}

impl From<Baton> for WorldLoan {
    fn from(baton: Baton) -> Self {
        WorldLoan::new(baton)
    }
}

impl Baton {
    /// Recover the Baton placed in a loan by [`From<Baton>`] or by
    /// [`WorldLoan::new`]. A foreign owner is leaked and then reported:
    /// dropping it could free backing the caller still treats as borrowed.
    ///
    /// `recover_transferred_source` calls this from the always-compiled
    /// adapter, so the function is live in the library build. The adapter
    /// itself stays dormant: nothing on the production admission path
    /// reaches it.
    pub(super) fn from_world_loan(loan: WorldLoan) -> Self {
        match EraWorldLoan::into_retained_owner(Box::new(loan)).downcast::<Self>() {
            Ok(baton) => *baton,
            Err(retained) => {
                std::mem::forget(retained);
                panic!("eval era returned a foreign retained owner")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::WorldProofEraExt;
    use super::{Baton, RegistrationError};
    use crate::emacs_core::eval::Context;
    use neovm_value::tagged::gc::{EraWorldLoan, SymbolRootDesignators, WorldIdentity, WorldLoan};

    #[test]
    fn pentry_e2b_world_loan_roundtrip_preserves_root_designators_and_ownership() {
        let _harness = crate::test_utils::unit_harness();
        Context::with_test_symbol_era(|era, _| {
            let (collector, baton) = era.finish().expect("empty era finishes");
            let loan = WorldLoan::from(baton);
            let heap = collector.heap_identity();
            let roots = loan.symbol_root_designators();
            assert_eq!(loan.parallel_heap_identity(), heap);
            assert!(loan.held_here());
            let retained = loan.parallel_epoch_retention().expect("backing retained");
            assert_eq!(retained.heap_identity(), heap);
            let nested = WorldLoan::new(loan);
            let baton = Baton::from_world_loan(nested);
            assert!(baton.held_here());
            assert_eq!(baton.check_era_symbol_root_designators(&roots), Ok(()));
            // Re-admission consumes the recovered Baton. ProofEra's keeper
            // stays Baton; the loan is the seam around it, not a replacement
            // of the era's type parameter.
            let era = super::super::WorldProofEra::admit(collector, baton)
                .expect("returned serial inputs remain usable");
            let (collector, baton) = era.finish().expect("re-admitted era finishes");
            let loan = WorldLoan::from(baton);
            assert_eq!(loan.check_symbol_root_designators(&roots), Ok(()));
            drop(collector);
            drop(loan);
            drop(retained);
        });
    }

    #[test]
    fn pentry_e2b_world_loan_rejects_foreign_world_with_same_manager_designators() {
        let _harness = crate::test_utils::unit_harness();
        Context::with_test_symbol_era(|era, _| {
            let (collector, baton) = era.finish().expect("empty era finishes");
            let loan = WorldLoan::from(baton);
            let roots = loan.symbol_root_designators();
            let dangling = std::ptr::NonNull::<()>::dangling().as_ptr() as usize;
            // SAFETY: the dangling address is a foreign identity word used
            // only for the equality refusal. It is not dereferenced.
            let foreign = unsafe {
                SymbolRootDesignators::new(
                    WorldIdentity::new(dangling),
                    roots.obarray(),
                    roots.projections(),
                )
            };
            assert_ne!(foreign.world(), roots.world());
            assert_eq!(
                loan.check_symbol_root_designators(&foreign),
                Err(RegistrationError::StaleRegistration)
            );
            assert_eq!(loan.check_symbol_root_designators(&roots), Ok(()));
            drop(collector);
            drop(loan);
        });
    }
}
