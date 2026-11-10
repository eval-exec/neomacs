//! Mutator-registration era shells.
//! The mutator-era boot and the World keeper preparation live here.
//! Assertions stay in the value bodies. The generation-counter test stays
//! in value.
use crate::emacs_core::eval::Context;
use crate::tagged::gc::mutator_registration::era_fixtures::EraFixtureExt;
use neovm_eval::emacs_core::eval::Baton;
use neovm_value::symbol::parallel::SymbolBacking;
use neovm_value::tagged::gc::{FacadeEpochRetention, RegistrationError};
use neovm_value::test_support::authority::{ProofEra, run_mutator_registration_era_test_for_test};

fn run(name: &str) {
    let mut with_mutator_era = |body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>)| {
        Context::with_test_mutator_era(|era| body(era));
    };
    let mut prepare =
        |keeper: &Baton| -> Result<(FacadeEpochRetention, SymbolBacking), RegistrationError> {
            crate::tagged::gc::mutator_registration::era_fixtures::prepare_keeper_backing(keeper)
        };
    run_mutator_registration_era_test_for_test(name, &mut with_mutator_era, &mut prepare);
}

macro_rules! shell {
    ($name:ident) => {
        #[test]
        fn $name() {
            run(stringify!($name));
        }
    };
}

shell!(two_destination_threads_publish_one_canonical_census);
shell!(cancelled_starts_are_never_runnable_and_owner_can_retry);
shell!(wrong_start_heap_or_owner_returns_the_live_permit);
shell!(wrong_key_heap_owner_and_generation_refuse_without_changes);
shell!(generation_exhaustion_and_duplicate_owner_leave_membership_unchanged);
shell!(running_or_starting_owner_cannot_be_omitted_from_summary);
shell!(owner_publication_restores_exact_image_and_refuses_old_epoch);
shell!(outstanding_worker_loan_refuses_publication_and_retirement_then_recovers);
shell!(wrong_owner_worker_return_keeps_the_only_payload_for_retry);
shell!(young_resolver_checks_one_canonical_entry_and_retiring_generation);
shell!(panic_abandonment_poison_retains_owner_state_and_world_authority);
shell!(unfinished_token_drop_and_lost_loan_fail_closed);
shell!(summary_reentry_refuses_busy_instead_of_blocking_keeper);
shell!(root_capacity_failure_returns_unclaimed_permit_for_safe_cancellation);
shell!(pending_root_retirement_failure_preserves_entry_and_allows_explicit_transfer);
shell!(old_registration_key_is_rejected_after_reentering_the_same_world);
