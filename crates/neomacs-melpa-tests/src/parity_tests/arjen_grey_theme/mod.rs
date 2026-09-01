use std::time::Duration;

use crate::{ARJEN_GREY_THEME_MELPA_PIN, CachedMelpaOracle, HELM_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ARJEN_GREY_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn arjen_grey_theme_oracle(source_file: &str, prelude: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARJEN_GREY_THEME_MELPA_PIN, source_file)
        .expect("prepare pinned arjen-grey-theme source below ./tmp")
        .with_prelude(prelude)
        .with_timeout(ARJEN_GREY_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed arjen-grey-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_arjen_grey_theme_autoload_parity` cases (2a).
pub(crate) fn assert_arjen_grey_theme_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        arjen_grey_theme_oracle("arjen-grey-theme-autoloads.el", ""),
        &name,
        "arjen_grey_theme_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_arjen_grey_theme_parity` cases (2a).
pub(crate) fn assert_arjen_grey_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        arjen_grey_theme_oracle("arjen-grey-theme.el", ""),
        &name,
        "arjen_grey_theme_parity",
        cases,
    );
}

pub(crate) fn assert_arjen_grey_theme_with_prelude_batch(prelude: &str, cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        arjen_grey_theme_oracle("arjen-grey-theme.el", prelude),
        &name,
        "arjen_grey_theme_with_prelude_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_arjen_grey_theme_with_helm_parity` cases (2a).
pub(crate) fn assert_arjen_grey_theme_with_helm_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(ARJEN_GREY_THEME_MELPA_PIN, "arjen-grey-theme.el")
            .expect("prepare pinned arjen-grey-theme source below ./tmp")
            .with_melpa_dependency(HELM_MELPA_PIN)
            .expect("prepare pinned Helm dependency below ./tmp")
            .with_timeout(ARJEN_GREY_THEME_TEST_TIMEOUT),
        &name,
        "arjen_grey_theme_with_helm_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn arjen_grey_theme_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [workflows::workflows_arjen_grey_theme_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_arjen_grey_theme_autoload_batch(&cases);
}

#[test]
fn arjen_grey_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_arjen_grey_theme_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_arjen_grey_theme_batch(&cases);
}

#[test]
fn arjen_grey_theme_with_helm_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [workflows::workflows_arjen_grey_theme_with_helm_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_arjen_grey_theme_with_helm_batch(&cases);
}

// END generated package batch tests
