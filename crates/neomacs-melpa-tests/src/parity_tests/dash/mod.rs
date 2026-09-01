use std::time::Duration;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const DASH_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// dash is a list library with no boundary of any kind, so the workflows are
/// what the standards use it as their own example of: a realistic dataset
/// carried through a composed public pipeline, asserting the complete
/// resulting structure, its ordering, and the state of the input afterwards.
///
/// The input matters here more than in most suites.  Several dash operations
/// share structure with their argument and a handful mutate it outright, so
/// every workflow reports the source data after running as well as the
/// result -- a pipeline that quietly rewrites the caller's list is a defect
/// no return value would show.
const DASH_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst dash-test-orders
  '((:id 1041 :customer "Ada"    :region north :cents 4200 :items ("book" "pen"))
    (:id 1042 :customer "Grace"  :region south :cents  950 :items ("pen"))
    (:id 1043 :customer "Ada"    :region north :cents 1875 :items ("desk"))
    (:id 1044 :customer "Katherine" :region east :cents 12300 :items ("desk" "lamp" "rug"))
    (:id 1045 :customer "Grace"  :region south :cents 4200 :items ())
    (:id 1046 :customer "Ada"    :region east  :cents  600 :items ("pen" "pen")))
  "A small order book: repeated customers, repeated totals, one empty list.")

(defun dash-test-plain (value)
  "Return VALUE with every string freshly copied, so nothing prints shared."
  (cond ((stringp value) (substring-no-properties value))
        ((consp value)
         (cons (dash-test-plain (car value)) (dash-test-plain (cdr value))))
        (t value)))

(defun dash-test-fresh-orders ()
  "Return a private deep copy of the order book to hand to a pipeline."
  (copy-tree dash-test-orders))

(defmacro dash-test-on-fresh (&rest body)
  "Run BODY with `orders' bound to a private copy of the order book.
Report what BODY returned and what it left the copy looking like, so an
operation that rewrites its argument cannot hide behind its return value."
  `(let* ((orders (dash-test-fresh-orders))
          (before (copy-tree orders))
          (result (progn ,@body)))
     (list :result (dash-test-plain result)
           :source-unchanged (equal orders before)
           :source (dash-test-plain orders))))
"##;

fn dash_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DASH_MELPA_PIN, "dash.el")
        .expect("prepare pinned Dash source below ./tmp")
        .with_prelude(DASH_TEST_PRELUDE)
        .with_timeout(DASH_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed Dash parity test").into()
}

/// Multi-probe batch for `assert_dash_parity` cases (2a).
pub(crate) fn assert_dash_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(dash_oracle(), &name, "dash_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn dash_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_dash_batch(&cases);
}

// END generated package batch tests
