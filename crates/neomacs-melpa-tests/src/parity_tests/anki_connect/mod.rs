use std::time::Duration;

use crate::{ANKI_CONNECT_MELPA_PIN, CachedMelpaOracle, S_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANKI_CONNECT_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn anki_connect_oracle(
    source_file: &str,
    include_undeclared_s_dependency: bool,
) -> CachedMelpaOracle {
    let oracle = CachedMelpaOracle::new(ANKI_CONNECT_MELPA_PIN, source_file)
        .expect("prepare pinned anki-connect source below ./tmp");
    let oracle = if include_undeclared_s_dependency {
        oracle
            .with_melpa_dependency(S_MELPA_PIN)
            .expect("prepare anki-connect's undeclared s dependency below ./tmp")
            .with_prelude("(require 's)")
    } else {
        oracle
    };
    oracle.with_timeout(ANKI_CONNECT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed anki-connect parity test")
        .into()
}

/// Multi-probe batch for `assert_anki_connect_parity` cases (2a).
pub(crate) fn assert_anki_connect_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        anki_connect_oracle("anki-connect.el", true),
        &name,
        "anki_connect_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_anki_connect_missing_dependency_signal` cases (2a).
pub(crate) fn assert_anki_connect_missing_dependency_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        anki_connect_oracle("anki-connect.el", false),
        &name,
        "anki_connect_missing_dependency_signal",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn anki_connect_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_anki_connect_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_anki_connect_batch(&cases);
}

#[test]
fn anki_connect_missing_dependency_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [workflows::workflows_anki_connect_missing_dependency_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_anki_connect_missing_dependency_batch(&cases);
}

// END generated package batch tests
