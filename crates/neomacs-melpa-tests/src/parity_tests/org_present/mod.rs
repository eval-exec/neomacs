use std::time::Duration;

use crate::{CachedMelpaOracle, ORG_PRESENT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'org)
(require 'org-present)

(defvar neomacs-org-present-test-events nil)

(defun neomacs-org-present-test-navigate (buffer heading)
  "Record navigation BUFFER and HEADING."
  (push (list buffer heading (point-min) (point-max))
        neomacs-org-present-test-events))

(defun neomacs-org-present-test-overlays ()
  "Describe current-buffer Org Present overlays in source order."
  (save-restriction
    (widen)
    (mapcar
     (lambda (overlay)
       (list :start (overlay-start overlay)
             :end (overlay-end overlay)
             :text (buffer-substring-no-properties
                    (overlay-start overlay) (overlay-end overlay))
             :invisible (overlay-get overlay 'invisible)))
     (sort (cl-remove-if-not
            (lambda (overlay) (eq (overlay-buffer overlay) (current-buffer)))
            (copy-sequence org-present-overlays-list))
           (lambda (left right) (< (overlay-start left) (overlay-start right)))))))

(defun neomacs-org-present-test-state ()
  "Describe visible presentation and editor state."
  (list :mode org-present-mode
        :text (buffer-substring-no-properties (point-min) (point-max))
        :restriction (list (point-min) (point-max))
        :point (list (point) (line-number-at-pos) (current-column))
        :heading (org-get-heading t t t t)
        :read-only buffer-read-only
        :cursor cursor-type
        :scale (and (boundp 'text-scale-mode-amount) text-scale-mode-amount)
        :one-page org-present-one-big-page
        :space (lookup-key org-present-mode-keymap (kbd "SPC"))
        :overlays (neomacs-org-present-test-overlays)))

(defmacro neomacs-org-present-test-with-buffer (&rest body)
  "Run BODY in a deterministic three-slide Org presentation."
  (declare (indent 0) (debug t))
  `(let ((buffer (generate-new-buffer "org-present-release"))
         (neomacs-org-present-test-events nil)
         (org-present-after-navigate-functions
          '(neomacs-org-present-test-navigate))
         (org-present-mode-hook nil)
         (org-present-mode-quit-hook nil)
         (org-present-overlays-list nil)
         (org-present-cursor-cache cursor-type)
         (old-space-binding (lookup-key org-present-mode-keymap (kbd "SPC"))))
     (unwind-protect
         (save-window-excursion
           ;; `org-present-read-only' and `org-present-read-write' mutate the
           ;; shared minor-mode keymap.  Give every workflow the package's
           ;; initial binding so batch order cannot change observable state.
           (define-key org-present-mode-keymap (kbd "SPC") nil)
           (set-window-buffer (selected-window) buffer)
           (with-current-buffer buffer
             (insert "#+title: Release Ω\n#+author: Platform\n#+options: toc:nil\nIntro *overview* and =code=.\n\n* Plan\n** Scope\nShip *candidate*.\n\n* Build\nCompile =binary=.\n\n* Release\nPublish /safely/.\n")
             (org-mode)
             (setq-local text-scale-mode-amount 0)
             (goto-char (point-min))
             ,@body))
       (define-key org-present-mode-keymap (kbd "SPC") old-space-binding)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (setq buffer-read-only nil)
           (when org-present-mode (ignore-errors (org-present-quit))))
         (kill-buffer buffer)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORG_PRESENT_MELPA_PIN, "org-present.el")
        .expect("prepare exact shallow Org Present source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn org_present_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "org_present_package_batch",
        "org_present_parity",
        &workflows::workflow_batch_cases(),
    );
}
