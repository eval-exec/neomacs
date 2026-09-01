use std::time::Duration;

use crate::{CONSULT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const CONSULT_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const CONSULT_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'consult)
(require 'consult-imenu)

(defun neomacs-consult-test-current-line ()
  "Return the current line without text properties."
  (buffer-substring-no-properties (line-beginning-position)
                                  (line-end-position)))
"##;

fn consult_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CONSULT_MELPA_PIN, "consult.el")
        .expect("prepare pinned Consult source below ./tmp")
        .with_prelude(CONSULT_TEST_PRELUDE)
        .with_timeout(CONSULT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Consult parity test")
        .into()
}

pub(crate) fn assert_consult_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(consult_oracle(), &name, "consult_parity", cases);
}

#[test]
fn consult_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_consult_batch(&cases);
}
