use expect_test::expect;

use super::ParityBatchCase;

fn linear_editing_undo_and_redo_restore_exact_buffer_states() -> ParityBatchCase {
    ParityBatchCase::value(
        "linear_editing_undo_and_redo_restore_exact_buffer_states",
        r##"
(with-temp-buffer
  (let ((undo-tree-auto-save-history nil))
    (undo-tree-mode 1)
    (neomacs-undo-tree-test-edit "alpha")
    (neomacs-undo-tree-test-edit " beta")
    (neomacs-undo-tree-test-edit " γ")
    (let ((edited (neomacs-undo-tree-test-state)))
      (undo-tree-undo 2)
      (let ((undone (neomacs-undo-tree-test-state)))
        (undo-tree-redo)
        (list :edited edited
              :undone undone
              :redone (neomacs-undo-tree-test-state)
              :mode undo-tree-mode)))))
"##,
        expect![[
            r#"OK (:edited (:text "alpha beta γ" :point 13 :modified t :count 3 :branches 0 :current-is-root nil) :undone (:text "alpha" :point 6 :modified t :count 3 :branches 1 :current-is-root nil) :redone (:text "alpha beta" :point 6 :modified t :count 3 :branches 1 :current-is-root nil) :mode t)"#
        ]],
    )
}

fn editing_after_undo_creates_a_branch_and_switching_recovers_both_futures() -> ParityBatchCase {
    ParityBatchCase::value(
        "editing_after_undo_creates_a_branch_and_switching_recovers_both_futures",
        r##"
(with-temp-buffer
  (let ((undo-tree-auto-save-history nil))
    (undo-tree-mode 1)
    (neomacs-undo-tree-test-edit "base")
    (neomacs-undo-tree-test-edit "-old")
    (undo-tree-undo)
    (neomacs-undo-tree-test-edit "-new")
    (undo-tree-undo)
    (let ((at-branch (neomacs-undo-tree-test-state))
          old new)
      (undo-tree-switch-branch 0)
      (undo-tree-redo)
      (setq new (neomacs-undo-tree-test-state))
      (undo-tree-undo)
      (undo-tree-switch-branch 1)
      (undo-tree-redo)
      (setq old (neomacs-undo-tree-test-state))
      (list :branch-point at-branch
            :new-branch new
            :old-branch old
            :child-count
            (length
             (undo-tree-node-next
              (undo-tree-node-previous
               (undo-tree-current buffer-undo-tree))))))))
"##,
        expect![[
            r#"OK (:branch-point (:text "base" :point 5 :modified t :count 3 :branches 2 :current-is-root nil) :new-branch (:text "base-new" :point 5 :modified t :count 3 :branches 0 :current-is-root nil) :old-branch (:text "base-old" :point 5 :modified t :count 3 :branches 0 :current-is-root nil) :child-count 2)"#
        ]],
    )
}

fn registers_restore_named_history_states_after_further_edits() -> ParityBatchCase {
    ParityBatchCase::value(
        "registers_restore_named_history_states_after_further_edits",
        r##"
(with-temp-buffer
  (let ((undo-tree-auto-save-history nil)
        (register-alist nil))
    (undo-tree-mode 1)
    (neomacs-undo-tree-test-edit "draft")
    (undo-tree-save-state-to-register ?r)
    (neomacs-undo-tree-test-edit " approved")
    (let ((latest (neomacs-undo-tree-test-state)))
      (undo-tree-restore-state-from-register ?r)
      (let ((restored (neomacs-undo-tree-test-state)))
        (undo-tree-redo)
        (list :latest latest
              :restored restored
              :redone (neomacs-undo-tree-test-state)
              :register-valid
              (undo-tree-register-data-p
               (registerv-data (get-register ?r))))))))
"##,
        expect![[
            r#"OK (:latest (:text "draft approved" :point 15 :modified t :count 2 :branches 0 :current-is-root nil) :restored (:text "draft" :point 6 :modified t :count 2 :branches 1 :current-is-root nil) :redone (:text "draft approved" :point 6 :modified t :count 2 :branches 0 :current-is-root nil) :register-valid t)"#
        ]],
    )
}

fn persistent_history_round_trips_to_a_workspace_local_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "persistent_history_round_trips_to_a_workspace_local_file",
        r##"
