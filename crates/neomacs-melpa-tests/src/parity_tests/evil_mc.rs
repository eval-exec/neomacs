use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_MC_MELPA_PIN, EVIL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-mc)

(defmacro neomacs-evil-mc-test-with-buffer (mode &rest body)
  "Run BODY in a live, selected-window buffer using MODE."
  `(let ((buffer (generate-new-buffer " *evil-mc-workflow*"))
         (this-command nil)
         (last-command nil))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (funcall ,mode)
           ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (when (bound-and-true-p evil-mc-mode)
             (neomacs-evil-mc-test-stop)))
         (kill-buffer buffer)))))

(defun neomacs-evil-mc-test-keys (&rest parts)
  "Execute PARTS as one real keyboard macro."
  (execute-kbd-macro (apply #'vconcat parts)))

(defun neomacs-evil-mc-test-cursor-positions ()
  "Return stable positions for all fake cursors."
  (mapcar #'evil-mc-get-cursor-start evil-mc-cursor-list))

(defun neomacs-evil-mc-test-state ()
  "Capture the user-visible multi-cursor editing state."
  (list
   :text (buffer-substring-no-properties (point-min) (point-max))
   :point (point)
   :line (line-number-at-pos)
   :column (current-column)
   :evil-state evil-state
   :cursor-count (evil-mc-get-cursor-count)
   :fake-cursors (neomacs-evil-mc-test-cursor-positions)
   :pattern (and evil-mc-pattern (evil-mc-get-pattern-text))
   :paused (not (null evil-mc-frozen))
   :cursor-overlays
   (length
    (cl-remove-if-not
     (lambda (overlay)
       (eq (overlay-get overlay 'type) 'evil-mc-cursor))
     (overlays-in (point-min) (point-max))))))

(defun neomacs-evil-mc-test-start (text &optional search)
  "Start a realistic Evil MC editing session over TEXT at SEARCH."
  (insert text)
  (goto-char (point-min))
  (when search
    (search-forward search)
    (goto-char (match-beginning 0)))
  (evil-local-mode 1)
  (evil-mc-mode 1)
  (evil-normal-state))

(defun neomacs-evil-mc-test-stop ()
  "Cleanly stop the current Evil MC editing session."
  (evil-mc-undo-all-cursors)
  (evil-mc-mode -1)
  (evil-local-mode -1))
"####;

fn all_match_change_refactors_a_symbol_across_real_code() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-mc-test-with-buffer #'emacs-lisp-mode
  (neomacs-evil-mc-test-start
   "(let ((draft 1))\n  (message \"draft=%s\" draft)\n  (+ draft 2))\n"
   "draft")
  (neomacs-evil-mc-test-keys (kbd "grm"))
  (let ((selected (neomacs-evil-mc-test-state)))
    (neomacs-evil-mc-test-keys (kbd "ciw") "result" [escape])
    (let ((edited (neomacs-evil-mc-test-state)))
      (neomacs-evil-mc-test-stop)
      (list :selected selected
            :edited edited
            :after-cleanup (neomacs-evil-mc-test-state)))))
"####;
    let expected = expect![[
        r#"OK (:selected (:text "(let ((draft 1))\n  (message \"draft=%s\" draft)\n  (+ draft 2))\n" :point 12 :line 1 :column 11 :evil-state normal :cursor-count 3 :fake-cursors (44 56) :pattern "\\_<draft\\_>" :paused nil :cursor-overlays 2) :edited (:text "(let ((result 1))\n  (message \"draft=%s\" result)\n  (+ result 2))\n" :point 13 :line 1 :column 12 :evil-state normal :cursor-count 3 :fake-cursors (46 59) :pattern "\\_<draft\\_>" :paused nil :cursor-overlays 2) :after-cleanup (:text "(let ((result 1))\n  (message \"draft=%s\" result)\n  (+ result 2))\n" :point 13 :line 1 :column 12 :evil-state nil :cursor-count 1 :fake-cursors nil :pattern nil :paused nil :cursor-overlays 0))"#
    ]];
    ParityBatchCase::value(
        "all_match_change_refactors_a_symbol_across_real_code",
        elisp_form,
        expected,
    )
}

fn incremental_selection_skips_one_match_before_editing() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-mc-test-with-buffer #'text-mode
  (neomacs-evil-mc-test-start
   "alpha = primary\nalpha = fallback\nalpha = deprecated\nalpha = emergency\n"
   "alpha")
  (neomacs-evil-mc-test-keys (kbd "C-n C-n C-t C-n"))
  (let ((selected (neomacs-evil-mc-test-state)))
    (neomacs-evil-mc-test-keys (kbd "ciw") "enabled" [escape])
    (let ((edited (neomacs-evil-mc-test-state)))
      (neomacs-evil-mc-test-stop)
      (list :selected selected :edited edited))))
"####;
    let expected = expect![[
        r#"OK (:selected (:text "alpha = primary\nalpha = fallback\nalpha = deprecated\nalpha = emergency\n" :point 5 :line 1 :column 4 :evil-state normal :cursor-count 3 :fake-cursors (21 57) :pattern "\\_<alpha\\_>" :paused nil :cursor-overlays 2) :edited (:text "enabled = primary\nenabled = fallback\nalpha = deprecated\nenabled = emergency\n" :point 7 :line 1 :column 6 :evil-state normal :cursor-count 3 :fake-cursors (25 63) :pattern "\\_<alpha\\_>" :paused nil :cursor-overlays 2))"#
    ]];
    ParityBatchCase::value(
        "incremental_selection_skips_one_match_before_editing",
        elisp_form,
        expected,
    )
}

fn visual_line_workflow_inserts_a_prefix_at_each_selected_line() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-mc-test-with-buffer #'text-mode
  (neomacs-evil-mc-test-start
   "build release\ntest release\npublish release\nnotify team\n")
  (neomacs-evil-mc-test-keys (kbd "V j j g r I") "TODO: " [escape])
  (let ((edited (neomacs-evil-mc-test-state)))
    (neomacs-evil-mc-test-stop)
    (list :edited edited
          :after-cleanup (neomacs-evil-mc-test-state))))
"####;
    let expected = expect![[
        r#"OK (:edited (:text "TODO: build release\nTODO: test release\nTODO: publish release\nnotify team\n" :point 45 :line 3 :column 5 :evil-state normal :cursor-count 3 :fake-cursors (6 26) :pattern nil :paused nil :cursor-overlays 2) :after-cleanup (:text "TODO: build release\nTODO: test release\nTODO: publish release\nnotify team\n" :point 45 :line 3 :column 5 :evil-state nil :cursor-count 1 :fake-cursors nil :pattern nil :paused nil :cursor-overlays 0))"#
    ]];
    ParityBatchCase::value(
        "visual_line_workflow_inserts_a_prefix_at_each_selected_line",
        elisp_form,
        expected,
    )
}

fn paused_cursors_leave_a_local_edit_alone_then_resume_replay() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-mc-test-with-buffer #'text-mode
  (neomacs-evil-mc-test-start
   "service one\nservice two\nservice three\n"
   "service")
  (neomacs-evil-mc-test-keys (kbd "grm g r s"))
  (let ((paused (neomacs-evil-mc-test-state)))
    (neomacs-evil-mc-test-keys (kbd "A") " # primary" [escape])
    (let ((local-edit (neomacs-evil-mc-test-state)))
      (neomacs-evil-mc-test-keys (kbd "g r r 0 ciw") "worker" [escape])
      (let ((resumed-edit (neomacs-evil-mc-test-state)))
        (neomacs-evil-mc-test-stop)
        (list :paused paused
              :local-edit local-edit
              :resumed-edit resumed-edit)))))
"####;
    let expected = expect![[
        r#"OK (:paused (:text "service one\nservice two\nservice three\n" :point 7 :line 1 :column 6 :evil-state normal :cursor-count 3 :fake-cursors (19 31) :pattern "\\_<service\\_>" :paused t :cursor-overlays 2) :local-edit (:text "service one # primary\nservice two\nservice three\n" :point 21 :line 1 :column 20 :evil-state normal :cursor-count 3 :fake-cursors (29 41) :pattern "\\_<service\\_>" :paused t :cursor-overlays 2) :resumed-edit (:text "worker one # primary\nworker two\nworker three\n" :point 6 :line 1 :column 5 :evil-state normal :cursor-count 3 :fake-cursors (27 38) :pattern "\\_<service\\_>" :paused nil :cursor-overlays 2))"#
    ]];
    ParityBatchCase::value(
        "paused_cursors_leave_a_local_edit_alone_then_resume_replay",
        elisp_form,
        expected,
    )
}

fn multi_cursor_edit_is_one_undo_step_and_cleanup_removes_overlays() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-mc-test-with-buffer #'text-mode
  (buffer-enable-undo)
  (neomacs-evil-mc-test-start
   "draft report\ndraft invoice\ndraft notice\n"
   "draft")
  (setq buffer-undo-list nil)
  (neomacs-evil-mc-test-keys (kbd "grm ciw") "final" [escape])
  (let ((edited (neomacs-evil-mc-test-state)))
    (neomacs-evil-mc-test-keys (kbd "u"))
    (let ((undone (neomacs-evil-mc-test-state)))
      (neomacs-evil-mc-test-stop)
      (list :edited edited
            :undone undone
            :after-cleanup (neomacs-evil-mc-test-state)
            :active-hooks
            (list (memq #'evil-mc-begin-command-save pre-command-hook)
                  (memq #'evil-mc-finish-command-save post-command-hook)
                  (memq #'evil-mc-execute-for-all post-command-hook))))))
"####;
    let expected = expect![[
        r#"OK (:edited (:text "final report\nfinal invoice\nfinal notice\n" :point 5 :line 1 :column 4 :evil-state normal :cursor-count 3 :fake-cursors (18 32) :pattern "\\_<draft\\_>" :paused nil :cursor-overlays 2) :undone (:text "draft report\ndraft invoice\ndraft notice\n" :point 5 :line 1 :column 4 :evil-state normal :cursor-count 3 :fake-cursors (14 28) :pattern "\\_<draft\\_>" :paused nil :cursor-overlays 2) :after-cleanup (:text "draft report\ndraft invoice\ndraft notice\n" :point 5 :line 1 :column 4 :evil-state nil :cursor-count 1 :fake-cursors nil :pattern nil :paused nil :cursor-overlays 0) :active-hooks (nil nil nil))"#
    ]];
    ParityBatchCase::value(
        "multi_cursor_edit_is_one_undo_step_and_cleanup_removes_overlays",
        elisp_form,
        expected,
    )
}

fn evil_mc_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_MC_MELPA_PIN, "evil-mc.el")
        .expect("prepare pinned Evil MC source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare pinned Evil dependency")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn evil_mc_practical_workflows_batch() {
    let cases = vec![
        all_match_change_refactors_a_symbol_across_real_code(),
        incremental_selection_skips_one_match_before_editing(),
        visual_line_workflow_inserts_a_prefix_at_each_selected_line(),
        paused_cursors_leave_a_local_edit_alone_then_resume_replay(),
        multi_cursor_edit_is_one_undo_step_and_cleanup_removes_overlays(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("evil-mc parity batch");
    assert_oracle_batch_cases(evil_mc_oracle(), test_name, "evil-mc parity", &cases);
}
