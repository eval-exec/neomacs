use std::time::Duration;

use crate::{AAS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AAS_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// aas expands snippets from `post-self-insert-hook`, so every workflow has to
/// type real keys.  `execute-kbd-macro` delivers them to the buffer of the
/// selected window, which is why the helper displays the work buffer instead of
/// merely making it current.
const AAS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defmacro aas-test-with-live-buffer (&rest body)
  "Run BODY in a real, window-displayed buffer so typed keys reach it."
  `(let ((buffer (generate-new-buffer "*aas-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           ,@body)
       (kill-buffer buffer))))
"##;

fn aas_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AAS_MELPA_PIN, "aas.el")
        .expect("prepare pinned aas source below ./tmp")
        .with_prelude(AAS_TEST_PRELUDE)
        .with_timeout(AAS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed aas parity test").into()
}

/// Multi-probe batch for `assert_aas_parity` cases (2a).
pub(crate) fn assert_aas_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(aas_oracle(), &name, "aas_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn aas_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aas_batch(&cases);
}

// END generated package batch tests
