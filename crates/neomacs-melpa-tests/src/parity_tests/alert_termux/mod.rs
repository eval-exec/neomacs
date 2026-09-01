use std::time::Duration;

use crate::{
    ALERT_MELPA_PIN, ALERT_TERMUX_MELPA_PIN, CachedMelpaOracle, EmacsRuntime,
    prepare_cached_locked_melpa_package,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod registry;
mod workflow;

const ALERT_TERMUX_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn alert_termux_oracle(source_file: &str) -> CachedMelpaOracle {
    let oracle = CachedMelpaOracle::new(ALERT_TERMUX_MELPA_PIN, source_file)
        .expect("prepare pinned alert-termux source below ./tmp")
        .with_timeout(ALERT_TERMUX_TEST_TIMEOUT);
    if source_file == "alert-termux.el" {
        let alert_directory =
            prepare_cached_locked_melpa_package(&EmacsRuntime::gnu_emacs(), ALERT_MELPA_PIN)
                .expect("prepare exact alert dependency below ./tmp");
        let alert_source = alert_directory.join("alert.el");
        oracle.with_prelude(format!(
            "(load {:?} nil t t)",
            alert_source.to_string_lossy()
        ))
    } else {
        oracle
    }
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed alert-termux parity test")
        .into()
}

/// Multi-probe batch for `assert_alert_termux_autoload_parity` cases (2a).
pub(crate) fn assert_alert_termux_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        alert_termux_oracle("alert-termux-autoloads.el"),
        &name,
        "alert_termux_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_alert_termux_parity` cases (2a).
pub(crate) fn assert_alert_termux_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        alert_termux_oracle("alert-termux.el"),
        &name,
        "alert_termux_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn alert_termux_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_alert_termux_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_alert_termux_autoload_batch(&cases);
}

#[test]
fn alert_termux_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        registry::registry_alert_termux_batch_cases(),
        workflow::workflow_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_alert_termux_batch(&cases);
}

// END generated package batch tests
