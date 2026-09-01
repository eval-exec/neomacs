use crate::{CachedMelpaOracle, ZENCODING_MODE_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ZENCODING_MODE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'paren)
(require 'sgml-mode)
(require 'zencoding-mode)

(defun neomacs-zencoding-test--overlay-state (overlay)
  (when (overlayp overlay)
    (list
     :live (bufferp (overlay-buffer overlay))
     :start (overlay-start overlay)
     :end (overlay-end overlay)
     :front-advance (overlay-get overlay 'front-advance)
     :rear-advance (overlay-get overlay 'rear-advance)
     :face (overlay-get overlay 'face)
     :key-ret (let ((map (overlay-get overlay 'keymap)))
                (and map (lookup-key map (kbd "RET"))))
     :key-c-g (let ((map (overlay-get overlay 'keymap)))
                (and map (lookup-key map (kbd "C-g"))))
     :before (let ((text (overlay-get overlay 'before-string)))
               (and text (substring-no-properties text)))
     :after (let ((text (overlay-get overlay 'after-string)))
              (and text (substring-no-properties text))))))

(defun neomacs-zencoding-test--hook-state ()
  (list
   :before-change
   (and (memq 'zencoding-preview-before-change before-change-functions) t)
   :post-command
   (and (memq 'zencoding-preview-post-command post-command-hook) t)
   :pending zencoding-preview-pending-abort))

(defun neomacs-zencoding-test--flash-state ()
  (when (overlayp zencoding-flash-ovl)
    (list
     :start (overlay-start zencoding-flash-ovl)
     :end (overlay-end zencoding-flash-ovl)
     :face (overlay-get zencoding-flash-ovl 'face)
     :text (buffer-substring-no-properties
            (overlay-start zencoding-flash-ovl)
            (overlay-end zencoding-flash-ovl)))))

(defun neomacs-zencoding-test--cleanup-preview ()
  (when (and (boundp 'zencoding-preview-input)
             (overlayp zencoding-preview-input))
    (zencoding-preview-abort))
  (when (and (boundp 'zencoding-flash-ovl)
             (overlayp zencoding-flash-ovl))
    (delete-overlay zencoding-flash-ovl)
    (setq zencoding-flash-ovl nil)))
"####;

fn zencoding_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ZENCODING_MODE_MELPA_PIN, "zencoding-mode.el")
        .expect("prepare pinned zencoding-mode source below ./tmp")
        .with_prelude(ZENCODING_MODE_TEST_PRELUDE)
}

#[test]
fn zencoding_mode_package_batch() {
    assert_oracle_batch_cases(
        zencoding_mode_oracle(),
        "zencoding_mode_package_batch",
        "zencoding_mode_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
