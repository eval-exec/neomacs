use std::time::Duration;

use crate::{ASSESS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ASSESS_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// assess is a support library for writing ERT tests: it compares buffers,
/// strings and files, builds throwaway file hierarchies, checks indentation
/// against a major mode and checks which face is applied where.  Its real
/// product is not the boolean -- it is the explanation ERT prints when the
/// comparison fails, which is registered as an `ert-explainer' on each
/// predicate.
///
/// The workflows therefore use it the way a test author does: they define real
/// `ert-deftest's, run them with `ert-run-test', and assert both the outcome
/// and the explanation.  Nothing is stubbed; every file and buffer below is
/// real.
///
/// One normalisation is unavoidable.  When two strings differ, assess writes
/// each side to a temporary file and runs `diff' over them, so the explanation
/// carries two generated file names and each file's modification time.
/// `assess-test-scrub' rewrites those two header lines and nothing else -- the
/// diff body, which is the part that says what differs, is asserted exactly as
/// produced.  It has to run on the string before anything prints it: the
/// printer escapes control characters, so the tab inside a diff header stops
/// being a tab once it has been through `format'.
const ASSESS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'ert)

(setq make-backup-files nil create-lockfiles nil)

(defun assess-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun assess-test-read (path)
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents path))
    (buffer-string)))

(defun assess-test-scrub (text)
  "Normalise the parts of an `assess' explanation that cannot repeat.
When two strings differ, assess writes each side to a temporary file and
runs `diff' over them, so the explanation carries two file names with a
random component and each file's modification time.  Only those two header
lines are rewritten; the diff body, which is the part that says what
differs, is left exactly as produced."
  ;; The two header lines are the only places with a tab, so requiring
  ;; one keeps the diff body's own `*** 1,2 ****' markers intact.  The
  ;; first header is not at the start of a line -- assess writes it
  ;; directly after "Differ at:" -- so this cannot be anchored.
  (replace-regexp-in-string
   "\\(\\*\\*\\*\\|---\\) [^\t\n]+\t[^\n]+"
   "\\1 <FILE> <TIME>"
   text))

;; --- running real ERT tests --------------------------------------
;; assess exists to be used from `ert-deftest', and its whole value on
;; a failure is the explanation ERT prints.  These helpers run real ERT
;; tests and return what a user would see.

(defun assess-test-run (name)
  "Run the ERT test NAME and report what a user would see."
  (let* ((result (ert-run-test (ert-get-test name)))
         (passed (ert-test-passed-p result)))
    (if passed
        (list :name name :passed t)
      (let* ((condition (ert-test-result-with-condition-condition result))
             (data (cadr condition)))
        (list :name name
              :passed nil
              :error (car condition)
              ;; The explanation is scrubbed as a string, before
              ;; anything prints it: the printer escapes control
              ;; characters, so a tab inside the diff header stops
              ;; being a tab once it has been through `format'.
              :explanation (let ((explanation (plist-get (cdr data) :explanation)))
                             (and (stringp explanation)
                                  (assess-test-scrub explanation)))
              :value (plist-get (cdr data) :value))))))

(defmacro assess-test-deftest (name &rest body)
  `(progn (ert-set-test ',name
                        (make-ert-test :name ',name
                                       :body (lambda () ,@body)))
          ',name))

(defun assess-test-buffers ()
  (sort (mapcar #'buffer-name (buffer-list)) #'string<))

(defmacro assess-test-outcome (&rest body)
  `(condition-case error (list :ok (progn ,@body))
     (error (list :error (car error) (cdr error)))))

(defun assess-test-plain (value)
  "VALUE without text properties, when it is a string."
  (if (stringp value) (substring-no-properties value) value))
"##;

fn assess_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASSESS_MELPA_PIN, "assess.el")
        .expect("prepare pinned assess source and dependencies below ./tmp")
        .with_prelude(ASSESS_TEST_PRELUDE)
        .with_timeout(ASSESS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed assess parity test").into()
}

/// Multi-probe batch for `assert_assess_parity` cases (2a).
pub(crate) fn assert_assess_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(assess_oracle(), &name, "assess_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn assess_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_assess_batch(&cases);
}

// END generated package batch tests
