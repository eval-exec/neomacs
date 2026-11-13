//! Function-executor and function-handoff shells.
//! The era comes from eval's symbol-functions fence. Assertions stay in the
//! value bodies. Each call finishes the era it receives, so kind, row, and
//! owner-count loops stay here. World roots are the baton walker.
use super::PlainSymbolBacking;
use neovm_eval::test_support::authority::{
    baton_collect_parallel_world_roots, with_test_symbol_functions_era,
};
use neovm_value::test_support::authority::{
    run_function_handoff_test_for_test, run_functions_executor_test_for_test,
};

fn executor(kind: PlainSymbolBacking, name: &'static str, row: usize) {
    crate::test_utils::init_test_tracing();
    with_test_symbol_functions_era(kind, |era, ids| {
        run_functions_executor_test_for_test(name, era, &ids, kind, row);
    });
}

fn both_kinds(name: &'static str) {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        executor(kind, name, 0);
    }
}

fn handoff(kind: PlainSymbolBacking, name: &'static str, count: usize) {
    crate::test_utils::init_test_tracing();
    with_test_symbol_functions_era(kind, |era, ids| {
        run_function_handoff_test_for_test(
            name,
            era,
            &ids,
            kind,
            count,
            baton_collect_parallel_world_roots,
        );
    });
}

#[test]
fn p3sym_c5_function_executor_cannot_admit_intern_even_if_unreachable() {
    executor(
        PlainSymbolBacking::Unprojected,
        "p3sym_c5_function_executor_cannot_admit_intern_even_if_unreachable",
        0,
    );
}

#[test]
fn p3sym_c4_executor_accepts_rooted_function_tapes_before_activation() {
    both_kinds("p3sym_c4_executor_accepts_rooted_function_tapes_before_activation");
}

#[test]
fn p3sym_c4_executor_noop_control() {
    both_kinds("p3sym_c4_executor_noop_control");
}

#[test]
fn p3sym_c4_executor_primitives_match_gnu() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        for row in 0..4 {
            executor(kind, "p3sym_c4_executor_primitives_match_gnu", row);
        }
    }
}

#[test]
fn p3sym_c4_executor_old_and_new_calls_match_gnu() {
    both_kinds("p3sym_c4_executor_old_and_new_calls_match_gnu");
}

#[test]
fn p3sym_c4_executor_unbind_preserves_selected_call_and_error_answer() {
    both_kinds("p3sym_c4_executor_unbind_preserves_selected_call_and_error_answer");
}

#[test]
fn p3sym_c4_executor_busy_and_yield_preserve_instruction_and_answer() {
    both_kinds("p3sym_c4_executor_busy_and_yield_preserve_instruction_and_answer");
}

#[test]
fn p3sym_c4_executor_gate_checks_every_tape_and_activation_lifecycle() {
    executor(
        PlainSymbolBacking::Unprojected,
        "p3sym_c4_executor_gate_checks_every_tape_and_activation_lifecycle",
        0,
    );
}

#[test]
fn p3sym_c4_executor_private_home_capacity_refusal_cleans_up() {
    executor(
        PlainSymbolBacking::Unprojected,
        "p3sym_c4_executor_private_home_capacity_refusal_cleans_up",
        0,
    );
}

#[test]
fn p3sym_c4_executor_call_small() {
    executor(
        PlainSymbolBacking::Projected,
        "p3sym_c4_executor_call_small",
        0,
    );
}

#[cfg(not(miri))]
#[test]
fn p3sym_c4_plain_multi_owner_function_handoff_gc() {
    for count in 2..=4 {
        handoff(
            PlainSymbolBacking::Unprojected,
            "p3sym_c4_plain_multi_owner_function_handoff_gc",
            count,
        );
    }
}

#[cfg(not(miri))]
#[test]
fn p3sym_c4_projected_multi_owner_function_handoff_gc() {
    for count in 2..=4 {
        handoff(
            PlainSymbolBacking::Projected,
            "p3sym_c4_projected_multi_owner_function_handoff_gc",
            count,
        );
    }
}

#[test]
fn p3sym_c4_journal_only_definition_gc() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        handoff(kind, "p3sym_c4_journal_only_definition_gc", 0);
    }
}

#[test]
fn p3sym_c4_activation_gc_small() {
    handoff(
        PlainSymbolBacking::Projected,
        "p3sym_c4_activation_gc_small",
        0,
    );
}
