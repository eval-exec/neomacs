use std::time::Duration;

use crate::{CachedMelpaOracle, ZENO_THEME_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ZENO_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const ZENO_THEME_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'cus-face)

(defun neomacs-zeno-test--reload (italics &optional no-enable)
  "Reload Zeno with its documented ITALICS option."
  (when (custom-theme-enabled-p 'zeno)
    (disable-theme 'zeno))
  (let ((zeno-theme-enable-italics italics))
    (load-theme 'zeno t no-enable)))

(defun neomacs-zeno-test--primary-face (value)
  "Return the first actual face named by text-property VALUE."
  (cond
   ((and (symbolp value) (facep value)) value)
   ((listp value)
    (cl-find-if
     (lambda (candidate)
       (and (symbolp candidate) (facep candidate)))
     value))))

(defun neomacs-zeno-test--face-state (faces)
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
        (copy-tree (face-attribute face :box nil t))
        (copy-tree (face-attribute face :inherit nil nil)))))
   faces))

(defun neomacs-zeno-test--token-state (tokens)
  "Return exact font-lock and resolved appearance for TOKENS."
  (save-excursion
    (mapcar
     (lambda (token)
       (goto-char (point-min))
       (search-forward token)
       (let* ((position (match-beginning 0))
              (displayed (get-char-property position 'face))
              (font-lock (get-text-property position 'font-lock-face))
              (face (neomacs-zeno-test--primary-face
                     (or displayed font-lock))))
         (list
          token position (copy-tree displayed) (copy-tree font-lock)
          (and face (face-attribute face :foreground nil 'default))
          (and face (face-attribute face :background nil 'default))
          (and face (face-attribute face :weight nil t))
          (and face (face-attribute face :slant nil t)))))
     tokens)))

(defun neomacs-zeno-test--theme-face-specs (faces)
  "Return Zeno's exact recorded face specs for FACES."
  (mapcar
   (lambda (face)
     (list face (copy-tree (cadr (assq 'zeno (get face 'theme-face))))))
   faces))

(defun neomacs-zeno-test--cleanup (root)
  "Restore themes and remove buffers and files below ROOT."
  (dolist (theme '(zeno neomacs-zeno-baseline))
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

fn zeno_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ZENO_THEME_MELPA_PIN, "zeno-theme.el")
        .expect("prepare pinned zeno-theme source below ./tmp")
        .with_prelude(ZENO_THEME_TEST_PRELUDE)
        .with_timeout(ZENO_THEME_TEST_TIMEOUT)
}

#[test]
fn zeno_theme_package_batch() {
    assert_oracle_batch_cases(
        zeno_theme_oracle(),
        "zeno_theme_package_batch",
        "zeno_theme_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
