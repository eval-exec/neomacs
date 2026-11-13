//! NMJIT construction shells.
//! The symbol-era boot and the World keeper preparation live here.
//! Assertions stay in the value bodies.
use neovm_eval::emacs_core::eval::Baton;
use neovm_eval::test_support::authority::with_test_symbol_era;
use neovm_value::symbol::parallel::SymbolBacking;
use neovm_value::tagged::gc::{FacadeEpochRetention, RegistrationError};
use neovm_value::test_support::authority::{ProofEra, run_construction_nmjit_test_for_test};

fn run(name: &str) {
    crate::test_utils::init_test_tracing();
    let mut with_symbol_era = |body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>)| {
        with_test_symbol_era(|era, _| body(era));
    };
    let mut prepare =
        |keeper: &Baton| -> Result<(FacadeEpochRetention, SymbolBacking), RegistrationError> {
            crate::tagged::gc::mutator_registration::era_fixtures::prepare_keeper_backing(keeper)
        };
    let held_here = |keeper: &Baton| keeper.held_here();
    run_construction_nmjit_test_for_test(name, &mut with_symbol_era, &mut prepare, &held_here);
}

macro_rules! shell {
    ($name:ident) => {
        #[test]
        fn $name() {
            run(stringify!($name));
        }
    };
}

shell!(nmjit_n2_actual_constructor_checks_mode_and_returns_complete_ownership);
shell!(nmjit_n2_proof_inputs_preserve_collector_mode_without_production_capability);
shell!(nmjit_n3_initial_table_preparation_refusal_returns_complete_ownership);
