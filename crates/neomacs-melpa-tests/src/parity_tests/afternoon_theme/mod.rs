use std::time::Duration;

use crate::{AFTERNOON_THEME_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AFTERNOON_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// afternoon differs from the other dark themes in this suite in two ways that
/// the workflows are built around: it picks its background palette at load
/// time from `(display-color-cells (selected-frame))', and it sets six
/// variables through `custom-theme-set-variables' as well as its faces.
///
/// Every face is guarded by ((class color) (min-colors 89)), which a batch
/// job's display never satisfies, so the prelude answers that display question
/// the way a real colour terminal answers it.  `display-color-cells' is left
/// under the workflow's control instead of being pinned, because the 256-colour
/// branch is exactly what one workflow is about.
const AFTERNOON_THEME_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'ansi-color)
(require 'vc-annotate)

;; afternoon themes `fci-rule-color', which belongs to fill-column-indicator.
;; Declare it the way that package does so the theme's value has somewhere to
;; land; the other five variables it sets are real Emacs variables.
(defvar fci-rule-color "unconfigured")

;; afternoon guards every face with ((class color) (min-colors 89)), which a
;; batch job's display never satisfies, and it *also* picks its background
;; palette at load time from `(display-color-cells (selected-frame))'.  So the
;; display spec is answered the way a real colour terminal answers it, while
;; `display-color-cells' stays under the workflow's control -- that branch is
;; the thing one of the workflows is about.
(defvar aft-test-color-cells 16777216
  "What `display-color-cells' reports while a workflow runs.")

(fset 'display-color-cells
      (lambda (&optional _display)
        aft-test-color-cells))
(defvar neomacs-melpa-tests--original-face-spec-set-match-display
  (symbol-function 'face-spec-set-match-display))
(fset 'face-spec-set-match-display
      (lambda (display frame)
        (if (equal display '((class color) (min-colors 89)))
            t
          (funcall neomacs-melpa-tests--original-face-spec-set-match-display
                   display frame))))

(defun aft-test-face-state (faces)
  "Resolved appearance of FACES, following `:inherit'."
  (mapcar
   (lambda (face)
     (if (not (facep face))
         (list :face face :defined nil)
       (list :face face
             :defined t
             :foreground (face-attribute face :foreground nil t)
             :background (face-attribute face :background nil t)
             :weight (face-attribute face :weight nil t)
             :slant (face-attribute face :slant nil t)
             :underline (face-attribute face :underline nil t)
             :box (copy-tree (face-attribute face :box nil nil))
             :inherit (copy-tree (face-attribute face :inherit nil nil)))))
   faces))

(defun aft-test-primary-face (value)
  (cond ((and (symbolp value) (facep value)) value)
        ((listp value) (cl-find-if (lambda (c) (and (symbolp c) (facep c))) value))))

(defun aft-test-token-state (tokens)
  "Face and the colours that face resolves to at each TOKEN."
  (save-excursion
    (mapcar
     (lambda (token)
       (goto-char (point-min))
       (search-forward token)
       (let* ((position (match-beginning 0))
              (displayed (get-char-property position 'face))
              (font-lock (get-text-property position 'font-lock-face))
              (face (aft-test-primary-face (or displayed font-lock))))
         (list :token token
               :face (copy-tree displayed)
               :foreground (and face (face-attribute face :foreground nil t))
               :weight (and face (face-attribute face :weight nil t))
               :slant (and face (face-attribute face :slant nil t)))))
     tokens)))

(defun aft-test-variables (names)
  (mapcar (lambda (name)
            (list name (if (boundp name) (copy-tree (symbol-value name)) :unbound)))
          names))

(defun aft-test-cleanup (root)
  (dolist (theme '(afternoon neomacs-afternoon-baseline))
    (when (custom-theme-enabled-p theme) (disable-theme theme)))
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer)))
      (when (and file (string-prefix-p root file))
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (ignore-errors (kill-buffer buffer)))))
  (when (file-exists-p root) (delete-directory root t)))
"####;

fn afternoon_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AFTERNOON_THEME_MELPA_PIN, "afternoon-theme.el")
        .expect("prepare pinned afternoon-theme source below ./tmp")
        .with_prelude(AFTERNOON_THEME_TEST_PRELUDE)
        .with_timeout(AFTERNOON_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed afternoon-theme parity test")
        .into()
}

/// Multi-probe batch for `assert_afternoon_theme_parity` cases (2a).
pub(crate) fn assert_afternoon_theme_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        afternoon_theme_oracle(),
        &name,
        "afternoon_theme_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn afternoon_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_afternoon_theme_batch(&cases);
}

// END generated package batch tests
