//! Strict combo oracle probes, batch 220: time-date arithmetic. time-add /
//! time-subtract, time-less-p / time-equal-p, days-between, time-to-days,
//! time-to-seconds, and seconds-to-string.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_time_add_subtract_less_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'time-date)
(list (time-add 0 1)
      (time-subtract 100 50)
      (time-add '(0 100 0 0) '(0 0 0 500000))
      (time-less-p 0 1)
      (time-less-p 5 5)
      (time-less-p 10 1)
      (time-equal-p 5 5)
      (time-equal-p 5 6)
      (time-subtract 5 5))
"##;
    let expect =
        expect_test::expect![[r#""OK (1 50 (0 100 0 500000) t nil nil t nil (0 0 0 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_days_between_time_to_seconds_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'time-date)
(list (days-between "2025-01-01" "2024-01-01")
      (days-between "2025-03-15" "2025-03-10")
      (time-to-days (date-to-time "2025-01-01"))
      (time-to-seconds '(0 1 0 0))
      (time-to-seconds 0)
      (seconds-to-string 3661)
      (seconds-to-string 86400))
"##;
    let expect = expect_test::expect![[r#""OK (366 5 739252 1.0 0.0 \"61.02m\" \"24.00h\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_date_to_time_encode_decode_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'time-date)
(let ((tz (getenv "TZ")))
  (unwind-protect
      (progn
        (set-time-zone-rule "UTC0")
        (list (date-to-time "2025-03-15 12:00:00")
              (encode-time 0 0 12 15 3 2025 nil -1 nil)
              (float-time (encode-time 0 0 0 1 1 1970 nil -1 nil))
              (float-time (encode-time 0 0 0 2 1 1970 nil -1 nil))))
    (set-time-zone-rule tz)))
"##;
    let expect = expect_test::expect![[r#""OK ((26581 27584) (26581 27584) 0.0 86400.0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
