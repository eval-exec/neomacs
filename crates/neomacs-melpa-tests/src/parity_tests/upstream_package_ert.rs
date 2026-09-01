use std::time::Duration;
use std::{process::Command, time::Instant};

use crate::{EmacsRuntime, ErtReport, ErtScenario, run_ert_scenario, workspace_root};

fn run_group(name: &str, tests: &[&str]) -> ErtReport {
    run_group_with_skip_policy(name, tests, false)
}

fn run_group_with_skip_policy(name: &str, tests: &[&str], allow_skips: bool) -> ErtReport {
    let selector = format!(r##"'(member {})"##, tests.join(" "));
    let scenario = ErtScenario::new(
        name,
        workspace_root().join("test/lisp/emacs-lisp/package-tests.el"),
        selector,
    );
    let report = run_ert_scenario(
        &EmacsRuntime::neomacs().with_timeout(Duration::from_secs(300)),
        &scenario,
    )
    .unwrap_or_else(|error| panic!("{name} failed:\n{error}"));

    assert_eq!(report.summary.total, tests.len(), "{report}");
    assert_eq!(report.summary.unexpected, 0, "{report}");
    if !allow_skips {
        assert_eq!(
            report.summary.skipped, 0,
            "required upstream contracts must not skip: {report}"
        );
    }
    assert_eq!(
        report.summary.expected + report.summary.skipped,
        report.summary.total,
        "{report}"
    );
    eprintln!("{report}");
    report
}

#[test]
fn upstream_package_install_and_archive_contracts() {
    run_group(
        "upstream-package-install-and-archive",
        &[
            "package-test-desc-from-buffer",
            "package-test-install-single",
            "package-test-install-file",
            "package-test-bug58367",
            "package-test-bug65475",
            "package-test-install-dependency",
            "package-test-macro-compilation",
            "package-test-macro-compilation-gz",
            "package-test-install-two-dependencies",
            "package-test-refresh-contents",
            "package-test-install-single-from-archive",
            "package-test-install-prioritized",
            "package-test-install-singlefile",
            "package-test-install-multifile",
        ],
    );
}

#[test]
fn upstream_package_menu_and_upgrade_contracts() {
    run_group(
        "upstream-package-menu-and-upgrade",
        &[
            "package-test-update-listing",
            "package-test-list-filter-by-archive",
            "package-test-list-filter-by-keyword",
            "package-test-list-filter-by-name",
            "package-test-list-filter-by-status",
            "package-test-list-filter-marked",
            "package-test-list-filter-by-version",
            "package-test-list-filter-by-version-=",
            "package-test-list-filter-by-version-<",
            "package-test-list-filter-by-version->",
            "package-test-list-clear-filter",
            "package-test-update-archives",
            "package-test-update-archives/ignore-nil-entry",
        ],
    );
}

#[test]
fn upstream_package_description_and_dependency_contracts() {
    run_group(
        "upstream-package-description-and-dependencies",
        &[
            "package-test-package-installed-p",
            "package-test-describe-package",
            "package-test-describe-installed-with-ws-only-readme",
            "package-test-describe-installed-multi-file-package",
            "package-test-describe-non-installed-package",
            "package-test-describe-non-installed-multi-file-package",
            "package-x-test-upload-buffer",
            "package-x-test-upload-new-version",
            "package-test-get-dependencies",
            "package-test-sort-by-dependence",
        ],
    );
}

#[test]
fn upstream_package_signature_contract() {
    let report = run_group_with_skip_policy(
        "upstream-package-signatures",
        &["package-test-signed"],
        true,
    );
    let gpg_available = Command::new("gpg")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if gpg_available {
        assert_eq!(
            report.summary.skipped, 0,
            "signature coverage must not skip when gpg is available: {report}"
        );
    } else {
        eprintln!("gpg is unavailable; upstream signature ERT test skipped");
    }
}

#[test]
#[ignore = "known Neomacs divergence: CRLF package version retains carriage return"]
fn upstream_package_eol_contract() {
    run_group("upstream-package-eol", &["package-test-install-file-EOLs"]);
}

#[test]
#[ignore = "known Neomacs divergence: async package refresh writes a read-only menu buffer"]
fn upstream_package_async_refresh_contract() {
    let started = Instant::now();
    run_group(
        "upstream-package-async-refresh",
        &["package-test-update-archives-async"],
    );
    assert!(
        started.elapsed() < Duration::from_secs(120),
        "async package refresh exceeded its outer contract timeout"
    );
}
