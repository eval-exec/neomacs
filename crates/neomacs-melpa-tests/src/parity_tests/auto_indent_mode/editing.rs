use expect_test::expect;

use super::ParityBatchCase;

fn auto_indent_mode_programming_detection_combines_text_list_and_flyspell_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_indent_mode_programming_detection_combines_text_list_and_flyspell_state",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (setq major-mode (nth 0 case)
                   auto-indent-known-text-modes
                   '(text-mode markdown-mode)
                   flyspell-mode (nth 1 case)
                   flyspell-generic-check-word-predicate
                   (nth 2 case))
             (list case (auto-indent-is-prog-mode-p))))
         '((text-mode nil nil)
           (markdown-mode nil nil)
           (emacs-lisp-mode nil nil)
           (text-mode t flyspell-generic-progmode-verify)
           (text-mode t other-predicate)))"##,
        expect![
            "OK (((text-mode nil nil) nil) ((markdown-mode nil nil) nil) ((emacs-lisp-mode nil nil) t) ((text-mode t flyspell-generic-progmode-verify) t) ((text-mode t other-predicate) nil))"
        ],
    )
}

fn auto_indent_mode_handle_end_of_line_collapses_and_removes_contextual_space() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_handle_end_of_line_collapses_and_removes_contextual_space",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (insert (nth 0 case))
             (goto-char (nth 1 case))
             (let ((auto-indent-delete-line-char-remove-last-space
                    (nth 2 case)))
               (auto-indent-handle-end-of-line
                '(("\\s(" "\\sw")
                  ("\\s." "\\s\"")))
               (list case
                     (buffer-string)
                     (point)))))
         '(("(   value" 2 t)
           ("list(    item" 6 t)
           ("list(    item" 6 nil)
           ("word    next" 5 t)))"##,
        expect![[
            r#"OK ((("(   value" 2 t) "(value" 2) (("list(    item" 6 t) "list(item" 6) (("list(    item" 6 nil) "list( item" 6) (("word    next" 5 t) "word next" 5))"#
        ]],
    )
}

fn auto_indent_mode_handle_end_of_line_adds_space_only_for_matching_nonspace_neighbors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_handle_end_of_line_adds_space_only_for_matching_nonspace_neighbors",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (insert (car case))
             (goto-char (cdr case))
             (let ((auto-indent-delete-line-char-remove-last-space t))
               (auto-indent-handle-end-of-line
                '(("\\sw" "\\sw")
                  ("\\s." "\\sw"))
                t)
               (list case
                     (buffer-string)
                     (point)))))
         '(("wordnext" . 5)
           ("word next" . 5)
           ("(item" . 2)
           ("x\"value" . 3)))"##,
        expect![[
            r#"OK ((("wordnext" . 5) "word next" 5) (("word next" . 5) "word next" 5) (("(item" . 2) "(item" 2) (("x\"value" . 3) "x\"value" 3))"#
        ]],
    )
}

fn auto_indent_mode_delete_char_joins_program_lines_with_contextual_spacing() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_delete_char_joins_program_lines_with_contextual_spacing",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (emacs-lisp-mode)
             (insert (car case))
             (goto-char (cdr case))
             (let ((auto-indent-mode t)
                   (auto-indent-force-interactive-advices nil)
                   (auto-indent-delete-line-char-remove-extra-spaces t)
                   (auto-indent-delete-line-char-add-extra-spaces t)
                   (auto-indent-delete-line-char-remove-last-space t)
                   (auto-indent-par-region-timer nil))
               (auto-indent-delete-char 1)
               (list case
                     (buffer-string)
                     (point)))))
         '(("(alpha\n    beta)" . 7)
           ("word\n    next" . 5)
           ("\"first\",\n    \"second\"" . 9)
           ("left \n    right" . 6)))"##,
        expect![[
            r#"OK ((("(alpha\n    beta)" . 7) "(alpha beta)" 7) (("word\n    next" . 5) "word next" 5) (("\"first\",\n    \"second\"" . 9) "\"first\", \"second\"" 9) (("left \n    right" . 6) "left right" 6))"#
        ]],
    )
}

