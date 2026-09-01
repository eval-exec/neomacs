use expect_test::expect;

use super::ParityBatchCase;

fn auto_auto_indent_indent_line_maybe_reindents_real_elisp_and_preserves_logical_location()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_indent_line_maybe_reindents_real_elisp_and_preserves_logical_location",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(let ((value 1))\n"
           "(message \"%s\" value))\n")
          (goto-char (point-min))
          (forward-line 1)
          (search-forward "message")
          (auto-auto-indent-mode 1)
          (let ((before
                 (auto-auto-indent-test-buffer-state)))
            (list
             before
             (aai-indent-line-maybe)
             (auto-auto-indent-test-buffer-state))))"##,
        expect![[
            r#"OK (("(let ((value 1))\n(message \"%s\" value))\n" 26 2 8 nil nil t) 28 ("(let ((value 1))\n  (message \"%s\" value))\n" 28 2 10 nil nil t))"#
        ]],
    )
}

fn auto_auto_indent_indent_line_maybe_obeys_mode_indent_function_and_predicate_gates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_indent_line_maybe_obeys_mode_indent_function_and_predicate_gates",
        r##"(mapcar
          (lambda (case)
            (with-temp-buffer
              (insert "body")
              (let ((aai-mode
                     (not (eq case 'mode-off)))
                    (aai-indentable-line-p-function
                     (if (eq case 'predicate-off)
                         (lambda () nil)
                       (lambda () t)))
                    (indent-line-function
                     (pcase case
                       ('insert-tab 'insert-tab)
                       ('indent-relative
                        'indent-relative)
                       (_
                        (lambda ()
                          (insert "<indent>"))))))
                (list
                 case
                 (aai-indent-line-maybe)
                 (buffer-string)))))
          '(normal
            mode-off
            predicate-off
            insert-tab
            indent-relative))"##,
        expect![[
            r#"OK ((normal nil "body<indent>") (mode-off nil "body") (predicate-off nil "body") (insert-tab nil "body") (indent-relative nil "body"))"#
        ]],
    )
}

fn auto_auto_indent_indent_line_maybe_swallows_indent_errors_but_not_predicate_errors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_indent_line_maybe_swallows_indent_errors_but_not_predicate_errors",
        r##"(mapcar
          (lambda (case)
            (with-temp-buffer
              (insert "unchanged")
              (let ((aai-mode t)
                    (indent-line-function
                     (lambda ()
                       (error
                        "indent fixture failed")))
                    (aai-indentable-line-p-function
                     (if (eq case 'predicate-error)
                         (lambda ()
                           (error
                            "predicate fixture failed"))
                       (lambda () t))))
                (list
                 case
                 (auto-auto-indent-test-error-data
                  #'aai-indent-line-maybe)
                 (buffer-string)))))
          '(indent-error predicate-error))"##,
        expect![[
            r#"OK ((indent-error (:ok nil) "unchanged") (predicate-error (:error error ("predicate fixture failed")) "unchanged"))"#
        ]],
    )
}

fn auto_auto_indent_indent_forward_visits_exact_limit_lines_and_repeats_eof_position()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_indent_forward_visits_exact_limit_lines_and_repeats_eof_position",
        r##"(with-temp-buffer
          (insert "one\ntwo\nthree")
          (goto-char (point-min))
          (let ((aai-mode t)
                (aai-indent-limit 5)
                calls)
            (setq-local
             indent-line-function
             (lambda ()
               (push
                (list
                 (line-number-at-pos)
                 (point))
                calls)))
            (list
             (aai-indent-forward)
             (point)
             (line-number-at-pos)
             (nreverse calls))))"##,
        expect!["OK (nil 1 1 ((1 1) (2 5) (3 9) (3 14) (3 14)))"],
    )
}

fn auto_auto_indent_indent_forward_reindents_only_configured_real_elisp_window() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_auto_indent_indent_forward_reindents_only_configured_real_elisp_window",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(progn\n"
           "(message \"one\")\n"
           "(when t\n"
           "(message \"two\"))\n"
           "(message \"three\"))\n")
          (goto-char (point-min))
          (forward-line 1)
          (let ((aai-mode t)
                (aai-indent-limit 3))
            (aai-indent-forward)
            (list
             (point)
             (buffer-string))))"##,
        expect![[
            r#"OK (8 "(progn\n  (message \"one\")\n  (when t\n    (message \"two\"))\n(message \"three\"))\n")"#
        ]],
    )
}

fn auto_auto_indent_indent_region_uses_inclusive_end_line_and_stops_at_eof() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_indent_region_uses_inclusive_end_line_and_stops_at_eof",
        r##"(with-temp-buffer
          (insert "one\ntwo\nthree\nfour")
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
            (goto-char 2)
            (let ((before (point)))
              (list
               (aai--indent-region 2 9)
               before
               (point)
               (nreverse calls)
               (progn
                 (setq calls nil)
                 (aai--indent-region
                  9
                  (point-max))
                 (nreverse calls))))))"##,
        expect!["OK (nil 2 2 ((1 2) (2 5) (3 9)) ((3 9) (4 15) (4 19)))"],
    )
}

