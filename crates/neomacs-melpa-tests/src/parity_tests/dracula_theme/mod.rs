use std::time::Duration;

use crate::{CachedMelpaOracle, DRACULA_THEME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const DRACULA_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const DRACULA_THEME_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
;; Batch-safe palette reader: live face-attribute is `unspecified' without a
;; display, so read colors straight from the theme's declared face specs.
(defun drac-theme-color (face kw)
  (let ((entry (cl-some (lambda (e) (and (consp e) (eq (cadr e) face) e))
                        (get 'dracula 'theme-settings))))
    (when entry (plist-get (cadr (car (cadddr entry))) kw))))
"####;

fn dracula_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DRACULA_THEME_MELPA_PIN, "dracula-theme.el")
        .expect("prepare exact shallow dracula-theme source below ./tmp")
        .with_prelude(DRACULA_THEME_TEST_PRELUDE)
        .with_timeout(DRACULA_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed dracula-theme parity test")
        .into()
}

fn assert_dracula_theme_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        dracula_theme_oracle(),
        &current_test_name(),
        "dracula_theme_parity",
        cases,
    )
}

#[test]
fn dracula_theme_package_batch() {
    assert_dracula_theme_batch(&workflows::workflow_batch_cases());
}
