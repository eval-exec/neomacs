use expect_test::expect;

use super::ParityBatchCase;

fn alda_font_lock_practical_score_assigns_faces_to_language_constructs() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_font_lock_practical_score_assigns_faces_to_language_constructs",
        r##"(with-temp-buffer
         (insert
          "piano \"lead\":\n"
          "V1: o4 c+8~2 *3 { d e } [f g] | @start\n"
          "(tempo! 120)\n"
          "# comment\n")
         (alda-mode)
         (font-lock-ensure)
         (mapcar
          (lambda (needle)
            (goto-char (point-min))
            (search-forward needle)
            (list needle
                  (get-text-property
                   (1- (point)) 'face)))
          '("piano" "V1" "o4" "c+" "*3" "{" "[" "|"
            "@start" "tempo" "# comment")))"##,
        expect![[
            r##"OK (("piano" font-lock-type-face) ("V1" font-lock-function-name-face) ("o4" font-lock-constant-face) ("c+" font-lock-preprocessor-face) ("*3" font-lock-builtin-face) ("{" font-lock-builtin-face) ("[" font-lock-builtin-face) ("|" font-lock-comment-face) ("@start" font-lock-builtin-face) ("tempo" font-lock-variable-name-face) ("# comment" font-lock-comment-face))"##
        ]],
    )
}

fn alda_calculate_indentation_handles_labels_comments_and_regular_notes() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_calculate_indentation_handles_labels_comments_and_regular_notes",
        r##"(with-temp-buffer
         (insert
          "piano:\n"
          "    c d e\n"
          "\n"
          "      # aligned comment\n"
          "f g a\n")
         (alda-mode)
         (let (results)
           (dolist (line '(1 2 4 5))
             (goto-char (point-min))
             (forward-line (1- line))
             (back-to-indentation)
             (push
              (list line
                    (current-indentation)
                    (alda-calculate-indentation))
              results))
           (nreverse results)))"##,
        expect!["OK ((1 0 0) (2 4 8) (4 6 4) (5 0 8))"],
    )
}

fn alda_indent_previous_level_skips_blank_lines_and_finds_prior_score_indent() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_indent_previous_level_skips_blank_lines_and_finds_prior_score_indent",
        r##"(with-temp-buffer
         (insert
          "piano:\n"
          "      c d e\n"
          "\n"
          "   \n"
          "# comment\n")
         (alda-mode)
         (goto-char (point-max))
         (forward-line -1)
         (back-to-indentation)
         (list
          (alda-indent-prev-level)
          (current-indentation)
          (line-number-at-pos)))"##,
        expect!["OK (3 0 5)"],
    )
}

fn alda_indent_line_reindents_labels_notes_and_comments_preserving_point_semantics()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alda_indent_line_reindents_labels_notes_and_comments_preserving_point_semantics",
        r##"(with-temp-buffer
         (insert
          "    piano:\n"
          "c d e\n"
          "       # comment\n")
         (alda-mode)
         (goto-char (point-min))
         (forward-char 6)
         (let ((label-point-before (point)))
           (alda-indent-line)
           (let ((label-result
                  (list (current-indentation)
                        label-point-before
                        (point))))
             (forward-line 1)
             (alda-indent-line)
             (let ((note-indent (current-indentation)))
               (forward-line 1)
               (alda-indent-line)
               (list
                label-result
                note-indent
                (current-indentation)
                (buffer-string))))))"##,
        expect![[r#"OK ((0 7 3) 8 8 "piano:\n\11c d e\n\11# comment\n")"#]],
    )
}

fn alda_colon_flushes_instrument_labels_but_preserves_inline_note_spacing() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_colon_flushes_instrument_labels_but_preserves_inline_note_spacing",
        r##"(let (results)
         (dolist (initial '("    piano" "c d e "))
           (with-temp-buffer
             (insert initial)
             (alda-mode)
             (goto-char (point-max))
             (cl-letf
                 (((symbol-function 'call-interactively)
                   (lambda (command)
                     (if (eq command 'self-insert-command)
                         (insert ":")
                       (error "unexpected command"))))
                  ((symbol-function 'tab-to-tab-stop)
                   (lambda () (insert "<TAB>"))))
               (alda-colon)
               (push
                (list initial (buffer-string) (point))
                results))))
         (nreverse results))"##,
        expect![[r#"OK (("    piano" "piano:<TAB>" 12) ("c d e " "c d e :<TAB>" 13))"#]],
    )
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alda_font_lock_practical_score_assigns_faces_to_language_constructs(),
        alda_calculate_indentation_handles_labels_comments_and_regular_notes(),
        alda_indent_previous_level_skips_blank_lines_and_finds_prior_score_indent(),
        alda_indent_line_reindents_labels_notes_and_comments_preserving_point_semantics(),
        alda_colon_flushes_instrument_labels_but_preserves_inline_note_spacing(),
    ]
}
