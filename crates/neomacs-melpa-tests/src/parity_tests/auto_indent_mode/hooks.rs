use expect_test::expect;

use super::ParityBatchCase;

fn auto_indent_mode_pre_command_records_position_and_orders_post_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_pre_command_records_position_and_orders_post_hooks",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (insert "(alpha\n  beta)")
         (goto-char 5)
         (setq auto-indent-mode t
               auto-indent-current-pairs nil
               auto-indent-next-pair nil
               post-command-hook
               '(fixture-before
                 auto-indent-mode-post-command-hook-last
                 fixture-after
                 auto-indent-mode-post-command-hook))
         (auto-indent-mode-pre-command-hook)
         (list
          auto-indent-mode-pre-command-hook-line
          auto-indent-last-pre-command-hook-point
          auto-indent-last-pre-command-hook-minibufferp
          post-command-hook))"##,
        expect![
            "OK (1 5 nil (auto-indent-mode-post-command-hook fixture-before fixture-after auto-indent-mode-post-command-hook-last))"
        ],
    )
}

fn auto_indent_mode_pre_command_expands_pair_region_around_nested_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_pre_command_expands_pair_region_around_nested_point",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (insert "(outer (inner value) tail)")
         (search-backward "value")
         (setq auto-indent-mode t
               auto-indent-current-pairs t
               auto-indent-next-pair nil
               auto-indent-indent-style 'aggressive
               auto-indent-pairs-begin nil
               auto-indent-pairs-end nil)
         (auto-indent-mode-pre-command-hook)
         (list
          (auto-indent-point-inside-pairs-p)
          auto-indent-pairs-begin
          auto-indent-pairs-end
          (buffer-substring
           auto-indent-pairs-begin
           auto-indent-pairs-end)))"##,
        expect![[r#"OK (t 8 21 "(inner value)")"#]],
    )
}

fn auto_indent_mode_point_inside_pairs_handles_code_strings_and_unbalanced_text() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_indent_mode_point_inside_pairs_handles_code_strings_and_unbalanced_text",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (emacs-lisp-mode)
             (insert (car case))
             (goto-char (cdr case))
             (list case
                   (syntax-ppss)
                   (auto-indent-point-inside-pairs-p))))
         '(("(alpha beta)" . 7)
           ("\"(text)\"" . 5)
           ("(unclosed" . 6)
           ("plain" . 3)))"##,
        expect![[
            r#"OK ((("(alpha beta)" . 7) (1 1 2 nil nil nil 0 nil nil (1) nil) t) (("\"(text)\"" . 5) (0 nil nil 34 nil nil 0 nil 1 nil nil) nil) (("(unclosed" . 6) (1 1 2 nil nil nil 0 nil nil (1) nil) t) (("plain" . 3) (0 nil 1 nil nil nil 0 nil nil nil nil) nil))"#
        ]],
    )
}

