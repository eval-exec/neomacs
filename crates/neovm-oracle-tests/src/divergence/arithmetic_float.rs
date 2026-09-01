//! Divergence tests: arithmetic edge cases and float semantics.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_bignum_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (27670116110564327424 9223372036854775808 36893488147419103232 2 0 18446744073709551617 18446744073709551615)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((a (expt 2 64))
        (b (expt 2 63)))
  (list (+ a b)
        (- a b)
        (* a 2)
        (/ a b)
        (mod a b)
        (1+ a)
        (1- a)))"#,
        expect,
    );
}

#[test]
fn divergence_float_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (3.0 3.3333333333333335 1.4142135623730951 3.5 3 1 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (+ 1.0 2.0)
  (/ 10.0 3.0)
  (sqrt 2.0)
  (abs -3.5)
  (max 1 2 3)
  (min 3 2 1)
  (< 1.0 2)
  (> 2 1.5)
  (= 3 3.0))"#,
        expect,
    );
}

#[test]
fn divergence_float_special_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (isnan 0.0e+NaN)
  (isnan 1.0)
  (< 0.0e+NaN 1.0)
  (= 1.0e+INF 1.0e+INF)
  (< 1.0e+INF most-positive-fixnum))"#,
        expect,
    );
}

#[test]
fn divergence_bitwise_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1024 0 15 7 9 -1 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (ash 1 10)
  (ash 1 -1)
  (logand 255 15)
  (logior 1 2 4)
  (logxor 15 6)
  (lognot 0)
  (logcount 255))"#,
        expect,
    );
}

#[test]
fn divergence_trig_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0.0 1.0 0.0 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (sin 0.0)
  (cos 0.0)
  (tan 0.0)
  (> (sin 1.5707963267948966) 0.999)
  (< (abs (- (cos 3.141592653589793) -1.0)) 0.0001))"#,
        expect,
    );
}

#[test]
fn divergence_fixnum_overflow_to_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function most-positive-fixnum)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (most-positive-fixnum)
  (most-negative-fixnum)
  (1+ (most-positive-fixnum))
  (1- (most-negative-fixnum))
  (> (1+ (most-positive-fixnum)) (most-positive-fixnum)))"#,
        expect,
    );
}

#[test]
fn division_by_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (caught arith-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
    (/ 1 0)
  (arith-error (list 'caught (car err))))"#,
        expect,
    );
}
