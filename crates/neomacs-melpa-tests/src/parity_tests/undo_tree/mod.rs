use std::time::Duration;

use crate::{CachedMelpaOracle, UNDO_TREE_SOURCE_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const UNDO_TREE_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const UNDO_TREE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'undo-tree)

(defun neomacs-undo-tree-test-state ()
  "Describe the current buffer and Undo Tree cursor state."
  (undo-list-transfer-to-tree)
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :modified (buffer-modified-p)
        :count (undo-tree-count buffer-undo-tree)
        :branches (undo-tree-num-branches)
        :current-is-root
        (eq (undo-tree-current buffer-undo-tree)
            (undo-tree-root buffer-undo-tree))))

(defun neomacs-undo-tree-test-edit (text)
  "Insert TEXT as one explicit undo changeset."
  (when (eq buffer-undo-list t)
    (buffer-enable-undo))
  (undo-boundary)
  (insert text)
  (undo-boundary)
  (undo-list-transfer-to-tree))

(defun neomacs-undo-tree-test-kill-visualizer ()
  "Kill any Undo Tree visualizer buffer without changing parent state."
  (dolist (buffer (buffer-list))
    (with-current-buffer buffer
      (when (eq major-mode 'undo-tree-visualizer-mode)
        (set-buffer-modified-p nil)
        (kill-buffer buffer)))))
"##;

fn undo_tree_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(UNDO_TREE_SOURCE_PIN, "undo-tree.el")
        .expect("prepare exact shallow Undo Tree source and Queue dependency below ./tmp")
        .with_prelude(UNDO_TREE_TEST_PRELUDE)
        .with_timeout(UNDO_TREE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Undo Tree parity test")
        .into()
}

fn assert_undo_tree_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        undo_tree_oracle(),
        &current_test_name(),
        "undo_tree_parity",
        cases,
    );
}

#[test]
fn undo_tree_package_batch() {
    assert_undo_tree_batch(&workflows::workflow_batch_cases());
}
