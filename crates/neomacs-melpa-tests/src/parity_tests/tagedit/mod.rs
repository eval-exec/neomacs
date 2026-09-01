use std::time::Duration;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, S_MELPA_PIN, TAGEDIT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TAGEDIT_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const TAGEDIT_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'sgml-mode)
(require 'tagedit)

(defun neomacs-tagedit-test-with-html (contents needle function)
  "Insert CONTENTS as html-mode, enable tagedit, place point on NEEDLE, call FUNCTION."
  (with-temp-buffer
    (html-mode)
    (tagedit-mode 1)
    (insert contents)
    (goto-char (point-min))
    (when needle
      (search-forward needle)
      (goto-char (match-beginning 0)))
    (funcall function)))

(defun neomacs-tagedit-test-state ()
  "Return buffer text and point line/column."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :mode (and tagedit-mode t)))
"####;

fn tagedit_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TAGEDIT_MELPA_PIN, "tagedit.el")
        .expect("prepare exact shallow tagedit source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare exact shallow dash dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare exact shallow s dependency below ./tmp")
        .with_prelude(TAGEDIT_TEST_PRELUDE)
        .with_timeout(TAGEDIT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed tagedit parity test")
        .into()
}

fn assert_tagedit_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        tagedit_oracle(),
        &current_test_name(),
        "tagedit_parity",
        cases,
    );
}

#[test]
fn tagedit_package_batch() {
    assert_tagedit_batch(&workflows::workflow_batch_cases());
}
