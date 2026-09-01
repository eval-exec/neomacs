use std::time::Duration;

use crate::{AUTH_SOURCE_KEYTAR_MELPA_PIN, CachedMelpaOracle, KEYTAR_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod backend;
mod enable;
mod parsing;
mod registry;
mod search;
mod workflows;

const AUTH_SOURCE_KEYTAR_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTH_SOURCE_KEYTAR_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun auth-source-keytar-test-error-data (thunk)
  (condition-case error-data
      (list :ok
            (funcall thunk))
    (error
     (list :error
           (car error-data)
           (cdr error-data)))))

(defun auth-source-keytar-test-backend-data (backend)
  (when backend
    (list
     (slot-value backend 'source)
     (slot-value backend 'type)
     (slot-value backend 'search-function))))
"##;

fn auth_source_keytar_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTH_SOURCE_KEYTAR_MELPA_PIN, source_file)
        .expect("prepare pinned auth-source-keytar source below ./tmp")
        .with_melpa_dependency(KEYTAR_MELPA_PIN)
        .expect("prepare pinned Keytar dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency below ./tmp")
        .with_prelude(AUTH_SOURCE_KEYTAR_TEST_PRELUDE)
        .with_timeout(AUTH_SOURCE_KEYTAR_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auth-source-keytar parity test")
        .into()
}

/// Multi-probe batch for `assert_auth_source_keytar_autoload_parity` cases (2a).
pub(crate) fn assert_auth_source_keytar_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auth_source_keytar_oracle("auth-source-keytar-autoloads.el"),
        &name,
        "auth_source_keytar_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auth_source_keytar_parity` cases (2a).
pub(crate) fn assert_auth_source_keytar_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auth_source_keytar_oracle("auth-source-keytar.el"),
        &name,
        "auth_source_keytar_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auth_source_keytar_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auth_source_keytar_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auth_source_keytar_autoload_batch(&cases);
}

#[test]
fn auth_source_keytar_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backend::backend_public_surface_batch_cases(),
        enable::enable_public_surface_batch_cases(),
        parsing::parsing_public_surface_batch_cases(),
        registry::registry_auth_source_keytar_batch_cases(),
        search::search_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auth_source_keytar_batch(&cases);
}

// END generated package batch tests
