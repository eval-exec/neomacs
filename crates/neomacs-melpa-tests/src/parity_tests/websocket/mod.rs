use std::time::Duration;

use crate::{CachedMelpaOracle, WEBSOCKET_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const WEBSOCKET_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const WEBSOCKET_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun websocket-parity-wait-until (predicate &optional timeout)
  "Pump process output until PREDICATE returns non-nil or TIMEOUT expires."
  (let ((deadline (+ (float-time) (or timeout 10.0)))
        value)
    (while (and (not (setq value (funcall predicate)))
                (< (float-time) deadline))
      (accept-process-output nil 0.01))
    value))

(defun websocket-parity-close-client (client)
  "Close CLIENT once, without firing its callback a second time."
  (when (and client
             (not (eq (websocket-ready-state client) 'closed)))
    (ignore-errors (websocket-close client))))

(defun websocket-parity-close-server (server)
  "Close SERVER and every accepted socket that still belongs to it."
  (when (and server (process-live-p server))
    (ignore-errors (websocket-server-close server))))
"##;

fn websocket_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(WEBSOCKET_MELPA_PIN, "websocket.el")
        .expect("prepare pinned websocket source below ./tmp")
        .with_prelude(WEBSOCKET_TEST_PRELUDE)
        .with_timeout(WEBSOCKET_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed websocket parity test")
        .into()
}

pub(crate) fn assert_websocket_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(websocket_oracle(), &name, "websocket_parity", cases);
}

#[test]
fn websocket_package_batch() {
    assert_websocket_batch(&workflows::practical_workflow_batch_cases());
}
