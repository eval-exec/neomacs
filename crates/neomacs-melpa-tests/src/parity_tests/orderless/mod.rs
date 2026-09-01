use std::time::Duration;

use crate::{COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, ORDERLESS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ORDERLESS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ORDERLESS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'orderless)

(defun neomacs-orderless-test-faces (string)
  "Return face runs across STRING in order."
  (let ((position 0) runs)
    (while (< position (length string))
      (let ((next (or (next-single-property-change position 'face string)
                      (length string))))
        (push (list (substring-no-properties string position next)
                    (get-text-property position 'face string))
              runs)
        (setq position next)))
    (nreverse runs)))
"####;

fn orderless_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORDERLESS_MELPA_PIN, "orderless.el")
        .expect("prepare exact shallow orderless source below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact shallow compat dependency below ./tmp")
        .with_prelude(ORDERLESS_TEST_PRELUDE)
        .with_timeout(ORDERLESS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed orderless parity test")
        .into()
}

fn assert_orderless_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        orderless_oracle(),
        &current_test_name(),
        "orderless_parity",
        cases,
    );
}

#[test]
fn orderless_package_batch() {
    assert_orderless_batch(&workflows::workflow_batch_cases());
}
