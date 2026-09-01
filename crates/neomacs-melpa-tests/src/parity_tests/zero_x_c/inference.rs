use expect_test::expect;

use super::ParityBatchCase;

fn zero_x_c_number_recognition_honors_strict_padding_and_extension_rules() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_number_recognition_honors_strict_padding_and_extension_rules",
        r##"(list
               (let ((0xc-strict nil))
                 (mapcar
                  #'0xc--is-number-string
                  '("123"
                    "1_000"
                    "0xff"
                    "16:beef"
                    "101.."
                    ".."
                    "0x.."
                    "12$3")))
               (let ((0xc-strict t))
                 (mapcar
                  #'0xc--is-number-string
                  '("123"
                    "1_000"
                    "0xff"
                    "101.."))))"##,
        expect!["OK ((t t t t t nil nil nil) (t nil t t))"],
    )
}

fn zero_x_c_strip_base_hint_covers_named_numeric_and_short_prefixes() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_strip_base_hint_covers_named_numeric_and_short_prefixes",
        r##"(mapcar
               #'0xc--strip-base-hint
               '("0:0"
                 "1234567890"
                 "0x10ff"
                 "'h10ff"
                 "0d246810"
                 "'d246810"
                 "0x"
                 "0b"
                 "0:"
                 "ffffff"
                 "1234567890:1"))"##,
        expect![[r#"OK ("0" "1234567890" "10ff" "10ff" "246810" "246810" "" "" "" "ffffff" "1")"#]],
    )
}

fn zero_x_c_base_prefix_and_output_prefix_cover_builtin_and_numeric_bases() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_base_prefix_and_output_prefix_cover_builtin_and_numeric_bases",
        r##"(list
               (mapcar
                #'0xc--base-prefix
                '("0b1"
                  "'b1"
                  "0t1"
                  "0o1"
                  "'o1"
                  "0d1"
                  "'d1"
                  "0x1"
                  "'h1"
                  "22:1"
                  "1"
                  "0x"
                  "plain"))
               (mapcar
                #'0xc--prefix-for-base
                '(2 3 8 10 16 7 36)))"##,
        expect![[
            r#"OK ((2 2 3 8 8 10 10 16 16 22 nil nil nil) ("0b" "0t" "0o" "0d" "0x" "7:" "36:"))"#
        ]],
    )
}

fn zero_x_c_infer_base_reproduces_every_upstream_clamp_profile() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_infer_base_reproduces_every_upstream_clamp_profile",
        r##"(let ((0xc-max-base 36)
                     (inputs
                      '("efefef"
                        "abcde"
                        "0a0a0a"
                        "75"
                        "12"
                        "101010"
                        "ziltoid"
                        "emacs"
                        "0b101010"
                        "0t102010"
                        "0o123456"
                        "0d246810"
                        "0x101010"
                        "'b101010"
                        "'o123456"
                        "'d246810"
                        "'h101010"
                        "22:10101"
                        "1:000000"
                        "7:100")))
               (mapcar
                (lambda (profile)
                  (let ((0xc-clamp-ten
                         (car profile))
                        (0xc-clamp-hex
                         (cdr profile)))
                    (mapcar
                     #'0xc--infer-base
                     inputs)))
                '((nil)
                  (t)
                  (nil . t)
                  (t . t))))"##,
        expect![[
            r#"OK ((16 15 11 8 3 2 36 29 2 3 8 10 16 2 8 10 16 22 1 7) (16 15 11 10 10 2 36 29 2 3 8 10 16 2 8 10 16 22 1 7) (16 16 16 16 16 2 36 29 2 3 8 10 16 2 8 10 16 22 1 7) (16 16 16 10 10 2 36 29 2 3 8 10 16 2 8 10 16 22 1 7))"#
        ]],
    )
}

fn zero_x_c_infer_base_reports_invalid_prefix_digit_and_maximum_cases() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_infer_base_reports_invalid_prefix_digit_and_maximum_cases",
        r##"(mapcar
               (lambda (case)
                 (condition-case err
                     (let ((0xc-max-base
                            (car case)))
                       (0xc--infer-base
                        (cdr case)))
                   (error
                    (list
                     (car err)
                     (cdr err)))))
               '((16 . "37:shouldfail")
                 (16 . "thi$willfail")
                 (16 . "16::f9281")
                 (16 . "-1:f9281")
                 (16 . "2:102")
                 (10 . "0x0101")
                 (10 . "ziltoid")))"##,
        expect![[
            r#"OK ((error ("Number exceeds maximum allowed base: 16")) (error ("Not a number")) (error ("Not a number")) (error ("Not a number")) (error ("Number has a digit of a higher base than its prefix")) (error ("Number exceeds maximum allowed base: 10")) (error ("Number exceeds maximum allowed base: 10")))"#
        ]],
    )
}

