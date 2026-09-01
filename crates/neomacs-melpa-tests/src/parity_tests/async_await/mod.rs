use std::time::Duration;

use crate::{ASYNC_AWAIT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod errors;
mod macros;
mod registry;
mod workflows;

const ASYNC_AWAIT_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ASYNC_AWAIT_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'imenu)
(require 'seq)
(require 'autoload)

(defun async-await-test-settle (promise)
  (let* ((settled (promise-wait 2 promise))
         (state
          (pcase (promise-_state settled)
            (0 'pending)
            (1 'fulfilled)
            (2 'rejected)
            (3 'adopted)
            (other (list 'unknown other)))))
    (list state (promise-_value settled))))

(defun async-await-test-delay
    (seconds value &optional reject-p)
  (promise-new
   (lambda (resolve reject)
     (run-at-time
      seconds nil
      (lambda ()
        (funcall
         (if reject-p reject resolve)
         value))))))

(defun async-await-test-path (filename)
  (expand-file-name
   filename
   (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
"##;

fn async_await_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASYNC_AWAIT_MELPA_PIN, source_file)
        .expect("prepare pinned async-await source and dependencies below ./tmp")
        .with_prelude(ASYNC_AWAIT_TEST_PRELUDE)
        .with_timeout(ASYNC_AWAIT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed async-await parity test")
        .into()
}

/// Multi-probe batch for `assert_async_await_autoload_parity` cases (2a).
pub(crate) fn assert_async_await_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        async_await_oracle("async-await-autoloads.el"),
        &name,
        "async_await_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_async_await_parity` cases (2a).
pub(crate) fn assert_async_await_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        async_await_oracle("async-await.el"),
        &name,
        "async_await_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn async_await_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_async_await_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_async_await_autoload_batch(&cases);
}

#[test]
fn async_await_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        errors::errors_public_surface_batch_cases(),
        macros::macros_public_surface_batch_cases(),
        registry::registry_async_await_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_async_await_batch(&cases);
}

// END generated package batch tests
