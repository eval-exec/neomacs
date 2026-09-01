//! Complex combo batch 132 — `calc` / `calendar` / `holidays` / `diary`
//! availability, `timeclock` operations, and basic computations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx132_calc_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calc)
      (list (fboundp 'full-calc)
            (fboundp 'calc-eval)
            (boundp 'calc-language)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx132_calc_eval_basic_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"3\" \"12\" \"3.33333333333\" \"1024\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calc)
      (list (calc-eval "1 + 2")
            (calc-eval "3 * 4")
            (calc-eval "10 / 3")
            (calc-eval "2^10")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx132_calendar_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calendar)
      (list (fboundp 'calendar)
            (boundp 'calendar-date-style)
            (boundp 'calendar-week-start-day)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx132_calendar_gregorian_to_absolute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (738886 739251 738945 (7 29 2021))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calendar)
      (list (calendar-absolute-from-gregorian '(1 1 2024))
            (calendar-absolute-from-gregorian '(12 31 2024))
            (calendar-absolute-from-gregorian '(2 29 2024))
            (calendar-gregorian-from-absolute 738000)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx132_holidays_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'holidays)
      (list (fboundp 'holiday-list)
            (boundp 'holiday-general-holidays)
            (boundp 'holiday-local-holidays)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx132_diary_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'diary-lib)
      (list (fboundp 'diary)
            (boundp 'diary-file)
            (boundp 'diary-entry-marker)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx132_timeclock_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'timeclock)
      (list (fboundp 'timeclock-in)
            (fboundp 'timeclock-out)
            (fboundp 'timeclock-status-string)
            (boundp 'timeclock-file)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx132_calendar_day_of_year_calc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calendar)
      (list (calendar-day-of-year '(1 1 2024))
            (calendar-day-of-year '(12 31 2024))
            (calendar-day-of-year '(3 1 2024))
            (calendar-day-name '(1 1 2024))
            (calendar-month-name 1)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx132_calendar_leap_year_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calendar)
      (list (calendar-leap-year-p 2024)
            (calendar-leap-year-p 2023)
            (calendar-leap-year-p 2000)
            (calendar-leap-year-p 1900)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx132_calendar_last_day_of_month() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (29 28 30 31)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calendar)
      (list (calendar-last-day-of-month 2 2024)
            (calendar-last-day-of-month 2 2023)
            (calendar-last-day-of-month 4 2024)
            (calendar-last-day-of-month 12 2024)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx132_calc_eval_radix_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"16#FF\" \"16#A\" \"16#40\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calc)
      (let ((calc-number-radix 16))
        (list (calc-eval "255")
              (let ((calc-number-radix 2)) (calc-eval "10"))
              (let ((calc-number-radix 8)) (calc-eval "64")))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx132_calc_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calc)
      (let ((result (calc-eval "(1 + 2) * 3")))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert (format "calc result: %s" result))
          (put-text-property 1 5 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (let ((state (list result (buffer-string)
                               (marker-position m)
                               (overlay-start ov) (overlay-end ov)
                               (text-properties-at 1))))
              (undo)
              (widen)
              (list state (buffer-string) (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (text-properties-at 1)))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
