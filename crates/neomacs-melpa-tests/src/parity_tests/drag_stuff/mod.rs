use std::time::Duration;

use crate::{CachedMelpaOracle, DRAG_STUFF_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const DRAG_STUFF_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const DRAG_STUFF_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'drag-stuff)

(defun neomacs-drag-stuff-test-state ()
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)))

(defun neomacs-drag-stuff-test-with-buffer (contents needle function)
  (with-temp-buffer
    (insert contents)
    (goto-char (point-min))
    (when needle
      (search-forward needle)
      (goto-char (match-beginning 0)))
    (drag-stuff-mode 1)
    (funcall function)))
"####;

fn drag_stuff_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DRAG_STUFF_MELPA_PIN, "drag-stuff.el")
        .expect("prepare exact shallow drag-stuff source below ./tmp")
        .with_prelude(DRAG_STUFF_TEST_PRELUDE)
        .with_timeout(DRAG_STUFF_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed drag-stuff parity test")
        .into()
}

fn assert_drag_stuff_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        drag_stuff_oracle(),
        &current_test_name(),
        "drag_stuff_parity",
        cases,
    );
}

#[test]
fn drag_stuff_package_batch() {
    assert_drag_stuff_batch(&workflows::workflow_batch_cases());
}
