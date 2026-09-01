use std::time::Duration;

use crate::{ADD_HOOKS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ADD_HOOKS_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Helpers shared by the workflows.
///
/// add-hooks is a hundred and thirty lines of pure list handling: `add-hooks'
/// takes an alist of (HOOKS . FUNCTIONS) and `add-hooks-pair' takes the two
/// halves, and between them they call `add-hook' once for every combination.
/// There is no state beyond the hook variables themselves, which makes those
/// variables - and what happens when the hooks run - the whole observable
/// surface.
///
/// The workflows assert both.  Reading a hook variable back says which objects
/// were added; running the hook says whether they were added as *functions*,
/// which is the question the package's own heuristic exists to answer.  A hook
/// holding a lambda and a hook holding the three symbols of an unevaluated form
/// look similarly plausible in a printed list and behave completely differently
/// when Emacs runs them.
///
/// `add-hooks-test-fire' is how the running half is observed: each function
/// pushes its own name onto a list, so a hook's contents come back as the order
/// in which its functions actually ran rather than as printed closures, which
/// would pin an implementation detail rather than a behaviour.
const ADD_HOOKS_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defvar add-hooks-test-fired nil
  "Names pushed by the fixture functions, most recent first.")

(defun add-hooks-test-reset (&rest hooks)
  "Empty the record and every hook variable in HOOKS."
  (setq add-hooks-test-fired nil)
  (dolist (hook hooks)
    (set hook nil)))

(defun add-hooks-test-fire (hook)
  "Run HOOK and return the names its functions pushed, in the order they ran."
  (setq add-hooks-test-fired nil)
  (run-hooks hook)
  (nreverse add-hooks-test-fired))

(defmacro add-hooks-test-recorder (name)
  "A function that records NAME when it runs."
  `(lambda () (push ,name add-hooks-test-fired)))

(defun add-hooks-test-emmet-mode ()
  "Stand in for the mode the package's own examples enable."
  (push 'emmet-mode add-hooks-test-fired))

(defun add-hooks-test-rainbow-mode ()
  "A second mode, so a pair can have more than one function."
  (push 'rainbow-mode add-hooks-test-fired))

(defvar css-mode-hook nil)
(defvar sgml-mode-hook nil)
(defvar text-mode-hook nil)
(defvar add-hooks-test-plain-hook nil)
"##;

fn add_hooks_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ADD_HOOKS_MELPA_PIN, "add-hooks.el")
        .expect("prepare pinned add-hooks source below ./tmp")
        .with_prelude(ADD_HOOKS_TEST_PRELUDE)
        .with_timeout(ADD_HOOKS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed add-hooks parity test")
        .into()
}

/// Multi-probe batch for `assert_add_hooks_parity` cases (2a).
pub(crate) fn assert_add_hooks_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(add_hooks_oracle(), &name, "add_hooks_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn add_hooks_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_add_hooks_batch(&cases);
}

// END generated package batch tests
