use std::time::Duration;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, EVIL_CLEVERPARENS_MELPA_PIN, EVIL_MELPA_PIN,
    PAREDIT_MELPA_PIN, SMARTPARENS_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const EVIL_CLEVERPARENS_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const EVIL_CLEVERPARENS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'evil-cleverparens)

(defun neomacs-evil-cleverparens-test-balanced-p ()
  "Return non-nil when the accessible buffer has balanced delimiters."
  (condition-case nil
      (progn (check-parens) t)
    (error nil)))

(defun neomacs-evil-cleverparens-test-state ()
  "Describe the user-visible structural editing state."
  (list :buffer (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :char (char-after)
        :evil-state evil-state
        :balanced (neomacs-evil-cleverparens-test-balanced-p)
        :kill (and kill-ring
                   (substring-no-properties (current-kill 0 t)))))

(defun neomacs-evil-cleverparens-test-range (range)
  "Describe Evil RANGE as positions, type, and selected source text."
  (list :begin (evil-range-beginning range)
        :end (evil-range-end range)
        :type (evil-type range)
        :text (buffer-substring-no-properties
               (evil-range-beginning range)
               (evil-range-end range))))
"##;

fn evil_cleverparens_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_CLEVERPARENS_MELPA_PIN, "evil-cleverparens.el")
        .expect("prepare exact shallow Evil Cleverparens source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare exact shallow Evil dependency below ./tmp")
        .with_melpa_dependency(PAREDIT_MELPA_PIN)
        .expect("prepare exact shallow Paredit dependency below ./tmp")
        .with_melpa_dependency(SMARTPARENS_MELPA_PIN)
        .expect("prepare exact shallow Smartparens dependency below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare exact shallow Dash dependency below ./tmp")
        .with_prelude(EVIL_CLEVERPARENS_TEST_PRELUDE)
        .with_timeout(EVIL_CLEVERPARENS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Evil Cleverparens parity test")
        .into()
}

fn assert_evil_cleverparens_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        evil_cleverparens_oracle(),
        &current_test_name(),
        "evil_cleverparens_parity",
        cases,
    );
}

#[test]
fn evil_cleverparens_package_batch() {
    assert_evil_cleverparens_batch(&workflows::workflow_batch_cases());
}
