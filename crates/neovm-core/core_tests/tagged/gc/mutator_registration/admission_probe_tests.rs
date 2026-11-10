//! Admission-probe shell.
//! The shell installs the era-backed runner and executes the legacy control
//! row through it; the body and its assertions stay in value.
use crate::emacs_core::eval::Context;
use crate::tagged::gc::mutator_registration::era_fixtures::EraFixtureExt;
use neovm_eval::test_support::authority::baton_collect_parallel_world_roots;
use neovm_value::test_support::authority::{
    AdmissionProbeCase, install_admission_probe_runner_for_test,
    run_admission_probe_control_for_test, run_admission_probe_test_for_test,
};

fn run_check() {
    // The eval symbol fixture boots a bare Context; install the canonical
    // core eval hooks first, the same boot new_context performs.
    let _hooks = crate::emacs_core::eval_hook_installers::install_eval_hooks();
    Context::with_test_symbol_era(|era, _| {
        run_admission_probe_test_for_test(
            AdmissionProbeCase::InSliceAdmissionWordRefusesCatalogTransition,
            era,
            baton_collect_parallel_world_roots,
        );
    });
}

const SHELL_CASES: &[AdmissionProbeCase] =
    &[AdmissionProbeCase::InSliceAdmissionWordRefusesCatalogTransition];

#[test]
fn shell_cases_cover_all_admission_probe_cases() {
    let mut listed = SHELL_CASES.to_vec();
    listed.sort();
    let mut all = AdmissionProbeCase::ALL.to_vec();
    all.sort();
    assert_eq!(listed, all, "one core shell per former value row");
}

#[test]
fn in_slice_admission_word_refuses_catalog_transition() {
    // Install the era-backed check for the value-side control tables, then
    // execute this row through the same legacy adapter the tables use.
    install_admission_probe_runner_for_test(run_check);
    run_admission_probe_control_for_test();
}
