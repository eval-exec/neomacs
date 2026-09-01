use expect_test::expect;

use super::ParityBatchCase;

fn auto_auto_indent_newline_and_indent_creates_real_lisp_indentation_at_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_newline_and_indent_creates_real_lisp_indentation_at_point",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(let ((value 1))\n"
           "  (message \"first\")"
           "(message \"second\"))")
          (search-backward
           "(message \"second\")")
          (auto-auto-indent-mode 1)
          (let ((before (point)))
            (aai-newline-and-indent)
            (list
             before
             (auto-auto-indent-test-buffer-state))))"##,
        expect![[
            r#"OK (37 ("(let ((value 1))\n  (message \"first\")\n  (message \"second\"))" 40 3 2 nil nil t))"#
        ]],
    )
}

fn auto_auto_indent_newline_between_matching_delimiters_creates_indented_blank_line()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_newline_between_matching_delimiters_creates_indented_blank_line",
        r##"(mapcar
          (lambda (text)
            (with-temp-buffer
              (emacs-lisp-mode)
              (insert text)
              (goto-char 2)
              (auto-auto-indent-mode 1)
              (list
               text
               (aai-newline-and-indent)
               (auto-auto-indent-test-buffer-state))))
          '("()"
            "[]"
            "{}"))"##,
        expect![[
            r#"OK (("()" nil ("(\n \n )" 4 2 1 nil nil t)) ("[]" nil ("[\n \n ]" 4 2 1 nil nil t)) ("{}" nil ("{\n\n}" 3 2 0 nil nil t)))"#
        ]],
    )
}

fn auto_auto_indent_newline_replaces_active_region_then_indents_remaining_form() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_auto_indent_newline_replaces_active_region_then_indents_remaining_form",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(progn\n"
           "  DELETE-ME"
           "(message \"kept\"))")
          (goto-char
           (point-min))
          (search-forward "DELETE-ME")
          (set-mark
           (- (point)
              (length "DELETE-ME")))
          (activate-mark)
          (auto-auto-indent-mode 1)
          (aai-newline-and-indent)
          (auto-auto-indent-test-buffer-state))"##,
        expect![[r#"OK ("(progn\n  \n  (message \"kept\"))" 13 3 2 10 nil t)"#]],
    )
}

fn auto_auto_indent_nxml_and_web_newline_paths_reindent_new_and_previous_lines() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_auto_indent_nxml_and_web_newline_paths_reindent_new_and_previous_lines",
        r##"(mapcar
          (lambda (mode)
            (with-temp-buffer
              (insert "alpha")
              (setq major-mode mode)
              (goto-char (point-max))
              (let ((aai-mode t)
                    calls)
                (setq-local
                 indent-line-function
                 (lambda ()
                   (push
                    (line-number-at-pos)
                    calls)))
                (aai-newline-and-indent)
                (list
                 mode
                 (buffer-string)
                 (point)
                 (nreverse calls)))))
          '(fundamental-mode
            nxml-mode
            web-mode))"##,
        expect![[
            r#"OK ((fundamental-mode "alpha\n" 7 (2)) (nxml-mode "alpha\n" 7 (2 1)) (web-mode "alpha\n" 7 (2 1)))"#
        ]],
    )
}

fn auto_auto_indent_open_line_indents_both_sides_without_moving_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_open_line_indents_both_sides_without_moving_point",
        r##"(with-temp-buffer
          (insert "alpha beta")
          (goto-char 6)
          (let ((aai-mode t)
                calls)
            (setq-local
             indent-line-function
             (lambda ()
               (push
                (list
                 (line-number-at-pos)
                 (point))
                calls)))
            (let ((before (point)))
              (list
               (aai-open-line)
               before
               (point)
               (buffer-string)
               (nreverse calls)))))"##,
        expect![[r#"OK (#1=((1 6)) 6 6 "alpha\n beta" ((2 7) . #1#))"#]],
    )
}

fn auto_auto_indent_delete_char_handles_text_visible_eol_and_whitespace_joining() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_auto_indent_delete_char_handles_text_visible_eol_and_whitespace_joining",
        r##"(mapcar
          (lambda (spec)
            (with-temp-buffer
              (emacs-lisp-mode)
              (insert
               (car spec))
              (goto-char
               (cadr spec))
              (list
               spec
               (aai-delete-char)
               (buffer-string)
               (point)
               (current-column))))
          '(("abc" 2)
            ("first\n  second" 6)
            ("first   \n  second" 6)
            ("(one)\n(two)" 6)))"##,
        expect![[
            r#"OK ((("abc" 2) nil "ac" 2 1) (("first\n  second" 6) nil "first second" 6 5) (("first   \n  second" 6) nil "first second" 6 5) (("(one)\n(two)" 6) nil "(one) (two)" 6 5))"#
        ]],
    )
}

