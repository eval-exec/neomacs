use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, VISUAL_FILL_COLUMN_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'face-remap)
(require 'visual-fill-column)

(defun neomacs-vfc-test-window-state (&optional window)
  "Describe WINDOW's text viewport and Visual Fill Column parameters."
  (setq window (or window (selected-window)))
  (list :total-width (window-total-width window)
        :text-width (window-width window)
        :remapped-width (window-width window 'remap)
        :margins (window-margins window)
        :fringes (window-fringes window)
        :split-window (window-parameter window 'split-window)
        :min-margins (window-parameter window 'min-margins)))

(defun neomacs-vfc-test-visual-line-positions (count)
  "Return positions reached by moving COUNT visual lines from buffer start."
  (goto-char (point-min))
  (let ((positions (list (point))))
    (dotimes (_ count)
      (vertical-motion 1)
      (push (point) positions))
    (nreverse positions)))
"####;

fn edits_a_release_note_at_fill_column_without_changing_its_text() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (delete-other-windows)
  (with-temp-buffer
    (switch-to-buffer (current-buffer))
    (insert
     "Release 2.7 keeps prose readable while preserving every logical line "
     "for review, version control, and later export.\n"
     "Operators can resize the editor without reflowing the source document.\n")
    (setq-local fill-column 48)
    (let ((original (buffer-string)))
      (add-hook 'visual-line-mode-hook #'visual-fill-column-for-vline nil t)
      (visual-line-mode 1)
      (let ((enabled
             (list
              :visual-line visual-line-mode
              :visual-fill visual-fill-column-mode
              :word-wrap word-wrap
              :line-move-visual line-move-visual
              :viewport (neomacs-vfc-test-window-state)
              :screen-lines (count-screen-lines (point-min) (point-max))
              :visual-positions (neomacs-vfc-test-visual-line-positions 4)
              :window-hook
              (not (null (memq #'visual-fill-column--adjust-window
                               window-state-change-functions))))))
        (visual-line-mode -1)
        (list
         :enabled enabled
         :disabled
         (list :visual-line visual-line-mode
               :visual-fill visual-fill-column-mode
               :word-wrap word-wrap
               :line-move-visual line-move-visual
               :viewport (neomacs-vfc-test-window-state))
         :text (buffer-string)
         :text-unchanged (equal original (buffer-string))
         :hook-installed
         (not (null (memq #'visual-fill-column-for-vline
                          visual-line-mode-hook))))))))
"####;
    let expected = expect![[
        r#"OK (:enabled (:visual-line t :visual-fill t :word-wrap t :line-move-visual t :viewport (:total-width 80 :text-width 48 :remapped-width 48 :margins (nil . 32) :fringes (0 0 nil nil) :split-window nil :min-margins (0 . 0)) :screen-lines 5 :visual-positions (1 48 95 117 164) :window-hook t) :disabled (:visual-line nil :visual-fill nil :word-wrap nil :line-move-visual t :viewport (:total-width 80 :text-width 80 :remapped-width 80 :margins (nil) :fringes (0 0 nil nil) :split-window nil :min-margins nil)) :text "Release 2.7 keeps prose readable while preserving every logical line for review, version control, and later export.\nOperators can resize the editor without reflowing the source document.\n" :text-unchanged t :hook-installed t)"#
    ]];
    ParityBatchCase::value(
        "edits_a_release_note_at_fill_column_without_changing_its_text",
        elisp_form,
        expected,
    )
}

fn toggles_centered_writing_layout_with_line_number_allowance() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (delete-other-windows)
  (with-temp-buffer
    (switch-to-buffer (current-buffer))
    (insert "Draft body\n")
    (setq-local visual-fill-column-width 44)
    (setq-local visual-fill-column-center-text t)
    (setq-local visual-fill-column-extra-text-width '(4 . 2))
    (setq-local visual-fill-column-fringes-outside-margins nil)
    (unwind-protect
        (progn
          (visual-fill-column-mode 1)
          (let ((centered (neomacs-vfc-test-window-state)))
            (visual-fill-column-toggle-center-text)
            (let ((left-aligned (neomacs-vfc-test-window-state)))
              (visual-fill-column-toggle-center-text)
              (list :centered centered
                    :left-aligned left-aligned
                    :centered-again (neomacs-vfc-test-window-state)
                    :center-option visual-fill-column-center-text
                    :extra-width visual-fill-column-extra-text-width
                    :text (buffer-string)))))
      (visual-fill-column-mode -1))))
"####;
    let expected = expect![[
        r#"OK (:centered (:total-width 80 :text-width 50 :remapped-width 50 :margins (14 . 16) :fringes (0 0 nil nil) :split-window nil :min-margins #1=(0 . 0)) :left-aligned (:total-width 80 :text-width 46 :remapped-width 46 :margins (nil . 34) :fringes (0 0 nil nil) :split-window nil :min-margins #1#) :centered-again (:total-width 80 :text-width 50 :remapped-width 50 :margins (14 . 16) :fringes (0 0 nil nil) :split-window nil :min-margins #1#) :center-option t :extra-width (4 . 2) :text "Draft body\n")"#
    ]];
    ParityBatchCase::value(
        "toggles_centered_writing_layout_with_line_number_allowance",
        elisp_form,
        expected,
    )
}

fn places_a_right_to_left_reading_view_at_the_right_edge() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (delete-other-windows)
  (with-temp-buffer
    (switch-to-buffer (current-buffer))
    (insert "مرحبا بفريق الإصدار\nهذا تقرير النشر اليومي.\n")
    (setq-local bidi-paragraph-direction 'right-to-left)
    (setq-local visual-fill-column-width 50)
    (setq-local visual-fill-column-center-text nil)
    (unwind-protect
        (progn
          (visual-fill-column-mode 1)
          (let ((right-aligned (neomacs-vfc-test-window-state)))
            (visual-fill-column-toggle-center-text)
            (list :right-aligned right-aligned
                  :centered (neomacs-vfc-test-window-state)
                  :direction bidi-paragraph-direction
                  :text (buffer-string))))
      (visual-fill-column-mode -1))))
"####;
    let expected = expect![[
        r#"OK (:right-aligned (:total-width 80 :text-width 50 :remapped-width 50 :margins (30) :fringes (0 0 nil nil) :split-window nil :min-margins #1=(0 . 0)) :centered (:total-width 80 :text-width 50 :remapped-width 50 :margins (15 . 15) :fringes (0 0 nil nil) :split-window nil :min-margins #1#) :direction right-to-left :text "مرحبا بفريق الإصدار\nهذا تقرير النشر اليومي.\n")"#
    ]];
    ParityBatchCase::value(
        "places_a_right_to_left_reading_view_at_the_right_edge",
        elisp_form,
        expected,
    )
}

fn keeps_two_editing_panes_usable_during_split_and_resize() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (delete-other-windows)
  (let ((draft (generate-new-buffer " *vfc-release-draft*"))
        (checklist (generate-new-buffer " *vfc-release-checklist*")))
    (unwind-protect
        (let* ((left (selected-window))
               right
               initial
               split-state
               resized-state
               disabled-state)
          (switch-to-buffer draft)
          (insert "Release announcement draft\n")
          (setq-local visual-fill-column-width 34)
          (setq-local visual-fill-column-center-text t)
          (visual-fill-column-mode 1)
          (setq initial (neomacs-vfc-test-window-state left))

          (setq right (split-window-right))
          (set-window-buffer right checklist)
          (with-selected-window right
            (insert "[ ] package\n[ ] sign\n[ ] publish\n")
            (setq-local visual-fill-column-width 28)
            (setq-local visual-fill-column-center-text nil)
            (visual-fill-column-mode 1))
          (visual-fill-column--adjust-window left)
          (visual-fill-column--adjust-window right)
          (setq split-state
                (list :left (neomacs-vfc-test-window-state left)
                      :right (neomacs-vfc-test-window-state right)))

          (window-resize left 8 t)
          (visual-fill-column--adjust-window left)
          (visual-fill-column--adjust-window right)
          (setq resized-state
                (list :left (neomacs-vfc-test-window-state left)
                      :right (neomacs-vfc-test-window-state right)))

          (with-selected-window left
            (visual-fill-column-mode -1))
          (setq disabled-state
                (list :left (neomacs-vfc-test-window-state left)
                      :right (neomacs-vfc-test-window-state right)
                      :right-mode
                      (buffer-local-value 'visual-fill-column-mode checklist)))
          (list :initial initial
                :after-split split-state
                :after-resize resized-state
                :after-left-disabled disabled-state
                :draft (with-current-buffer draft (buffer-string))
                :checklist (with-current-buffer checklist (buffer-string))))
      (when (buffer-live-p draft)
        (with-current-buffer draft
          (when visual-fill-column-mode (visual-fill-column-mode -1)))
        (kill-buffer draft))
      (when (buffer-live-p checklist)
        (with-current-buffer checklist
          (when visual-fill-column-mode (visual-fill-column-mode -1)))
        (kill-buffer checklist)))))
"####;
    let expected = expect![[
        r#"OK (:initial (:total-width 80 :text-width 34 :remapped-width 34 :margins (23 . 23) :fringes (0 0 nil nil) :split-window nil :min-margins #1=(0 . 0)) :after-split (:left (:total-width 40 :text-width 34 :remapped-width 34 :margins (2 . 3) :fringes (0 0 nil nil) :split-window nil :min-margins #1#) :right (:total-width 40 :text-width 28 :remapped-width 28 :margins (nil . 12) :fringes (0 0 nil nil) :split-window nil :min-margins #1#)) :after-resize (:left (:total-width 48 :text-width 34 :remapped-width 34 :margins (6 . 7) :fringes (0 0 nil nil) :split-window nil :min-margins #1#) :right (:total-width 32 :text-width 28 :remapped-width 28 :margins (nil . 4) :fringes (0 0 nil nil) :split-window nil :min-margins #1#)) :after-left-disabled (:left (:total-width 48 :text-width 47 :remapped-width 47 :margins (nil) :fringes (0 0 nil nil) :split-window nil :min-margins nil) :right (:total-width 32 :text-width 28 :remapped-width 28 :margins (nil . 4) :fringes (0 0 nil nil) :split-window nil :min-margins #1#) :right-mode t) :draft "Release announcement draft\n" :checklist "[ ] package\n[ ] sign\n[ ] publish\n")"#
    ]];
    ParityBatchCase::value(
        "keeps_two_editing_panes_usable_during_split_and_resize",
        elisp_form,
        expected,
    )
}

fn readjusts_the_viewport_when_a_reader_changes_text_scale() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (delete-other-windows)
  (with-temp-buffer
    (switch-to-buffer (current-buffer))
    (insert "Incident review\n\nA readable report remains fifty columns wide.\n")
    (setq-local visual-fill-column-width 50)
    (setq-local visual-fill-column-center-text t)
    (let ((text-scale-mode-step 1.2))
      (unwind-protect
          (progn
            (visual-fill-column-mode 1)
            (let ((normal
                   (list :amount text-scale-mode-amount
                         :max-width
                         (visual-fill-column--window-max-text-width)
                         :viewport (neomacs-vfc-test-window-state))))
              (text-scale-increase 2)
              (visual-fill-column-adjust)
              (let ((enlarged
                     (list :amount text-scale-mode-amount
                           :scale (expt text-scale-mode-step
                                        text-scale-mode-amount)
                           :max-width
                           (visual-fill-column--window-max-text-width)
                           :viewport (neomacs-vfc-test-window-state))))
                (text-scale-increase 0)
                (visual-fill-column-adjust)
                (list :normal normal
                      :enlarged enlarged
                      :reset
                      (list :amount text-scale-mode-amount
                            :mode text-scale-mode
                            :max-width
                            (visual-fill-column--window-max-text-width)
                            :viewport (neomacs-vfc-test-window-state))
                      :text (buffer-string)))))
        (text-scale-mode -1)
        (visual-fill-column-mode -1)))))
"####;
    let expected = expect![[
        r#"OK (:normal (:amount 0 :max-width 80 :viewport (:total-width 80 :text-width 50 :remapped-width 50 :margins (15 . 15) :fringes (0 0 nil nil) :split-window nil :min-margins #1=(0 . 0))) :enlarged (:amount 2 :scale 1.44 :max-width 55 :viewport (:total-width 80 :text-width 75 :remapped-width 75 :margins (2 . 3) :fringes (0 0 nil nil) :split-window nil :min-margins #1#)) :reset (:amount 0 :mode nil :max-width 80 :viewport (:total-width 80 :text-width 50 :remapped-width 50 :margins (15 . 15) :fringes (0 0 nil nil) :split-window nil :min-margins #1#)) :text "Incident review\n\nA readable report remains fifty columns wide.\n")"#
    ]];
    ParityBatchCase::value(
        "readjusts_the_viewport_when_a_reader_changes_text_scale",
        elisp_form,
        expected,
    )
}

fn sensible_display_split_ignores_temporary_wide_margins() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (delete-other-windows)
  (with-temp-buffer
    (switch-to-buffer (current-buffer))
    (insert "Primary writing pane\n")
    (setq-local visual-fill-column-width 36)
    (setq-local visual-fill-column-center-text t)
    (let ((visual-fill-column-enable-sensible-window-split t)
          (split-window-preferred-direction 'horizontal)
          (split-width-threshold 60)
          (split-height-threshold nil))
      (unwind-protect
          (progn
            (visual-fill-column-mode 1)
            (let* ((window (selected-window))
                   (before (neomacs-vfc-test-window-state window))
                   (new (visual-fill-column-split-window-sensibly window)))
              (visual-fill-column--adjust-window window)
              (visual-fill-column--adjust-window new)
              (list
               :preferred split-window-preferred-function
               :before before
               :old-window (neomacs-vfc-test-window-state window)
               :new-window (neomacs-vfc-test-window-state new)
               :same-buffer (eq (window-buffer window) (window-buffer new))
               :window-count (length (window-list)))))
        (visual-fill-column-mode -1)))))
"####;
    let expected = expect![
        "OK (:preferred visual-fill-column-split-window-sensibly :before (:total-width 80 :text-width 36 :remapped-width 36 :margins (22 . 22) :fringes (0 0 nil nil) :split-window nil :min-margins #1=(0 . 0)) :old-window (:total-width 40 :text-width 36 :remapped-width 36 :margins (1 . 2) :fringes (0 0 nil nil) :split-window nil :min-margins #1#) :new-window (:total-width 40 :text-width 36 :remapped-width 36 :margins (2 . 2) :fringes (0 0 nil nil) :split-window nil :min-margins #1#) :same-buffer t :window-count 2)"
    ];
    ParityBatchCase::value(
        "sensible_display_split_ignores_temporary_wide_margins",
        elisp_form,
        expected,
    )
}

