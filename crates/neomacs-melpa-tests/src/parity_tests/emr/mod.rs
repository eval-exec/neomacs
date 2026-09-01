use std::time::Duration;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, EMR_MELPA_PIN, LIST_UTILS_MELPA_PIN, PAREDIT_MELPA_PIN,
    POPUP_MELPA_PIN, S_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const EMR_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const EMR_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'emr)
(require 'emr-elisp)
(emr-el-initialize)

(defun neomacs-emr-test-with-elisp (text body)
  "Insert TEXT in a temp emacs-lisp buffer and call BODY."
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert text)
    (goto-char (point-min))
    (funcall body)))
"####;

fn emr_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EMR_MELPA_PIN, "emr.el")
        .expect("prepare exact shallow emr source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare dash")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare s")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare popup")
        .with_melpa_dependency(LIST_UTILS_MELPA_PIN)
        .expect("prepare list-utils")
        .with_melpa_dependency(PAREDIT_MELPA_PIN)
        .expect("prepare paredit")
        .with_prelude(EMR_TEST_PRELUDE)
        .with_timeout(EMR_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed emr parity test")
        .into()
}

fn assert_emr_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(emr_oracle(), &current_test_name(), "emr_parity", cases);
}

#[test]
fn emr_package_batch() {
    assert_emr_batch(&workflows::workflow_batch_cases());
}
