use expect_test::expect;

use super::ParityBatchCase;

fn auto_auto_indent_practical_unformatted_lisp_insert_is_repaired_by_real_change_and_post_hooks()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_practical_unformatted_lisp_insert_is_repaired_by_real_change_and_post_hooks",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (auto-auto-indent-mode 1)
          (insert
           "(defun deploy-token (service)\n"
           "(let ((token (concat service \"-token\")))\n"
           "(when token\n"
           "(message \"%s\" token))\n"
           "token))\n")
          (let ((flag-after-insert
                 aai--change-flag)
                (this-command
                 'self-insert-command)
                (last-command 'other)
                (last-input-event 41))
            (aai-post-command-hook)
            (list
             flag-after-insert
             aai--change-flag
             (buffer-string)
             (point)
             (line-number-at-pos)
             (current-column))))"##,
        expect![[
            r#"OK (t t "(defun deploy-token (service)\n(let ((token (concat service \"-token\")))\n(when token\n  (message \"%s\" token))\ntoken))\n" 116 6 0)"#
        ]],
    )
}

fn auto_auto_indent_practical_paste_newline_and_backspace_workflow_preserves_structure_and_mark()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_practical_paste_newline_and_backspace_workflow_preserves_structure_and_mark",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert "(progn\n)")
          (goto-char 8)
          (let ((kill-ring
                 '("(let ((user \"alice\"))\n(message \"hello %s\" user))"))
                (kill-ring-yank-pointer nil))
            (setq kill-ring-yank-pointer kill-ring)
            (auto-auto-indent-mode 1)
            (aai-indented-yank)
            (let ((paste-mark
                   (mark t)))
              (aai-newline-and-indent)
              (insert "()")
              (backward-char)
              (aai-backspace)
              (list
               paste-mark
               (mark t)
               (buffer-string)
               (point)
               (line-number-at-pos)
               (current-column)))))"##,
        expect![[
            r#"OK (8 8 "(progn\n  (let ((user \"alice\"))\n    (message \"hello %s\" user))\n  )" 65 4 2)"#
        ]],
    )
}

fn auto_auto_indent_readme_style_selective_predicate_preserves_heredoc_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_readme_style_selective_predicate_preserves_heredoc_lines",
        r##"(with-temp-buffer
          (insert
           "function demo() {\n"
           "EOD\n"
           "payload\n"
           "EOD\n"
           "return value;\n"
           "}\n")
          (let ((aai-mode t)
                (aai-indentable-line-p-function
                 (lambda ()
                   (not
                    (looking-at-p
                     "EOD"))))
                calls)
            (setq-local
             indent-line-function
             (lambda ()
               (let ((line
                      (line-number-at-pos)))
                 (push line calls)
                 (delete-horizontal-space)
                 (insert
                  (make-string line ?\s)))))
            (aai--indent-region
             (point-min)
             (point-max))
            (list
             (buffer-string)
             (nreverse calls))))"##,
        expect![[
            r#"OK (" function demo() {\nEOD\n   payload\nEOD\n     return value;\n      }\n       " (1 3 5 6 7))"#
        ]],
    )
}

