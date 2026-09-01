use std::time::Duration;

use crate::{AMPLE_REGEXPS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AMPLE_REGEXPS_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// `define-arx' turns a list of named forms into an `rx'-like macro, so the
/// product is a regexp string and that is what the workflows pin -- whole,
/// never "does it match this example", because a regexp that matches the
/// example can still be wrong.
///
/// The fixture is one realistic log-line grammar defined once in the prelude:
/// literal, `regexp', alias and sub-form definitions, one named form defined in
/// terms of an earlier one, and a `:func' form with an arity range.  It is
/// defined through `eval' with lexical binding because that is the only spelling
/// of `:func' that works on this Emacs -- see the last workflow, which pins why
/// the two spellings the docstring suggests do not.
const AMPLE_REGEXPS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun arx-test-define-log-rx ()
  "Define the fixture arx macro `log-rx' and return its name."
  (eval '(define-arx log-rx
           `((ws (regexp "[ \t]+"))
             (level (or "DEBUG" "INFO" "WARN" "ERROR"))
             (ident (regexp "[A-Za-z_][A-Za-z0-9_]*"))
             (qualified (seq ident (* "." ident)))
             (stamp (seq (= 4 digit) "-" (= 2 digit) "-" (= 2 digit)))
             (bracketed (:func ,(lambda (form &rest args)
                                  (rx-to-string `(seq "[" (seq ,@args) "]") t))
                               :min-args 1 :max-args 2))))
        t))

(defun arx-test-wrap (form &rest args)
  "A named function of the shape a `:func' form is documented to take."
  (rx-to-string `(seq "<" (seq ,@args) ">") t))

(defun arx-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun arx-test-write (name text)
  (let ((path (arx-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun arx-test-expand (form)
  "Return the regexp FORM produces, or the error it signals."
  (condition-case failure
      (eval form t)
    (error failure)))

(defun arx-test-surface (macro)
  "Report what defining MACRO left behind."
  (let ((to-string (intern (concat (symbol-name macro) "-to-string")))
        (bindings (intern (concat (symbol-name macro) "-bindings"))))
    (list :macro (and (fboundp macro) (macrop macro) t)
          :to-string (and (functionp to-string) t)
          :bindings-bound (boundp bindings)
          :arx-name (get macro 'arx-name)
          :to-string-arx-name (get to-string 'arx-name)
          :form-count (length (get macro 'arx-form-defs)))))
"##;

fn ample_regexps_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AMPLE_REGEXPS_MELPA_PIN, "ample-regexps.el")
        .expect("prepare pinned ample-regexps source below ./tmp")
        .with_prelude(AMPLE_REGEXPS_TEST_PRELUDE)
        .with_timeout(AMPLE_REGEXPS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ample-regexps parity test")
        .into()
}

/// Multi-probe batch for `assert_ample_regexps_parity` cases (2a).
pub(crate) fn assert_ample_regexps_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ample_regexps_oracle(), &name, "ample_regexps_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ample_regexps_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ample_regexps_batch(&cases);
}

// END generated package batch tests
