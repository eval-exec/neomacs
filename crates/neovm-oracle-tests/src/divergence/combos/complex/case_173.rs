//! Complex combo batch 173 — `number` / `bignum` / `ratio` / `float`
//! extreme edge cases: precision overflow, signed zero, denormals,
//! most-positive-fixnum boundary, expt chains.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx173_fixnum_bignum_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (0 0 2305843009213693952 -2305843009213693953 4611686018427387902 -4611686018427387904 0 4611686018427387904 18446744073709551616)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((mpf most-positive-fixnum)
      (mnf most-negative-fixnum))
  (list mpf mnf
        (1+ mpf)
        (1- mnf)
        (* mpf 2)
        (* mnf 2)
        (expt 2 60)
        (expt 2 62)
        (expt 2 64)))
"##,
        expect,
    );
}

#[test]
fn div_cx173_float_precision_overflow_underflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (1e+308 1.0e+INF 1e-308 0.0 1.0e+INF -1.0e+INF -0.0e+NaN 1.0 0.0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (* 1.0 1e308)
      (* 1.0 1e309)
      (* 1.0 1e-308)
      (* 1.0 1e-324)
      (/ 1.0 0.0)
      (/ -1.0 0.0)
      (/ 0.0 0.0)
      (+ 0.5 0.5 0.0)
      (- 0.0 0.0))
"##,
        expect,
    );
}

#[test]
fn div_cx173_signed_zero_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t t nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (eq 0.0 0.0)
      (eq 0.0 -0.0)
      (= 0.0 0.0)
      (= 0.0 -0.0)
      (< 0.0 -0.0)
      (< -0.0 0.0)
      (= 0.0 0)
      (eq 0.0 0))
"##,
        expect,
    );
}

#[test]
fn div_cx173_ratio_arithmetic_no_overflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 1/2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (+ 1/2 1/3)
      (- 1/2 1/3)
      (* 1/2 1/3)
      (/ 1/2 1/3)
      (+ 1/2 1)
      (+ 1/2 0.5)
      (denominator 6/4)
      (numerator 6/4)
      (denominator 1/3)
      (numerator 1/3))
"##,
        expect,
    );
}

#[test]
fn div_cx173_expt_chains_int_vs_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (1 2 18446744073709551616 340282366920938463463374607431768211456 115792089237316195423570985008687907853269984665640564039457584007913129639936 1.4142135623730951 0.5 0.7071067811865476 100000000000000000000 1e-20 1 0 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (expt 2 0)
      (expt 2 1)
      (expt 2 64)
      (expt 2 128)
      (expt 2 256)
      (expt 2 0.5)
      (expt 2 -1)
      (expt 2 -0.5)
      (expt 10 20)
      (expt 10 -20)
      (expt 0 0)
      (expt 0 1)
      (expt 1 100))
"##,
        expect,
    );
}

#[test]
fn div_cx173_floor_ceiling_round_with_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 -3 3 -2 2 4 -2 -4 2 -2 2.0 3.0 2.0 2.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (floor 2.7)
      (floor -2.7)
      (ceiling 2.3)
      (ceiling -2.3)
      (round 2.5)
      (round 3.5)
      (round -2.5)
      (round -3.5)
      (truncate 2.7)
      (truncate -2.7)
      (ffloor 2.7)
      (fceiling 2.3)
      (fround 2.5)
      (ftruncate 2.7))
"##,
        expect,
    );
}

#[test]
fn div_cx173_modulo_with_negative_divisor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 -1 1 -1 1 2 -2 -1 1.5 1.5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (% 7 3)
      (% -7 3)
      (% 7 -3)
      (% -7 -3)
      (mod 7 3)
      (mod -7 3)
      (mod 7 -3)
      (mod -7 -3)
      (mod 7.5 3)
      (mod -7.5 3))
"##,
        expect,
    );
}

#[test]
fn div_cx173_bignum_factorial_via_reduction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-reduce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums (list 10 20 30 40 50)))
  (mapcar (lambda (n)
            (cl-reduce #'* (number-sequence 1 n)))
          nums))
"##,
        expect,
    );
}

#[test]
fn div_cx173_bignum_arithmetic_with_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"115792089237316195423570985008687907853269984665640564039457584007913129639936\" \"10000000000000000000000000000000000000000000000000000000000000000\" \"20000000000000000000000000000000000000000000000000000000000000000000000000000000000000\" \"10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\" 78)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((big (expt 2 256)))
  (list (format "%d" big)
        (format "%x" big)
        (format "%o" big)
        (format "%b" big)
        (length (format "%d" big))))
"##,
        expect,
    );
}

#[test]
fn div_cx173_ash_overflow_to_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (0 4611686018427387902 2361183241434822605824 42535295865117307914475081855261474816 0 0 -1 -1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((mpf most-positive-fixnum))
  (list mpf
        (ash mpf 1)
        (ash mpf 10)
        (ash mpf 64)
        (ash mpf -1)
        (ash mpf -10)
        (ash -1 -1)
        (ash -1 -64)))
"##,
        expect,
    );
}

#[test]
fn div_cx173_float_formatting_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"3.141593\" \"3.14\" \"3.1415926536\" \"3.141593e+00\" \"3.14159\" \"1e-09\" \"1e+09\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%f" 3.141592653589793)
      (format "%.2f" 3.141592653589793)
      (format "%.10f" 3.141592653589793)
      (format "%e" 3.141592653589793)
      (format "%g" 3.141592653589793)
      (format "%g" 0.000000001)
      (format "%g" 1000000000.0))
"##,
        expect,
    );
}

#[test]
fn div_cx173_number_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 355/113)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((big (expt 2 128))
      (ratio 355/113)
      (pi-approx 3.141592653589793))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "big=%s ratio=%s pi=%.15f" big ratio pi-approx))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list (integerp big)
                        (> big most-positive-fixnum)
                        (format "%d" big)
                        (number-to-string ratio)
                        (buffer-string)
                        (marker-position m)
                        (overlay-start ov) (overlay-end ov)
                        (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
