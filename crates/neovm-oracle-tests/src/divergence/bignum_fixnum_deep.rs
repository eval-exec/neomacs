//! Divergence tests: memory-layout, fixnum overflow, bignum edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_fixnum_arithmetic_overflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (2305843009213693952 -2305843009213693953 2305843009213693952 -2305843009213693953)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (1+ most-positive-fixnum)
  (1- most-negative-fixnum)
  (+ most-positive-fixnum 1)
  (- most-negative-fixnum 1))"#,
        expect,
    );
}

#[test]
fn divergence_bignum_multiply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((big (* most-positive-fixnum most-positive-fixnum)))
  (list (> big most-positive-fixnum)
        (integerp big)
        (> (length (number-to-string big)) 10)))"#,
        expect,
    );
}

#[test]
fn divergence_bignum_expt() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1267650600228229401496703205376 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((big (expt 2 100)))
  (list big
        (= big 1267650600228229401496703205376)
        (integerp big)))"#,
        expect,
    );
}

#[test]
fn divergence_bignum_division() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10000000000000000000000000 t 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((big (expt 10 50))
         (div (expt 10 25))
         (result (/ big div)))
  (list result
        (= result (expt 10 25))
        (mod big div)))"#,
        expect,
    );
}

#[test]
fn divergence_float_bignum_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1.2676506002282294e+30 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (float (expt 2 100))
  (> (float (expt 2 100)) 0.0)
  (integerp (expt 2 100))
  (floatp (float (expt 2 100))))"#,
        expect,
    );
}

#[test]
fn divergence_ash_bitwise_shift() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1024 0 4611686018427387902 -256)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (ash 1 10)
  (ash 1 -1)
  (ash most-positive-fixnum 1)
  (ash -1 8))"#,
        expect,
    );
}

#[test]
fn divergence_logand_logior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 255 240 -1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (logand #xFF #x0F)
  (logior #xF0 #x0F)
  (logxor #xFF #x0F)
  (lognot 0)
  (lognot -1))"#,
        expect,
    );
}

#[test]
fn divergence_logbitp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function logbitp)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (logbitp 0 1)
  (logbitp 1 1)
  (logbitp 7 128)
  (logcount 255)
  (integer-length 255))"#,
        expect,
    );
}

#[test]
fn divergence_bignum_equality() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((a (expt 2 100))
        (b (expt 2 100)))
  (list (eq a b)
        (eql a b)
        (equal a b)
        (= a b)))"#,
        expect,
    );
}

#[test]
fn divergence_bignum_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((big1 (expt 2 100))
        (big2 (expt 2 101)))
  (list (< big1 big2)
        (> big2 big1)
        (<= big1 big2)
        (>= big2 big1)
        (/= big1 big2)))"#,
        expect,
    );
}
