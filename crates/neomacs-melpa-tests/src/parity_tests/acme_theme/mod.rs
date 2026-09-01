use std::time::Duration;

use crate::{ACME_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACME_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// acme is one light `deftheme' with a single documented option, so every
/// workflow enters through `load-theme' and reads back the appearance a user
/// sees.  The helpers resolve `:inherit' (`face-attribute ... nil t') and read
/// both the `face' and the `font-lock-face' text property, because Emacs only
/// aliases the latter into the former once `font-lock-mode' is on, which a
/// batch job never allows.
const ACME_THEME_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

;; acme guards `region', `mode-line', `hl-line', the `diff-*' and the `term-*'
;; faces with ((class color) (min-colors 89)).  A batch job has no display at
;; all -- `display-color-p' is nil and `display-color-cells' is 0 -- so those
;; specs would never match and the tests would observe the absence of a
;; terminal rather than the theme.  Answer the two display questions the way a
;; real user's colour terminal answers them, and nothing else.
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

(defun neomacs-acme-test-primary-face (value)
  "Return the first real face named by a `face' text-property VALUE."
  (cond
   ((and (symbolp value) (facep value)) value)
   ((listp value)
    (cl-find-if
     (lambda (candidate)
       (and (symbolp candidate) (facep candidate)))
     value))))

(defun neomacs-acme-test-face-state (faces)
  "Resolved appearance of FACES, following `:inherit'."
  (mapcar
   (lambda (face)
     (if (not (facep face))
         (list :face face :defined nil)
       (list
        :face face
        :defined t
        :foreground (face-attribute face :foreground nil t)
        :background (face-attribute face :background nil t)
        :weight (face-attribute face :weight nil t)
        :slant (face-attribute face :slant nil t)
        :underline (face-attribute face :underline nil t)
        :overline (face-attribute face :overline nil t)
        :box (copy-tree (face-attribute face :box nil nil))
        :inherit (copy-tree (face-attribute face :inherit nil nil)))))
   faces))

(defun neomacs-acme-test-token-state (tokens)
  "Face and the colours that face resolves to at each TOKEN in the buffer."
  (save-excursion
    (mapcar
     (lambda (token)
       (goto-char (point-min))
       (search-forward token)
       (let* ((position (match-beginning 0))
              (displayed (get-char-property position 'face))
              (font-lock (get-text-property position 'font-lock-face))
              (face
               (neomacs-acme-test-primary-face
                (or displayed font-lock))))
         (list
          :token token
          :face (copy-tree displayed)
          :font-lock-face (copy-tree font-lock)
          :foreground (and face (face-attribute face :foreground nil t))
          :background (and face (face-attribute face :background nil t))
          :weight (and face (face-attribute face :weight nil t))
          :slant (and face (face-attribute face :slant nil t)))))
     tokens)))

(defun neomacs-acme-test-file-string (file)
  (with-temp-buffer
    (insert-file-contents file)
    (buffer-string)))

(defun neomacs-acme-test-cleanup (root)
  (dolist (theme '(acme neomacs-acme-baseline))
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

fn acme_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACME_THEME_MELPA_PIN, "acme-theme.el")
        .expect("prepare pinned acme-theme source below ./tmp")
        .with_prelude(ACME_THEME_TEST_PRELUDE)
        .with_timeout(ACME_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed acme-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_acme_theme_parity` cases (2a).
pub(crate) fn assert_acme_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(acme_theme_oracle(), &name, "acme_theme_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn acme_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_acme_theme_batch(&cases);
}

// END generated package batch tests
