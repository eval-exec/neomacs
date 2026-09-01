//! Oracle parity tests for GNU `calendar/time-date.el` helper semantics.
//!
//! These focus on deterministic date parsing/formatting, leap-year and month
//! length helpers, interval formatting, and readable seconds formatting.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_time_date_parse_safe_and_day_helpers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'time-date)
  (list
   (format-time-string "%Y-%m-%d %H:%M:%S %z"
                       (date-to-time "Thu, 01 Jan 1970 00:00:00 GMT") t)
   (format-time-string "%Y-%m-%d %H:%M:%S %z"
                       (date-to-time "2000-02-29T12:34:56Z") t)
   (format-time-string "%Y-%m-%d" (safe-date-to-time "not a date") t)
   (date-to-day "2000-01-02T00:00:00Z")
   (days-between "2000-01-10T00:00:00Z" "2000-01-01T00:00:00Z")
   (time-to-day-in-year (date-to-time "2020-12-31T00:00:00Z"))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"1970-01-01 00:00:00 +0000\" \"2000-02-29 12:34:56 +0000\" \"1970-01-01\" 730120 9 365)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_time_date_leap_month_and_ordinal_helpers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'time-date)
  (list
   (mapcar #'date-leap-year-p '(1900 1996 2000 2100))
   (mapcar (lambda (ym) (date-days-in-month (car ym) (cdr ym)))
           '((2020 . 2) (2021 . 2) (2021 . 4) (2021 . 12)))
   (condition-case err
       (date-days-in-month 2021 0)
     (error (list (car err) (cadr err))))
   (condition-case err
       (date-days-in-month 2021 13)
     (error (list (car err) (cadr err))))
   (date-ordinal-to-time 2020 60)
   (date-ordinal-to-time 2021 365)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((nil t t nil) (29 28 30 31) (error \"Month 0 is invalid\") (error \"Month 13 is invalid\") (nil nil nil 29 2 2020 nil nil nil) (nil nil nil 31 12 2021 nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_time_date_format_seconds_flags_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'time-date)
  (list
   (format-seconds "%Y %D %H %M %S" (+ (* 2 31536000) (* 3 86400) (* 4 3600) (* 5 60) 6))
   (format-seconds "%z%Y %D %H %M %S" (+ (* 4 3600) (* 5 60) 6))
   (format-seconds "%Y %D %H %M %S%x" (+ (* 2 31536000) 6))
   (format-seconds "%.3Y %.2D %.2H %.2M %.2S" 3661.25)
   (format-seconds "%,1s" 1.25)
   (format-seconds "%% %s" 9)
   (format-seconds "%s" -1.5)
   (condition-case err
       (format-seconds "%h %d" 1)
     (error (list (car err) (cadr err))))
   (condition-case err
       (format-seconds "%s %s" 1)
     (error (list (car err) (cadr err))))
   (condition-case err
       (format-seconds "%q" 1)
     (error (list (car err) (cadr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"2 years 3 days 4 hours 5 minutes 6 seconds\" \"0 years 0 days 4 hours 5 minutes 6 seconds\" \"2 years 0 days 0 hours 0 minutes 6 seconds\" \"000 years 00 days 01 hour 01 minute 01 second\" \"1.2\" \"% 9\" \"-1\" \"0 0\" (error \"Multiple instances of specifier: ‘s’\") (error \"Bad format specifier: ‘q’\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_time_date_seconds_to_string_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'time-date)
  (list
   (seconds-to-string 0)
   (seconds-to-string 0 t)
   (seconds-to-string 0 t t)
   (seconds-to-string 0.001)
   (seconds-to-string 12.345)
   (seconds-to-string 120)
   (seconds-to-string 3661 t)
   (seconds-to-string 3661 'expanded)
   (seconds-to-string 3661 'expanded t)
   (seconds-to-string 90 t nil 1)
   (seconds-to-string 90 t nil 0.1)
   (seconds-to-string -2.5)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"0s\" \"0 seconds\" \"0s\" \"1.00ms\" \"12.35s\" \"2.00m\" \"1 hour\" \"1 hour 1 minute\" \"1h 1m\" \"1.5 minutes\" \"1.5 minutes\" \"-2.50s\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
