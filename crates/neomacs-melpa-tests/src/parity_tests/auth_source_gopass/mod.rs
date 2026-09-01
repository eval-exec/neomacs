use std::time::Duration;

use crate::{AUTH_SOURCE_GOPASS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod backend;
mod paths;
mod registry;
mod search;
mod workflows;

const AUTH_SOURCE_GOPASS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTH_SOURCE_GOPASS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun auth-source-gopass-test-error-data
    (thunk)
  (condition-case error-data
      (list :ok
            (funcall thunk))
    (error
     (list :error
           (car error-data)
           (cdr error-data)))))
"##;

fn auth_source_gopass_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTH_SOURCE_GOPASS_MELPA_PIN, source_file)
        .expect("prepare pinned auth-source-gopass source below ./tmp")
        .with_prelude(AUTH_SOURCE_GOPASS_TEST_PRELUDE)
        .with_timeout(AUTH_SOURCE_GOPASS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auth-source-gopass parity test")
        .into()
}

/// Multi-probe batch for `assert_auth_source_gopass_autoload_parity` cases (2a).
pub(crate) fn assert_auth_source_gopass_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auth_source_gopass_oracle("auth-source-gopass-autoloads.el"),
        &name,
        "auth_source_gopass_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auth_source_gopass_parity` cases (2a).
pub(crate) fn assert_auth_source_gopass_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auth_source_gopass_oracle("auth-source-gopass.el"),
        &name,
        "auth_source_gopass_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auth_source_gopass_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auth_source_gopass_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auth_source_gopass_autoload_batch(&cases);
}

#[test]
fn auth_source_gopass_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backend::backend_public_surface_batch_cases(),
        paths::paths_public_surface_batch_cases(),
        registry::registry_auth_source_gopass_batch_cases(),
        search::search_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auth_source_gopass_batch(&cases);
}

// END generated package batch tests
