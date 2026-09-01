use std::time::Duration;

use crate::{CachedMelpaOracle, MATERIAL_THEME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const MATERIAL_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const MATERIAL_THEME_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
;; Batch-safe palette reader: live face-attribute is `unspecified' without a
;; display, so read colors straight from the theme's declared face specs.
(defun mat-theme-color (face kw)
  (let ((entry (cl-some (lambda (e) (and (consp e) (eq (cadr e) face) e))
                        (get 'material 'theme-settings))))
    (when entry (plist-get (cadr (car (cadddr entry))) kw))))
"####;

fn material_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MATERIAL_THEME_MELPA_PIN, "material-theme.el")
        .expect("prepare exact shallow material-theme source below ./tmp")
        .with_prelude(MATERIAL_THEME_TEST_PRELUDE)
        .with_timeout(MATERIAL_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed material-theme parity test")
        .into()
}

fn assert_material_theme_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        material_theme_oracle(),
        &current_test_name(),
        "material_theme_parity",
        cases,
    )
}

#[test]
fn material_theme_package_batch() {
    assert_material_theme_batch(&workflows::workflow_batch_cases());
}
