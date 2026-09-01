use std::time::Duration;

use crate::{AUTH_SOURCE_KWALLET_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod backend;
mod process;
mod registry;
mod workflows;

const AUTH_SOURCE_KWALLET_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTH_SOURCE_KWALLET_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'subr-x)
(require 'auth-source)
(require 'warnings)

(defalias
  'auth-source-kwallet-test-real-executable-find
  (symbol-function 'executable-find))
(defalias
  'auth-source-kwallet-test-real-call-process
  (symbol-function 'call-process))

(defvar auth-source-kwallet-test-executable-found t)
(defvar auth-source-kwallet-test-output "fixture-secret\n")
(defvar auth-source-kwallet-test-status 0)
(defvar auth-source-kwallet-test-signal nil)
(defvar auth-source-kwallet-test-executable-calls nil)
(defvar auth-source-kwallet-test-process-calls nil)

(defun auth-source-kwallet-test-executable-find
    (command)
  (push command
        auth-source-kwallet-test-executable-calls)
  (when auth-source-kwallet-test-executable-found
    (concat "/fixture/bin/" command)))

(defun auth-source-kwallet-test-call-process
    (program infile destination display &rest args)
  (push
   (list
    program
    infile
    (cond
     ((bufferp destination)
      (buffer-name destination))
     (t destination))
    display
    args
    (and
     (bufferp destination)
     (buffer-live-p destination)))
   auth-source-kwallet-test-process-calls)
  (when auth-source-kwallet-test-signal
    (signal
     (car auth-source-kwallet-test-signal)
     (cdr auth-source-kwallet-test-signal)))
  (when (bufferp destination)
    (with-current-buffer destination
      (insert auth-source-kwallet-test-output)))
  auth-source-kwallet-test-status)

(fset
 'executable-find
 #'auth-source-kwallet-test-executable-find)
(fset
 'call-process
 #'auth-source-kwallet-test-call-process)

(defun auth-source-kwallet-test-reset-process
    ()
  (setq
   auth-source-kwallet-test-executable-found t
   auth-source-kwallet-test-output "fixture-secret\n"
   auth-source-kwallet-test-status 0
   auth-source-kwallet-test-signal nil
   auth-source-kwallet-test-executable-calls nil
   auth-source-kwallet-test-process-calls nil))

(defun auth-source-kwallet-test-error
    (thunk)
  (condition-case error
      (list :ok (funcall thunk))
    (error
     (list
      :signal
      (car error)
      (cdr error)))))

(defun auth-source-kwallet-test-backend
    (backend)
  (and
   backend
   (list
    (eieio-object-class-name backend)
    (slot-value backend 'source)
    (slot-value backend 'type)
    (slot-value backend 'host)
    (slot-value backend 'user)
    (slot-value backend 'port)
    (slot-value backend 'search-function)
    (slot-value backend 'create-function))))

(defun auth-source-kwallet-test-enable-clean
    ()
  (advice-remove
   'auth-source-backend-parse
   #'auth-source-kwallet--kwallet-backend-parse)
  (setq auth-sources nil)
  (auth-source-forget-all-cached)
  (auth-source-kwallet-test-reset-process)
  (auth-source-kwallet-enable))
"##;

fn auth_source_kwallet_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTH_SOURCE_KWALLET_MELPA_PIN, source_file)
        .expect("prepare pinned auth-source-kwallet source below ./tmp")
        .with_prelude(AUTH_SOURCE_KWALLET_TEST_PRELUDE)
        .with_timeout(AUTH_SOURCE_KWALLET_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auth-source-kwallet parity test")
        .into()
}

/// Multi-probe batch for `assert_auth_source_kwallet_autoload_parity` cases (2a).
pub(crate) fn assert_auth_source_kwallet_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auth_source_kwallet_oracle("auth-source-kwallet-autoloads.el"),
        &name,
        "auth_source_kwallet_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auth_source_kwallet_parity` cases (2a).
pub(crate) fn assert_auth_source_kwallet_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auth_source_kwallet_oracle("auth-source-kwallet.el"),
        &name,
        "auth_source_kwallet_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auth_source_kwallet_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auth_source_kwallet_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auth_source_kwallet_autoload_batch(&cases);
}

#[test]
fn auth_source_kwallet_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backend::backend_public_surface_batch_cases(),
        process::process_public_surface_batch_cases(),
        registry::registry_auth_source_kwallet_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auth_source_kwallet_batch(&cases);
}

// END generated package batch tests
