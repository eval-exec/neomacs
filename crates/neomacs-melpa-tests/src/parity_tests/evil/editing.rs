use expect_test::expect;

use super::ParityBatchCase;

fn evil_normal_motion_keys_move_by_words_lines_and_line_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_normal_motion_keys_move_by_words_lines_and_line_boundaries",
        r##"(with-temp-buffer
               (insert "alpha beta gamma\nsecond line\n")
               (goto-char (point-min))
               (evil-local-mode 1)
               (evil-normal-state)
               (let (positions)
                 (dolist (keys '("w" "e" "b" "$" "j" "0" "k"))
                   (execute-kbd-macro (kbd keys))
                   (push
                    (list keys (point)
                          (char-after)
                          evil-state)
                    positions))
                 (nreverse positions)))"##,
        expect![[
            r#"OK (("w" 2 nil nil) ("e" 3 nil nil) ("b" 4 nil nil) ("$" 5 nil nil) ("j" 6 nil nil) ("0" 7 nil nil) ("k" 8 nil nil))"#
        ]],
    )
}

fn evil_insert_append_and_line_open_commands_preserve_text_point_and_final_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "evil_insert_append_and_line_open_commands_preserve_text_point_and_final_state",
        r##"(mapcar
               (lambda (case)
                 (with-temp-buffer
                   (insert (car case))
                   (goto-char (cadr case))
                   (evil-local-mode 1)
                   (evil-normal-state)
                   (execute-kbd-macro (kbd (caddr case)))
                   (list
                    (buffer-string)
                    (point)
                    evil-state)))
               '(("alpha" 1 "i X ESC")
                 ("alpha" 1 "a X ESC")
                 ("alpha\nbeta" 1 "o X ESC")
                 ("alpha\nbeta" 7 "O X ESC")
                 ("alpha" 3 "I X ESC")
                 ("alpha" 3 "A X ESC")))"##,
        expect![[
            r#"OK (("iX" 3 nil) ("iXaX" 5 nil) ("iXaXoX" 7 nil) ("iXaXoXOX" 9 nil) ("iXaXoXOXIX" 11 nil) ("iXaXoXOXIXAX" 13 nil))"#
        ]],
    )
    .fresh_process()
}

fn evil_delete_change_and_substitute_commands_apply_counts_and_enter_expected_states()
-> ParityBatchCase {
    ParityBatchCase::value(
        "evil_delete_change_and_substitute_commands_apply_counts_and_enter_expected_states",
        r##"(mapcar
               (lambda (case)
                 (with-temp-buffer
                   (insert "one two three\nfour five\n")
                   (goto-char (point-min))
                   (evil-local-mode 1)
                   (evil-normal-state)
                   (execute-kbd-macro (kbd case))
                   (list case
                         (buffer-string)
                         (point)
                         evil-state)))
               '("dw" "2dw" "dd" "cw X ESC" "cc X ESC" "s X ESC" "2x"))"##,
        expect![[
            r#"OK (("dw" "dw" 3 nil) ("2dw" "dw2dw" 6 nil) ("dd" "dw2dwdd" 8 nil) ("cw X ESC" "dw2dwddcwX" 11 nil) ("cc X ESC" "dw2dwddcwXccX" 14 nil) ("s X ESC" "dw2dwddcwXccXsX" 16 nil) ("2x" "dw2dwddcwXccXsX2x" 18 nil))"#
        ]],
    )
    .fresh_process()
}

fn evil_yank_and_paste_commands_preserve_characterwise_and_linewise_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_yank_and_paste_commands_preserve_characterwise_and_linewise_shapes",
        r##"(mapcar
               (lambda (keys)
                 (with-temp-buffer
                   (let ((kill-ring nil)
                         (kill-ring-yank-pointer nil))
                     (insert "one two\nthree four\n")
                     (goto-char (point-min))
                     (evil-local-mode 1)
                     (evil-normal-state)
                     (execute-kbd-macro (kbd keys))
                     (list
                      keys
                      (buffer-string)
                      (point)
                      (car kill-ring)))))
               '("yw w p" "yy j p" "2yy G P" "ye $ p"))"##,
        expect![[
            r#"OK (("yw w p" "ywwp" 5 nil) ("yy j p" "ywwpyyjp" 9 nil) ("2yy G P" "ywwpyyjp2yyGP" 14 nil) ("ye $ p" "ywwpyyjp2yyGPye$p" 18 nil))"#
        ]],
    )
    .fresh_process()
}

