//! iso8601 parsing (datetime, zone offset, duration P..., date-only, ISO week)
//! + decoded-time accessors / make-decoded-time, format argument indices
//! (%N$s), pp-to-string output, format-time-string of an encoded decoded-time.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn decoded_time_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2024 4 22 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((dt (decode-time '(26150 29968) t)))
  (list (decoded-time-year dt) (decoded-time-month dt) (decoded-time-day dt)
        (decoded-time-weekday dt) (decoded-time-dst dt)))"##,
        expect,
    );
}

#[test]
fn format_arg_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello world\" \"x x\" \"a b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%2$s %1$s" "world" "hello")
        (format "%1$s %1$s" "x")
        (format "%s %s" "a" "b"))"##,
        expect,
    );
}

#[test]
fn format_time_string_decoded() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function make-decoded-time)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((enc (encode-time (make-decoded-time :year 2024 :month 3 :day 1 :hour 8 :minute 0 :second 0 :zone 0))))
  (format-time-string "%Y-%m-%d %H:%M" enc t))"##,
        expect,
    );
}

#[test]
fn iso8601_date_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2024 6 15)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'iso8601)
  (let ((p (iso8601-parse-date "2024-06-15")))
    (list (decoded-time-year p) (decoded-time-month p) (decoded-time-day p)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn iso8601_duration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4 5 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'iso8601)
  (let ((d (iso8601-parse-duration "P1Y2M3DT4H5M6S")))
    (list (decoded-time-year d) (decoded-time-month d) (decoded-time-day d)
          (decoded-time-hour d) (decoded-time-minute d) (decoded-time-second d)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn iso8601_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2024 6 15 12 30 45)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'iso8601)
  (let ((p (iso8601-parse "2024-06-15T12:30:45")))
    (list (decoded-time-year p) (decoded-time-month p) (decoded-time-day p)
          (decoded-time-hour p) (decoded-time-minute p) (decoded-time-second p)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn iso8601_parse_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (18000 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'iso8601)
  (let ((p (iso8601-parse "2024-06-15T12:30:45+05:00")))
    (list (decoded-time-zone p) (decoded-time-hour p)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn iso8601_week() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2024 6 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'iso8601)
  (let ((p (iso8601-parse-date "2024-W25-3")))
    (list (decoded-time-year p) (decoded-time-month p) (decoded-time-day p)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn make_decoded_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function make-decoded-time)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((dt (make-decoded-time :year 2024 :month 6 :day 15 :hour 10)))
  (list (decoded-time-year dt) (decoded-time-hour dt) (decoded-time-minute dt)))"##,
        expect,
    );
}

#[test]
fn pp_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"(1 2 3)\" \"((a . 1) (b . 2))\" \"\\\"string\\\"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-trim (pp-to-string '(1 2 3)))
        (string-trim (pp-to-string '((a . 1) (b . 2))))
        (string-trim (pp-to-string "string")))"##,
        expect,
    );
}
