use std::time::Duration;

use crate::{AIO_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AIO_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// aio gives Emacs async/await over promises, so every workflow runs a real
/// asynchronous story: `aio-defun' functions awaiting each other, timers,
/// `aio-select' racing promises, and a real subprocess feeding a callback
/// chain.  Nothing is stood in.
///
/// The determinism rule for this package is narrower than usual.  Timer-based
/// promises resolve in a defined order and can be pinned; process output
/// cannot, because the number of chunks a filter receives is up to the
/// kernel.  So the process workflow chains on every chunk but asserts the
/// joined text and the sentinel's own event, never the chunk boundaries.
const AIO_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun aio-test-plain (value)
  (cond ((stringp value) (substring-no-properties value))
        ((consp value)
         (cons (aio-test-plain (car value)) (aio-test-plain (cdr value))))
        (t value)))

(defun aio-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun aio-test-script (name text)
  "Write an executable NAME below the sandbox and return its path."
  (let ((path (aio-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    (set-file-modes path #o755)
    path))
"##;

fn aio_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AIO_MELPA_PIN, "aio.el")
        .expect("prepare pinned aio source below ./tmp")
        .with_prelude(AIO_TEST_PRELUDE)
        .with_timeout(AIO_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed aio parity test").into()
}

/// Multi-probe batch for `assert_aio_parity` cases (2a).
pub(crate) fn assert_aio_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aio_oracle(), &name, "aio_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn aio_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aio_batch(&cases);
}

// END generated package batch tests
