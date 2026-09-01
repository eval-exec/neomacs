use std::time::Duration;

use crate::{
    EmacsRuntime, ErtScenario, PackageScenario, PackageSource, run_delete_and_probe_scenario,
    run_ert_scenario, run_quickstart_scenario, workspace_root,
};
use neomacs_test_oracle::EvalOutcome;

#[test]
fn package_autoremove_and_incompatible_requirements_match_gnu_contracts() {
    let scenario = ErtScenario::new(
        "package-lifecycle-contracts",
        workspace_root().join("crates/neomacs-melpa-tests/scenarios/package-lifecycle-tests.el"),
        r##"'(member neomacs-package-autoremove-removes-unused-dependencies
                    neomacs-package-rejects-incompatible-emacs-requirement)"##,
    );
    let report = run_ert_scenario(
        &EmacsRuntime::neomacs().with_timeout(Duration::from_secs(180)),
        &scenario,
    )
    .expect("run deterministic package lifecycle ERT contracts");

    assert_eq!(report.summary.total, 2);
    assert_eq!(report.summary.expected, 2);
    assert_eq!(report.summary.unexpected, 0);
    eprintln!("{report}");
}

#[test]
fn package_quickstart_survives_a_fresh_process() {
    let scenario = PackageScenario::from_probe_file(
        "package-quickstart-contract",
        ["simple-two-depend", "multi-file"],
        workspace_root()
            .join("crates/neomacs-melpa-tests/scenarios/quickstart-package-contract.el"),
    )
    .expect("load package quickstart probe");
    let source =
        PackageSource::frozen(workspace_root().join("test/lisp/emacs-lisp/package-resources"));
    let report = run_quickstart_scenario(
        &EmacsRuntime::neomacs().with_timeout(Duration::from_secs(180)),
        &source,
        &scenario,
    )
    .expect("generate and load package quickstart across a fresh process");

    assert_eq!(
        report.outcome,
        EvalOutcome::Value("(:quickstart t :autoloads t :dependencies t :restart t)".to_string())
    );
    eprintln!("{report}");
}

#[test]
fn archive_package_deletion_survives_a_fresh_process() {
    let scenario = PackageScenario::new(
        "archive-package-delete-contract",
        ["simple-depend"],
        r##"
        (when (package-installed-p 'simple-depend)
          (error "deleted archive package reappeared after restart"))
        (unless (package-installed-p 'simple-single)
          (error "undeleted dependency disappeared after restart"))
        '(:deleted t :dependency-retained t :restart t)"##,
    );
    let source =
        PackageSource::frozen(workspace_root().join("test/lisp/emacs-lisp/package-resources"));
    let report = run_delete_and_probe_scenario(
        &EmacsRuntime::neomacs().with_timeout(Duration::from_secs(180)),
        &source,
        &scenario,
        "simple-depend",
    )
    .expect("delete an archive package and probe the next process");

    assert_eq!(
        report.outcome,
        EvalOutcome::Value("(:deleted t :dependency-retained t :restart t)".to_string())
    );
    eprintln!("{report}");
}