fn auto_auto_indent_delete_char_replaces_active_region_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_delete_char_replaces_active_region_exactly",
        r##"(with-temp-buffer
          (insert "prefix DELETE suffix")
          (goto-char 8)
          (set-mark 14)
          (activate-mark)
          (list
           (aai-delete-char)
           (buffer-string)
           (point)
           (mark t)
           (region-active-p)))"##,
        expect![[r#"OK (nil "prefix  suffix" 8 8 t)"#]],
    )
}

fn auto_auto_indent_backspace_deletes_matching_pairs_as_one_edit() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_backspace_deletes_matching_pairs_as_one_edit",
        r##"(mapcar
          (lambda (text)
            (with-temp-buffer
              (insert text)
              (goto-char 2)
              (list
               text
               (aai-backspace)
               (buffer-string)
               (point))))
          '("()"
            "[]"
            "{}"
            "\"\""
            "''"))"##,
        expect![[
            r#"OK (("()" nil "" 1) ("[]" nil "" 1) ("{}" nil "" 1) ("\"\"" nil "" 1) ("''" nil "" 1))"#
        ]],
    )
}

fn auto_auto_indent_backspace_at_indentation_joins_with_previous_line_and_fixes_gap()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_backspace_at_indentation_joins_with_previous_line_and_fixes_gap",
        r##"(mapcar
          (lambda (text)
            (with-temp-buffer
              (emacs-lisp-mode)
              (insert text)
              (goto-char (point-max))
              (back-to-indentation)
              (list
               text
               (aai-backspace)
               (buffer-string)
               (point)
               (current-column))))
          '("(message \"one\")\n  (message \"two\")"
            "(one)\n    (two)"
            "word\n  next"))"##,
        expect![[
            r#"OK (("(message \"one\")\n  (message \"two\")" nil "(message \"one\") (message \"two\")" 17 16) ("(one)\n    (two)" nil "(one) (two)" 7 6) ("word\n  next" nil "word next" 6 5))"#
        ]],
    )
}

fn auto_auto_indent_backspace_uses_region_paredit_and_plain_fallback_branches() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_backspace_uses_region_paredit_and_plain_fallback_branches",
        r##"(list
          (with-temp-buffer
            (insert "region-delete")
            (goto-char 1)
            (set-mark 7)
            (activate-mark)
            (list
             (aai-backspace)
             (buffer-string)
             (point)))
          (with-temp-buffer
            (insert "plain")
            (goto-char (point-max))
            (list
             (aai-backspace)
             (buffer-string)
             (point)))
          (with-temp-buffer
            (insert "paredit")
            (goto-char (point-max))
            (setq-local paredit-mode t)
            (let (calls)
              (cl-letf
                  (((symbol-function
                     'paredit-backward-delete)
                    (lambda ()
                      (push
                       (list
                        (buffer-string)
                        (point))
                       calls)
                      :paredit)))
                (list
                 (aai-backspace)
                 (buffer-string)
                 (point)
                 (nreverse calls))))))"##,
        expect![[
            r#"OK ((nil "-delete" 1) (nil "plai" 5) (:paredit "paredit" 8 (("paredit" 8))))"#
        ]],
    )
}

fn auto_auto_indent_indented_yank_formats_practical_multiline_lisp_and_sets_mark() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_auto_indent_indented_yank_formats_practical_multiline_lisp_and_sets_mark",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert "(progn\n")
          (let ((kill-ring
                 '("(let ((value 3))\n(message \"%s\" value))"))
                (kill-ring-yank-pointer nil))
            (setq kill-ring-yank-pointer kill-ring)
            (auto-auto-indent-mode 1)
            (aai-indented-yank)
            (insert "\n)")
            (list
             (auto-auto-indent-test-buffer-state)
             (buffer-substring-no-properties
              (mark t)
              (save-excursion
                (goto-char (mark t))
                (forward-sexp)
                (point))))))"##,
        expect![[
            r#"OK (("(progn\n  (let ((value 3))\n    (message \"%s\" value))\n)" 54 4 1 8 nil t) "  (let ((value 3))\n    (message \"%s\" value))")"#
        ]],
    )
}

