use std::time::Duration;

use crate::{CachedMelpaOracle, RAINBOW_MODE_SOURCE_PIN, ZENBURN_THEME_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ZENBURN_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const ZENBURN_THEME_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'cus-face)
(require 'rainbow-mode)

(cl-defun neomacs-zenburn-test--reload
    (&key
     (colors nil)
     (semantic-colors nil)
     (variable-pitch nil)
     (scale-org nil)
     (scale-outline nil)
     (height-1 1.1)
     (height-2 1.15)
     (height-3 1.2)
     (height-4 1.3)
     no-enable)
  "Reload Zenburn under one complete, explicit user configuration."
  (when (custom-theme-enabled-p 'zenburn)
    (disable-theme 'zenburn))
  (let ((zenburn-override-colors-alist colors)
        (zenburn-override-semantic-colors-alist semantic-colors)
        (zenburn-use-variable-pitch variable-pitch)
        (zenburn-scale-org-headlines scale-org)
        (zenburn-scale-outline-headlines scale-outline)
        (zenburn-height-plus-1 height-1)
        (zenburn-height-plus-2 height-2)
        (zenburn-height-plus-3 height-3)
        (zenburn-height-plus-4 height-4))
    (load-theme 'zenburn t no-enable)))

(defun neomacs-zenburn-test--face-state (faces)
  "Return exact raw and inherited appearance for FACES."
  (mapcar
   (lambda (face)
     (if (not (facep face))
         (list face :undefined)
       (list
        face
        (face-attribute face :foreground nil nil)
        (face-attribute face :background nil nil)
        (face-attribute face :foreground nil 'default)
        (face-attribute face :background nil 'default)
        (face-attribute face :weight nil t)
        (face-attribute face :slant nil t)
        (copy-tree (face-attribute face :underline nil t))
        (face-attribute face :extend nil t)
        (copy-tree (face-attribute face :inherit nil nil)))))
   faces))

(defun neomacs-zenburn-test--primary-face (value)
  "Return the first actual face named by text property VALUE."
  (cond
   ((and (symbolp value) (facep value)) value)
   ((listp value)
    (cl-find-if
     (lambda (candidate)
       (and (symbolp candidate) (facep candidate)))
     value))))

(defun neomacs-zenburn-test--token-state (tokens)
  "Return exact font-lock and resolved appearance for TOKENS."
  (save-excursion
    (mapcar
     (lambda (token)
       (goto-char (point-min))
       (search-forward token)
       (let* ((position (match-beginning 0))
              (displayed (get-char-property position 'face))
              (face (neomacs-zenburn-test--primary-face
                     displayed)))
         (list
          token
          position
          (copy-tree displayed)
          (and face (face-attribute face :foreground nil 'default))
          (and face (face-attribute face :background nil 'default))
          (and face (face-attribute face :weight nil t))
          (and face (face-attribute face :slant nil t)))))
     tokens)))

(defun neomacs-zenburn-test--property-runs (property)
  "Return every contiguous non-nil PROPERTY run in the current buffer."
  (let ((position (point-min)) runs)
    (while (< position (point-max))
      (let* ((value (get-text-property position property))
             (next (next-single-property-change
                    position property nil (point-max))))
        (when value
          (push
           (list
            (buffer-substring-no-properties position next)
            (copy-tree value)
            position next)
           runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-zenburn-test--theme-face-specs (faces)
  "Return Zenburn's exact recorded face specs for FACES."
  (mapcar
   (lambda (face)
     (list face (copy-tree (cadr (assq 'zenburn (get face 'theme-face))))))
   faces))

(defun neomacs-zenburn-test--cleanup (root)
  "Restore themes and remove buffers and files below ROOT."
  (dolist (theme '(zenburn neomacs-zenburn-baseline))
    (when (custom-theme-enabled-p theme)
      (disable-theme theme)))
  (when root
    (dolist (buffer (buffer-list))
      (let ((file (buffer-file-name buffer)))
        (when (and file (string-prefix-p root file))
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (ignore-errors (kill-buffer buffer)))))
    (when (file-exists-p root)
      (delete-directory root t))))
"####;

fn zenburn_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ZENBURN_THEME_MELPA_PIN, "zenburn-theme.el")
        .expect("prepare pinned zenburn-theme source below ./tmp")
        .with_melpa_dependency(RAINBOW_MODE_SOURCE_PIN)
        .expect("prepare pinned rainbow-mode integration source below ./tmp")
        .with_prelude(ZENBURN_THEME_TEST_PRELUDE)
        .with_timeout(ZENBURN_THEME_TEST_TIMEOUT)
}

#[test]
fn zenburn_theme_package_batch() {
    assert_oracle_batch_cases(
        zenburn_theme_oracle(),
        "zenburn_theme_package_batch",
        "zenburn_theme_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