fn auto_indent_mode_post_command_routes_yank_to_yank_engine() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_post_command_routes_yank_to_yank_engine",
        r##"(let (calls)
         (cl-letf (((symbol-function 'auto-indent-yank-post-command)
                    (lambda ()
                      (push 'yank calls))))
           (with-temp-buffer
             (emacs-lisp-mode)
             (setq auto-indent-mode t
                   auto-indent-last-pre-command-hook-minibufferp nil
                   this-command 'yank
                   auto-indent-current-pairs nil
                   auto-indent-next-pair nil)
             (auto-indent-mode-post-command-hook)
             (list
              (nreverse calls)
              (memq 'auto-indent-mode-pre-command-hook
                    pre-command-hook)))))"##,
        expect![
            "OK ((yank) (auto-indent-mode-pre-command-hook eldoc-pre-command-refresh-echo-area t))"
        ],
    )
}

fn auto_indent_mode_post_command_handles_return_and_blank_line_motion() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_post_command_handles_return_and_blank_line_motion",
        r##"(mapcar
         (lambda (case)
           (let (calls)
             (cl-letf (((symbol-function 'auto-indent-par-region)
                        (lambda ()
                          (push 'pair calls)
                          nil))
                       ((symbol-function 'auto-indent-according-to-mode)
                        (lambda ()
                          (push (list 'indent (point)) calls))))
               (with-temp-buffer
                 (emacs-lisp-mode)
                 (insert (car case))
                 (goto-char (nth 1 case))
                 (setq auto-indent-mode t
                       auto-indent-last-pre-command-hook-minibufferp nil
                       auto-indent-mode-pre-command-hook-line
                       (nth 2 case)
                       auto-indent-last-pre-command-hook-point
                       (point)
                       auto-indent-current-pairs nil
                       auto-indent-next-pair nil
                       auto-indent-block-close nil
                       this-command (nth 3 case)
                       last-command-event (nth 4 case))
                 (auto-indent-mode-post-command-hook)
                 (list case (nreverse calls))))))
         '(("line\n  " 8 1 newline 10)
           ("line\n  " 8 1 next-line nil)
           ("line\ntext" 8 2 next-line nil)))"##,
        expect![[
            r#"OK ((("line\n  " 8 1 newline 10) (pair (indent 8))) (("line\n  " 8 1 next-line nil) ((indent 8))) (("line\ntext" 8 2 next-line nil) nil))"#
        ]],
    )
}

fn auto_indent_mode_post_command_last_schedules_pair_timer_deterministically() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_post_command_last_schedules_pair_timer_deterministically",
        r##"(let (calls)
         (cl-letf (((symbol-function 'run-with-timer)
                    (lambda (delay repeat function &rest arguments)
                      (push
                       (list delay repeat function arguments)
                       calls)
                      :fixture-timer))
                   ((symbol-function 'cancel-timer)
                    (lambda (timer)
                      (push (list :cancel timer) calls))))
           (with-temp-buffer
             (emacs-lisp-mode)
             (insert "(alpha beta)")
             (goto-char 5)
             (setq auto-indent-mode t
                   auto-indent-last-pre-command-hook-minibufferp nil
                   auto-indent-current-pairs t
                   auto-indent-next-pair nil
                   auto-indent-indent-style 'aggressive
                   auto-indent-pairs-begin 1
                   auto-indent-pairs-end 8
                   auto-indent-par-region-timer :old-timer
                   auto-indent-next-pair-timer-geo-mean
                   '((emacs-lisp-mode 0.01 2))
                   auto-indent-next-pair-throttle 1)
             (auto-indent-mode-post-command-hook-last)
             (list
              auto-indent-pairs-begin
              auto-indent-pairs-end
              auto-indent-par-region-timer
              (nreverse calls)))))"##,
        expect![
            "OK (1 8 :fixture-timer ((:cancel :old-timer) (0.0 nil auto-indent-par-region nil)))"
        ],
    )
}

fn auto_indent_mode_pair_region_indents_and_clears_when_point_leaves_region() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_pair_region_indents_and_clears_when_point_leaves_region",
        r##"(let (calls)
         (cl-letf (((symbol-function 'indent-region)
                    (lambda (begin end &rest _arguments)
                      (push (list begin end) calls)))
                   ((symbol-function 'auto-indent-par-region-interval-update)
                    (lambda (interval)
                      (push (list :interval (numberp interval))
                            calls))))
           (with-temp-buffer
             (emacs-lisp-mode)
             (insert "(alpha\nbeta)\noutside")
             (goto-char (point-max))
             (setq auto-indent-next-pair t
                   auto-indent-pairs-begin 1
                   auto-indent-pairs-end 13
                   auto-indent-multiple-indent-modes nil)
             (let ((result (auto-indent-par-region)))
               (list
                result
                auto-indent-pairs-begin
                auto-indent-pairs-end
                (nreverse calls))))))"##,
        expect!["OK (nil nil nil ((1 13) (:interval t)))"],
    )
}

fn auto_indent_mode_minibuffer_hook_sets_global_guard_flag() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_minibuffer_hook_sets_global_guard_flag",
        r##"(progn
         (setq auto-indent-last-pre-command-hook-minibufferp nil)
         (let ((result (auto-indent-minibuffer-hook)))
           (list
            result
            auto-indent-last-pre-command-hook-minibufferp)))"##,
        expect!["OK (t t)"],
    )
}

pub(super) fn hooks_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_indent_mode_pre_command_records_position_and_orders_post_hooks(),
        auto_indent_mode_pre_command_expands_pair_region_around_nested_point(),
        auto_indent_mode_point_inside_pairs_handles_code_strings_and_unbalanced_text(),
        auto_indent_mode_post_command_routes_yank_to_yank_engine(),
        auto_indent_mode_post_command_handles_return_and_blank_line_motion(),
        auto_indent_mode_post_command_last_schedules_pair_timer_deterministically(),
        auto_indent_mode_pair_region_indents_and_clears_when_point_leaves_region(),
        auto_indent_mode_minibuffer_hook_sets_global_guard_flag(),
    ]
}
