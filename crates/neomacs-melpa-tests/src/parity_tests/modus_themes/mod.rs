use std::time::Duration;

use crate::{CachedMelpaOracle, MODUS_THEMES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const MODUS_THEMES_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const MODUS_THEMES_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'modus-themes)

(defun neomacs-modus-themes-test-disable ()
  "Disable every enabled Modus theme."
  (dolist (theme (copy-sequence custom-enabled-themes))
    (when (string-prefix-p "modus-" (symbol-name theme))
      (disable-theme theme)))
  (setq modus-themes--activated-themes nil))
"####;

fn modus_themes_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MODUS_THEMES_MELPA_PIN, "modus-themes.el")
        .expect("prepare exact shallow modus-themes source below ./tmp")
        .with_prelude(MODUS_THEMES_TEST_PRELUDE)
        .with_timeout(MODUS_THEMES_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed modus-themes parity test")
        .into()
}

fn assert_modus_themes_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        modus_themes_oracle(),
        &current_test_name(),
        "modus_themes_parity",
        cases,
    );
}

#[test]
fn modus_themes_package_batch() {
    assert_modus_themes_batch(&workflows::workflow_batch_cases());
}