fn auto_auto_indent_indented_yank_dont_indent_limit_and_region_replacement_contracts_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_indented_yank_dont_indent_limit_and_region_replacement_contracts_match",
        r##"(mapcar
          (lambda (case)
            (with-temp-buffer
              (emacs-lisp-mode)
              (insert
               (if (eq case 'region)
                   "before REMOVE after"
                 "(progn\n"))
              (when (eq case 'region)
                (goto-char 8)
                (set-mark 14)
                (activate-mark))
              (let ((kill-ring
                     '("(message \"pasted\")"))
                    (kill-ring-yank-pointer nil)
                    (aai-mode t)
                    (aai-indented-yank-limit
                     (if (eq case 'over-limit)
                         1
                       4000))
                    calls)
                (setq kill-ring-yank-pointer kill-ring)
                (cl-letf
                    (((symbol-function
                       'aai--indent-region)
                      (lambda (start end)
                        (push
                         (list start end)
                         calls))))
                  (aai-indented-yank
                   (eq case 'dont-indent))
                  (list
                   case
                   (buffer-string)
                   (point)
                   (mark t)
                   (nreverse calls))))))
          '(normal
            dont-indent
            over-limit
            region))"##,
        expect![[
            r#"OK ((normal "(progn\n(message \"pasted\")" 26 8 ((8 26))) (dont-indent "(progn\n(message \"pasted\")" 26 8 nil) (over-limit "(progn\n(message \"pasted\")" 26 8 nil) (region "before (message \"pasted\") after" 26 8 ((8 26))))"#
        ]],
    )
}

fn auto_auto_indent_indented_yank_comint_path_removes_trailing_prompt_gap() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_indented_yank_comint_path_removes_trailing_prompt_gap",
        r##"(with-temp-buffer
          (insert "prompt>    ")
          (setq major-mode
                'comint-mode)
          (let ((kill-ring
                 '("command   \n"))
                (kill-ring-yank-pointer nil)
                (aai-mode t))
            (setq kill-ring-yank-pointer kill-ring)
            (aai-indented-yank t)
            (auto-auto-indent-test-buffer-state)))"##,
        expect![[r#"OK ("prompt>    command" 19 1 18 12 nil t)"#]],
    )
}

fn auto_auto_indent_mouse_yank_inside_region_replaces_selection_and_honors_no_indent_variant()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_mouse_yank_inside_region_replaces_selection_and_honors_no_indent_variant",
        r##"(mapcar
          (lambda (dont-indent)
            (with-temp-buffer
              (insert "before SELECT after")
              (goto-char 8)
              (set-mark 14)
              (activate-mark)
              (let ((kill-ring
                     '("PASTE"))
                    (kill-ring-yank-pointer nil)
                    calls)
                (setq kill-ring-yank-pointer kill-ring)
                (cl-letf
                    (((symbol-function 'mouse-set-point)
                      (lambda (event)
                        (push event calls)
                        (goto-char 10)))
                     ((symbol-function
                       'aai--indent-region)
                      (lambda (start end)
                        (push
                         (list start end)
                         calls))))
                  (if dont-indent
                      (aai-mouse-yank-dont-indent
                       :fixture-event)
                    (aai-mouse-yank
                     :fixture-event))
                  (list
                   dont-indent
                   (buffer-string)
                   (point)
                   (mark t)
                   (nreverse calls))))))
          '(nil t))"##,
        expect![[
            r#"OK ((nil "before PASTE after" 13 8 (:fixture-event (8 13))) (t "before PASTE after" 13 8 (:fixture-event)))"#
        ]],
    )
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_auto_indent_newline_and_indent_creates_real_lisp_indentation_at_point(),
        auto_auto_indent_newline_between_matching_delimiters_creates_indented_blank_line(),
        auto_auto_indent_newline_replaces_active_region_then_indents_remaining_form(),
        auto_auto_indent_nxml_and_web_newline_paths_reindent_new_and_previous_lines(),
        auto_auto_indent_open_line_indents_both_sides_without_moving_point(),
        auto_auto_indent_delete_char_handles_text_visible_eol_and_whitespace_joining(),
        auto_auto_indent_delete_char_replaces_active_region_exactly(),
        auto_auto_indent_backspace_deletes_matching_pairs_as_one_edit(),
        auto_auto_indent_backspace_at_indentation_joins_with_previous_line_and_fixes_gap(),
        auto_auto_indent_backspace_uses_region_paredit_and_plain_fallback_branches(),
        auto_auto_indent_indented_yank_formats_practical_multiline_lisp_and_sets_mark(),
        auto_auto_indent_indented_yank_dont_indent_limit_and_region_replacement_contracts_match(),
        auto_auto_indent_indented_yank_comint_path_removes_trailing_prompt_gap(),
        auto_auto_indent_mouse_yank_inside_region_replaces_selection_and_honors_no_indent_variant(),
    ]
}
