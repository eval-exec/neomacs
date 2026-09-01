use std::time::Duration;

use crate::{ARDUINO_CLI_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ARDUINO_CLI_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(defun acli-test-compilation-complete-p (buffer)
  "Non-nil once `compilation-handle-exit' has written BUFFER's last line.
That line is the causal end of the output rather than a guess about it: Emacs
drains a dying process's remaining reads before it runs the sentinel
\(GNU src/process.c:7896-7910), the sentinel is what calls
`compilation-handle-exit', and that function marks the text it writes with a
`compilation-handle-exit' text property (GNU lisp/progmodes/compile.el:2630).
The property therefore cannot appear until every byte arduino-cli wrote has
already been through `compilation-filter'."
  (and buffer
       (buffer-live-p (get-buffer buffer))
       (with-current-buffer buffer
         (and (text-property-not-all (point-min) (point-max)
                                     'compilation-handle-exit nil)
              t))))

(defun acli-test-await-compilation (buffer)
  "Wait until BUFFER holds all of its compilation's output, or signal.
Every workflow here used to wait for `process-live-p' to go nil and then take
one more `accept-process-output'.  That is not the same fact: a process can be
gone with reads still queued, and the pins below read the compiled-size line
the child prints last.  Signalling rather than returning means a future edit
that goes back to the clock fails on its first run instead of moving a
snapshot months later.  See DIVERGENCES.md entries 133, 140 and 144."
  (let ((waited 0))
    (while (and (< waited 1200)
                (not (acli-test-compilation-complete-p buffer)))
      (accept-process-output nil 0.05)
      (setq waited (1+ waited)))
    (unless (acli-test-compilation-complete-p buffer)
      (error "acli-test-await-compilation: %S never reached \
`compilation-handle-exit'; its text records only as much of arduino-cli's \
output as had been read" buffer))
    :finished))
"####;

fn arduino_cli_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARDUINO_CLI_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned arduino-cli-mode source below ./tmp")
        .with_timeout(ARDUINO_CLI_MODE_TEST_TIMEOUT)
        .with_prelude(PRELUDE)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed arduino-cli-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_arduino_cli_mode_parity` cases (2a).
pub(crate) fn assert_arduino_cli_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        arduino_cli_mode_oracle("arduino-cli-mode.el"),
        &name,
        "arduino_cli_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn arduino_cli_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_arduino_cli_mode_batch(&cases);
}

// END generated package batch tests
