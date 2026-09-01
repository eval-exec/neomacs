use std::time::Duration;

use crate::{ANTI_ZENBURN_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANTI_ZENBURN_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ANTI_ZENBURN_THEME_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(fset 'display-color-cells
      (lambda (&optional _display)
        16777216))
(defvar neomacs-melpa-tests--original-face-spec-set-match-display
  (symbol-function 'face-spec-set-match-display))
(fset 'face-spec-set-match-display
      (lambda (display frame)
        (if (equal display
                   '((class color)
                     (min-colors 89)))
            t
          (funcall
           neomacs-melpa-tests--original-face-spec-set-match-display
           display
           frame))))

(defun neomacs-anti-zenburn-test-file-string (file)
  (with-temp-buffer
    (insert-file-contents file)
    (buffer-string)))

(defun neomacs-anti-zenburn-test-face-state (faces)
  (mapcar
   (lambda (face)
     (list
      :face face
      :foreground (face-attribute face :foreground nil t)
      :background (face-attribute face :background nil t)
      :weight (face-attribute face :weight nil t)
      :slant (face-attribute face :slant nil t)
      :underline (face-attribute face :underline nil nil)
      :box (face-attribute face :box nil nil)
      :inherit (face-attribute face :inherit nil nil)))
   faces))

(defun neomacs-anti-zenburn-test-token-state (tokens)
  (save-excursion
    (mapcar
     (lambda (token)
       (goto-char (point-min))
       (search-forward token)
       (list
        :token token
        :face
        (copy-tree
         (get-text-property
          (match-beginning 0)
          'face))))
     tokens)))

(defun neomacs-anti-zenburn-test-token-display-state (tokens)
  (save-excursion
    (mapcar
     (lambda (token)
       (goto-char (point-min))
       (search-forward token)
       (let ((position (match-beginning 0)))
         (list
          :token token
          :face
          (copy-tree
           (get-char-property position 'face))
          :font-lock-face
          (copy-tree
           (get-text-property position 'font-lock-face)))))
     tokens)))

(defun neomacs-anti-zenburn-test-cleanup (root)
  (dolist (theme
           '(anti-zenburn
             neomacs-anti-zenburn-baseline))
    (when
        (custom-theme-enabled-p theme)
      (disable-theme theme)))
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer)))
      (when
          (and file
               (string-prefix-p root file))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer))))
  (when
      (file-exists-p root)
    (delete-directory root t)))
"##;

fn anti_zenburn_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANTI_ZENBURN_THEME_MELPA_PIN, "anti-zenburn-theme.el")
        .expect("prepare pinned anti-zenburn-theme source below ./tmp")
        .with_prelude(ANTI_ZENBURN_THEME_TEST_PRELUDE)
        .with_timeout(ANTI_ZENBURN_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed anti-zenburn-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_anti_zenburn_theme_parity` cases (2a).
pub(crate) fn assert_anti_zenburn_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        anti_zenburn_theme_oracle(),
        &name,
        "anti_zenburn_theme_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn anti_zenburn_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_anti_zenburn_theme_batch(&cases);
}

// END generated package batch tests
