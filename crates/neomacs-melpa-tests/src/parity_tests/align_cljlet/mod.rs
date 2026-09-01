use std::time::Duration;

use crate::{ALIGN_CLJLET_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALIGN_CLJLET_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// align-cljlet is one interactive command over a real Clojure buffer, so every
/// workflow writes a `.clj' file into the per-case sandbox, visits it with
/// `find-file-noselect' (which picks `clojure-mode' from the dependency
/// closure), puts point inside the form the way a user would, and compares the
/// buffer text before and after.
///
/// Alignment is whitespace, so a fixture that is one space off would hide the
/// very thing under test.  The fixtures below are therefore written with each
/// binding deliberately *unaligned* by a different amount, and every workflow
/// pins the complete buffer text rather than a column number, so a wrong column
/// shows up as text rather than as an arithmetic mistake shared between the
/// fixture and the assertion.
///
/// Point cannot keep its numeric position when text is inserted before it, so
/// what the workflows pin is the property a user cares about: point still sits
/// at the same place in the same line, reported as the line number and the text
/// to its left.
const ALIGN_CLJLET_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'clojure-mode)

(defun acl-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun acl-test-write (name text)
  "Write TEXT to sandbox file NAME and return its path."
  (let ((path (acl-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun acl-test-open (name text)
  "Visit a sandbox Clojure file holding TEXT and display it."
  (let ((buffer (find-file-noselect (acl-test-write name text))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defun acl-test-text ()
  (buffer-substring-no-properties (point-min) (point-max)))

(defun acl-test-where ()
  "Where point sits, in terms that survive text being inserted before it."
  (list :line (line-number-at-pos)
        :before-point (buffer-substring-no-properties (line-beginning-position)
                                                      (point))))

(defmacro acl-test-with-file (name text needle &rest body)
  "Visit NAME holding TEXT, put point after NEEDLE, then run BODY."
  `(let ((buffer (acl-test-open ,name ,text)))
     (unwind-protect
         (progn
           (goto-char (point-min))
           (search-forward ,needle)
           ,@body)
       (with-current-buffer buffer
         (set-buffer-modified-p nil))
       (kill-buffer buffer))))

(defun acl-test-align ()
  "Run the command, reporting an error instead of letting it escape."
  (condition-case failure
      (progn (align-cljlet) 'aligned)
    (error failure)))

(defun acl-test-visible-characters (text)
  "Return TEXT with all whitespace removed, to prove nothing was lost."
  (replace-regexp-in-string "[ \t\n]" "" text))
"##;

fn align_cljlet_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALIGN_CLJLET_MELPA_PIN, "align-cljlet.el")
        .expect("prepare pinned align-cljlet source below ./tmp")
        .with_prelude(ALIGN_CLJLET_TEST_PRELUDE)
        .with_timeout(ALIGN_CLJLET_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed align-cljlet parity test")
        .into()
}

/// Multi-probe batch for `assert_align_cljlet_parity` cases (2a).
pub(crate) fn assert_align_cljlet_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(align_cljlet_oracle(), &name, "align_cljlet_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn align_cljlet_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_align_cljlet_batch(&cases);
}

// END generated package batch tests
