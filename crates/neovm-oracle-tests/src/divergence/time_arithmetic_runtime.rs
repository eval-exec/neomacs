//! Time encode/decode/arithmetic parity: decode-time fields, time-add/
//! subtract/convert, float-time, format-seconds, parse-time-string,
//! time-to-days, days-between, current-time-zone. Fixed inputs stay stable.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn time_decode_time_dow_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 nil 0 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((dec (decode-time '(26150 29968) t)))
  (list (nth 6 dec) (nth 7 dec) (nth 8 dec) (length dec)))"##,
        expect,
    );
}

#[test]
fn time_time_add_subtract() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3600.0 t \"15:32\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((t1 '(26150 29968)) (t2 (time-add t1 3600)))
  (list (float-time (time-subtract t2 t1))
        (time-less-p t1 t2)
        (format-time-string "%H:%M" t2 t)))"##,
        expect,
    );
}

#[test]
fn time_time_convert_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3661 0 \"(90000 . 1000)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (time-convert 3661 'integer)
        (time-convert '(1 . 1000) 'integer)
        (format "%S" (time-convert 90 1000)))"##,
        expect,
    );
}

#[test]
fn time_float_time_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1713796368.0 1713796368.5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (float-time '(26150 29968))
        (float-time '(26150 29968 500000 0)))"##,
        expect,
    );
}

#[test]
fn time_format_seconds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"1 hour:1 minute:1 second\" \"1 day days 1 hour hours\" \"3 years 62 days 9 hours 46 minutes 40 seconds\" \"2m 5s\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-seconds "%H:%M:%S" 3661)
        (format-seconds "%D days %H hours" 90061)
        (format-seconds "%Y %D %H %M %S" 100000000)
        (format-seconds "%mm %ss" 125))"##,
        expect,
    );
}

#[test]
fn time_parse_time_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (30 15 12 1 6 2024)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((p (parse-time-string "2024-06-01 12:15:30")))
  (list (nth 0 p) (nth 1 p) (nth 2 p) (nth 3 p) (nth 4 p) (nth 5 p)))"##,
        expect,
    );
}

#[test]
fn time_time_to_days() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (738998 113 t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (time-to-days '(26150 29968))
        (time-to-day-in-year '(26150 29968))
        (date-leap-year-p 2024)
        (date-leap-year-p 2023))"##,
        expect,
    );
}

#[test]
fn time_days_between_dates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (60 739052)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (days-between "2024-03-01" "2024-01-01")
        (date-to-day "2024-06-15"))"##,
        expect,
    );
}

#[test]
fn time_current_time_zone_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((z (current-time-zone '(26150 29968) t)))
  (list (car z) (stringp (cadr z))))"##,
        expect,
    );
}

#[test]
fn time_time_subtract_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (-120.0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((t1 '(26150 29968)) (t2 (time-add t1 -120)))
  (list (float-time (time-subtract t2 t1)) (time-less-p t2 t1)))"##,
        expect,
    );
}

#[test]
fn time_encode_time_dst() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 6 2024 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((enc (encode-time (list 0 0 0 15 6 2024 nil nil 0)))
       (dec (decode-time enc 0)))
  (list (nth 3 dec) (nth 4 dec) (nth 5 dec) (nth 8 dec)))"##,
        expect,
    );
}
