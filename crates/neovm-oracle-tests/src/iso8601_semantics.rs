//! Oracle parity tests for GNU `calendar/iso8601.el` parsing semantics.
//!
//! GNU `iso8601.el` accepts calendar dates, ordinal dates, week dates, partial
//! dates, fractional times, zones, durations, and intervals.  These tests pin
//! the public parser entry points against GNU Emacs for valid and invalid input.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_iso8601_parse_date_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'iso8601)
  (list
   (iso8601-parse-date "2020")
   (iso8601-parse-date "2020-01")
   (iso8601-parse-date "2020-01-15")
   (iso8601-parse-date "20200115")
   (iso8601-parse-date "2020-015")
   (iso8601-parse-date "2020015")
   (iso8601-parse-date "2020-W01-1")
   (iso8601-parse-date "2020W017")
   (iso8601-parse-date "--01-15")
   (iso8601-parse-date "---15")
   (condition-case err
       (iso8601-parse-date "2020-13-40")
     (error (list (car err) (cadr err))))
   (condition-case err
       (iso8601-parse-date 42)
     (error (list (car err) (cadr err))))))"#;

    let expect = expect_test::expect![[
        r#""OK ((nil nil nil nil nil 2020 nil -1 nil) (nil nil nil nil 1 2020 nil -1 nil) (nil nil nil 15 1 2020 nil -1 nil) (nil nil nil 15 1 2020 nil -1 nil) (nil nil nil 15 1 2020 nil -1 nil) (nil nil nil 15 1 2020 nil -1 nil) (nil nil nil 30 12 2019 nil -1 nil) (nil nil nil 5 1 2020 nil -1 nil) (nil nil nil 15 1 nil nil -1 nil) (nil nil nil 15 nil nil nil -1 nil) (nil nil nil 40 13 2020 nil -1 nil) (wrong-type-argument stringp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso8601_parse_time_zone_and_fraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'iso8601)
  (list
   (iso8601-parse-time "16")
   (iso8601-parse-time "1612")
   (iso8601-parse-time "16:12:21")
   (iso8601-parse-time "16:12:21Z")
   (iso8601-parse-time "16:12:21+05:30")
   (iso8601-parse-time "16:12:21.25")
   (iso8601-parse-time "16:12:21.25" t)
   (iso8601-parse-time "16:12.5" t)
   (iso8601-parse-time "16.5" t)
   (iso8601-parse-zone "Z")
   (iso8601-parse-zone "+0530")
   (iso8601-parse-zone "-03:30")
   (condition-case err
       (iso8601-parse-time "99:99:99")
     (error (list (car err) (cadr err))))
   (condition-case err
       (iso8601-parse-zone "UTC")
     (error (list (car err) (cadr err))))))"#;

    let expect = expect_test::expect![[
        r#""OK ((0 0 16 nil nil nil nil -1 nil) (0 12 16 nil nil nil nil -1 nil) (21 12 16 nil nil nil nil -1 nil) (21 12 16 nil nil nil nil nil 0) (21 12 16 nil nil nil nil -1 19800) (21 12 16 nil nil nil nil -1 nil) ((2125 . 100) 12 16 nil nil nil nil -1 nil) (30 12 16 nil nil nil nil -1 nil) (0 30 16 nil nil nil nil -1 nil) 0 330 -210 (99 99 99 nil nil nil nil -1 nil) (wrong-type-argument \"UTC\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso8601_parse_combined_and_validity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'iso8601)
  (list
   (iso8601-valid-p "2020-01-15T16:12:21Z")
   (iso8601-valid-p "20200115T161221-0800")
   (iso8601-valid-p "2020-W01-1T01:02:03+02")
   (iso8601-valid-p "not-a-date")
   (iso8601-parse "2020-01-15T16:12:21Z")
   (iso8601-parse "20200115T161221-0800")
   (iso8601-parse "2020-W01-1T01:02:03+02")
   (condition-case err
       (iso8601-parse "not-a-date")
     (error (list (car err) (cadr err))))
   (condition-case err
       (iso8601-valid-p 42)
     (error (list (car err) (cadr err))))))"#;

    let expect = expect_test::expect![[
        r#""OK (0 0 0 nil (21 12 16 15 1 2020 nil nil 0) (21 12 16 15 1 2020 nil -1 -28800) (3 2 1 30 12 2019 nil -1 7200) (wrong-type-argument \"not-a-date\") (wrong-type-argument stringp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso8601_duration_and_interval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'iso8601)
  (list
   (iso8601-parse-duration "P3Y6M4DT12H30M5S")
   (iso8601-parse-duration "P2W")
   (iso8601-parse-duration "P2020-01-15T16:12:21Z")
   (condition-case err
       (iso8601-parse-duration "P")
     (error (list (car err) (cadr err))))
   (iso8601-parse-interval "2020-01-01/2020-01-03")
   (iso8601-parse-interval "2020-01-01/P2D")
   (iso8601-parse-interval "P2D/2020-01-03")
   (condition-case err
       (iso8601-parse-interval "not/an/interval")
     (error (list (car err) (cadr err))))))"#;

    let expect = expect_test::expect![[
        r#""OK ((5 30 12 4 6 3 nil -1 nil) (nil nil nil 14 nil nil nil -1 nil) (21 12 16 15 1 2020 nil nil 0) (wrong-type-argument \"P\") ((nil nil nil 1 1 2020 nil -1 nil) (nil nil nil 3 1 2020 nil -1 nil) (0 0 0 3 1 1970 6 nil 0)) ((nil nil nil 1 1 2020 nil -1 nil) (0 0 0 3 1 2020 nil -1 nil) (0 0 0 2 0 0 nil -1 nil)) ((0 0 0 1 1 2020 nil -1 nil) (nil nil nil 3 1 2020 nil -1 nil) (0 0 0 2 0 0 nil -1 nil)) (wrong-type-argument \"not/an/interval\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
