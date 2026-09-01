use std::time::Duration;

use crate::{CachedMelpaOracle, SHELL_POP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const SHELL_POP_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SHELL_POP_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'shell-pop)

(defun neomacs-shell-pop-test-fake-shell ()
  "Create a plain buffer instead of a live shell process."
  (switch-to-buffer (get-buffer-create "*shell*"))
  (erase-buffer)
  (insert "# fake shell\n"))
"####;

fn shell_pop_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SHELL_POP_MELPA_PIN, "shell-pop.el")
        .expect("prepare exact shallow shell-pop source below ./tmp")
        .with_prelude(SHELL_POP_TEST_PRELUDE)
        .with_timeout(SHELL_POP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed shell-pop parity test")
        .into()
}

fn assert_shell_pop_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        shell_pop_oracle(),
        &current_test_name(),
        "shell_pop_parity",
        cases,
    );
}

#[test]
fn shell_pop_package_batch() {
    assert_shell_pop_batch(&workflows::workflow_batch_cases());
}
