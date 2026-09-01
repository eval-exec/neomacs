use std::time::Duration;

use crate::{ANAPHORA_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANAPHORA_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Fixture data shared by the workflows.
///
/// anaphora is a macro library, so the user value is what the macros mean:
/// what `it' is bound to, where it is visible, how often the tested form is
/// evaluated, and what the whole form returns.  The workflows therefore write
/// the kind of code the macros exist for -- reading a nested project structure,
/// walking into it, classifying its records, summing a tree -- and pin the
/// complete result, the evaluation order recorded by a counter, and the errors
/// the edges produce.
///
/// `anaphora-test-projects' is the data every workflow works over: two
/// projects, one fully populated and one with a missing owner e-mail and no
/// tasks, so that both the found and the missing path through every macro is a
/// real lookup rather than a literal nil.
const ANAPHORA_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst anaphora-test-projects
  '((:name "neomacs"
     :owner (:login "eval-exec" :email "exec@example.com")
     :tasks ((:id 1 :title "port isearch" :points 5 :state done)
             (:id 2 :title "fix the collector" :points 8 :state open)
             (:id 3 :title "write docs" :points nil :state open)))
    (:name "scratch"
     :owner (:login "nobody")
     :tasks nil)))

(defun anaphora-test-project (name)
  "Find the project called NAME, the way a caller would."
  (cl-find name anaphora-test-projects
           :key (lambda (project) (plist-get project :name))
           :test #'equal))

(defun anaphora-test-tasks (name)
  (plist-get (anaphora-test-project name) :tasks))
"##;

fn anaphora_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANAPHORA_MELPA_PIN, "anaphora.el")
        .expect("prepare pinned anaphora source below ./tmp")
        .with_prelude(ANAPHORA_TEST_PRELUDE)
        .with_timeout(ANAPHORA_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed anaphora parity test")
        .into()
}

/// Multi-probe batch for `assert_anaphora_parity` cases (2a).
pub(crate) fn assert_anaphora_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(anaphora_oracle(), &name, "anaphora_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn anaphora_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_anaphora_batch(&cases);
}

// END generated package batch tests
