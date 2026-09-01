use std::time::Duration;

use crate::{APPLESCRIPT_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const APPLESCRIPT_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const APPLESCRIPT_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun applescript-test-face-at
    (token)
  (goto-char (point-min))
  (search-forward token)
  (or
   (get-text-property
     (match-beginning 0)
     'face)
   (get-text-property
    (match-beginning 0)
    'font-lock-face)))

(defun applescript-test-kill-buffers
    (regexp)
  (dolist (buffer (buffer-list))
    (when (string-match-p
           regexp
           (buffer-name buffer))
      (set-buffer-modified-p nil)
      (kill-buffer buffer))))
"##;

fn applescript_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(APPLESCRIPT_MODE_MELPA_PIN, "applescript-mode.el")
        .expect("prepare pinned applescript-mode source below ./tmp")
        .with_prelude(APPLESCRIPT_MODE_TEST_PRELUDE)
        .with_timeout(APPLESCRIPT_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed applescript-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_applescript_mode_parity` cases (2a).
pub(crate) fn assert_applescript_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        applescript_mode_oracle(),
        &name,
        "applescript_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn applescript_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_applescript_mode_batch(&cases);
}

// END generated package batch tests
