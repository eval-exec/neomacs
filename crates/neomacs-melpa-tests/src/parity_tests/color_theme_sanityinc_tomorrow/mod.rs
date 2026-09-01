use std::time::Duration;

use crate::{COLOR_THEME_SANITYINC_TOMORROW_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'cus-face)
(require 'ansi-color)
(require 'vc-annotate)
(require 'color-theme-sanityinc-tomorrow)

(defconst neomacs-tomorrow-test-themes
  '(sanityinc-tomorrow-night sanityinc-tomorrow-day
    sanityinc-tomorrow-eighties sanityinc-tomorrow-blue
    sanityinc-tomorrow-bright))

(defun neomacs-tomorrow-test-cleanup ()
  "Disable every Tomorrow variant and restore a neutral theme state."
  (dolist (theme neomacs-tomorrow-test-themes)
    (when (custom-theme-enabled-p theme) (disable-theme theme))))

(defun neomacs-tomorrow-test-load (variant)
  "Load and enable Tomorrow VARIANT from its public theme file."
  (neomacs-tomorrow-test-cleanup)
  (load-theme (color-theme-sanityinc-tomorrow--theme-name variant) t))

(defun neomacs-tomorrow-test-face-state (faces)
  "Describe raw and inherited attributes of FACES."
  (mapcar
   (lambda (face)
     (list face
           (face-attribute face :foreground nil nil)
           (face-attribute face :background nil nil)
           (face-attribute face :foreground nil 'default)
           (face-attribute face :background nil 'default)
           (face-attribute face :weight nil t)
           (face-attribute face :slant nil t)
           (copy-tree (face-attribute face :underline nil t))
           (face-attribute face :extend nil t)
           (copy-tree (face-attribute face :inherit nil nil))))
   faces))

(defun neomacs-tomorrow-test-token-state (tokens)
  "Describe font-lock and resolved appearance for TOKENS."
  (save-excursion
    (mapcar
     (lambda (token)
       (goto-char (point-min))
       (search-forward token)
       (let* ((position (match-beginning 0))
              (displayed (get-char-property position 'face))
              (face (cond ((and (symbolp displayed) (facep displayed)) displayed)
                          ((listp displayed)
                           (cl-find-if (lambda (item)
                                         (and (symbolp item) (facep item)))
                                       displayed)))))
         (list token position (copy-tree displayed)
               (and face (face-attribute face :foreground nil 'default))
               (and face (face-attribute face :background nil 'default))
               (and face (face-attribute face :weight nil t)))))
     tokens)))

(defun neomacs-tomorrow-test-theme-settings (theme)
  "Return stable registered face and variable counts for THEME."
  (let ((settings (get theme 'theme-settings)))
    (list :faces (cl-count 'theme-face settings :key #'car)
          :variables (cl-count 'theme-value settings :key #'car)
          :immediate (get theme 'theme-immediate))))

(defun neomacs-tomorrow-test-theme-face-specs (theme faces)
  "Return THEME's exact registered specs for FACES."
  (mapcar
   (lambda (face)
     (list face (copy-tree (cadr (assq theme (get face 'theme-face))))))
   faces))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(
        COLOR_THEME_SANITYINC_TOMORROW_MELPA_PIN,
        "color-theme-sanityinc-tomorrow.el",
    )
    .expect("prepare exact shallow Tomorrow theme source below ./tmp")
    .with_prelude(PRELUDE)
    .with_timeout(TEST_TIMEOUT)
}

#[test]
fn color_theme_sanityinc_tomorrow_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "color_theme_sanityinc_tomorrow_package_batch",
        "color_theme_sanityinc_tomorrow_parity",
        &workflows::workflow_batch_cases(),
    );
}