fn auto_auto_indent_indent_region_reformats_practical_elisp_and_preserves_point() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_auto_indent_indent_region_reformats_practical_elisp_and_preserves_point",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(let ((ready t))\n"
           "(when ready\n"
           "(message \"first\")\n"
           "(message \"second\")))\n"
           "(message \"outside\")\n")
          (goto-char (point-min))
          (search-forward "second")
          (let ((aai-mode t)
                (before (point)))
            (aai--indent-region
             (point-min)
             (save-excursion
               (goto-char (point-min))
               (forward-line 3)
               (line-end-position)))
            (list
             before
             (point)
             (buffer-string))))"##,
        expect![[
            r#"OK (64 74 "(let ((ready t))\n  (when ready\n    (message \"first\")\n    (message \"second\")))\n(message \"outside\")\n")"#
        ]],
    )
}

fn auto_auto_indent_indent_defun_reformats_only_small_current_definition() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_indent_defun_reformats_only_small_current_definition",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(defun first (value)\n"
           "(let ((next (+ value 1)))\n"
           "(message \"%s\" next)))\n\n"
           "(defun second ()\n"
           "(message \"untouched\"))\n")
          (goto-char (point-min))
          (search-forward "next")
          (let ((aai-mode t)
                (aai-indent-limit 20)
                (before (point)))
            (aai-indent-defun)
            (list
             before
             (point)
             (buffer-string))))"##,
        expect![[
            r#"OK (33 33 "(defun first (value)\n(let ((next (+ value 1)))\n  (message \"%s\" next)))\n\n(defun second ()\n(message \"untouched\"))\n")"#
        ]],
    )
}

fn auto_auto_indent_indent_defun_falls_back_to_forward_window_for_large_definition()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_indent_defun_falls_back_to_forward_window_for_large_definition",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(defun large ()\n"
           "(let ((one 1))\n"
           "(message \"%s\" one)\n"
           "(when one\n"
           "(message \"still large\"))))\n")
          (goto-char (point-min))
          (forward-line 1)
          (let ((aai-mode t)
                (aai-indent-limit 2)
                (before (point)))
            (aai-indent-defun)
            (list
             before
             (point)
             (buffer-string))))"##,
        expect![[
            r#"OK (17 17 "(defun large ()\n(let ((one 1))\n(message \"%s\" one)\n(when one\n  (message \"still large\"))))\n")"#
        ]],
    )
}

fn auto_auto_indent_indent_defun_falls_back_when_definition_navigation_signals() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_auto_indent_indent_defun_falls_back_when_definition_navigation_signals",
        r##"(with-temp-buffer
          (insert "first\nsecond\nthird\n")
          (goto-char (point-min))
          (let ((aai-mode t)
                (aai-indent-limit 2)
                calls)
            (setq-local
             indent-line-function
             (lambda ()
               (push
                (line-number-at-pos)
                calls)))
            (cl-letf
                (((symbol-function 'end-of-defun)
                  (lambda ()
                    (error
                     "no definition here"))))
              (list
               (aai-indent-defun)
               (point)
               (nreverse calls)))))"##,
        expect!["OK (nil 1 (1 2))"],
    )
}

fn auto_auto_indent_indentable_predicate_can_skip_real_generated_or_literal_lines()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_indentable_predicate_can_skip_real_generated_or_literal_lines",
        r##"(with-temp-buffer
          (emacs-lisp-mode)
          (insert
           "(progn\n"
           ";; GENERATED: preserve column zero\n"
           "(message \"indent me\")\n"
           "\"literal line\"\n"
           "(message \"also indent\"))\n")
          (goto-char (point-min))
          (let ((aai-mode t)
                (aai-indentable-line-p-function
                 (lambda ()
                   (not
                    (or
                     (looking-at-p
                      "[ \t]*;; GENERATED")
                     (nth 3
                          (syntax-ppss
                           (line-beginning-position))))))))
            (aai--indent-region
             (point-min)
             (point-max))
            (buffer-string)))"##,
        expect![[
            r#"OK "(progn\n;; GENERATED: preserve column zero\n  (message \"indent me\")\n  \"literal line\"\n  (message \"also indent\"))\n""#
        ]],
    )
}

fn auto_auto_indent_correct_position_moves_only_points_before_indentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_auto_indent_correct_position_moves_only_points_before_indentation",
        r##"(mapcar
          (lambda (column)
            (with-temp-buffer
              (insert "    value")
              (goto-char
               (+ (point-min) column))
              (list
               column
               (aai-correct-position-this)
               (point)
               (current-column))))
          '(0 2 4 7))"##,
        expect!["OK ((0 5 5 4) (2 5 5 4) (4 nil 5 4) (7 nil 8 7))"],
    )
}

pub(super) fn indentation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_auto_indent_indent_line_maybe_reindents_real_elisp_and_preserves_logical_location(),
        auto_auto_indent_indent_line_maybe_obeys_mode_indent_function_and_predicate_gates(),
        auto_auto_indent_indent_line_maybe_swallows_indent_errors_but_not_predicate_errors(),
        auto_auto_indent_indent_forward_visits_exact_limit_lines_and_repeats_eof_position(),
        auto_auto_indent_indent_forward_reindents_only_configured_real_elisp_window(),
        auto_auto_indent_indent_region_uses_inclusive_end_line_and_stops_at_eof(),
        auto_auto_indent_indent_region_reformats_practical_elisp_and_preserves_point(),
        auto_auto_indent_indent_defun_reformats_only_small_current_definition(),
        auto_auto_indent_indent_defun_falls_back_to_forward_window_for_large_definition(),
        auto_auto_indent_indent_defun_falls_back_when_definition_navigation_signals(),
        auto_auto_indent_indentable_predicate_can_skip_real_generated_or_literal_lines(),
        auto_auto_indent_correct_position_moves_only_points_before_indentation(),
    ]
}
