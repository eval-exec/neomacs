//! Divergence tests: float arithmetic, bignum operations, math deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_float_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (floatp 3.14)
  (floatp 42)
  (= (+ 1.5 2.5) 4.0)
  (= (* 2.0 3.0) 6.0)
  (= (/ 10.0 3.0) (/ 10.0 3.0))
  (= (- 5.5 2.5) 3.0)) "#,
        expect,
    );
}

#[test]
fn divergence_float_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (< 1.5 2.5)
  (> 3.5 2.5)
  (<= 2.0 2.0)
  (>= 3.0 3.0)
  (= 0.0 -0.0)
  (/= 1.0 2.0)) "#,
        expect,
    );
}

#[test]
fn divergence_float_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'isnan)
  (fboundp 'frexp)
  (fboundp 'ldexp)
  (fboundp 'copysign)
  (fboundp 'logb)
  (fboundp 'float-sign)
  (fboundp 'float-digits)
  (fboundp 'float-precision)) "#,
        expect,
    );
}

#[test]
fn divergence_trig_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'sin)
  (fboundp 'cos)
  (fboundp 'tan)
  (fboundp 'asin)
  (fboundp 'acos)
  (fboundp 'atan)
  (floatp (sin 3.14))
  (floatp (cos 3.14))) "#,
        expect,
    );
}

#[test]
fn divergence_math_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'sqrt)
  (fboundp 'exp)
  (fboundp 'log)
  (fboundp 'expt)
  (floatp (sqrt 4.0))
  (= (sqrt 4.0) 2.0)
  (= (expt 2 10) 1024)) "#,
        expect,
    );
}

#[test]
fn divergence_abs_floor_ceil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 3.14 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (abs -5)
  (abs -3.14)
  (= (floor 3.7) 3)
  (= (ceiling 3.2) 4)
  (= (round 3.5) 4)
  (= (truncate 3.7) 3)) "#,
        expect,
    );
}

#[test]
fn divergence_mod_rem() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function fmod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (mod 10 3)
  (mod -10 3)
  (% 10 3)
  (% -10 3)
  (= (fmod 10.0 3.0) (fmod 10.0 3.0))) "#,
        expect,
    );
}

#[test]
fn divergence_bignum_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (integerp (expt 2 64))
  (> (expt 2 64) 0)
  (= (expt 2 10) 1024)
  (= (* (expt 2 60) (expt 2 4)) (expt 2 64))
  (= (1+ most-positive-fixnum) (+ most-positive-fixnum 1))) "#,
        expect,
    );
}

#[test]
fn divergence_bignum_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((a (expt 2 64))
        (b (expt 2 64)))
  (list (= a b)
        (eql a b)
        (eq a b)
        (equal a b))) "#,
        expect,
    );
}

#[test]
fn divergence_random() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'random)
  (integerp (random))
  (>= (random 100) 0)
  (< (random 100) 100)
  (fboundp 'random-seed)) "#,
        expect,
    );
}
