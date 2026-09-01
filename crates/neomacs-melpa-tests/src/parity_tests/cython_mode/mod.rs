use std::time::Duration;

use crate::{CYTHON_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const CYTHON_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const CYTHON_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'python)
(require 'cython-mode)

(defun neomacs-cython-mode-test-with-buffer (body)
  "Call BODY in a temporary Cython buffer with sample source."
  (with-temp-buffer
    (insert
     "cdef class Point:\n"
     "    cdef public double x\n"
     "    cdef public double y\n"
     "\n"
     "    cpdef double magnitude(self):\n"
     "        return (self.x ** 2 + self.y ** 2) ** 0.5\n"
     "\n"
     "def make_point(double x, double y):\n"
     "    return Point(x, y)\n"
     "\n"
     "# trailing comment\n")
    (cython-mode)
    (goto-char (point-min))
    (funcall body)))
"####;

fn cython_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CYTHON_MODE_MELPA_PIN, "cython-mode.el")
        .expect("prepare exact shallow cython-mode source below ./tmp")
        .with_prelude(CYTHON_MODE_TEST_PRELUDE)
        .with_timeout(CYTHON_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed cython-mode parity test")
        .into()
}

fn assert_cython_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        cython_mode_oracle(),
        &current_test_name(),
        "cython_mode_parity",
        cases,
    );
}

#[test]
fn cython_mode_package_batch() {
    assert_cython_mode_batch(&workflows::workflow_batch_cases());
}