fn global_mode_admits_only_file_visiting_workspaces() -> ParityBatchCase {
    let elisp_form = r####"
(let ((existing-file (generate-new-buffer " *vfc-existing-file*"))
      (existing-scratch (generate-new-buffer " *vfc-existing-scratch*"))
      (later-file (generate-new-buffer " *vfc-later-file*"))
      (later-scratch (generate-new-buffer " *vfc-later-scratch*")))
  (unwind-protect
      (progn
        (global-visual-fill-column-mode -1)
        (with-current-buffer existing-file
          (setq buffer-file-name "/workspace/reports/existing.md")
          (text-mode))
        (with-current-buffer existing-scratch
          (text-mode))
        (global-visual-fill-column-mode 1)
        (with-current-buffer later-file
          (setq buffer-file-name "/workspace/reports/later.md")
          (text-mode))
        (with-current-buffer later-scratch
          (text-mode))
        (let ((enabled
               (list
                :global global-visual-fill-column-mode
                :existing-file
                (buffer-local-value 'visual-fill-column-mode existing-file)
                :existing-scratch
                (buffer-local-value 'visual-fill-column-mode existing-scratch)
                :later-file
                (buffer-local-value 'visual-fill-column-mode later-file)
                :later-scratch
                (buffer-local-value 'visual-fill-column-mode later-scratch))))
          (global-visual-fill-column-mode -1)
          (list
           :enabled enabled
           :disabled
           (list
            :global global-visual-fill-column-mode
            :existing-file
            (buffer-local-value 'visual-fill-column-mode existing-file)
            :existing-scratch
            (buffer-local-value 'visual-fill-column-mode existing-scratch)
            :later-file
            (buffer-local-value 'visual-fill-column-mode later-file)
            :later-scratch
            (buffer-local-value 'visual-fill-column-mode later-scratch)))))
    (global-visual-fill-column-mode -1)
    (dolist (buffer (list existing-file existing-scratch later-file later-scratch))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))
"####;
    let expected = expect![
        "OK (:enabled (:global t :existing-file t :existing-scratch nil :later-file t :later-scratch nil) :disabled (:global nil :existing-file nil :existing-scratch nil :later-file nil :later-scratch nil))"
    ];
    ParityBatchCase::value(
        "global_mode_admits_only_file_visiting_workspaces",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn visual_fill_column_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(VISUAL_FILL_COLUMN_MELPA_PIN, "visual-fill-column.el")
        .expect("prepare pinned Visual Fill Column source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn visual_fill_column_practical_workflows_batch() {
    let cases = vec![
        edits_a_release_note_at_fill_column_without_changing_its_text(),
        toggles_centered_writing_layout_with_line_number_allowance(),
        places_a_right_to_left_reading_view_at_the_right_edge(),
        keeps_two_editing_panes_usable_during_split_and_resize(),
        readjusts_the_viewport_when_a_reader_changes_text_scale(),
        sensible_display_split_ignores_temporary_wide_margins(),
        global_mode_admits_only_file_visiting_workspaces(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("visual-fill-column parity batch");
    assert_oracle_batch_cases(
        visual_fill_column_oracle(),
        test_name,
        "visual-fill-column parity",
        &cases,
    );
}
