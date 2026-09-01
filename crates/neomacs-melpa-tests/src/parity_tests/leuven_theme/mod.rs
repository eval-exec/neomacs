use std::time::Duration;

use crate::{CachedMelpaOracle, LEUVEN_THEME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const LEUVEN_THEME_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const LEUVEN_THEME_TEST_PRELUDE: &str = r####"
(require 'ansi-color)
(require 'cl-lib)
(require 'org-habit)

;; The theme integrates with the optional highlight-sexp package by setting
;; this public variable.  Give that consumer a deterministic pre-theme value
;; so public enable/disable precedence and restoration are observable.
(defvar hl-sexp-background-color "neomacs-leuven-baseline")

(defconst neomacs-leuven-test-unconditional-faces
  '((org-habit-clear-face :foreground :background)
    (org-habit-clear-future-face :foreground :background)
    (org-habit-ready-face :foreground :background)
    (org-habit-ready-future-face :foreground :background)
    (org-habit-alert-face :foreground :background)
    (org-habit-alert-future-face :foreground :background)
    (org-habit-overdue-face :foreground :background)
    (org-habit-overdue-future-face :foreground :background))
  "All unconditional faces shared by the light and dark themes.")

(defun neomacs-leuven-test-copy (value)
  "Copy VALUE recursively, including strings and vectors."
  (cond ((consp value)
         (cons (neomacs-leuven-test-copy (car value))
               (neomacs-leuven-test-copy (cdr value))))
        ((vectorp value)
         (apply #'vector
                (mapcar #'neomacs-leuven-test-copy (append value nil))))
        ((stringp value) (copy-sequence value))
        (t value)))

(defun neomacs-leuven-test-face (face attributes)
  "Return direct and fully resolved ATTRIBUTES for FACE on the real frame."
  (list
   face
   :direct
   (mapcar (lambda (attribute)
             (cons attribute
                   (neomacs-leuven-test-copy
                    (face-attribute face attribute nil nil))))
           attributes)
   :resolved
   (mapcar (lambda (attribute)
             (cons attribute
                   (neomacs-leuven-test-copy
                    (face-attribute face attribute nil 'default))))
           attributes)))

(defun neomacs-leuven-test-disable-themes ()
  "Disable both public themes, newest first."
  (dolist (theme '(leuven-dark leuven))
    (when (custom-theme-enabled-p theme)
      (disable-theme theme))))

(defun neomacs-leuven-test-face-list (specs)
  "Return direct and resolved face state for every entry in SPECS."
  (mapcar (lambda (spec)
            (neomacs-leuven-test-face (car spec) (cdr spec)))
          specs))

(defun neomacs-leuven-test-variable-state (symbol)
  "Return SYMBOL's binding and copied value without conflating nil and void."
  (if (boundp symbol)
      (list :bound t :value
            (neomacs-leuven-test-copy (symbol-value symbol)))
    '(:bound nil)))

(defun neomacs-leuven-test-restore-variable (symbol state)
  "Restore SYMBOL to the exact binding STATE returned by the state helper."
  (if (plist-get state :bound)
      (set symbol (neomacs-leuven-test-copy (plist-get state :value)))
    (makunbound symbol)))

(defmacro neomacs-leuven-test-isolated (&rest body)
  "Run BODY with no Leuven theme enabled and prove unconditional cleanup."
  (declare (indent 0) (debug t))
  `(progn
     (when (or (custom-theme-enabled-p 'leuven)
               (custom-theme-enabled-p 'leuven-dark))
       (error "Leuven theme leaked into the next workflow"))
     (let ((original-enabled (copy-sequence custom-enabled-themes))
           (original-ansi-faces
            (neomacs-leuven-test-copy ansi-color-faces-vector))
           (original-ansi-names
            (neomacs-leuven-test-copy ansi-color-names-vector))
           (original-hl-sexp
            (neomacs-leuven-test-variable-state
             'hl-sexp-background-color)))
       (unwind-protect
           (progn ,@body)
         (neomacs-leuven-test-disable-themes)
         (setq ansi-color-faces-vector original-ansi-faces
               ansi-color-names-vector original-ansi-names)
         (neomacs-leuven-test-restore-variable
          'hl-sexp-background-color original-hl-sexp)
         (unless (equal custom-enabled-themes original-enabled)
           (error "Leuven cleanup left enabled themes: %S"
                  custom-enabled-themes))))))
"####;

pub(crate) fn leuven_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LEUVEN_THEME_MELPA_PIN, "leuven-theme.el")
        .expect("prepare exact Leuven Theme source below ./tmp")
        .with_prelude(LEUVEN_THEME_TEST_PRELUDE)
        .with_timeout(LEUVEN_THEME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Leuven Theme parity test")
        .into()
}

fn assert_leuven_theme_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        leuven_theme_oracle(),
        &current_test_name(),
        "leuven_theme_parity",
        cases,
    );
}

#[test]
fn leuven_theme_package_batch() {
    assert_leuven_theme_batch(&workflows::public_workflow_cases());
}