fn auto_auto_indent_two_real_editing_buffers_keep_modes_strategies_and_changes_independent()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_two_real_editing_buffers_keep_modes_strategies_and_changes_independent",
        r##"(let ((lisp-buffer
                                (generate-new-buffer
                                 " *aai-lisp-workflow*"))
                               (text-buffer
                                (generate-new-buffer
                                 " *aai-text-workflow*")))
          (unwind-protect
              (progn
                (with-current-buffer lisp-buffer
                  (emacs-lisp-mode)
                  (auto-auto-indent-mode 1)
                  (insert
                   "(defun one ()\n"
                   "(message \"one\"))\n"))
                (with-current-buffer text-buffer
                  (text-mode)
                  (insert "plain\n  text")
                  (auto-auto-indent-mode 1)
                  (auto-auto-indent-mode -1))
                (list
                 (with-current-buffer lisp-buffer
                   (let ((this-command
                         'self-insert-command)
                         (last-command 'other)
                         (last-input-event 41))
                     (aai-post-command-hook)
                     (list
                      aai-mode
                      aai-indent-function
                      aai--change-flag
                      (buffer-string))))
                 (with-current-buffer text-buffer
                   (list
                    aai-mode
                    aai-indent-function
                    aai--change-flag
                    (buffer-string)))))
            (when
                (buffer-live-p lisp-buffer)
              (kill-buffer lisp-buffer))
            (when
                (buffer-live-p text-buffer)
              (kill-buffer text-buffer))))"##,
        expect![[
            r#"OK ((t aai-indent-defun t "(defun one ()\n  (message \"one\"))\n") (nil aai-indent-line-maybe nil "plain\n  text"))"#
        ]],
    )
    .fresh_process()
}

fn auto_auto_indent_typing_burst_schedules_then_structural_edit_indents_immediately()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_typing_burst_schedules_then_structural_edit_indents_immediately",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(progn\n"
           "(message \"typing\"))\n")
          (goto-char (point-min))
          (forward-line 1)
          (auto-auto-indent-mode 1)
          (let (callback events)
            (cl-letf
                (((symbol-function
                   'run-with-idle-timer)
                  (lambda (delay repeat function)
                    (setq callback function)
                    (push
                     (list :scheduled delay repeat)
                     events)
                    :pending-timer))
                 ((symbol-function 'cancel-timer)
                  (lambda (timer)
                    (push
                     (list :cancelled timer)
                     events))))
              (let ((this-command
                     'self-insert-command)
                    (last-command
                     'self-insert-command)
                    (last-input-event ?x))
                (insert "x")
                (aai-post-command-hook))
              (let ((timer-after-burst
                     aai--timer)
                    (this-command
                     'self-insert-command)
                    (last-command
                     'self-insert-command)
                    (last-input-event 41))
                (insert ")")
                (aai-post-command-hook)
                (list
                 timer-after-burst
                 aai--timer
                 (functionp callback)
                 (nreverse events)
                 (buffer-string)
                 aai--change-flag)))))"##,
        expect![[
            r#"OK (:pending-timer :pending-timer t ((:scheduled 0.5 nil)) "(progn\n  x)(message \"typing\"))\n" t)"#
        ]],
    )
    .fresh_process()
}

fn auto_auto_indent_limit_switches_same_large_function_between_window_and_defun_workflows()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_limit_switches_same_large_function_between_window_and_defun_workflows",
        r##"(mapcar
          (lambda (limit)
            (with-temp-buffer
              (emacs-lisp-mode)
              (insert
               "(defun configured ()\n"
               "(let ((one 1))\n"
               "(when one\n"
               "(message \"value\"))\n"
               "one))\n")
              (goto-char (point-min))
              (forward-line 1)
              (let ((aai-mode t)
                    (aai-indent-limit limit))
                (aai-indent-defun)
                (list
                 limit
                 (buffer-string)
                 (point)))))
          '(2 20))"##,
        expect![[
            r#"OK ((2 "(defun configured ()\n(let ((one 1))\n(when one\n  (message \"value\"))\none))\n" 22) (20 "(defun configured ()\n(let ((one 1))\n(when one\n  (message \"value\"))\none))\n" 22))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_auto_indent_practical_unformatted_lisp_insert_is_repaired_by_real_change_and_post_hooks(),
        auto_auto_indent_practical_paste_newline_and_backspace_workflow_preserves_structure_and_mark(),
        auto_auto_indent_readme_style_selective_predicate_preserves_heredoc_lines(),
        auto_auto_indent_two_real_editing_buffers_keep_modes_strategies_and_changes_independent(),
        auto_auto_indent_typing_burst_schedules_then_structural_edit_indents_immediately(),
        auto_auto_indent_limit_switches_same_large_function_between_window_and_defun_workflows(),
    ]
}
