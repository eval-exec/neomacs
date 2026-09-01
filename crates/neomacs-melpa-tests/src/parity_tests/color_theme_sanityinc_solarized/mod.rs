use std::time::Duration;

use crate::{COLOR_THEME_SANITYINC_SOLARIZED_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const COLOR_THEME_SANITYINC_SOLARIZED_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const COLOR_THEME_SANITYINC_SOLARIZED_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
;; Batch-safe palette reader: live face-attribute is `unspecified' without a
;; display, so read colors straight from the theme's declared face specs.
(defun ssol-theme-color (face kw)
  (let ((entry (cl-some (lambda (e) (and (consp e) (eq (cadr e) face) e))
                        (get 'sanityinc-solarized-dark 'theme-settings))))
    (when entry (plist-get (cadr (car (cadddr entry))) kw))))
"####;

fn color_theme_sanityinc_solarized_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(
        COLOR_THEME_SANITYINC_SOLARIZED_MELPA_PIN,
        "sanityinc-solarized-dark-theme.el",
    )
    .expect("prepare exact shallow color-theme-sanityinc-solarized source below ./tmp")
    .with_prelude(COLOR_THEME_SANITYINC_SOLARIZED_TEST_PRELUDE)
    .with_timeout(COLOR_THEME_SANITYINC_SOLARIZED_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed color-theme-sanityinc-solarized parity test")
        .into()
}

fn assert_color_theme_sanityinc_solarized_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        color_theme_sanityinc_solarized_oracle(),
        &current_test_name(),
        "color_theme_sanityinc_solarized_parity",
        cases,
    )
}

#[test]
fn color_theme_sanityinc_solarized_package_batch() {
    assert_color_theme_sanityinc_solarized_batch(&workflows::workflow_batch_cases());
}
