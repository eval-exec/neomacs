use std::time::Duration;

use crate::{CachedMelpaOracle, PASSWORD_GENERATOR_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const PASSWORD_GENERATOR_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PASSWORD_GENERATOR_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'password-generator)

(defvar neomacs-password-generator-test-draw 0)

(defun neomacs-password-generator-test-random (max)
  "Return a deterministic draw below MAX for the current parity case."
  (prog1 (% (+ (* neomacs-password-generator-test-draw 17) 5) max)
    (setq neomacs-password-generator-test-draw
          (1+ neomacs-password-generator-test-draw))))
"##;

fn password_generator_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PASSWORD_GENERATOR_MELPA_PIN, "password-generator.el")
        .expect("prepare exact shallow Password Generator source below ./tmp")
        .with_prelude(PASSWORD_GENERATOR_TEST_PRELUDE)
        .with_timeout(PASSWORD_GENERATOR_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Password Generator parity test")
        .into()
}

fn assert_password_generator_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        password_generator_oracle(),
        &current_test_name(),
        "password_generator_parity",
        cases,
    );
}

#[test]
fn password_generator_package_batch() {
    assert_password_generator_batch(&workflows::workflow_batch_cases());
}
