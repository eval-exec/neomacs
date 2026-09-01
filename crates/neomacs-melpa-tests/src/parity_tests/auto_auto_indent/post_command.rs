use expect_test::expect;

use super::ParityBatchCase;

fn auto_auto_indent_before_change_marks_only_enabled_buffer_and_accepts_hook_arguments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_before_change_marks_only_enabled_buffer_and_accepts_hook_arguments",
        r##"(mapcar
          (lambda (enabled)
            (with-temp-buffer
              (setq aai-mode enabled
                    aai--change-flag nil)
              (list
               enabled
               (aai-before-change-function
                3
                7
                :extra)
               aai--change-flag)))
          '(nil t 1))"##,
        expect!["OK ((nil nil nil) (t t t) (1 t t))"],
    )
}

fn auto_auto_indent_post_command_returns_early_when_disabled_or_cua_rectangle_active()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_post_command_returns_early_when_disabled_or_cua_rectangle_active",
        r##"(mapcar
          (lambda (case)
            (with-temp-buffer
              (insert "  body")
              (goto-char (point-min))
              (let ((aai-mode
                     (not (eq case 'disabled)))
                    (cua--rectangle
                     (eq case 'rectangle))
                    (aai--change-flag :pending)
                    (aai-indent-function
                     (lambda ()
                       (error
                        "must not indent"))))
                (list
                 case
                 (aai-post-command-hook)
                 (point)
                 aai--change-flag))))
          '(disabled rectangle))"##,
        expect!["OK ((disabled nil 1 :pending) (rectangle nil 1 :pending))"],
    )
}

fn auto_auto_indent_post_command_immediately_indents_first_structural_and_regular_edits()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_post_command_immediately_indents_first_structural_and_regular_edits",
        r##"(mapcar
          (lambda (spec)
            (with-temp-buffer
              (insert "body")
              (set-buffer-modified-p t)
              (let ((aai-mode t)
                    (aai--change-flag t)
                    (this-command
                     (nth 0 spec))
                    (last-command
                     (nth 1 spec))
                    (last-input-event
                     (nth 2 spec))
                    calls)
                (setq aai-indent-function
                      (lambda ()
                        (push
                         (list
                          this-command
                          last-command
                          last-input-event)
                         calls)))
                (list
                 spec
                 (aai-post-command-hook)
                 aai--change-flag
                 (nreverse calls)))))
          '((self-insert-command other 120)
            (self-insert-command
             self-insert-command
             40)
            (forward-word other 120)))"##,
        expect![
            "OK (((self-insert-command other 120) nil t ((self-insert-command other 120))) ((self-insert-command self-insert-command 40) nil t ((self-insert-command self-insert-command 40))) ((forward-word other 120) nil t ((forward-word other 120))))"
        ],
    )
}

fn auto_auto_indent_repeated_nonstructural_self_insert_cancels_and_reschedules_idle_timer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_repeated_nonstructural_self_insert_cancels_and_reschedules_idle_timer",
        r##"(with-temp-buffer
          (insert "body")
          (set-buffer-modified-p t)
          (let ((aai-mode t)
                (aai--change-flag t)
                (this-command
                 'self-insert-command)
                (last-command
                 'self-insert-command)
                (last-input-event ?x)
                (aai--timer
                 :old-timer)
                (aai-timer-delay 0.75)
                events)
            (cl-letf
                (((symbol-function 'cancel-timer)
                  (lambda (timer)
                    (push
                     (list :cancel timer)
                     events)))
                 ((symbol-function
                   'run-with-idle-timer)
                  (lambda (delay repeat callback)
                    (push
                     (list
                      :schedule
                      delay
                      repeat
                      (functionp callback))
                     events)
                    :new-timer)))
              (list
               (aai-post-command-hook)
               aai--change-flag
               aai--timer
               (nreverse events)))))"##,
        expect!["OK (nil t :new-timer ((:cancel :old-timer) (:schedule 0.75 nil t)))"],
    )
}

fn auto_auto_indent_post_command_command_matrix_freezes_immediate_delayed_and_skipped_paths()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_post_command_command_matrix_freezes_immediate_delayed_and_skipped_paths",
        r##"(mapcar
          (lambda (command)
            (with-temp-buffer
              (insert "body")
              (set-buffer-modified-p t)
              (let ((aai-mode t)
                    (aai--change-flag t)
                    (this-command command)
                    (last-command 'other)
                    (last-input-event ?x)
                    events)
                (setq aai-indent-function
                      (lambda ()
                        (push :indent events)))
                (cl-letf
                    (((symbol-function
                       'run-with-idle-timer)
                      (lambda (&rest _)
                        (push :timer events)
                        :timer)))
                  (aai-post-command-hook)
                  (list
                   command
                   (nreverse events)
                   aai--change-flag
                   aai--timer)))))
          '(self-insert-command
            delete-horizontal-space
            quoted-insert
            backward-paragraph
            kill-region
            save-buffer
            undo
            undo-tree-undo
            undo-tree-redo
            forward-word))"##,
        expect![
            "OK ((self-insert-command (:indent) t nil) (delete-horizontal-space nil t nil) (quoted-insert nil t nil) (backward-paragraph nil t nil) (kill-region nil t nil) (save-buffer nil t nil) (undo nil t nil) (undo-tree-undo nil t nil) (undo-tree-redo nil t nil) (forward-word (:indent) t nil))"
        ],
    )
}

