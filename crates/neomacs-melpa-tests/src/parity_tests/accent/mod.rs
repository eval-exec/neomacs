use std::time::Duration;

use crate::{ACCENT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACCENT_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// accent's popup is a real `popup.el` menu driven by real keys, so the work
/// buffer has to be the selected window's buffer for `execute-kbd-macro` to
/// reach it.  No part of the package is stubbed in these workflows.
const ACCENT_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defmacro accent-test-with-live-buffer (&rest body)
  "Run BODY in a real, window-displayed buffer so typed keys reach it."
  `(let ((buffer (generate-new-buffer "*accent-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (global-set-key (kbd "C-x C-a") #'accent-menu)
           ,@body)
       (kill-buffer buffer))))

(defun accent-test-last-message ()
  (with-current-buffer (get-buffer-create "*Messages*")
    (car (last (split-string (buffer-string) "\n" t)))))
"##;

fn accent_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACCENT_MELPA_PIN, "accent.el")
        .expect("prepare pinned accent source below ./tmp")
        .with_prelude(ACCENT_TEST_PRELUDE)
        .with_timeout(ACCENT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed accent parity test").into()
}

/// Multi-probe batch for `assert_accent_parity` cases (2a).
pub(crate) fn assert_accent_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(accent_oracle(), &name, "accent_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn accent_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_accent_batch(&cases);
}

// END generated package batch tests
