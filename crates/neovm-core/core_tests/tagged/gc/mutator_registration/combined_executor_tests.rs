//! Combined-executor shells.
//! The symbol-era boot and the baton World walk live here. Assertions stay
//! in the value bodies. The typed-tape refusal stays a value `#[test]`.
use neovm_eval::test_support::authority::{
    baton_collect_parallel_world_roots, with_test_symbol_backing_era,
    with_test_symbol_functions_era, with_test_symbol_functions_init_era,
    with_test_symbol_name_inputs_init_era,
};
use neovm_value::intern::SymId;
use neovm_value::symbol::Obarray;
use neovm_value::tagged::gc::PlainSymbolBacking;
use neovm_value::test_support::authority::run_combined_executor_test_for_test;

fn run(name: &str) {
    crate::test_utils::init_test_tracing();
    let mut symbol_backing = |kind: PlainSymbolBacking,
                              body: &mut dyn for<'e> FnMut(
        neovm_eval::test_support::authority::WorldProofEra<'e>,
        SymId,
    )| {
        with_test_symbol_backing_era(kind, |era, id| {
            body(era, id);
        });
    };
    let mut symbol_functions = |kind: PlainSymbolBacking,
                                body: &mut dyn for<'e> FnMut(
        neovm_eval::test_support::authority::WorldProofEra<'e>,
        Vec<SymId>,
    )| {
        with_test_symbol_functions_era(kind, |era, ids| {
            body(era, ids);
        });
    };
    let mut symbol_functions_init = |kind: PlainSymbolBacking,
                                     init: &mut dyn FnMut(&mut Obarray, &[SymId]),
                                     body: &mut dyn for<'e> FnMut(
        neovm_eval::test_support::authority::WorldProofEra<'e>,
        Vec<SymId>,
    )| {
        with_test_symbol_functions_init_era(
            kind,
            |obarray, ids| init(obarray, ids),
            |era, ids| body(era, ids),
        );
    };
    let mut symbol_name_inputs_init = |kind: PlainSymbolBacking,
                                       spelling: &str,
                                       init: &mut dyn FnMut(&mut Obarray, &[SymId]),
                                       body: &mut dyn for<'e> FnMut(
        neovm_eval::test_support::authority::WorldProofEra<'e>,
        Vec<SymId>,
    )| {
        with_test_symbol_name_inputs_init_era(
            kind,
            spelling,
            |obarray, ids| init(obarray, ids),
            |era, ids| body(era, ids),
        );
    };
    run_combined_executor_test_for_test(
        name,
        &mut symbol_backing,
        &mut symbol_functions,
        &mut symbol_functions_init,
        &mut symbol_name_inputs_init,
        baton_collect_parallel_world_roots,
    );
}

macro_rules! shell {
    ($name:ident) => {
        #[test]
        fn $name() {
            run(stringify!($name));
        }
    };
}

shell!(p3sym_c6_global_function_plain);
shell!(p3sym_c6_global_function_projected);
shell!(p3sym_c6_function_intern_plain);
shell!(p3sym_c6_function_intern_projected);
shell!(p3sym_c6_keeper_census_plain);
shell!(p3sym_c6_keeper_census_projected);
shell!(p3sym_c6_minor_plain);
shell!(p3sym_c6_minor_projected);
shell!(p3sym_c6_noop_control_finishes_plain_and_projected);
shell!(p3sym_c6_unwind_control_releases_live_reader_before_finish);
shell!(p3sym_c6_mixed_noop_control_finishes_with_a_real_frame);
shell!(p3sym_c6_mixed_unwind_releases_reader_and_selected_activation);
shell!(p3sym_c6_mixed_selected_old_name_survives_scalar_replacement);
shell!(p3sym_c6_mixed_intern_and_soft_allow_the_same_source_destination_home);
shell!(p3sym_c6_mixed_keyword_refusal_preserves_pc_and_destination);
shell!(p3sym_c6_mixed_seed_capacity_and_repeated_seed_preserve_the_bank);
shell!(p3sym_c6_mixed_expired_pending_instruction_never_restarts);
shell!(p3sym_c6_mixed_program_budget_is_independent_of_contention);
shell!(p3sym_c6_mixed_retired_session_and_stale_frame_refuse_before_access);
shell!(p3sym_c6_mixed_admission_checks_every_program_before_preparing_or_registering);
shell!(p3sym_c6_cohort_two_plain_full_root_transfers);
shell!(p3sym_c6_cohort_two_projected_full_root_transfers);
shell!(p3sym_c6_cohort_four_plain_full_root_transfers);
shell!(p3sym_c6_cohort_four_projected_full_root_transfers);
shell!(p3sym_c6_cohort_noop_control_releases_every_owner);
shell!(p3sym_c6_cohort_unwind_releases_every_owner);
shell!(p3sym_c6_minor_adapter_two_plain);
shell!(p3sym_c6_minor_adapter_two_projected);
shell!(p3sym_c6_minor_adapter_four_plain);
shell!(p3sym_c6_minor_adapter_four_projected);
shell!(p3sym_c6_fresh_two_plain);
shell!(p3sym_c6_fresh_two_projected);
shell!(p3sym_c6_fresh_four_plain);
shell!(p3sym_c6_fresh_four_projected);
shell!(p3sym_c6_fresh_unwind_returns_catalog_and_releases_banks);
shell!(p3sym_c6_fresh_noop_returns_catalog_and_releases_banks);
shell!(p3sym_c6_relog_two_plain);
shell!(p3sym_c6_relog_two_projected);
shell!(p3sym_c6_relog_four_plain);
shell!(p3sym_c6_relog_four_projected);
shell!(p3sym_c6_relog_noop_returns_catalog_and_releases_banks);
shell!(p3sym_c6_relog_unwind_returns_catalog_and_releases_banks);
shell!(p3sym_c6_name_minor_four_plain);
shell!(p3sym_c6_name_minor_four_void);
shell!(p3sym_c6_name_minor_noop_releases_banks);
shell!(p3sym_c6_name_minor_unwind_releases_banks);