fn auto_auto_indent_unmodified_buffer_skips_immediate_indent_but_can_schedule_delayed_work()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_unmodified_buffer_skips_immediate_indent_but_can_schedule_delayed_work",
        r##"(with-temp-buffer
          (insert "body")
          (set-buffer-modified-p nil)
          (let ((aai-mode t)
                (aai--change-flag t)
                (this-command 'forward-word)
                (last-command 'other)
                (last-input-event ?x)
                events)
            (setq aai-indent-function
                  (lambda ()
                    (push :indent events)))
            (cl-letf
                (((symbol-function
                   'run-with-idle-timer)
                  (lambda (&rest _)
                    (push :timer events)
                    :timer)))
              (list
               (aai-post-command-hook)
               (nreverse events)
               aai--timer
               aai--change-flag))))"##,
        expect!["OK (nil (:timer) :timer t)"],
    )
}

fn auto_auto_indent_post_command_cursor_correction_depends_on_navigation_direction()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_post_command_cursor_correction_depends_on_navigation_direction",
        r##"(mapcar
          (lambda (command)
            (with-temp-buffer
              (insert "first\n    second\n")
              (goto-char
               (save-excursion
                 (goto-char (point-min))
                 (forward-line 1)
                 (line-beginning-position)))
              (let ((aai-mode t)
                    (aai-after-change-indentation nil)
                    (this-command command)
                    (last-command 'other)
                    (last-input-event nil))
                (list
                 command
                 (aai-post-command-hook)
                 (point)
                 (line-number-at-pos)
                 (current-column)))))
          '(backward-char
            left-char
            forward-char
            right-char
            previous-line
            next-line
            other-command))"##,
        expect![
            "OK ((backward-char nil 6 1 5) (left-char nil 6 1 5) (forward-char nil 11 2 4) (right-char nil 11 2 4) (previous-line nil 11 2 4) (next-line nil 11 2 4) (other-command nil 7 2 0))"
        ],
    )
}

fn auto_auto_indent_active_region_and_multiple_cursors_suppress_position_correction()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_active_region_and_multiple_cursors_suppress_position_correction",
        r##"(mapcar
          (lambda (case)
            (with-temp-buffer
              (insert "    selected")
              (goto-char (point-min))
              (let ((aai-mode t)
                    (aai-after-change-indentation nil)
                    (this-command 'forward-char)
                    (last-command 'other))
                (setq-local
                 multiple-cursors-mode
                 (eq case 'multiple-cursors))
                (when (eq case 'region)
                  (set-mark (point-max))
                  (activate-mark)
                  (setq deactivate-mark nil))
                (list
                 case
                 (aai-post-command-hook)
                 (point)
                 (current-column)
                 (region-active-p)))))
          '(normal region multiple-cursors))"##,
        expect!["OK ((normal nil 5 4 nil) (region nil 1 0 t) (multiple-cursors nil 1 0 nil))"],
    )
}

fn auto_auto_indent_post_command_swallows_errors_unless_debug_handler_is_enabled() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_auto_indent_post_command_swallows_errors_unless_debug_handler_is_enabled",
        r##"(mapcar
          (lambda (debug-enabled)
            (with-temp-buffer
              (insert "body")
              (set-buffer-modified-p t)
              (let ((aai-mode t)
                    (aai-debug debug-enabled)
                    (aai--change-flag t)
                    (this-command 'forward-word)
                    (last-command 'other)
                    (last-input-event ?x)
                    events)
                (setq aai-indent-function
                      (lambda ()
                        (error
                         "fixture indentation failed")))
                (cl-letf
                    (((symbol-function 'debug)
                      (lambda (&rest arguments)
                        (push arguments events)
                        :debugged)))
                  (list
                   debug-enabled
                   (auto-auto-indent-test-error-data
                    #'aai-post-command-hook)
                   aai--change-flag
                   (nreverse events))))))
          '(nil t))"##,
        expect![[
            r#"OK ((nil (:ok nil) t nil) (t (:ok :debugged) t ((nil (error "fixture indentation failed")))))"#
        ]],
    )
}

fn auto_auto_indent_real_insert_hook_and_post_command_reformat_practical_lisp_edit()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_real_insert_hook_and_post_command_reformat_practical_lisp_edit",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(progn\n"
           "(message \"one\"))\n")
          (goto-char (point-min))
          (forward-line 1)
          (auto-auto-indent-mode 1)
          (let ((this-command
                 'self-insert-command)
                (last-command 'other)
                (last-input-event ?\s))
            (insert " ")
            (let ((flag-before
                   aai--change-flag))
              (aai-post-command-hook)
              (list
               flag-before
               aai--change-flag
               (buffer-string)
               (point)
               (current-column)))))"##,
        expect![[r#"OK (t t "(progn\n  (message \"one\"))\n" 10 2)"#]],
    )
}

pub(super) fn post_command_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_auto_indent_before_change_marks_only_enabled_buffer_and_accepts_hook_arguments(),
        auto_auto_indent_post_command_returns_early_when_disabled_or_cua_rectangle_active(),
        auto_auto_indent_post_command_immediately_indents_first_structural_and_regular_edits(),
        auto_auto_indent_repeated_nonstructural_self_insert_cancels_and_reschedules_idle_timer(),
        auto_auto_indent_post_command_command_matrix_freezes_immediate_delayed_and_skipped_paths(),
        auto_auto_indent_unmodified_buffer_skips_immediate_indent_but_can_schedule_delayed_work(),
        auto_auto_indent_post_command_cursor_correction_depends_on_navigation_direction(),
        auto_auto_indent_active_region_and_multiple_cursors_suppress_position_correction(),
        auto_auto_indent_post_command_swallows_errors_unless_debug_handler_is_enabled(),
        auto_auto_indent_real_insert_hook_and_post_command_reformat_practical_lisp_edit(),
    ]
}
