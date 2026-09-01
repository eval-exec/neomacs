use std::time::Duration;

use crate::{CachedMelpaOracle, PYTEST_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const PYTEST_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PYTEST_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'python)
(require 'pytest)

(defun neomacs-pytest-test-with-project (body)
  "Call BODY with a temporary Python project containing tests."
  (let* ((root (make-temp-file "neomacs-pytest-" t))
         (default-directory root))
    (unwind-protect
        (progn
          (with-temp-file (expand-file-name "setup.py" root)
            (insert "from setuptools import setup\nsetup(name='demo')\n"))
          (with-temp-file (expand-file-name "test_demo.py" root)
            (insert
             "class TestMath:\n"
             "    def test_add(self):\n"
             "        assert 1 + 1 == 2\n"
             "\n"
             "def test_top_level():\n"
             "    assert True\n"))
          (funcall body root))
      (ignore-errors (delete-directory root t)))))
"####;

fn pytest_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PYTEST_MELPA_PIN, "pytest.el")
        .expect("prepare exact shallow pytest source below ./tmp")
        .with_prelude(PYTEST_TEST_PRELUDE)
        .with_timeout(PYTEST_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed pytest parity test")
        .into()
}

fn assert_pytest_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        pytest_oracle(),
        &current_test_name(),
        "pytest_parity",
        cases,
    );
}

#[test]
fn pytest_package_batch() {
    assert_pytest_batch(&workflows::workflow_batch_cases());
}
