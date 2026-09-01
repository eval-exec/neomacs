use std::time::Duration;

use crate::{ACADEMIC_PHRASES_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACADEMIC_PHRASES_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// academic-phrases only reaches the user through `completing-read`, which is
/// a genuine interactive boundary, so the workflows script the answers and let
/// every other part of the package - category lookup, phrase listing,
/// placeholder rendering, choice substitution and buffer insertion - run for
/// real.  The scripted reader refuses an answer the package did not actually
/// offer, so a wrong or empty collection fails the test instead of passing
/// silently.
const ACADEMIC_PHRASES_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar academic-test-prompts nil
  "Each `completing-read' the package issued, as (PROMPT CANDIDATE-COUNT).")

(defun academic-test-candidates (collection)
  (mapcar (lambda (candidate)
            (if (consp candidate) (car candidate) candidate))
          collection))

(defun academic-test-answer (answers body)
  "Call BODY, answering each `completing-read' with the next of ANSWERS.
Signal if an answer was never offered, and record every prompt in
`academic-test-prompts'."
  (setq academic-test-prompts nil)
  (let ((remaining answers))
    (cl-letf (((symbol-function 'completing-read)
               (lambda (prompt collection &rest _ignored)
                 (let* ((answer (pop remaining))
                        (candidates (academic-test-candidates collection))
                        (answer (if (functionp answer)
                                    (funcall answer candidates)
                                  answer)))
                   (push (list prompt (length candidates)) academic-test-prompts)
                   (unless (member answer candidates)
                     (error "academic-phrases never offered %S at %S" answer prompt))
                   answer))))
      (funcall body))))

(defun academic-test-prompts ()
  (reverse academic-test-prompts))

(defun academic-test-offered (answers body)
  "Call BODY like `academic-test-answer', returning the offered candidates."
  (let (offered)
    (setq academic-test-prompts nil)
    (let ((remaining answers))
      (cl-letf (((symbol-function 'completing-read)
                 (lambda (prompt collection &rest _ignored)
                   (let* ((answer (pop remaining))
                          (candidates (academic-test-candidates collection))
                          (answer (if (functionp answer)
                                      (funcall answer candidates)
                                    answer)))
                     (push (list prompt candidates) offered)
                     (unless (member answer candidates)
                       (error "academic-phrases never offered %S at %S" answer prompt))
                     answer))))
        (funcall body)))
    (reverse offered)))
"##;

fn academic_phrases_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACADEMIC_PHRASES_MELPA_PIN, "academic-phrases.el")
        .expect("prepare pinned academic-phrases source below ./tmp")
        .with_prelude(ACADEMIC_PHRASES_TEST_PRELUDE)
        .with_timeout(ACADEMIC_PHRASES_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed academic-phrases parity test")
        .into()
}

/// Multi-probe batch for `assert_academic_phrases_parity` cases (2a).
pub(crate) fn assert_academic_phrases_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        academic_phrases_oracle(),
        &name,
        "academic_phrases_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn academic_phrases_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_academic_phrases_batch(&cases);
}

// END generated package batch tests
