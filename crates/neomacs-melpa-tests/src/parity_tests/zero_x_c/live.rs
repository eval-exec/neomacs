use expect_test::expect;

use super::ParityBatchCase;

fn zero_x_c_live_defaults_match_the_pinned_library() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_live_defaults_match_the_pinned_library",
        r##"(list
               0xc-live-display-bases
               0xc-live-input-bases
               (featurep '0xc)
               (featurep '0xc-live))"##,
        expect!["OK ((16 10 8 2) (16 10 8 2) t t)"],
    )
}

fn zero_x_c_live_table_rows_cover_ambiguous_and_prefixed_inputs() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_live_table_rows_cover_ambiguous_and_prefixed_inputs",
        r##"(let ((0xc-max-base 16)
                     (0xc-live-display-bases
                      '(16 10 8 2))
                     (0xc-live-input-bases
                      '(16 10 8 2)))
               (list
                (0xc-live--table-rows "10")
                (0xc-live--table-rows
                 "0xff")
                (0xc-live--table-rows
                 "8:17")))"##,
        expect![[
            r#"OK ((("Input" "Base 16" "Base 10" "Base 8" "Base 2") ("0x10" "10" "16" "20" "10000") ("0d10" "A" "10" "12" "1010") ("0o10" "8" "8" "10" "1000") ("0b10" "2" "2" "2" "10")) (("Input" "Base 16" "Base 10" "Base 8" "Base 2") ("0xff" "FF" "255" "377" "11111111")) (("Input" "Base 16" "Base 10" "Base 8" "Base 2") ("0o17" "F" "15" "17" "1111")))"#
        ]],
    )
}

fn zero_x_c_live_table_rows_signal_when_configured_input_bases_filter_to_empty() -> ParityBatchCase
{
    ParityBatchCase::signal(
        "zero_x_c_live_table_rows_signal_when_configured_input_bases_filter_to_empty",
        r##"(let ((0xc-max-base 36)
                     (0xc-clamp-ten nil)
                     (0xc-clamp-hex nil)
                     (0xc-live-display-bases
                      '(10 16))
                     (0xc-live-input-bases
                      '(2 8)))
               (0xc-live--table-rows
                "z"))"##,
        expect!["ERR (wrong-type-argument sequencep 36)"],
    )
}

fn zero_x_c_live_display_formats_columns_and_renders_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_live_display_formats_columns_and_renders_errors",
        r##"(let (success failure)
               (unwind-protect
                   (progn
                     (0xc-live--display "0xff")
                     (setq success
                           (with-current-buffer
                               "*0xc Live Conversion*"
                             (buffer-string)))
                     (0xc-live--display
                      "not$a$number")
                     (setq failure
                           (with-current-buffer
                               "*0xc Live Conversion*"
                             (buffer-string)))
                     (list success failure))
                 (when
                     (get-buffer
                      "*0xc Live Conversion*")
                   (kill-buffer
                    "*0xc Live Conversion*"))))"##,
        expect![[
            r#"OK ("Input  Base 16  Base 10  Base 8  Base 2    \n0xff   FF       255      377     11111111  \n" "0xc-live: (error Not a number)")"#
        ]],
    )
}

fn zero_x_c_live_maybe_number_at_point_filters_words_by_parser_rules() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_live_maybe_number_at_point_filters_words_by_parser_rules",
        r##"(list
               (with-temp-buffer
                 (insert "value 1234 here")
                 (goto-char 9)
                 (0xc-live--maybe-number-at-point))
               (with-temp-buffer
                 (insert "value word here")
                 (goto-char 9)
                 (0xc-live--maybe-number-at-point))
               (with-temp-buffer
                 (0xc-live--maybe-number-at-point)))"##,
        expect![[r#"OK ("1234" "word" "")"#]],
    )
}

fn zero_x_c_live_convert_installs_a_local_change_hook_around_read_string() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_live_convert_installs_a_local_change_hook_around_read_string",
        r##"(let ((original-hook
                      minibuffer-setup-hook)
                     observed)
               (cl-letf (((symbol-function
                           '0xc-live--maybe-number-at-point)
                          (lambda () "101"))
                         ((symbol-function 'read-string)
                          (lambda
                              (prompt
                               &optional initial
                               history
                               default
                               inherit)
                            (setq observed
                                  (list
                                   prompt
                                   initial
                                   history
                                   default
                                   inherit
                                   (-
                                    (length
                                     minibuffer-setup-hook)
                                    (length
                                     original-hook))))
                            "result")))
                 (list
                  (0xc-live-convert)
                  observed
                  (equal
                   minibuffer-setup-hook
                   original-hook))))"##,
        expect![[r#"OK ("result" ("Number: " nil nil "101" nil 1) t)"#]],
    )
}

pub(super) fn live_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        zero_x_c_live_defaults_match_the_pinned_library(),
        zero_x_c_live_table_rows_cover_ambiguous_and_prefixed_inputs(),
        zero_x_c_live_table_rows_signal_when_configured_input_bases_filter_to_empty(),
        zero_x_c_live_display_formats_columns_and_renders_errors(),
        zero_x_c_live_maybe_number_at_point_filters_words_by_parser_rules(),
        zero_x_c_live_convert_installs_a_local_change_hook_around_read_string(),
    ]
}
