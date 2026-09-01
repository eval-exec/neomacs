use std::time::Duration;

use crate::{COFFEE_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const COFFEE_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const COFFEE_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'coffee-mode)

(defun neomacs-coffee-mode-test-face-at (needle)
  "Return NEEDLE and its effective font-lock face."
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (list needle
          (or (get-text-property (match-beginning 0) 'face)
              (get-text-property (match-beginning 0) 'font-lock-face)))))

(defun neomacs-coffee-mode-test-indent-snapshot ()
  "Return non-empty lines with their indentation widths."
  (let (rows)
    (save-excursion
      (goto-char (point-min))
      (while (not (eobp))
        (let ((text (string-trim-right
                     (buffer-substring-no-properties
                      (line-beginning-position) (line-end-position)))))
          (unless (string-empty-p text)
            (push (list (current-indentation) text) rows)))
        (forward-line 1)))
    (nreverse rows)))

(defun neomacs-coffee-mode-test-with-buffer (contents function)
  "Insert CONTENTS in a temporary coffee-mode buffer and call FUNCTION."
  (with-temp-buffer
    (insert contents)
    (coffee-mode)
    (setq-local coffee-tab-width 2)
    (font-lock-ensure)
    (funcall function)))
"####;

fn coffee_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COFFEE_MODE_MELPA_PIN, "coffee-mode.el")
        .expect("prepare exact shallow coffee-mode source below ./tmp")
        .with_prelude(COFFEE_MODE_TEST_PRELUDE)
        .with_timeout(COFFEE_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed coffee-mode parity test")
        .into()
}

fn assert_coffee_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        coffee_mode_oracle(),
        &current_test_name(),
        "coffee_mode_parity",
        cases,
    );
}

#[test]
fn coffee_mode_package_batch() {
    assert_coffee_mode_batch(&workflows::workflow_batch_cases());
}
