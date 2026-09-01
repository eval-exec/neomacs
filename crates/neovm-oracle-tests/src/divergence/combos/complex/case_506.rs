/// Batch 506: time encoding characterization — encode-time with various slot values.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx506_encode_time_int_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (26002 18128)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (encode-time 0 0 0 1 1 2024 nil) (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx506_encode_time_float_second() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (483732376395791212740608 . 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (encode-time 30.5 30 14 16 6 2024 nil) (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx506_encode_time_float_minute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (encode-time 30 30.5 14 16 6 2024 nil) (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx506_encode_time_float_hour() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (encode-time 30 30 14.5 16 6 2024 nil) (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx506_encode_time_float_day() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (encode-time 30 30 14 16.5 6 2024 nil) (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx506_encode_time_float_month() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (encode-time 30 30 14 16 6.5 2024 nil) (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx506_encode_time_float_year() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (encode-time 30 30 14 16 6 2024.5 nil) (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx506_encode_time_zone_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (26222 54208)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (encode-time 0 0 12 16 6 2024 "UTC") (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx506_encode_time_zone_int() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (26222 54208)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (encode-time 0 0 12 16 6 2024 0) (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx506_encode_time_zone_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (26222 54208)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (encode-time '(0 0 12 16 6 2024 0 nil "UTC")) (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx506_time_add_different_formats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1704088800""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 0 0 1 1 2024 nil)))
  (condition-case e (time-add t1 (seconds-to-time 3600)) (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx506_time_subtract() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 86400""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 0 0 2 1 2024 nil))
      (t2 (encode-time 0 0 0 1 1 2024 nil)))
  (condition-case e (time-subtract t1 t2) (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx506_time_less_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 0 0 1 1 2024 nil))
      (t2 (encode-time 0 0 0 2 1 2024 nil)))
  (time-less-p t1 t2))
"##,
        expect,
    );
}

#[test]
fn div_cx506_time_equal_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 0 0 1 1 2024 nil))
      (t2 (encode-time 0 0 0 1 1 2024 nil)))
  (list (time-equal-p t1 t2) (time-equal-p t1 nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx506_float_time_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1704085200.0 error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 0 0 1 1 2024 nil)))
  (list (condition-case e (float-time t1) (error (car e)))
        (condition-case e (float-time t) (error (car e)))))
"##,
        expect,
    );
}
