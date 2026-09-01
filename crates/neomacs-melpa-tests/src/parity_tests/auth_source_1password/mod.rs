use std::time::Duration;

use crate::{AUTH_SOURCE_1PASSWORD_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod backend;
mod reference;
mod registry;
mod search;

const AUTH_SOURCE_1PASSWORD_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTH_SOURCE_1PASSWORD_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)

(defun auth-source-1password-test-error-data (thunk)
  (condition-case error-data
      (list :ok
            (funcall thunk))
    (error
     (list :error
           (car error-data)
           (cdr error-data)))))

(defun auth-source-1password-test-backend-shape (backend)
  (list
   (eieio-object-p backend)
   (eieio-object-class-name backend)
   (slot-value backend 'type)
   (slot-value backend 'source)
   (slot-value backend 'host)
   (slot-value backend 'user)
   (slot-value backend 'port)
   (slot-value backend 'data)
   (slot-value backend 'create-function)
   (slot-value backend 'search-function)))

(defun auth-source-1password-test-read-file (path)
  (with-temp-buffer
    (insert-file-contents-literally path)
    (buffer-string)))
"##;

fn auth_source_1password_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTH_SOURCE_1PASSWORD_MELPA_PIN, source_file)
        .expect("prepare pinned auth-source-1password source below ./tmp")
        .with_prelude(AUTH_SOURCE_1PASSWORD_TEST_PRELUDE)
        .with_timeout(AUTH_SOURCE_1PASSWORD_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auth-source-1password parity test")
        .into()
}

/// Multi-probe batch for `assert_auth_source_1password_autoload_parity` cases (2a).
pub(crate) fn assert_auth_source_1password_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auth_source_1password_oracle("auth-source-1password-autoloads.el"),
        &name,
        "auth_source_1password_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auth_source_1password_parity` cases (2a).
pub(crate) fn assert_auth_source_1password_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auth_source_1password_oracle("auth-source-1password.el"),
        &name,
        "auth_source_1password_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auth_source_1password_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auth_source_1password_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auth_source_1password_autoload_batch(&cases);
}

#[test]
fn auth_source_1password_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backend::backend_public_surface_batch_cases(),
        reference::reference_public_surface_batch_cases(),
        registry::registry_auth_source_1password_batch_cases(),
        search::search_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auth_source_1password_batch(&cases);
}

// END generated package batch tests
