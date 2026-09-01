use std::time::Duration;

use crate::{APIWRAP_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod documentation;
mod practical;

const APIWRAP_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// apiwrap generates the client side of a REST API: one macro per HTTP method
/// per backend, and from each use of those macros a wrapper function whose
/// name, argument list, docstring and behaviour are all derived from a
/// resource template like `/repos/:owner.login/:name/issues'.
///
/// Nothing here is stubbed.  The `:request' primitive is not a test double --
/// it is the extension point the package is built around, and every backend
/// has to supply one; `apiwrap-new-backend' refuses to work without it.  The
/// one used below records the method, resolved resource, parameters and data
/// it was handed, which is exactly the surface a real backend would forward to
/// `url-retrieve'.
const APIWRAP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'let-alist)

(defvar apiwrap-test-calls nil)

(defun apiwrap-test-request (method resource params data)
  "The `:request' primitive a backend has to supply.  Records and replies."
  (push (list :method method :resource resource :params params :data data)
        apiwrap-test-calls)
  (list :status 200 :body (list (cons 'echo resource))))

(defun apiwrap-test-log ()
  (reverse (mapcar #'copy-tree apiwrap-test-calls)))

(defmacro apiwrap-test-outcome (&rest body)
  `(condition-case error (list :ok (progn ,@body))
     (error (list :error (car error) (cdr error)))))

(defun apiwrap-test-doc (symbol)
  (let ((doc (documentation symbol)))
    (and doc (substring-no-properties doc))))

(defvar apiwrap-test-around-log nil)

(defmacro apiwrap-test-around (form)
  "An `:around' macro: records that it ran and tags the result."
  `(progn (push :around-ran apiwrap-test-around-log)
          (list :wrapped ,form)))

(defun apiwrap-test-exploding-request (method resource params data)
  (push (list :method method :resource resource :params params :data data)
        apiwrap-test-calls)
  (signal 'wrong-type-argument (list 'stringp 42)))
"##;

fn apiwrap_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(APIWRAP_MELPA_PIN, "apiwrap.el")
        .expect("prepare pinned apiwrap source below ./tmp")
        .with_prelude(APIWRAP_TEST_PRELUDE)
        .with_timeout(APIWRAP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed apiwrap parity test")
        .into()
}

/// Multi-probe batch for `assert_apiwrap_parity` cases (2a).
pub(crate) fn assert_apiwrap_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(apiwrap_oracle(), &name, "apiwrap_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn apiwrap_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        documentation::documentation_public_surface_batch_cases(),
        practical::practical_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_apiwrap_batch(&cases);
}

// END generated package batch tests
