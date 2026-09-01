use std::time::Duration;

use crate::{CachedMelpaOracle, GIT_MODES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const GITATTRIBUTES_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'git-modes)

(defun neomacs-gitattributes-test-face-spans ()
  "Return every semantically fontified span in buffer order."
  (font-lock-ensure)
  (let ((position (point-min))
        spans)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (next-single-property-change
                    position 'face nil (point-max))))
        (when face
          (push (list :range (list position next)
                      :text (buffer-substring-no-properties position next)
                      :face face)
                spans))
        (setq position next)))
    (nreverse spans)))

(defun neomacs-gitattributes-test-eldoc-at (marker)
  "Return the installed ElDoc result at the start of MARKER."
  (goto-char (point-min))
  (search-forward marker)
  (goto-char (- (point) (length marker)))
  (list :marker marker
        :line (line-number-at-pos)
        :column (current-column)
        :documentation (funcall eldoc-documentation-function)))

(defun neomacs-gitattributes-test-position ()
  "Describe the current field-editing position without moving point."
  (list :point (point)
        :column (current-column)
        :field (thing-at-point 'symbol t)))
"####;

fn gitattributes_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GIT_MODES_MELPA_PIN, "git-modes.el")
        .expect("reuse revision-pinned Git Modes source below ./tmp")
        .with_prelude(GITATTRIBUTES_MODE_TEST_PRELUDE)
        .with_timeout(Duration::from_secs(240))
}

fn assert_gitattributes_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        gitattributes_mode_oracle(),
        "gitattributes-mode-package-batch",
        "Gitattributes Mode (from Git Modes)",
        cases,
    );
}

#[test]
fn gitattributes_mode_package_batch() {
    assert_gitattributes_mode_batch(&workflows::workflow_batch_cases());
}
