//! Divergence tests: calendar, diary, holidays, solar, lunar deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_calendar_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'calendar)
  (fboundp 'calendar-current-date)
  (featurep 'calendar))"#,
        expect,
    );
}

#[test]
fn divergence_diary_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'diary)
  (fboundp 'diary-view-entries)
  (featurep 'diary-lib))"#,
        expect,
    );
}

#[test]
fn divergence_holidays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'list-holidays)
  (fboundp 'calendar-holiday-list)
  (featurep 'holidays))"#,
        expect,
    );
}

#[test]
fn divergence_solar_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'sunrise-sunset)
  (featurep 'solar))"#,
        expect,
    );
}

#[test]
fn divergence_lunar_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'lunar-phases)
  (featurep 'lunar))"#,
        expect,
    );
}

#[test]
fn divergence_time_date_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'format-time-string)
  (fboundp 'decode-time)
  (fboundp 'encode-time)
  (fboundp 'current-time)
  (fboundp 'time-add)
  (fboundp 'time-subtract))"#,
        expect,
    );
}

#[test]
fn divergence_decode_time_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((time (decode-time)))
  (list (listp time)
        (>= (length time) 9)
        (integerp (nth 0 time))
        (integerp (nth 1 time))
        (integerp (nth 2 time))
        (integerp (nth 3 time))
        (integerp (nth 4 time))
        (integerp (nth 5 time)))) "#,
        expect,
    );
}

#[test]
fn divergence_encode_decode_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (30 45 12 1 6 2025)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((encoded (encode-time 30 45 12 1 6 2025 nil -1 nil))
        (decoded (decode-time encoded)))
  (list (nth 0 decoded)
        (nth 1 decoded)
        (nth 2 decoded)
        (nth 3 decoded)
        (nth 4 decoded)
        (nth 5 decoded))) "#,
        expect,
    );
}

#[test]
fn divergence_float_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'float-time)
  (floatp (float-time))
  (>= (float-time) 0)
  (fboundp 'seconds-to-time))"#,
        expect,
    );
}

#[test]
fn divergence_time_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (t t (27153 25841 493127 0) (27153 25691 493127 0) t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((now (current-time))
        (fixed '(27153 25741 493127 0)))
  (list (listp now)
        (= (length now) 4)
        (time-add fixed 100)
        (time-subtract fixed 50)
        (time-equal-p fixed fixed)
        (time-less-p fixed (time-add fixed 1)))) "#,
        expect,
    );
}