fn zero_x_c_padding_removal_obeys_the_custom_character_set() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_padding_removal_obeys_the_custom_character_set",
        r##"(list
               (let ((0xc-padding " _.,"))
                 (mapcar
                  #'0xc--strip-padding
                  '("1_2_3"
                    "1..."
                    "100,000:0,123,456,789"
                    "0.,xa..b_f  f"
                    "1       7.:,30,30,30")))
               (let ((0xc-padding "0"))
                 (0xc--strip-padding
                  "0b001010110100")))"##,
        expect![[r#"OK (("123" "1" "100000:0123456789" "0xabff" "17:303030") "b11111")"#]],
    )
}

fn zero_x_c_highest_base_tracks_empty_numeric_and_alphabetic_digits() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_highest_base_tracks_empty_numeric_and_alphabetic_digits",
        r##"(mapcar
               #'0xc--highest-base
               '("" "0" "101" "75" "0a0a" "abcde" "Z"))"##,
        expect!["OK (0 1 2 8 11 15 36)"],
    )
}

fn zero_x_c_extension_expands_to_power_of_two_widths() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_extension_expands_to_power_of_two_widths",
        r##"(mapcar
               #'0xc--extend-number
               '("12345"
                 "10100.."
                 "1010.."
                 "ffffff.."
                 "1.2.3.4.5.6.."
                 "WHEEEEEEEE.."
                 "..111"
                 "01..100"
                 "..."))"##,
        expect![[
            r#"OK ("12345" "10100000" "1010" "ffffffff" "1.2.3.4.5.666666" "WHEEEEEEEEEEEEEE" "1111" "01111100" ".")"#
        ]],
    )
}

fn zero_x_c_extension_rejects_multiple_tokens_and_mismatched_neighbors() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_extension_rejects_multiple_tokens_and_mismatched_neighbors",
        r##"(mapcar
               (lambda (number)
                 (condition-case err
                     (0xc--extend-number
                      number)
                   (error
                    (list
                     (car err)
                     (cdr err)))))
               '("..00.."
                 "12345..6789"))"##,
        expect![[
            r#"OK ((error ("Only one extension token may be used")) (error ("The digit before and after the extension token must be the same")))"#
        ]],
    )
}

fn zero_x_c_next_power_of_two_returns_the_smallest_power_at_least_as_large() -> ParityBatchCase {
    ParityBatchCase::value(
        "zero_x_c_next_power_of_two_returns_the_smallest_power_at_least_as_large",
        r##"(mapcar
               #'0xc--next-power-of-2
               '(1 2 3 4 5 7 8 9 15 16))"##,
        expect!["OK (1 2 4 4 8 8 8 16 16 16)"],
    )
}

fn zero_x_c_string_to_number_rejects_a_prefix_above_the_maximum() -> ParityBatchCase {
    ParityBatchCase::signal(
        "zero_x_c_string_to_number_rejects_a_prefix_above_the_maximum",
        r##"(let ((0xc-max-base 10))
               (0xc-string-to-number
                "16:ff"))"##,
        expect![[r#"ERR (error "Number exceeds maximum allowed base: 10")"#]],
    )
}

pub(super) fn inference_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        zero_x_c_number_recognition_honors_strict_padding_and_extension_rules(),
        zero_x_c_strip_base_hint_covers_named_numeric_and_short_prefixes(),
        zero_x_c_base_prefix_and_output_prefix_cover_builtin_and_numeric_bases(),
        zero_x_c_infer_base_reproduces_every_upstream_clamp_profile(),
        zero_x_c_infer_base_reports_invalid_prefix_digit_and_maximum_cases(),
        zero_x_c_padding_removal_obeys_the_custom_character_set(),
        zero_x_c_highest_base_tracks_empty_numeric_and_alphabetic_digits(),
        zero_x_c_extension_expands_to_power_of_two_widths(),
        zero_x_c_extension_rejects_multiple_tokens_and_mismatched_neighbors(),
        zero_x_c_next_power_of_two_returns_the_smallest_power_at_least_as_large(),
        zero_x_c_string_to_number_rejects_a_prefix_above_the_maximum(),
    ]
}
