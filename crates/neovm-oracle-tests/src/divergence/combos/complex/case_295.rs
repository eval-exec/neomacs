//! Complex combo batch 295 — `number` arithmetic deep: `expt` with
//! negative exponents, `log` with base, `sqrt` negative, `gcd`/`lcm`
//! matrix, `floor`/`ceiling`/`round`/`truncate` with all sign combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx295_expt_with_negative_and_fractional() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 1/3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (expt 2 -1)
      (expt 2 -2)
      (expt 2 0.5)
      (expt 2 -0.5)
      (expt 10 -3)
      (expt -1 0.5)
      (expt 8 1/3)
      (expt 0 0)
      (expt 1 1000))
"##,
        expect,
    )
}

#[test]
fn div_cx295_log_with_various_bases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable exp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (log 100)
      (log 100 10)
      (log exp)
      (log 1)
      (log 256 2)
      (log 1000 10))
"##,
        expect,
    )
}

#[test]
fn div_cx295_gcd_lcm_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function gcd)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (gcd 12 18)
      (gcd 17 23)
      (gcd 100 75)
      (gcd 0 5)
      (lcm 4 6)
      (lcm 3 7)
      (lcm 12 18)
      (lcm 1 1))
"##,
        expect,
    )
}

#[test]
fn div_cx295_floor_ceiling_round_truncate_all_signs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 -3 -3 2 3 -2 -2 3 2 -2 -2 2 2 -2 -2 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (floor 7 3) (floor -7 3) (floor 7 -3) (floor -7 -3)
      (ceiling 7 3) (ceiling -7 3) (ceiling 7 -3) (ceiling -7 -3)
      (round 7 3) (round -7 3) (round 7 -3) (round -7 -3)
      (truncate 7 3) (truncate -7 3) (truncate 7 -3) (truncate -7 -3))
"##,
        expect,
    )
}

#[test]
fn div_cx295_mod_remainder_all_sign_combos() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 -1 1 -1 1 2 -2 -1 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (% 7 3) (% -7 3) (% 7 -3) (% -7 -3)
      (mod 7 3) (mod -7 3) (mod 7 -3) (mod -7 -3)
      (% 0 5) (mod 0 5))
"##,
        expect,
    )
}

#[test]
fn div_cx295_bignum_factorial_fibonacci() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (3628800 2432902008176640000 \"265252859812191058636308480000000\" 55 6765 832040)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(letrec ((fact (lambda (n) (if (= n 0) 1 (* n (funcall fact (1- n))))))
         (fib (lambda (n) (if (< n 2) n (+ (funcall fib (1- n)) (funcall fib (- n 2)))))))
  (list (funcall fact 10)
        (funcall fact 20)
        (number-to-string (funcall fact 30))
        (funcall fib 10)
        (funcall fib 20)
        (funcall fib 30)))
"##,
        expect,
    )
}

#[test]
fn div_cx295_ash_lsh_bignum_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (36893488147419103232 18889465931478580854784 9223372036854775808 0 36893488147419103232 0 18446744073709551617 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((big (expt 2 64)))
  (list (ash big 1)
        (ash big 10)
        (ash big -1)
        (ash big -10)
        (lsh big 1)
        (logand big (1- big))
        (logior big 1)
        (logxor big big)))
"##,
        expect,
    )
}

#[test]
fn div_cx295_sqrt_with_negative_returns_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4.0 1.4142135623730951 0.0 -0.0e+NaN -0.0e+NaN)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (sqrt 16)
      (sqrt 2)
      (sqrt 0)
      (condition-case e (sqrt -1) (error (cons :err (car e))))
      (condition-case e (sqrt -4) (error (cons :err (car e)))))
"##,
        expect,
    )
}

#[test]
fn div_cx295_ratio_arithmetic_full_reduction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 1/2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (+ 1/2 1/3)
      (- 5/6 1/2)
      (* 2/3 3/4)
      (/ 2/3 4/5)
      (+ 1/2 1/2)
      (* 6/4 2/3)
      (denominator 6/4)
      (numerator 6/4)
      (+ 1/2 0)
      (* 1/3 0))
"##,
        expect,
    )
}

#[test]
fn div_cx295_number_arith_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 355/113)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(letrec ((fact (lambda (n) (if (= n 0) 1 (* n (funcall fact (1- n)))))))
  (let ((f10 (funcall fact 10))
        (big (expt 2 128))
        (ratio 355/113))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert (format "Number mega: %d %s %s" f10 big ratio))
      (put-text-property 1 6 'face 'bold)
      (let ((m (set-marker (make-marker) 10))
            (ov (make-overlay 4 18)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 25)
        (let ((state (list f10 big ratio
                           (gcd f10 30)
                           (lcm 12 f10)
                           (log big 2)
                           (+ ratio 1/3)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    )
}
