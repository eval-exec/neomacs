//! Combined-inputs shells.
//! The shell owns the Context call and the backing; the init closure, the era
//! body, and the assertions stay in value. One #[test] per former value row,
//! generated from the same list the completeness check compares with ALL.
use crate::emacs_core::eval::Context;
use crate::tagged::gc::mutator_registration::era_fixtures::EraFixtureExt;
use neovm_eval::emacs_core::eval::Baton;
use neovm_value::intern::SymId;
use neovm_value::symbol::Obarray;
use neovm_value::tagged::gc::mutator_registration::PlainSymbolBacking;
use neovm_value::test_support::authority::{
    CombinedInputsCase, ProofEra, run_combined_inputs_case_for_test,
};

fn symbol_functions_init_era_impl(
    backing: PlainSymbolBacking,
    init: &mut dyn FnMut(&mut Obarray, &[SymId]),
    body: &mut dyn for<'e> FnMut(ProofEra<'e, Baton>, Vec<SymId>),
) {
    Context::with_test_symbol_functions_init_era(
        backing,
        |obarray, ids| init(obarray, ids),
        |era, ids| body(era, ids),
    );
}

fn run(case: CombinedInputsCase) {
    // The eval symbol-functions fixture boots a bare Context; install the
    // canonical core eval hooks first, the same boot new_context performs.
    let _hooks = crate::emacs_core::eval_hook_installers::install_eval_hooks();
    run_combined_inputs_case_for_test(case, symbol_functions_init_era_impl);
}

macro_rules! shells {
    ($($name:ident => $case:ident),* $(,)?) => {
        $(#[test]
        fn $name() {
            run(CombinedInputsCase::$case);
        })*
        const SHELL_CASES: &[CombinedInputsCase] = &[$(CombinedInputsCase::$case),*];
    };
}

shells! {
    p3sym_c6_roles_admit_plain => P3symC6RolesAdmitPlain,
    p3sym_c6_roles_admit_projected => P3symC6RolesAdmitProjected,
    p3sym_c6_roles_admit_direct_name_and_scalar_without_widening_legacy => P3symC6RolesAdmitDirectNameAndScalarWithoutWideningLegacy,
    p3sym_c6_roles_property_refusal_small => P3symC6RolesPropertyRefusalSmall,
    p3sym_c6_roles_refuse_wrong_roles_shapes_properties_and_cycles => P3symC6RolesRefuseWrongRolesShapesPropertiesAndCycles,
    p3sym_c6_roles_foreign_receipt_and_all_keys_checked_before_decode => P3symC6RolesForeignReceiptAndAllKeysCheckedBeforeDecode,
    p3sym_c6_roles_stale_metadata_refuses_without_changing_live_backing => P3symC6RolesStaleMetadataRefusesWithoutChangingLiveBacking,
    p3sym_c6_roles_budget_and_noop_control_finish_cleanly => P3symC6RolesBudgetAndNoopControlFinishCleanly,
    p3sym_c6_roles_repeated_and_conflicting_aliases_are_checked_separately => P3symC6RolesRepeatedAndConflictingAliasesAreCheckedSeparately,
    p3sym_c6_roles_unwind_control_finishes_before_propagating => P3symC6RolesUnwindControlFinishesBeforePropagating,
}

#[test]
fn shell_cases_cover_all_combined_inputs_cases() {
    let mut listed = SHELL_CASES.to_vec();
    listed.sort();
    let mut all = CombinedInputsCase::ALL.to_vec();
    all.sort();
    assert_eq!(listed, all, "one core shell per moved value row");
}
