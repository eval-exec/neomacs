//! Deep combo: format + encode-time + decode-time + float arithmetic + parse-time.
//! Tests time/date operations with format and arithmetic.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_encode_decode_time_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (30 45 14 15 6 2025)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((time (encode-time 30 45 14 15 6 2025 nil)))\n\
         (let ((decoded (decode-time time)))\n\
         (list (nth 0 decoded) (nth 1 decoded) (nth 2 decoded)\n\
         (nth 3 decoded) (nth 4 decoded) (nth 5 decoded)))))",
        expect,
    );
}

#[test]
fn deficiency_format_time_string_various_formats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"2024-01-01\" \"12:00:00\" \"Monday\" \"January\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((time (encode-time 0 0 12 1 1 2024 nil)))\n\
         (list (format-time-string \"%Y-%m-%d\" time)\n\
         (format-time-string \"%H:%M:%S\" time)\n\
         (format-time-string \"%A\" time)\n\
         (format-time-string \"%B\" time))))",
        expect,
    );
}

#[test]
fn deficiency_time_add_and_subtract() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"01:00\" \"23:59\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((base (encode-time 0 0 0 1 1 2024 nil))\n\
         (plus (time-add base 3600))\n\
         (minus (time-subtract base 60)))\n\
         (list (format-time-string \"%H:%M\" plus)\n\
         (format-time-string \"%H:%M\" minus))))",
        expect,
    );
}

#[test]
fn deficiency_time_less_p_and_float_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((t1 (encode-time 0 0 10 1 1 2024 nil))\n\
         (t2 (encode-time 0 0 14 1 1 2024 nil)))\n\
         (list (time-less-p t1 t2)\n\
         (time-less-p t2 t1)\n\
         (time-less-p t1 t1)\n\
         (> (float-time t2) (float-time t1)))))",
        expect,
    );
}

#[test]
fn deficiency_format_seconds_with_various_durations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"1 day 1 hour:1 minute:1 second\" \"1 hour:1 minute:1 second\" \"2 minutes:5 seconds\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (format-seconds \"%D %H:%M:%S\" 90061)\n\
         (format-seconds \"%H:%M:%S\" 3661)\n\
         (format-seconds \"%M:%S\" 125)))",
        expect,
    );
}

#[test]
fn deficiency_current_time_format_and_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((now (current-time))\n\
         (formatted (format-time-string \"%Y-%m-%d\" now)))\n\
         (list (length formatted)\n\
         (string-match \"[0-9]\\\\{4\\\\}-[0-9]\\\\{2\\\\}-[0-9]\\\\{2\\\\}\" formatted)\n\
         (>= (nth 5 (decode-time now)) 2024))))",
        expect,
    );
}

#[test]
fn deficiency_time_difference_in_days() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10.0 10)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((d1 (encode-time 0 0 0 1 1 2024 nil))\n\
         (d2 (encode-time 0 0 0 11 1 2024 nil))\n\
         (diff (float-time (time-subtract d2 d1))))\n\
         (list (/ diff 86400.0)\n\
         (round (/ diff 86400.0)))))",
        expect,
    );
}

#[test]
fn deficiency_decode_time_day_of_week() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 nil -18000)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((time (encode-time 0 0 0 1 1 2024 nil))\n\
         (decoded (decode-time time))\n\
         (dow (nth 6 decoded)))\n\
         (list dow\n\
         (nth 7 decoded)\n\
         (nth 8 decoded))))",
        expect,
    );
}

#[test]
fn deficiency_format_number_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"42\" \"00042\" \"ff\" \"10\" \"3.14\" \"1.000000e+03\" \"hello\" \"A\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (format \"%d\" 42)\n\
         (format \"%05d\" 42)\n\
         (format \"%x\" 255)\n\
         (format \"%o\" 8)\n\
         (format \"%.2f\" 3.14159)\n\
         (format \"%e\" 1000.0)\n\
         (format \"%s\" \"hello\")\n\
         (format \"%c\" 65)))",
        expect,
    );
}

#[test]
fn deficiency_format_with_field_width_and_alignment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"        hi\" \"hi        \" \"   42\" \"00042\" \"+42\" \"-42\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (format \"%10s\" \"hi\")\n\
         (format \"%-10s\" \"hi\")\n\
         (format \"%5d\" 42)\n\
         (format \"%05d\" 42)\n\
         (format \"%+d\" 42)\n\
         (format \"%+d\" -42)))",
        expect,
    );
}
