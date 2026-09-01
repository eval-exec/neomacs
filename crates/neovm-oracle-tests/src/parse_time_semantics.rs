//! Oracle parity tests for GNU `calendar/parse-time.el` parsing semantics.
//!
//! GNU `parse-time-string` first tries ISO 8601 parsing and then falls back to
//! a liberal token/rule parser.  These tests pin returned decoded-time fields,
//! tokenization, two-digit year rules, timezone parsing, and malformed input.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_parse_time_tokenize_and_rfc_dates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'parse-time)
  (list
   (parse-time-tokenize "Wed, 15 Jan 2020 16:12:21 -0800")
   (parse-time-string "Wed, 15 Jan 2020 16:12:21 -0800")
   (parse-time-string "Thu, 01 Jan 1970 00:00:00 GMT")
   (parse-time-string "Fri Nov 21 09:55:06 1997")
   (parse-time-string "21 Nov 97 09:55 EST")))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"ed\" 15 \"an\" 2020 \"16:12:21\" \"-0800\") (21 12 16 15 1 2020 3 -1 -28800) (0 0 0 1 1 1970 4 nil 0) (6 55 9 21 11 1997 5 -1 nil) (0 55 9 21 11 1997 nil nil -18000))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_parse_time_iso8601_variants_and_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'parse-time)
  (list
   (parse-time-string "2020-01-15T16:12:21-08:00")
   (parse-time-string "2020-01-15T16:12:21Z")
   (parse-time-string "20200115T161221Z")
   (format-time-string "%Y-%m-%d %H:%M:%S %z"
                       (parse-iso8601-time-string "2020-01-15T16:12:21Z") t)
   (format-time-string "%Y-%m-%d %H:%M:%S %z"
                       (parse-iso8601-time-string "2020-01-15T16:12:21-08:00") t)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((21 12 16 15 1 2020 nil -1 -28800) (21 12 16 15 1 2020 nil nil 0) (21 12 16 15 1 2020 nil nil 0) \"2020-01-15 16:12:21 +0000\" \"2020-01-16 00:12:21 +0000\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_parse_time_two_digit_years_times_and_zones() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'parse-time)
  (list
   (parse-time-string "Jan 2 49 1:02")
   (parse-time-string "Jan 2 50 1:02")
   (parse-time-string "Jan 2 99 1:02:03")
   (parse-time-string "Jan 2 00 1:02:03")
   (parse-time-string "Jan 2 2020 1:02")
   (parse-time-string "Jan 2 2020 01:02:03 PDT")
   (parse-time-string "Jan 2 2020 01:02:03 +0530")
   (parse-time-string "Jan 2 2020 01:02:03 -0330")))
"#;

    let expect = expect_test::expect![[
        r#""OK ((0 2 1 2 1 2049 nil -1 nil) (0 2 1 2 1 1950 nil -1 nil) (3 2 1 2 1 1999 nil -1 nil) (3 2 1 2 1 2000 nil -1 nil) (0 2 1 2 1 2020 nil -1 nil) (3 2 1 2 1 2020 nil t -25200) (3 2 1 2 1 2020 nil -1 19800) (3 2 1 2 1 2020 nil -1 -12600))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_parse_time_malformed_and_partial_inputs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'parse-time)
  (list
   (parse-time-string "")
   (parse-time-string "not a date")
   (parse-time-string "25:99")
   (parse-time-string "March 2020")
   (parse-time-string "2020-13-99")
   (condition-case err
       (parse-time-tokenize 42)
     (error (list (car err) (cadr err))))
   (condition-case err
       (parse-time-string 42)
     (error (list (car err) (cadr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((nil nil nil nil nil nil nil -1 nil) (nil nil nil nil nil nil nil -1 nil) (0 99 25 nil nil nil nil -1 nil) (nil nil nil nil 3 2020 nil -1 nil) (nil nil nil 99 13 2020 nil -1 nil) (wrong-type-argument sequencep) (wrong-type-argument sequencep))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