fn auto_indent_mode_backward_delete_variants_apply_configured_whitespace_method() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_indent_mode_backward_delete_variants_apply_configured_whitespace_method",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (insert (nth 0 case))
             (goto-char (point-max))
             (let ((auto-indent-mode t)
                   (auto-indent-backward-delete-char-behavior
                    (nth 1 case))
                   (auto-indent-par-region-timer nil))
               (funcall (nth 2 case) 1)
               (list case
                     (buffer-string)
                     (point)
                     this-command))))
         '(("word    " hungry auto-indent-delete-backward-char)
           ("word    " all auto-indent-backward-delete-char)
           ("word\t" untabify auto-indent-backward-delete-char-untabify)
           ("word x" nil auto-indent-delete-backward-char)))"##,
        expect![[
            r#"OK ((("word    " hungry auto-indent-delete-backward-char) "word" 5 auto-indent-delete-backward-char) (("word    " all auto-indent-backward-delete-char) "word" 5 auto-indent-delete-backward-char) (("word\11" untabify auto-indent-backward-delete-char-untabify) "word   " 8 auto-indent-delete-backward-char) (("word x" nil auto-indent-delete-backward-char) "word " 6 auto-indent-delete-backward-char))"#
        ]],
    )
}

fn auto_indent_mode_yank_post_command_runs_hook_indents_and_untabifies_region() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_yank_post_command_runs_hook_indents_and_untabifies_region",
        r##"(let (events)
         (with-temp-buffer
           (emacs-lisp-mode)
           (insert "(progn\n\t(message \"x\")\n)")
           (set-mark (point-min))
           (goto-char (point-max))
           (let ((auto-indent-after-yank-hook
                  (list
                   (lambda (begin end)
                     (push
                      (list begin end
                            (buffer-substring begin end))
                      events))))
                 (auto-indent-on-yank-or-paste t)
                 (auto-indent-mode-untabify-on-yank-or-paste t))
             (auto-indent-yank-post-command)
             (list
              (buffer-string)
              (nreverse events)
              (mark t)
              (point)))))"##,
        expect![[
            r#"OK ("(progn\n  (message \"x\")\n  )" ((1 24 "(progn\n\11(message \"x\")\n)")) 1 27)"#
        ]],
    )
}

fn auto_indent_mode_yank_post_command_supports_reverse_region_and_tabify() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_yank_post_command_supports_reverse_region_and_tabify",
        r##"(with-temp-buffer
         (setq tab-width 4)
         (insert "    alpha\n        beta")
         (set-mark (point-max))
         (goto-char (point-min))
         (let ((auto-indent-after-yank-hook nil)
               (auto-indent-on-yank-or-paste nil)
               (auto-indent-mode-untabify-on-yank-or-paste
                'tabify))
           (auto-indent-yank-post-command)
           (list
            (buffer-string)
            (mark t)
            (point))))"##,
        expect![[r#"OK ("\11alpha\n\11\11beta" 14 1)"#]],
    )
}

fn auto_indent_mode_whole_buffer_save_indents_untabifies_and_trims() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_whole_buffer_save_indents_untabifies_and_trims",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (insert "(progn\n(message \"x\")   \n\t(message \"y\"))\n")
         (let ((auto-indent-indent-style 'aggressive)
               (auto-indent-on-save-file t)
               (auto-indent-untabify-on-save-file t)
               (auto-indent-delete-trailing-whitespace-on-save-file t)
               (auto-indent-disabled-modes-list nil)
               (auto-indent-disabled-modes-on-save nil))
           (list
            (auto-indent-whole-buffer t)
            (buffer-string)
            (buffer-modified-p))))"##,
        expect![[r#"OK (nil "(progn\n  (message \"x\")\n  (message \"y\"))\n" t)"#]],
    )
}

fn auto_indent_mode_whole_buffer_visit_uses_distinct_visit_options() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_whole_buffer_visit_uses_distinct_visit_options",
        r##"(with-temp-buffer
         (emacs-lisp-mode)
         (insert "(progn\n(message \"x\") \n\t(message \"y\"))\n")
         (let ((auto-indent-indent-style 'aggressive)
               (auto-indent-on-visit-file t)
               (auto-indent-untabify-on-visit-file 'tabify)
               (auto-indent-delete-trailing-whitespace-on-visit-file t)
               (auto-indent-disabled-modes-list nil))
           (list
            (auto-indent-whole-buffer nil)
            (buffer-string)
            (buffer-modified-p))))"##,
        expect![[r#"OK (nil "(progn\n  (message \"x\")\n  (message \"y\"))\n" t)"#]],
    )
}

fn auto_indent_mode_whole_buffer_skips_disabled_and_conservative_cases() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_whole_buffer_skips_disabled_and_conservative_cases",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (funcall (nth 0 case))
             (insert "\ttext   \n")
             (let ((auto-indent-indent-style (nth 1 case))
                   (auto-indent-disabled-modes-list (nth 2 case))
                   (auto-indent-on-save-file t)
                   (auto-indent-untabify-on-save-file t)
                   (auto-indent-delete-trailing-whitespace-on-save-file t))
               (list
                case
                (auto-indent-whole-buffer t)
                (buffer-string)))))
         '((fundamental-mode aggressive (fundamental-mode))
           (text-mode conservative nil)
           (text-mode aggressive nil)))"##,
        expect![[
            r#"OK (((fundamental-mode aggressive (fundamental-mode)) nil "\11text   \n") ((text-mode conservative nil) nil "text\n") ((text-mode aggressive nil) nil "text\n"))"#
        ]],
    )
}

