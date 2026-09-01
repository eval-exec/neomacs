use std::time::Duration;

use crate::{AC_JS2_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_JS2_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ac-js2 derives JavaScript completions from js2-mode's parse tree of the
/// buffer, and optionally from a browser reached through skewer.  The js2 half
/// runs for real in these workflows: real `.js` files below the sandbox, really
/// visited in `js2-mode`, really parsed, and completed through the
/// `completion-at-point` entry point the commentary documents for users without
/// auto-complete (which is not part of ac-js2's dependency closure).
///
/// The browser is the one true external boundary.  Where a workflow needs it,
/// skewer's own transport function `skewer-eval-synchronously` is replaced by a
/// recorder that reports what the browser was asked and answers with a
/// realistic result; ac-js2 keeps running its own callback, merging and
/// formatting.
const AC_JS2_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'js2-mode)

(setq make-backup-files nil
      create-lockfiles nil
      js2-mode-show-parse-errors nil
      js2-mode-show-strict-warnings nil)

(defvar ac-js2-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defconst ac-js2-test-source
  "/* Utilities for the demo app. */
var greeting = \"Grüße\";

// Returns a polite greeting for NAME.
function greet(name, punctuation) {
    return greeting + \", \" + name + punctuation;
}

var config = {
    locale: \"de-DE\",
    retries: 3,
    onError: function (error) { return error; }
};

var shout = function (text) {
    return text.toUpperCase();
};

function main() {
    var visitor = \"Ada\";
    greet(visitor, \"!\");
    shout(config.locale);
}
")

(defun ac-js2-test-write (name text)
  (let ((path (expand-file-name name ac-js2-test-root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defmacro ac-js2-test-in-source (text &rest body)
  "Visit a real .js file holding TEXT in a window-displayed js2-mode buffer."
  `(let* ((path (ac-js2-test-write "app/main.js" ,text))
          (buffer (find-file-noselect path)))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (js2-mode)
           (js2-reparse)
           (ac-js2-mode 1)
           ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer (set-buffer-modified-p nil))
         (kill-buffer buffer)))))

(defun ac-js2-test-extras ()
  "Every candidate ac-js2 appends that does not come from the buffer."
  (append (mapcar #'symbol-name js2-keywords)
          js2-ecma-262-externs
          js2-browser-externs))

(defun ac-js2-test-local-candidates (candidates)
  "CANDIDATES that were derived from the parsed buffer, in order."
  (let ((extras (ac-js2-test-extras)))
    (cl-remove-if (lambda (name) (member name extras)) candidates)))

(defun ac-js2-test-completion ()
  "Run the documented `completion-at-point' entry point and report it."
  (let ((result (ac-js2-completion-function)))
    (list :beg (nth 0 result)
          :end (nth 1 result)
          :locals (ac-js2-test-local-candidates (nth 2 result))
          :total (length (nth 2 result))
          :point (point))))

(defun ac-js2-test-docs (names)
  (mapcar (lambda (name) (cons name (ac-js2-document name))) names))

(defun ac-js2-test-point-in (context word)
  "Put point inside WORD of the line matching CONTEXT and return it."
  (goto-char (point-min))
  (search-forward context)
  (search-backward word)
  (forward-char 2)
  (point))
"##;

fn ac_js2_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_JS2_MELPA_PIN, "ac-js2.el")
        .expect("prepare pinned ac-js2 source below ./tmp")
        .with_prelude(AC_JS2_TEST_PRELUDE)
        .with_timeout(AC_JS2_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed ac-js2 parity test").into()
}

/// Multi-probe batch for `assert_ac_js2_parity` cases (2a).
pub(crate) fn assert_ac_js2_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_js2_oracle(), &name, "ac_js2_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_js2_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_js2_batch(&cases);
}

// END generated package batch tests