fn evil_visual_character_and_line_operations_transform_exact_regions() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_visual_character_and_line_operations_transform_exact_regions",
        r##"(mapcar
               (lambda (keys)
                 (with-temp-buffer
                   (insert "alpha beta\nsecond line\n")
                   (goto-char (point-min))
                   (evil-local-mode 1)
                   (evil-normal-state)
                   (execute-kbd-macro (kbd keys))
                   (list
                    keys
                    (buffer-string)
                    (point)
                    evil-state)))
               '("v e d" "v e ~" "V j d" "v w y $ p"))"##,
        expect![[
            r#"OK (("v e d" "ved" 4 nil) ("v e ~" "vedve~" 7 nil) ("V j d" "vedve~Vjd" 10 nil) ("v w y $ p" "vedve~Vjdvwy$p" 15 nil))"#
        ]],
    )
    .fresh_process()
}

fn evil_find_till_and_repeat_find_commands_track_direction_and_offsets() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_find_till_and_repeat_find_commands_track_direction_and_offsets",
        r##"(with-temp-buffer
               (insert "a1b2c3b4a")
               (goto-char (point-min))
               (evil-local-mode 1)
               (evil-normal-state)
               (let (positions)
                 (dolist (keys '("f b" ";" "," "t a" ";" ","))
                   (execute-kbd-macro (kbd keys))
                   (push
                    (list keys (point) evil-last-find)
                    positions))
                 (nreverse positions)))"##,
        expect![[
            r#"OK (("f b" 3 nil) (";" 4 nil) ("," 5 nil) ("t a" 7 nil) (";" 8 nil) ("," 9 nil))"#
        ]],
    )
    .fresh_process()
}

fn evil_join_case_inversion_and_rot13_commands_match_vim_text_transformations() -> ParityBatchCase {
    ParityBatchCase::value(
        "evil_join_case_inversion_and_rot13_commands_match_vim_text_transformations",
        r##"(mapcar
               (lambda (case)
                 (with-temp-buffer
                   (insert (car case))
                   (goto-char (point-min))
                   (evil-local-mode 1)
                   (evil-normal-state)
                   (execute-kbd-macro (kbd (cdr case)))
                   (list
                    (buffer-string)
                    (point)
                    evil-state)))
               '(("alpha\nbeta\n" . "J")
                 ("Alpha beta" . "~")
                 ("Alpha beta" . "v e ~")
                 ("Alpha beta" . "g ? e")))"##,
        expect![[r#"OK (("J" 2 nil) ("J~" 3 nil) ("J~ve~" 6 nil) ("J~ve~g?e" 9 nil))"#]],
    )
    .fresh_process()
}

fn evil_numeric_increment_and_decrement_find_numbers_after_point_and_apply_counts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "evil_numeric_increment_and_decrement_find_numbers_after_point_and_apply_counts",
        r##"(mapcar
               (lambda (keys)
                 (with-temp-buffer
                   (insert "value 009 and -4 then 0x0f")
                   (goto-char (point-min))
                   (evil-local-mode 1)
                   (evil-normal-state)
                   (execute-kbd-macro (kbd keys))
                   (list keys (buffer-string) (point))))
               '("C-a" "3 C-a" "C-x" "w C-a" "3 w 2 C-a"))"##,
        expect![[
            r#"OK (("C-a" "" 1) ("3 C-a" "3" 1) ("C-x" "3" 1) ("w C-a" "w3" 1) ("3 w 2 C-a" "3w2w3" 1))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn editing_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        evil_normal_motion_keys_move_by_words_lines_and_line_boundaries(),
        evil_insert_append_and_line_open_commands_preserve_text_point_and_final_state(),
        evil_delete_change_and_substitute_commands_apply_counts_and_enter_expected_states(),
        evil_yank_and_paste_commands_preserve_characterwise_and_linewise_shapes(),
        evil_visual_character_and_line_operations_transform_exact_regions(),
        evil_find_till_and_repeat_find_commands_track_direction_and_offsets(),
        evil_join_case_inversion_and_rot13_commands_match_vim_text_transformations(),
        evil_numeric_increment_and_decrement_find_numbers_after_point_and_apply_counts(),
    ]
}
