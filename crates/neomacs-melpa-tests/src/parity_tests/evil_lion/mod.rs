use std::time::Duration;

use crate::{CachedMelpaOracle, EVIL_LION_MELPA_PIN, EVIL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const EVIL_LION_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const EVIL_LION_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'evil-lion)

(defun neomacs-evil-lion-test-align (text direction count char &optional mode squeeze)
  "Align TEXT in DIRECTION with COUNT and CHAR, returning visible state.

MODE defaults to `text-mode'.  SQUEEZE is a cons whose cdr is the desired
value, so callers can distinguish an explicit nil from the default setting."
  (with-temp-buffer
    (funcall (or mode #'text-mode))
    (insert text)
    (let ((evil-lion-squeeze-spaces
           (if squeeze (cdr squeeze) evil-lion-squeeze-spaces)))
      (funcall (if (eq direction 'left)
                   #'evil-lion-left
                 #'evil-lion-right)
               count (point-min) (point-max) char))
    (list :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :mark (mark t)
          :narrowed (buffer-narrowed-p)
          :mode major-mode)))

(defun neomacs-evil-lion-test-binding (state key)
  "Return the Evil Lion minor-mode binding for STATE and KEY."
  (lookup-key (evil-get-minor-mode-keymap state 'evil-lion-mode) key))
"##;

fn evil_lion_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_LION_MELPA_PIN, "evil-lion.el")
        .expect("prepare exact shallow Evil Lion source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare exact Evil dependency")
        .with_prelude(EVIL_LION_TEST_PRELUDE)
        .with_timeout(EVIL_LION_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Evil Lion parity test")
        .into()
}

fn assert_evil_lion_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        evil_lion_oracle(),
        &current_test_name(),
        "evil_lion_parity",
        cases,
    );
}

#[test]
fn evil_lion_package_batch() {
    assert_evil_lion_batch(&workflows::workflow_batch_cases());
}