(let* ((root (file-name-as-directory
              (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (visited (expand-file-name "release λ.txt" root))
       (history (expand-file-name "release.undo" root))
       first-state loaded-state)
  (with-temp-file visited (insert "base"))
  (let ((first (find-file-noselect visited)))
    (unwind-protect
        (with-current-buffer first
          (let ((undo-tree-auto-save-history nil))
            (undo-tree-mode 1)
            (goto-char (point-max))
            (neomacs-undo-tree-test-edit "\nchange one")
            (neomacs-undo-tree-test-edit "\nchange two")
            (save-buffer)
            (setq first-state (neomacs-undo-tree-test-state))
            (undo-tree-save-history history t)))
      (when (buffer-live-p first)
        (with-current-buffer first (set-buffer-modified-p nil))
        (kill-buffer first))))
  (let ((second (find-file-noselect visited)))
    (unwind-protect
        (with-current-buffer second
          (let ((undo-tree-auto-save-history nil))
            (undo-tree-mode 1)
            (undo-tree-load-history history)
            (undo-tree-undo)
            (setq loaded-state (neomacs-undo-tree-test-state))))
      (when (buffer-live-p second)
        (with-current-buffer second (set-buffer-modified-p nil))
        (kill-buffer second))))
  (list :history-exists (file-exists-p history)
        :history-nonempty (> (nth 7 (file-attributes history)) 0)
        :history-version
        (with-temp-buffer
          (insert-file-contents history)
          (goto-char (point-min))
          (read (current-buffer)))
        :first first-state
        :loaded-after-undo loaded-state))
"##,
        expect![[
            r#"OK (:history-exists t :history-nonempty t :history-version (undo-tree-save-format-version . 1) :first (:text "base\nchange one\nchange two\n" :point 27 :modified nil :count 3 :branches 0 :current-is-root nil) :loaded-after-undo (:text "base\nchange one\nchange two" :point 27 :modified t :count 0 :branches 1 :current-is-root nil))"#
        ]],
    )
}

fn visualizer_renders_the_tree_and_quit_preserves_the_selected_parent_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "visualizer_renders_the_tree_and_quit_preserves_the_selected_parent_state",
        r##"
(with-temp-buffer
  (let ((undo-tree-auto-save-history nil)
        parent visualizer)
    (undo-tree-mode 1)
    (neomacs-undo-tree-test-edit "one")
    (neomacs-undo-tree-test-edit " two")
    (undo-tree-undo)
    (neomacs-undo-tree-test-edit " three")
    (setq parent (current-buffer))
    (unwind-protect
        (progn
          (undo-tree-visualize)
          (setq visualizer (current-buffer))
          (let ((rendered
                 (list :mode major-mode
                       :text (buffer-substring-no-properties
                              (point-min) (point-max))
                       :parent-live (buffer-live-p parent)
                       :timestamps undo-tree-visualizer-timestamps)))
            (undo-tree-visualizer-quit)
            (list :rendered rendered
                  :parent-current (eq (current-buffer) parent)
                  :parent-state
                  (with-current-buffer parent
                    (neomacs-undo-tree-test-state)))))
      (when (buffer-live-p visualizer)
        (kill-buffer visualizer))
      (neomacs-undo-tree-test-kill-visualizer))))
"##,
        expect![[r#"OK (:rendered (:mode undo-tree-visualizer-mode :text "\n                                        o\n                                        |\n                                        |\n                                        o\n                                        | \n                                       / \\\n                                      x   o" :parent-live t :timestamps nil) :parent-current t :parent-state (:text "one three" :point 10 :modified t :count 3 :branches 0 :current-is-root nil))"#]],
    )
    .fresh_process()
}

fn disabling_mode_rebuilds_standard_undo_and_restores_buffer_local_limits() -> ParityBatchCase {
    ParityBatchCase::value(
        "disabling_mode_rebuilds_standard_undo_and_restores_buffer_local_limits",
        r##"
(with-temp-buffer
  (let ((undo-tree-auto-save-history nil)
        (initial-undo-limit undo-limit)
        (initial-strong-limit undo-strong-limit)
        (initial-outer-limit undo-outer-limit))
    (undo-tree-mode 1)
    (neomacs-undo-tree-test-edit "first")
    (neomacs-undo-tree-test-edit " second")
    (let ((enabled
           (list :mode undo-tree-mode
                 :tree (undo-tree-p buffer-undo-tree)
                 :undo-limit undo-limit
                 :strong-limit undo-strong-limit
                 :outer-limit undo-outer-limit
                 :post-gc (and (memq #'undo-tree-post-gc post-gc-hook) t))))
      (undo-tree-mode -1)
      (list :enabled enabled
            :disabled
            (list :mode undo-tree-mode
                  :tree buffer-undo-tree
                  :undo-list-present (consp buffer-undo-list)
                  :limits
                  (list (equal undo-limit initial-undo-limit)
                        (equal undo-strong-limit initial-strong-limit)
                        (equal undo-outer-limit initial-outer-limit))
                  :post-gc (and (memq #'undo-tree-post-gc post-gc-hook)
                                t))))))
"##,
        expect![
            "OK (:enabled (:mode t :tree t :undo-limit 80000000 :strong-limit 120000000 :outer-limit nil :post-gc t) :disabled (:mode nil :tree nil :undo-list-present t :limits (t t t) :post-gc t))"
        ],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        linear_editing_undo_and_redo_restore_exact_buffer_states(),
        editing_after_undo_creates_a_branch_and_switching_recovers_both_futures(),
        registers_restore_named_history_states_after_further_edits(),
        persistent_history_round_trips_to_a_workspace_local_file(),
        visualizer_renders_the_tree_and_quit_preserves_the_selected_parent_state(),
        disabling_mode_rebuilds_standard_undo_and_restores_buffer_local_limits(),
    ]
}
