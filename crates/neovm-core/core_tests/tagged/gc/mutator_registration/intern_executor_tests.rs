//! Intern-executor shells.
//! The symbol-era boot and the baton World walk live here. Assertions stay
//! in the value bodies.
use neovm_eval::test_support::authority::{
    baton_collect_parallel_world_roots, with_test_symbol_intern_race_era,
    with_test_symbol_name_inputs_era, with_test_symbol_name_inputs_init_era,
};
use neovm_value::intern::SymId;
use neovm_value::symbol::Obarray;
use neovm_value::tagged::gc::PlainSymbolBacking;
use neovm_value::test_support::authority::run_intern_executor_test_for_test;

fn run(name: &str) {
    crate::test_utils::init_test_tracing();
    let mut name_inputs = |kind: PlainSymbolBacking,
                           spelling: &str,
                           body: &mut dyn for<'e> FnMut(
        neovm_eval::test_support::authority::WorldProofEra<'e>,
        Vec<SymId>,
    )| {
        with_test_symbol_name_inputs_era(kind, spelling, |era, ids| {
            body(era, ids);
        });
    };
    let mut name_inputs_init = |kind: PlainSymbolBacking,
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
    let mut intern_race = |kind: PlainSymbolBacking,
                           spellings: &[String],
                           body: &mut dyn for<'e> FnMut(
        neovm_eval::test_support::authority::WorldProofEra<'e>,
        Vec<SymId>,
    )| {
        with_test_symbol_intern_race_era(kind, spellings, |era, ids| {
            body(era, ids);
        });
    };
    run_intern_executor_test_for_test(
        name,
        &mut name_inputs,
        &mut name_inputs_init,
        &mut intern_race,
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

shell!(p3sym_c5_executor_partial_seed_refusal_has_total_teardown);
shell!(p3sym_c5_executor_noop_control_finishes);
shell!(p3sym_c5_executor_intern_and_soft_match_gnu);
shell!(p3sym_c5_executor_aliased_name_and_destination);
shell!(p3sym_c5_executor_wrong_type_preserves_destination_and_pc);
shell!(p3sym_c5_executor_busy_has_elapsed_deadline_and_retries_same_pc);
shell!(p3sym_c5_executor_gate_checks_unreachable_ops_and_all_participants);
shell!(p3sym_c5_executor_separate_input_gate_rejects_non_names);
shell!(p3sym_c5_executor_plain_name_survives_two_stops);
shell!(p3sym_c5_executor_projected_name_survives_two_stops);
shell!(p3sym_c5_executor_name_gc_pretrace_unwind_recovers);
shell!(p3sym_c5_executor_cohort_same_object_winner);
shell!(p3sym_c5_executor_cohort_equal_name_winner);
shell!(p3sym_c5_executor_cohort_distinct_name_growth);
shell!(p3sym_c5_executor_cohort_partial_seed_still_joins_and_cleans_all_owners);
