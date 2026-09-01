use std::time::Duration;

use crate::{AVY_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AVY_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const AVY_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'avy)

(defun neomacs-avy-test-goto-deploy ()
  "Run a real Avy character jump to an uppercase deployment marker."
  (interactive)
  (avy-goto-char ?D))

(defun neomacs-avy-test-goto-cross-window ()
  "Run a real Avy character jump to an uppercase cross-window marker."
  (interactive)
  (avy-goto-char ?X))

(defun neomacs-avy-test-yank-release ()
  "Select a parenthesized release expression and dispatch an Avy action."
  (interactive)
  (avy-goto-char ?\())

(defun neomacs-avy-test-copy-line ()
  "Copy one keyboard-selected line above point."
  (interactive)
  (avy-copy-line 1))

(defun neomacs-avy-test-move-line ()
  "Move one keyboard-selected line above point."
  (interactive)
  (avy-move-line 1))

(defun neomacs-avy-test-current-line ()
  "Return the current line without text properties."
  (buffer-substring-no-properties (line-beginning-position)
                                  (line-end-position)))

(defun neomacs-avy-test-overlay-count ()
  "Count live Avy category overlays in the current buffer."
  (cl-count-if (lambda (overlay)
                 (eq (overlay-get overlay 'category) 'avy))
               (overlays-in (point-min) (point-max))))
"##;

fn avy_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AVY_MELPA_PIN, "avy.el")
        .expect("prepare pinned Avy source below ./tmp")
        .with_prelude(AVY_TEST_PRELUDE)
        .with_timeout(AVY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed Avy parity test").into()
}

pub(crate) fn assert_avy_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(avy_oracle(), &name, "avy_parity", cases);
}

#[test]
fn avy_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_avy_batch(&cases);
}
