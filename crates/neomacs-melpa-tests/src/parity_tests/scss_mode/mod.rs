use std::time::Duration;

use crate::{CachedMelpaOracle, SCSS_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const SCSS_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const SCSS_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
;; This 2018 package uses GNU's legacy Flymake API and defines `scss-mode'
;; before referencing its `css-mode' parent.  Load both GNU compatibility
;; surfaces first so their later autoloads cannot replace the package mode.
(require 'flymake-proc)
(require 'css-mode)
(require 'scss-mode)

(defmacro neomacs-scss-test-with-isolated-globals (&rest body)
  "Run BODY without retaining SCSS Mode's global CSS and compiler mutations."
  `(let ((css-mode-syntax-table (copy-syntax-table css-mode-syntax-table))
         (compilation-error-regexp-alist
          (copy-tree compilation-error-regexp-alist)))
     ,@body))

(defun neomacs-scss-test-root (name)
  "Return a clean deterministic sandbox directory named NAME."
  (let ((root (file-name-as-directory
               (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-scss-test-normalize (value)
  "Replace this editor's sandbox root in VALUE with a stable marker."
  (replace-regexp-in-string
   (regexp-quote
    (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
   "<ROOT>/" value t t))

(defun neomacs-scss-test-write-checker (file trailing-newline)
  "Write a deterministic Sass checker to FILE.
When TRAILING-NEWLINE is non-nil, terminate its diagnostic with a newline."
  (with-temp-file file
    (insert "#!/bin/sh\n"
            "log=$1\n"
            "shift\n"
            "printf '%s\\n' \"$@\" > \"$log\"\n"
            "last=\n"
            "for arg do last=$arg; done\n"
            "sleep 0.05\n"
            (if trailing-newline
                "printf 'Syntax error: invalid value\\n        on line 2 of %s\\n' \"$last\"\n"
              "printf 'Syntax error: invalid value\\n        on line 2 of %s' \"$last\"\n")
            "exit 1\n"))
  (set-file-modes file #o755))

(defun neomacs-scss-test-face-at (needle)
  "Return NEEDLE and its effective font-lock face."
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (list needle
          (or (get-text-property (match-beginning 0) 'face)
              (get-text-property (match-beginning 0) 'font-lock-face)))))

(defun neomacs-scss-test-syntax-at (needle)
  "Return string/comment syntax state at the middle of NEEDLE."
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (goto-char (- (point) (/ (length needle) 2)))
    (let ((state (syntax-ppss)))
      (list needle :string (nth 3 state) :comment (nth 4 state)))))
"##;

fn scss_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SCSS_MODE_MELPA_PIN, "scss-mode.el")
        .expect("prepare exact shallow scss-mode source below ./tmp")
        .with_prelude(SCSS_MODE_TEST_PRELUDE)
        .with_timeout(SCSS_MODE_TEST_TIMEOUT)
}

fn scss_mode_default_load_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SCSS_MODE_MELPA_PIN, "scss-mode.el")
        .expect("prepare exact shallow scss-mode source below ./tmp")
        .with_timeout(SCSS_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed SCSS Mode parity test")
        .into()
}

fn assert_scss_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        scss_mode_oracle(),
        &current_test_name(),
        "scss_mode_parity",
        cases,
    );
}

#[test]
fn scss_mode_package_batch() {
    assert_scss_mode_batch(&workflows::workflow_batch_cases());
}

#[test]
fn scss_mode_default_load_batch() {
    assert_oracle_batch_cases(
        scss_mode_default_load_oracle(),
        &current_test_name(),
        "scss_mode_default_load_parity",
        &[workflows::default_load_case()],
    );
}