fn auto_indent_mode_file_visit_can_restore_unmodified_state_after_cleanup() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_file_visit_can_restore_unmodified_state_after_cleanup",
        r##"(let ((file
                                (expand-file-name
                                 "auto-indent-visit.el"
                                 default-directory)))
         (when (file-exists-p file)
           (delete-file file))
         (unwind-protect
             (with-temp-buffer
               (emacs-lisp-mode)
               (setq buffer-file-name file)
               (insert "(progn\n(message \"x\"))\n")
               (set-buffer-modified-p t)
               (let ((auto-indent-indent-style 'aggressive)
                     (auto-indent-on-visit-file t)
                     (auto-indent-untabify-on-visit-file nil)
                     (auto-indent-delete-trailing-whitespace-on-visit-file nil)
                     (auto-indent-on-visit-pretend-nothing-changed t)
                     (auto-indent-disabled-modes-list nil))
                 (auto-indent-file-when-visit)
                 (list
                  (buffer-string)
                  (buffer-modified-p)
                  (point))))
           (when (file-exists-p file)
             (delete-file file))))"##,
        expect![[r#"OK ("(progn\n  (message \"x\"))\n" nil 25)"#]],
    )
}

fn auto_indent_mode_text_boundaries_distinguish_visual_whitespace_from_physical_edges()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_indent_mode_text_boundaries_distinguish_visual_whitespace_from_physical_edges",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (insert (car case))
             (goto-char (cdr case))
             (mapcar
              (lambda (use-text)
                (let ((auto-indent-use-text-boundaries
                       use-text))
                  (list use-text
                        (auto-indent-bolp)
                        (auto-indent-eolp))))
              '(nil t))))
         '(("   alpha   \nbeta" . 3)
           ("   alpha   \nbeta" . 9)
           ("alpha\n\nbeta" . 7)))"##,
        expect!["OK (((nil nil nil) (t t nil)) ((nil nil nil) (t nil t)) ((nil t t) (t t t)))"],
    )
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_indent_mode_programming_detection_combines_text_list_and_flyspell_state(),
        auto_indent_mode_handle_end_of_line_collapses_and_removes_contextual_space(),
        auto_indent_mode_handle_end_of_line_adds_space_only_for_matching_nonspace_neighbors(),
        auto_indent_mode_delete_char_joins_program_lines_with_contextual_spacing(),
        auto_indent_mode_backward_delete_variants_apply_configured_whitespace_method(),
        auto_indent_mode_yank_post_command_runs_hook_indents_and_untabifies_region(),
        auto_indent_mode_yank_post_command_supports_reverse_region_and_tabify(),
        auto_indent_mode_whole_buffer_save_indents_untabifies_and_trims(),
        auto_indent_mode_whole_buffer_visit_uses_distinct_visit_options(),
        auto_indent_mode_whole_buffer_skips_disabled_and_conservative_cases(),
        auto_indent_mode_file_visit_can_restore_unmodified_state_after_cleanup(),
        auto_indent_mode_text_boundaries_distinguish_visual_whitespace_from_physical_edges(),
    ]
}
