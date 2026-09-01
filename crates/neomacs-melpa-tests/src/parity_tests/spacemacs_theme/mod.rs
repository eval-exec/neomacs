use std::time::Duration;

use crate::{CachedMelpaOracle, SPACEMACS_THEME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const SPACEMACS_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SPACEMACS_THEME_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
;; Batch-safe palette reader: live face-attribute is `unspecified' without a
;; display, so read colors straight from the theme's declared face specs.
(defun spc-theme-color (face kw)
  (let ((entry (cl-some (lambda (e) (and (consp e) (eq (cadr e) face) e))
                        (get 'spacemacs-dark 'theme-settings))))
    (when entry (plist-get (cadr (car (cadddr entry))) kw))))
"####;

fn spacemacs_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SPACEMACS_THEME_MELPA_PIN, "spacemacs-dark-theme.el")
        .expect("prepare exact shallow spacemacs-theme source below ./tmp")
        .with_prelude(SPACEMACS_THEME_TEST_PRELUDE)
        .with_timeout(SPACEMACS_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed spacemacs-theme parity test")
        .into()
}

fn assert_spacemacs_theme_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        spacemacs_theme_oracle(),
        &current_test_name(),
        "spacemacs_theme_parity",
        cases,
    )
}

#[test]
fn spacemacs_theme_package_batch() {
    assert_spacemacs_theme_batch(&workflows::workflow_batch_cases());
}
