use std::time::Duration;

use crate::{CachedMelpaOracle, MMM_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'mmm-mode)
(require 'mmm-cmds)
(require 'mmm-erb)

(defmacro neomacs-mmm-test-with-buffer (&rest body)
  "Run BODY in a user-visible buffer with interactive MMM eligibility."
  (declare (indent 0) (debug t))
  `(let ((buffer (generate-new-buffer "mmm-parity"))
         ;; MMM intentionally declines batch and hidden buffers.  The parity
         ;; workflow models an ordinary interactive editing buffer while the
         ;; harness itself remains batch-driven.
         (noninteractive nil))
     (unwind-protect
         (with-current-buffer buffer ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (when mmm-mode (mmm-mode-off))
           (set-buffer-modified-p nil))
         (kill-buffer buffer)))))

(defun neomacs-mmm-test-overlays ()
  "Describe all MMM overlays in source order."
  (mapcar
   (lambda (overlay)
     (list :range (list (overlay-start overlay) (overlay-end overlay))
           :text (buffer-substring-no-properties
                  (overlay-start overlay) (overlay-end overlay))
           :mode (overlay-get overlay 'mmm-mode)
           :name (overlay-get overlay 'name)
           :face (overlay-get overlay 'face)
           :delimiter (overlay-get overlay 'delim)
           :special (overlay-get overlay 'mmm-special-tag)))
   (sort (cl-remove-if-not (lambda (overlay) (overlay-get overlay 'mmm))
                           (overlays-in (point-min) (point-max)))
         (lambda (left right)
           (if (= (overlay-start left) (overlay-start right))
               (< (overlay-end left) (overlay-end right))
             (< (overlay-start left) (overlay-start right)))))))

(defun neomacs-mmm-test-token-state (tokens)
  "Describe faces and submodes at TOKENS."
  (save-excursion
    (mapcar
     (lambda (token)
       (goto-char (point-min))
       (search-forward token)
       (let ((position (match-beginning 0)))
         (list token position
               (get-char-property position 'face)
               (mmm-submode-at position))))
     tokens)))

(defun neomacs-mmm-test-state ()
  "Describe current MMM buffer state."
  (list :mode mmm-mode :major major-mode :primary mmm-primary-mode
        :current mmm-current-submode
        :mode-name mode-name
        :indent indent-line-function
        :fontifier font-lock-fontify-region-function
        :syntax syntax-propertize-function
        :overlays (neomacs-mmm-test-overlays)
        :bindings (mapcar (lambda (key) (cons key (lookup-key mmm-mode-map (kbd key))))
                          '("C-c % c" "C-c % x" "C-c % r" "C-c % b"
                            "C-c % k" "C-c % z"))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MMM_MODE_MELPA_PIN, "mmm-mode.el")
        .expect("prepare exact shallow MMM Mode source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn mmm_mode_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "mmm_mode_package_batch",
        "mmm_mode_parity",
        &workflows::workflow_batch_cases(),
    );
}
