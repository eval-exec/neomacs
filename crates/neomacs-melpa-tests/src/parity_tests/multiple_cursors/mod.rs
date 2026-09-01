use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MULTIPLE_CURSORS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const MULTIPLE_CURSORS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const MULTIPLE_CURSORS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'multiple-cursors)
(require 'mc-cycle-cursors)
(require 'mc-hide-unmatched-lines-mode)

(defun mc-test-insert-prefix ()
  (interactive)
  (insert "TODO: "))

(defun mc-test-insert-separator ()
  (interactive)
  (insert " / "))

(defvar mc-test-helper-mode-events nil)
(define-minor-mode mc-test-helper-mode
  "A deterministic unsupported-mode fixture."
  :lighter " helper"
  (push (list :helper-mode (and mc-test-helper-mode t))
        mc-test-helper-mode-events))

(defun mc-test-fake-cursors ()
  (sort (copy-sequence (mc/all-fake-cursors))
        (lambda (left right)
          (< (marker-position (overlay-get left 'point))
             (marker-position (overlay-get right 'point))))))

(defun mc-test-cursor-state (cursor)
  (let* ((point (marker-position (overlay-get cursor 'point)))
         (mark (marker-position (overlay-get cursor 'mark)))
         (region (overlay-get cursor 'region-overlay))
         (active (and (overlay-get cursor 'mark-active) t)))
    (list :point point
          :mark mark
          :active active
          :cursor-range (list (overlay-start cursor) (overlay-end cursor))
          :region-range (and region
                             (list (overlay-start region) (overlay-end region)))
          :region-text (and active
                            (buffer-substring-no-properties
                             (min point mark) (max point mark))))))

(defun mc-test-state ()
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :real (list :point (point)
                    :mark (mark t)
                    :active (and (use-region-p) t)
                    :region-text
                    (and (use-region-p)
                         (buffer-substring-no-properties
                          (region-beginning) (region-end))))
        :fake (mapcar #'mc-test-cursor-state (mc-test-fake-cursors))
        :count (mc/num-cursors)
        :mode (and multiple-cursors-mode t)))

(defun mc-test-invisible-overlays ()
  (mapcar
   (lambda (overlay)
     (list :range (list (overlay-start overlay) (overlay-end overlay))
           :text (buffer-substring-no-properties
                  (overlay-start overlay) (overlay-end overlay))
           :invisible (overlay-get overlay 'invisible)
           :intangible (overlay-get overlay 'intangible)
           :after-string (overlay-get overlay 'after-string)))
   (sort
    (cl-remove-if-not
     (lambda (overlay) (overlay-get overlay hum/invisible-overlay-name))
     (overlays-in (point-min) (point-max)))
    (lambda (left right) (< (overlay-start left) (overlay-start right))))))
"##;

fn multiple_cursors_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MULTIPLE_CURSORS_MELPA_PIN, "multiple-cursors.el")
        .expect("prepare pinned multiple-cursors source below ./tmp")
        .with_prelude(MULTIPLE_CURSORS_TEST_PRELUDE)
        .with_timeout(MULTIPLE_CURSORS_TEST_TIMEOUT)
}

fn edit_beginnings_replays_a_real_prefix_command_across_selected_task_lines() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((mc--current-cursor-id 0)
        (transient-mark-mode t))
    (insert "ship release\nupdate changelog\nnotify users\n")
    (goto-char (point-min))
    (move-to-column 4)
    (set-mark (point))
    (forward-line 2)
    (move-to-column 4)
    (activate-mark)
    (let ((before (mc-test-state)))
      (mc/edit-beginnings-of-lines)
      (let ((selected (mc-test-state)))
        (mc/execute-command-for-all-cursors #'mc-test-insert-prefix)
        (let ((edited (mc-test-state)))
          (multiple-cursors-mode -1)
          (list :before before
                :selected selected
                :edited edited
                :finished (mc-test-state)))))))
"##;
    let expect = expect![[
        r####"OK (:before (:text "ship release\nupdate changelog\nnotify users\n" :real (:point 35 :mark 5 :active t :region-text " release\nupdate changelog\nnoti") :fake nil :count 1 :mode nil) :selected (:text "ship release\nupdate changelog\nnotify users\n" :real (:point 31 :mark 35 :active nil :region-text nil) :fake ((:point 1 :mark 35 :active nil :cursor-range (1 2) :region-range nil :region-text nil) (:point 14 :mark 35 :active nil :cursor-range (14 15) :region-range nil :region-text nil)) :count 3 :mode t) :edited (:text "TODO: ship release\nTODO: update changelog\nTODO: notify users\n" :real (:point 49 :mark 53 :active nil :region-text nil) :fake ((:point 7 :mark 53 :active nil :cursor-range (7 8) :region-range nil :region-text nil) (:point 26 :mark 53 :active nil :cursor-range (26 27) :region-range nil :region-text nil)) :count 3 :mode t) :finished (:text "TODO: ship release\nTODO: update changelog\nTODO: notify users\n" :real (:point 49 :mark 53 :active nil :region-text nil) :fake nil :count 1 :mode nil))"####
    ]];
    ParityBatchCase::value(
        "edit_beginnings_replays_a_real_prefix_command_across_selected_task_lines",
        elisp_form,
        expect,
    )
}

fn mark_all_symbols_renames_complete_identifiers_and_keyboard_quit_unwinds_regions_first()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((mc--current-cursor-id 0)
        (transient-mark-mode t))
    (emacs-lisp-mode)
    (insert "(let ((customer_id 1)) (+ customer_id customer customer_id))")
    (goto-char (point-min))
    (search-forward "customer_id")
    (backward-char 3)
    (mc/mark-all-symbols-like-this)
    (let ((selected (mc-test-state)))
      (mc/execute-command-for-all-cursors #'upcase-region)
      (let ((renamed (mc-test-state)))
        (mc/keyboard-quit)
        (let ((regions-quit (mc-test-state)))
          (mc/keyboard-quit)
          (list :selected selected
                :renamed renamed
                :regions-quit regions-quit
                :finished (mc-test-state)))))))
"##;
    let expect = expect![[
        r####"OK (:selected (:text "(let ((customer_id 1)) (+ customer_id customer customer_id))" :real (:point 19 :mark 8 :active t :region-text "customer_id") :fake ((:point 38 :mark 27 :active t :cursor-range (38 39) :region-range (27 38) :region-text "customer_id") (:point 59 :mark 48 :active t :cursor-range (59 60) :region-range (48 59) :region-text "customer_id")) :count 3 :mode t) :renamed (:text "(let ((CUSTOMER_ID 1)) (+ CUSTOMER_ID customer CUSTOMER_ID))" :real (:point 19 :mark 8 :active t :region-text "CUSTOMER_ID") :fake ((:point 38 :mark 27 :active nil :cursor-range (38 39) :region-range nil :region-text nil) (:point 59 :mark 48 :active nil :cursor-range (59 60) :region-range nil :region-text nil)) :count 3 :mode t) :regions-quit (:text "(let ((CUSTOMER_ID 1)) (+ CUSTOMER_ID customer CUSTOMER_ID))" :real (:point 19 :mark 8 :active nil :region-text nil) :fake ((:point 38 :mark 27 :active nil :cursor-range (38 39) :region-range nil :region-text nil) (:point 59 :mark 48 :active nil :cursor-range (59 60) :region-range nil :region-text nil)) :count 3 :mode t) :finished (:text "(let ((CUSTOMER_ID 1)) (+ CUSTOMER_ID customer CUSTOMER_ID))" :real (:point 19 :mark 8 :active nil :region-text nil) :fake nil :count 1 :mode nil))"####
    ]];
    ParityBatchCase::value(
        "mark_all_symbols_renames_complete_identifiers_and_keyboard_quit_unwinds_regions_first",
        elisp_form,
        expect,
    )
}

fn ordered_number_and_letter_insertion_builds_stable_labels_from_top_to_bottom() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (let ((mc--current-cursor-id 0))
    (insert "alpha:\nbeta:\ngamma:\n")
    (goto-char (point-min))
    (set-mark (point))
    (forward-line 2)
    (end-of-line)
    (activate-mark)
    (mc/edit-ends-of-lines)
    (let ((selected (mc-test-state)))
      (mc/insert-numbers 7)
      (mc/execute-command-for-all-cursors #'mc-test-insert-separator)
      (mc/insert-letters 24)
      (let ((labeled (mc-test-state)))
        (multiple-cursors-mode -1)
        (list :selected selected
              :labeled labeled
              :finished (mc-test-state))))))
"##;
    let expect = expect![[
        r####"OK (:selected (:text "alpha:\nbeta:\ngamma:\n" :real (:point 20 :mark 20 :active nil :region-text nil) :fake ((:point 7 :mark 20 :active nil :cursor-range (7 7) :region-range nil :region-text nil) (:point 13 :mark 20 :active nil :cursor-range (13 13) :region-range nil :region-text nil)) :count 3 :mode t) :labeled (:text "alpha:7 / y\nbeta:8 / z\ngamma:9 / aa\n" :real (:point 36 :mark 30 :active nil :region-text nil) :fake ((:point 12 :mark 30 :active nil :cursor-range (12 12) :region-range nil :region-text nil) (:point 23 :mark 30 :active nil :cursor-range (23 23) :region-range nil :region-text nil)) :count 3 :mode t) :finished (:text "alpha:7 / y\nbeta:8 / z\ngamma:9 / aa\n" :real (:point 36 :mark 30 :active nil :region-text nil) :fake nil :count 1 :mode nil))"####
    ]];
    ParityBatchCase::value(
        "ordered_number_and_letter_insertion_builds_stable_labels_from_top_to_bottom",
        elisp_form,
        expect,
    )
}

fn sort_and_reverse_regions_reorder_parallel_list_values_without_moving_delimiters()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (cl-labels
      ((run
        (operation)
        (erase-buffer)
        (let ((mc--current-cursor-id 0)
              (transient-mark-mode t))
          (insert "(pear)\n(apple)\n(orange)\n")
          (goto-char (point-min))
          (dotimes (index 3)
            (re-search-forward "[[:alpha:]]+")
            (set-mark (match-beginning 0))
            (goto-char (match-end 0))
            (activate-mark)
            (when (< index 2)
              (mc/create-fake-cursor-at-point)))
          (multiple-cursors-mode 1)
          (let ((selected (mc-test-state))
                (original-order (mc--ordered-region-strings)))
            (funcall operation)
            (let ((transformed (mc-test-state)))
              (multiple-cursors-mode -1)
              (list :selected selected
                    :original-order original-order
                    :transformed transformed
                    :finished (mc-test-state)))))))
    (list :sort (run #'mc/sort-regions)
          :reverse (run #'mc/reverse-regions))))
"##;
    let expect = expect![[
        r####"OK (:sort (:selected (:text "(pear)\n(apple)\n(orange)\n" :real (:point 23 :mark 17 :active t :region-text "orange") :fake ((:point 6 :mark 2 :active t :cursor-range (6 7) :region-range (2 6) :region-text "pear") (:point 14 :mark 9 :active t :cursor-range (14 15) :region-range (9 14) :region-text "apple")) :count 3 :mode t) :original-order ("pear" "apple" "orange") :transformed (:text "(apple)\n(orange)\n(pear)\n" :real (:point 19 :mark 19 :active nil :region-text nil) :fake ((:point 2 :mark 2 :active nil :cursor-range (2 3) :region-range nil :region-text nil) (:point 10 :mark 10 :active nil :cursor-range (10 11) :region-range nil :region-text nil)) :count 3 :mode t) :finished (:text "(apple)\n(orange)\n(pear)\n" :real (:point 19 :mark 19 :active nil :region-text nil) :fake nil :count 1 :mode nil)) :reverse (:selected (:text "(pear)\n(apple)\n(orange)\n" :real (:point 23 :mark 17 :active t :region-text "orange") :fake ((:point 6 :mark 2 :active t :cursor-range (6 7) :region-range (2 6) :region-text "pear") (:point 14 :mark 9 :active t :cursor-range (14 15) :region-range (9 14) :region-text "apple")) :count 3 :mode t) :original-order ("pear" "apple" "orange") :transformed (:text "(orange)\n(apple)\n(pear)\n" :real (:point 19 :mark 19 :active nil :region-text nil) :fake ((:point 2 :mark 2 :active nil :cursor-range (2 3) :region-range nil :region-text nil) (:point 11 :mark 11 :active nil :cursor-range (11 12) :region-range nil :region-text nil)) :count 3 :mode t) :finished (:text "(orange)\n(apple)\n(pear)\n" :real (:point 19 :mark 19 :active nil :region-text nil) :fake nil :count 1 :mode nil)))"####
    ]];
    ParityBatchCase::value(
        "sort_and_reverse_regions_reorder_parallel_list_values_without_moving_delimiters",
        elisp_form,
        expect,
    )
}

fn vertical_alignment_places_values_at_one_column_across_uneven_configuration_keys()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((mc--current-cursor-id 0)
        (transient-mark-mode t))
    (insert "a=1\nlong_name=2\nmid=3\n")
    (mc/mark-all-in-region (point-min) (point-max) "=")
    (let ((selected (mc-test-state)))
      (mc/vertical-align-with-space)
      (let ((aligned (mc-test-state))
            columns)
        (mc/for-each-cursor-ordered
         (push (save-excursion
                 (goto-char (overlay-get cursor 'point))
                 (current-column))
               columns))
        (multiple-cursors-mode -1)
        (list :selected selected
              :aligned aligned
              :cursor-columns (nreverse columns)
              :finished (mc-test-state))))))
"##;
    let expect = expect![[
        r####"OK (:selected (:text "a=1\nlong_name=2\nmid=3\n" :real (:point 3 :mark 2 :active nil :region-text nil) :fake ((:point 15 :mark 14 :active nil :cursor-range (15 16) :region-range nil :region-text nil) (:point 21 :mark 20 :active nil :cursor-range (21 22) :region-range nil :region-text nil)) :count 3 :mode t) :aligned (:text "a=        1\nlong_name=2\nmid=      3\n" :real (:point 11 :mark 2 :active nil :region-text nil) :fake ((:point 23 :mark 22 :active nil :cursor-range (23 24) :region-range nil :region-text nil) (:point 35 :mark 28 :active nil :cursor-range (35 36) :region-range nil :region-text nil)) :count 3 :mode t) :cursor-columns (10 10 10) :finished (:text "a=        1\nlong_name=2\nmid=      3\n" :real (:point 11 :mark 2 :active nil :region-text nil) :fake nil :count 1 :mode nil))"####
    ]];
    ParityBatchCase::value(
        "vertical_alignment_places_values_at_one_column_across_uneven_configuration_keys",
        elisp_form,
        expect,
    )
}

fn mode_lifecycle_temporarily_disables_incompatible_modes_and_restores_hooks_and_cursors()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((mc--current-cursor-id 0)
        (mc-test-helper-mode-events nil)
        (mc/unsupported-minor-modes '(mc-test-helper-mode))
        (multiple-cursors-mode-enabled-hook nil)
        (multiple-cursors-mode-disabled-hook nil)
        lifecycle)
    (add-hook 'multiple-cursors-mode-enabled-hook
              (lambda () (push :mc-enabled lifecycle)))
    (add-hook 'multiple-cursors-mode-disabled-hook
              (lambda () (push :mc-disabled lifecycle)))
    (insert "abcdef")
    (mc-test-helper-mode 1)
    (goto-char 2)
    (mc/create-fake-cursor-at-point)
    (goto-char 5)
    (multiple-cursors-mode 1)
    (let ((enabled
           (list :state (mc-test-state)
                 :helper (and mc-test-helper-mode t)
                 :disabled-modes mc/temporarily-disabled-minor-modes
                 :pre-hook (and (memq #'mc/make-a-note-of-the-command-being-run
                                      pre-command-hook) t)
                 :post-hook (and (memq #'mc/execute-this-command-for-all-cursors
                                       post-command-hook) t)
                 :keymap
                 (mapcar
                  (lambda (key) (list key (lookup-key mc/keymap (kbd key))))
                  '("C-g" "<return>" "C-:" "C-v" "M-v" "C-'")))))
      (multiple-cursors-mode -1)
      (list :enabled enabled
            :finished
            (list :state (mc-test-state)
                  :helper (and mc-test-helper-mode t)
                  :disabled-modes mc/temporarily-disabled-minor-modes
                  :pre-hook (and (memq #'mc/make-a-note-of-the-command-being-run
                                       pre-command-hook) t)
                  :post-hook (and (memq #'mc/execute-this-command-for-all-cursors
                                        post-command-hook) t))
            :helper-events (nreverse mc-test-helper-mode-events)
            :lifecycle (nreverse lifecycle)))))
"##;
    let expect = expect![[
        r####"OK (:enabled (:state (:text "abcdef" :real (:point 5 :mark nil :active nil :region-text nil) :fake ((:point 2 :mark nil :active nil :cursor-range (2 3) :region-range nil :region-text nil)) :count 2 :mode t) :helper nil :disabled-modes (mc-test-helper-mode) :pre-hook t :post-hook t :keymap (("C-g" mc/keyboard-quit) ("<return>" multiple-cursors-mode) ("C-:" mc/repeat-command) ("C-v" mc/cycle-forward) ("M-v" mc/cycle-backward) ("C-'" mc-hide-unmatched-lines-mode))) :finished (:state (:text "abcdef" :real (:point 5 :mark nil :active nil :region-text nil) :fake nil :count 1 :mode nil) :helper t :disabled-modes nil :pre-hook nil :post-hook nil) :helper-events ((:helper-mode t) (:helper-mode nil) (:helper-mode t)) :lifecycle (:mc-enabled :mc-disabled))"####
    ]];
    ParityBatchCase::value(
        "mode_lifecycle_temporarily_disables_incompatible_modes_and_restores_hooks_and_cursors",
        elisp_form,
        expect,
    )
}

fn hide_unmatched_lines_focuses_distant_cursor_contexts_and_restores_the_full_document()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((mc--current-cursor-id 0)
        (hum/lines-to-expand 1)
        (hum/placeholder "[hidden]\n"))
    (dotimes (line 12)
      (insert (format "line-%02d payload\n" (1+ line))))
    (goto-char (point-min))
    (forward-line 2)
    (move-to-column 4)
    (mc/create-fake-cursor-at-point)
    (goto-char (point-min))
    (forward-line 8)
    (move-to-column 4)
    (multiple-cursors-mode 1)
    (let ((selected (mc-test-state)))
      (mc-hide-unmatched-lines-mode 1)
      (let ((focused
             (list :hide-mode (and mc-hide-unmatched-lines-mode t)
                   :cursor-count (mc/num-cursors)
                   :mc-mode (and multiple-cursors-mode t)
                   :overlays (mc-test-invisible-overlays)
                   :quit-key
                   (lookup-key hum/hide-unmatched-lines-mode-map (kbd "C-g")))))
        (mc-hide-unmatched-lines-mode -1)
        (let ((revealed
               (list :hide-mode (and mc-hide-unmatched-lines-mode t)
                     :cursor-count (mc/num-cursors)
                     :mc-mode (and multiple-cursors-mode t)
                     :overlays (mc-test-invisible-overlays))))
          (multiple-cursors-mode -1)
          (list :selected selected
                :focused focused
                :revealed revealed
                :finished (mc-test-state)))))))
"##;
    let expect = expect![[
        r####"OK (:selected (:text "line-01 payload\nline-02 payload\nline-03 payload\nline-04 payload\nline-05 payload\nline-06 payload\nline-07 payload\nline-08 payload\nline-09 payload\nline-10 payload\nline-11 payload\nline-12 payload\n" :real (:point 133 :mark nil :active nil :region-text nil) :fake ((:point 37 :mark nil :active nil :cursor-range (37 38) :region-range nil :region-text nil)) :count 2 :mode t) :focused (:hide-mode t :cursor-count 2 :mc-mode t :overlays ((:range (65 112) :text "line-05 payload\nline-06 payload\nline-07 payload" :invisible t :intangible t :after-string "[hidden]\n") (:range (161 193) :text "line-11 payload\nline-12 payload\n" :invisible t :intangible t :after-string "[hidden]\n")) :quit-key hum/keyboard-quit) :revealed (:hide-mode nil :cursor-count 2 :mc-mode t :overlays nil) :finished (:text "line-01 payload\nline-02 payload\nline-03 payload\nline-04 payload\nline-05 payload\nline-06 payload\nline-07 payload\nline-08 payload\nline-09 payload\nline-10 payload\nline-11 payload\nline-12 payload\n" :real (:point 133 :mark nil :active nil :region-text nil) :fake nil :count 1 :mode nil))"####
    ]];
    ParityBatchCase::value(
        "hide_unmatched_lines_focuses_distant_cursor_contexts_and_restores_the_full_document",
        elisp_form,
        expect,
    )
}

#[test]
fn multiple_cursors_package_batch() {
    let cases = vec![
        edit_beginnings_replays_a_real_prefix_command_across_selected_task_lines(),
        mark_all_symbols_renames_complete_identifiers_and_keyboard_quit_unwinds_regions_first(),
        ordered_number_and_letter_insertion_builds_stable_labels_from_top_to_bottom(),
        sort_and_reverse_regions_reorder_parallel_list_values_without_moving_delimiters(),
        vertical_alignment_places_values_at_one_column_across_uneven_configuration_keys(),
        mode_lifecycle_temporarily_disables_incompatible_modes_and_restores_hooks_and_cursors(),
        hide_unmatched_lines_focuses_distant_cursor_contexts_and_restores_the_full_document(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed multiple-cursors parity test");
    assert_oracle_batch_cases(
        multiple_cursors_oracle(),
        test_name,
        "multiple_cursors_parity",
        &cases,
    );
}
