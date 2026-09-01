use std::time::Duration;

use crate::{AUTH_SOURCE_XOAUTH2_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod backend;
mod credentials;
mod enable;
mod password_store;
mod registry;
mod transport;
mod workflows;

const AUTH_SOURCE_XOAUTH2_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTH_SOURCE_XOAUTH2_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun auth-source-xoauth2-test-error-data
    (thunk)
  (condition-case error-data
      (list :ok
            (funcall thunk))
    (error
     (list :error
           (car error-data)
           (cdr error-data)))))

(defun auth-source-xoauth2-test-file
    (name)
  (expand-file-name
   name
   (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
"##;

fn auth_source_xoauth2_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTH_SOURCE_XOAUTH2_MELPA_PIN, source_file)
        .expect("prepare pinned auth-source-xoauth2 source below ./tmp")
        .with_prelude(AUTH_SOURCE_XOAUTH2_TEST_PRELUDE)
        .with_timeout(AUTH_SOURCE_XOAUTH2_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auth-source-xoauth2 parity test")
        .into()
}

/// Multi-probe batch for `assert_auth_source_xoauth2_autoload_parity` cases (2a).
pub(crate) fn assert_auth_source_xoauth2_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auth_source_xoauth2_oracle("auth-source-xoauth2-autoloads.el"),
        &name,
        "auth_source_xoauth2_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auth_source_xoauth2_parity` cases (2a).
pub(crate) fn assert_auth_source_xoauth2_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auth_source_xoauth2_oracle("auth-source-xoauth2.el"),
        &name,
        "auth_source_xoauth2_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auth_source_xoauth2_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auth_source_xoauth2_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auth_source_xoauth2_autoload_batch(&cases);
}

#[test]
fn auth_source_xoauth2_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backend::backend_public_surface_batch_cases(),
        credentials::credentials_public_surface_batch_cases(),
        enable::enable_public_surface_batch_cases(),
        password_store::password_store_public_surface_batch_cases(),
        registry::registry_auth_source_xoauth2_batch_cases(),
        transport::transport_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auth_source_xoauth2_batch(&cases);
}

// END generated package batch tests
