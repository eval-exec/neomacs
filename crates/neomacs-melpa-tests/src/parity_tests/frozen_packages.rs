use std::time::Duration;

use crate::{
    EmacsRuntime, PackageScenario, PackageSource, run_oracle_scenario, run_scenario, workspace_root,
};
use neomacs_test_oracle::EvalOutcome;

fn frozen_source() -> PackageSource {
    PackageSource::frozen(workspace_root().join("test/lisp/emacs-lisp/package-resources"))
}

fn frozen_scenario() -> PackageScenario {
    PackageScenario::from_probe_file(
        "frozen-package-contract",
        ["simple-two-depend", "multi-file"],
        workspace_root().join("crates/neomacs-melpa-tests/scenarios/frozen-package-contract.el"),
    )
    .expect("load frozen package probe")
}

fn neomacs_runtime() -> EmacsRuntime {
    EmacsRuntime::neomacs().with_timeout(Duration::from_secs(120))
}

#[test]
fn frozen_package_contract_survives_a_neomacs_restart() {
    let report = run_scenario(&neomacs_runtime(), &frozen_source(), &frozen_scenario())
        .expect("Neomacs must install and restart with frozen packages");

    assert_eq!(
        report.outcome,
        EvalOutcome::Value(
            "(:dependency-chain t :multi-file t :autoloads t :restart t)".to_string()
        )
    );
    eprintln!("{report}");
}

#[test]
fn frozen_package_contract_matches_gnu_emacs() {
    let scenario = frozen_scenario();
    let source = frozen_source();
    let report = run_oracle_scenario(
        &neomacs_runtime(),
        &EmacsRuntime::gnu_emacs().with_timeout(Duration::from_secs(120)),
        &source,
        &scenario,
    )
    .expect("frozen scenario must match GNU Emacs");

    eprintln!("{}", report.neomacs);
    eprintln!("{}", report.gnu_emacs);
}

#[test]
fn oracle_normalizes_each_editors_sandbox_paths_in_signals() {
    let scenario = PackageScenario::new(
        "sandbox-path-signal",
        ["simple-single"],
        r##"(error "sandbox paths: %s | %s | %s"
                   (getenv "HOME")
                   (getenv "TMPDIR")
                   (getenv "NEOMACS_TEST_SANDBOX_ROOT"))"##,
    );
    let report = run_oracle_scenario(
        &neomacs_runtime(),
        &EmacsRuntime::gnu_emacs().with_timeout(Duration::from_secs(120)),
        &frozen_source(),
        &scenario,
    )
    .expect("equivalent path-bearing signals must have oracle parity");

    let EvalOutcome::Signal(signal) = &report.neomacs.outcome else {
        panic!(
            "expected a normalized signal, got {}",
            report.neomacs.outcome
        );
    };
    assert!(signal.contains("[ORACLE-HOME]"));
    assert!(signal.contains("[ORACLE-TMPDIR]"));
    assert!(signal.contains("[ORACLE-SANDBOX]"));
    assert!(!signal.contains("tmp/melpa"));
}

#[test]
fn oracle_preserves_small_integer_values() {
    let report = crate::run_elisp_oracle(
        &neomacs_runtime(),
        &EmacsRuntime::gnu_emacs().with_timeout(Duration::from_secs(120)),
        "small-integer-outcome",
        "",
        "'(0 1 2 3)",
    )
    .expect("small integers must not be mistaken for opaque runtime handles");

    assert_eq!(report.neomacs, EvalOutcome::Value("(0 1 2 3)".to_string()));
}

#[test]
fn generic_surface_includes_package_custom_variables() {
    let surface = PackageScenario::autoload_surface("simple-single-surface", ["simple-single"]);
    let scenario = PackageScenario::new(
        "simple-single-custom-surface",
        ["simple-single"],
        format!(
            r##"(progn
                   (custom-autoload
                    'simple-single-super-sunday
                    "simple-single")
                   {})"##,
            surface.probe
        ),
    );
    let report = run_oracle_scenario(
        &neomacs_runtime(),
        &EmacsRuntime::gnu_emacs().with_timeout(Duration::from_secs(120)),
        &frozen_source(),
        &scenario,
    )
    .expect("generic package surface must match GNU Emacs");

    let EvalOutcome::Value(value) = &report.neomacs.outcome else {
        panic!("expected a surface value, got {}", report.neomacs.outcome);
    };
    assert!(
        value.contains("simple-single-super-sunday"),
        "custom variable missing from surface: {value}"
    );
}
