use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MOVE_TEXT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'move-text)

(defun neomacs-move-text-test-state ()
  "Return the visible editing state of the current buffer."
  (let ((saved-mark (mark t)))
    (list
     :text (buffer-substring-no-properties (point-min) (point-max))
     :point (point)
     :line (line-number-at-pos)
     :column (current-column)
     :mark saved-mark
     :mark-line (and saved-mark (line-number-at-pos saved-mark))
     :active (not (null (region-active-p)))
     :region
     (and (use-region-p)
          (buffer-substring-no-properties
           (region-beginning) (region-end))))))
"####;

fn repeated_interactive_line_moves_preserve_the_working_column() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "Backlog\n"
   "Design API\n"
   "Implement parser\n"
   "Review changes\n"
   "Deploy\n")
  (goto-char (point-min))
  (forward-line 3)
  (move-to-column 7)
  (let ((transient-mark-mode t)
        after-up)
    (deactivate-mark)
    (let ((current-prefix-arg 2))
      (call-interactively #'move-text-up))
    (setq after-up (neomacs-move-text-test-state))
    (let ((current-prefix-arg 1))
      (call-interactively #'move-text-down))
    (list :after-up after-up
          :after-down (neomacs-move-text-test-state))))
"####;
    let expected = expect![[
        r#"OK (:after-up (:text "Backlog\nReview changes\nDesign API\nImplement parser\nDeploy\n" :point 16 :line 2 :column 7 :mark nil :mark-line nil :active nil :region nil) :after-down (:text "Backlog\nDesign API\nReview changes\nImplement parser\nDeploy\n" :point 27 :line 3 :column 7 :mark nil :mark-line nil :active nil :region nil))"#
    ]];
    ParityBatchCase::value(
        "repeated_interactive_line_moves_preserve_the_working_column",
        elisp_form,
        expected,
    )
}

fn active_multiline_region_moves_as_one_property_preserving_block() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "release\n"
   "plan\n"
   "build\n"
   "test\n"
   "deploy\n"
   "retrospective\n")
  (goto-char (point-min))
  (forward-line 2)
  (set-mark (point))
  (forward-line 2)
  (activate-mark)
  (add-text-properties (region-beginning) (region-end)
                       '(workflow-step selected))
  (let ((transient-mark-mode t)
        after-down)
    (let ((current-prefix-arg 1))
      (call-interactively #'move-text-down))
    (setq after-down
          (list
           :state (neomacs-move-text-test-state)
           :property
           (get-text-property (region-beginning) 'workflow-step)
           :deactivate-mark deactivate-mark))
    (let ((current-prefix-arg 1))
      (call-interactively #'move-text-up))
    (list
     :after-down after-down
     :after-up
     (list
      :state (neomacs-move-text-test-state)
      :property (get-text-property (region-beginning) 'workflow-step)
      :deactivate-mark deactivate-mark))))
"####;
    let expected = expect![[
        r#"OK (:after-down (:state (:text "release\nplan\ndeploy\nbuild\ntest\nretrospective\n" :point 32 :line 6 :column 0 :mark 21 :mark-line 4 :active t :region "build\ntest\n") :property selected :deactivate-mark nil) :after-up (:state (:text "release\nplan\nbuild\ntest\ndeploy\nretrospective\n" :point 25 :line 5 :column 0 :mark 14 :mark-line 3 :active t :region "build\ntest\n") :property selected :deactivate-mark nil))"#
    ]];
    ParityBatchCase::value(
        "active_multiline_region_moves_as_one_property_preserving_block",
        elisp_form,
        expected,
    )
}

fn first_last_and_trailing_newline_boundaries_do_not_lose_text() -> ParityBatchCase {
    let elisp_form = r####"
(cl-labels
    ((run (text line column command)
       (with-temp-buffer
         (insert text)
         (goto-char (point-min))
         (forward-line (1- line))
         (move-to-column column)
         (let ((transient-mark-mode t))
           (deactivate-mark)
           (funcall command nil nil 1)
           (neomacs-move-text-test-state)))))
  (list
   :first-up
   (run "one\ntwo\nthree\n" 1 2 #'move-text-up)
   :last-down-no-final-newline
   (run "one\ntwo\nthree" 3 2 #'move-text-down)
   :penultimate-down-before-empty-final-line
   (run "one\ntwo\nthree\n" 3 2 #'move-text-down)
   :last-up-no-final-newline
   (run "one\ntwo\nthree" 3 2 #'move-text-up)
   :last-up-with-final-newline
   (run "one\ntwo\nthree\n" 3 2 #'move-text-up)
   :single-line-down
   (run "only" 1 3 #'move-text-down)))
"####;
    let expected = expect![[
        r#"OK (:first-up (:text "one\ntwo\nthree\n" :point 3 :line 1 :column 2 :mark nil :mark-line nil :active nil :region nil) :last-down-no-final-newline (:text "one\ntwo\nthree" :point 11 :line 3 :column 2 :mark nil :mark-line nil :active nil :region nil) :penultimate-down-before-empty-final-line (:text "one\ntwo\nthree\n" :point 11 :line 3 :column 2 :mark nil :mark-line nil :active nil :region nil) :last-up-no-final-newline (:text "one\nthree\ntwo\n" :point 5 :line 2 :column 0 :mark 5 :mark-line 2 :active nil :region nil) :last-up-with-final-newline (:text "one\nthree\ntwo\n" :point 7 :line 2 :column 2 :mark nil :mark-line nil :active nil :region nil) :single-line-down (:text "only" :point 4 :line 1 :column 3 :mark nil :mark-line nil :active nil :region nil))"#
    ]];
    ParityBatchCase::value(
        "first_last_and_trailing_newline_boundaries_do_not_lose_text",
        elisp_form,
        expected,
    )
}

fn narrowed_release_section_reorders_without_touching_surrounding_text() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "outside-top\n"
   "alpha\n"
   "beta\n"
   "gamma\n"
   "outside-bottom\n")
  (let* ((narrow-start
          (save-excursion
            (goto-char (point-min))
            (forward-line 1)
            (point)))
         (narrow-end
          (save-excursion
            (goto-char (point-min))
            (forward-line 4)
            (point)))
         (restricted
          (save-restriction
            (narrow-to-region narrow-start narrow-end)
            (goto-char (point-min))
            (forward-line 1)
            (move-to-column 2)
            (let ((before
                   (list
                    :total-lines (move-text--total-lines)
                    :first (move-text--at-first-line-p)
                    :last (move-text--at-last-line-p)
                    :state (neomacs-move-text-test-state))))
              (move-text-down nil nil 1)
              (list :before before
                    :after (neomacs-move-text-test-state))))))
    (list :restricted restricted
          :whole (buffer-substring-no-properties (point-min) (point-max))
          :whole-line (line-number-at-pos)
          :whole-column (current-column))))
"####;
    let expected = expect![[
        r#"OK (:restricted (:before (:total-lines 4 :first nil :last nil :state (:text "alpha\nbeta\ngamma\n" :point 21 :line 2 :column 2 :mark nil :mark-line nil :active nil :region nil)) :after (:text "alpha\ngamma\nbeta\n" :point 27 :line 3 :column 2 :mark nil :mark-line nil :active nil :region nil)) :whole "outside-top\nalpha\ngamma\nbeta\noutside-bottom\n" :whole-line 4 :whole-column 2)"#
    ]];
    ParityBatchCase::value(
        "narrowed_release_section_reorders_without_touching_surrounding_text",
        elisp_form,
        expected,
    )
}

fn moved_lines_undo_as_one_edit_and_default_bindings_are_reversible() -> ParityBatchCase {
    let elisp_form = r####"
(let ((old-up (lookup-key global-map [M-up]))
      (old-down (lookup-key global-map [M-down])))
  (unwind-protect
      (list
       :undo
       (with-temp-buffer
         (buffer-enable-undo)
         (insert "prepare\nship\nverify\n")
         (goto-char (point-min))
         (forward-line 1)
         (move-to-column 2)
         (setq buffer-undo-list nil)
         (undo-boundary)
         (move-text-down nil nil 1)
         (undo-boundary)
         (let ((moved (neomacs-move-text-test-state)))
           (undo 1)
           (list :moved moved
                 :restored (neomacs-move-text-test-state)
                 :modified (buffer-modified-p))))
       :bindings
       (progn
         (move-text-default-bindings)
         (list
          :up (lookup-key global-map [M-up])
          :down (lookup-key global-map [M-down])
          :up-command (commandp (lookup-key global-map [M-up]))
          :down-command (commandp (lookup-key global-map [M-down]))
          :where-up (where-is-internal 'move-text-up global-map t)
          :where-down (where-is-internal 'move-text-down global-map t))))
    (define-key global-map [M-up] old-up)
    (define-key global-map [M-down] old-down)))
"####;
    let expected = expect![[
        r#"OK (:undo (:moved (:text "prepare\nverify\nship\n" :point 18 :line 3 :column 2 :mark nil :mark-line nil :active nil :region nil) :restored (:text "prepare\nship\nverify\n" :point 11 :line 2 :column 2 :mark nil :mark-line nil :active nil :region nil) :modified t) :bindings (:up move-text-up :down move-text-down :up-command t :down-command t :where-up [M-up] :where-down [M-down]))"#
    ]];
    ParityBatchCase::value(
        "moved_lines_undo_as_one_edit_and_default_bindings_are_reversible",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn move_text_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MOVE_TEXT_MELPA_PIN, "move-text.el")
        .expect("prepare pinned Move Text source below ./tmp")
        .with_timeout(Duration::from_secs(180))
        .with_prelude(PRELUDE)
}

#[test]
fn move_text_practical_workflows_batch() {
    let cases = vec![
        repeated_interactive_line_moves_preserve_the_working_column(),
        active_multiline_region_moves_as_one_property_preserving_block(),
        first_last_and_trailing_newline_boundaries_do_not_lose_text(),
        narrowed_release_section_reorders_without_touching_surrounding_text(),
        moved_lines_undo_as_one_edit_and_default_bindings_are_reversible(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("move-text parity batch");
    assert_oracle_batch_cases(move_text_oracle(), test_name, "move-text parity", &cases);
}
