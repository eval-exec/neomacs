use std::time::Duration;

use crate::{CachedMelpaOracle, ZEN_AND_ART_THEME_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ZEN_AND_ART_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);

const ZEN_AND_ART_THEME_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defun neomacs-zen-and-art-test-primary-face (value)
  "Return the first real face named by a `face' text-property VALUE."
  (cond
   ((and (symbolp value) (facep value)) value)
   ((listp value)
    (cl-find-if
     (lambda (candidate)
       (and (symbolp candidate) (facep candidate)))
     value))))

(defun neomacs-zen-and-art-test-face-state (faces)
  "Return raw and default-resolved appearance for each face in FACES."
  (mapcar
   (lambda (face)
     (if (not (facep face))
         (list :face face :defined nil)
       (list
        :face face
        :defined t
        :foreground (face-attribute face :foreground nil nil)
        :background (face-attribute face :background nil nil)
        :resolved-foreground
        (face-attribute face :foreground nil 'default)
        :resolved-background
        (face-attribute face :background nil 'default)
        :weight (face-attribute face :weight nil t)
        :slant (face-attribute face :slant nil t)
        :underline (face-attribute face :underline nil t)
        :inherit (copy-tree (face-attribute face :inherit nil nil)))))
   faces))

(defun neomacs-zen-and-art-test-token-state (tokens)
  "Return each TOKEN's font-lock face plus raw and resolved colours."
  (save-excursion
    (mapcar
     (lambda (token)
       (goto-char (point-min))
       (search-forward token)
       (let* ((position (match-beginning 0))
              (displayed (get-char-property position 'face))
              (font-lock (get-text-property position 'font-lock-face))
              (face
               (neomacs-zen-and-art-test-primary-face
                (or displayed font-lock))))
         (list
          :token token
          :face (copy-tree displayed)
          :font-lock-face (copy-tree font-lock)
          :foreground (and face (face-attribute face :foreground nil nil))
          :background (and face (face-attribute face :background nil nil))
          :resolved-foreground
          (and face (face-attribute face :foreground nil 'default))
          :resolved-background
          (and face (face-attribute face :background nil 'default))
          :weight (and face (face-attribute face :weight nil t))
          :slant (and face (face-attribute face :slant nil t)))))
     tokens)))

(defun neomacs-zen-and-art-test-recorded-face-settings ()
  "Return every face specification recorded by the installed theme."
  (sort
   (mapcar
    (lambda (setting)
      (list (nth 1 setting) (copy-tree (nth 3 setting))))
    (get 'zen-and-art 'theme-settings))
   (lambda (left right)
     (string< (symbol-name (car left)) (symbol-name (car right))))))

(defun neomacs-zen-and-art-test-cleanup (root)
  (dolist (theme '(zen-and-art neomacs-zen-and-art-baseline))
    (when (custom-theme-enabled-p theme)
      (disable-theme theme)))
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer)))
      (when (and file (string-prefix-p root file))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (ignore-errors (kill-buffer buffer)))))
  (when (file-exists-p root)
    (delete-directory root t)))
"####;

fn zen_and_art_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ZEN_AND_ART_THEME_MELPA_PIN, "zen-and-art-theme.el")
        .expect("prepare pinned zen-and-art-theme source below ./tmp")
        .with_prelude(ZEN_AND_ART_THEME_TEST_PRELUDE)
        .with_timeout(ZEN_AND_ART_THEME_TEST_TIMEOUT)
}

#[test]
fn zen_and_art_theme_package_batch() {
    assert_oracle_batch_cases(
        zen_and_art_theme_oracle(),
        "zen_and_art_theme_package_batch",
        "zen_and_art_theme_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
