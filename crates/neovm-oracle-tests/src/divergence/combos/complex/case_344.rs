//! Complex combo batch 344 — `number`/`bignum`/`ratio`/`float` ultimate:
//! expt/log/sqrt full matrix, floor/ceiling/round/truncate all sign combos,
//! mod/remainder with negatives, ash/lsh/logand with bignum, format all
//! specifiers, number-to-string/string-to-number edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx344_format_all_specifiers_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"42\" \"   42\" \"42   |\" \"00042\" \"+42\" \"100\" \"ff\" \"FF\" \"1010\" \"A\" \"β\" \"1.234568e+04\" \"12345.678900\" \"1e-05\" \"3.14\" \"     3.142\" \"hello\" \"(1 \\\"two\\\" 3)\" \"%\" \"a b c\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%d" 42)
      (format "%5d" 42)
      (format "%-5d|" 42)
      (format "%05d" 42)
      (format "%+d" 42)
      (format "%o" 64)
      (format "%x" 255)
      (format "%X" 255)
      (format "%b" 10)
      (format "%c" 65)
      (format "%c" 946)
      (format "%e" 12345.6789)
      (format "%f" 12345.6789)
      (format "%g" 0.00001)
      (format "%.2f" 3.14159)
      (format "%10.3f" 3.14159)
      (format "%s" "hello")
      (format "%S" '(1 "two" 3))
      (format "%%")
      (format "%3$s %2$s %1$s" "c" "b" "a"))
"##,
        expect,
    )
}

#[test]
fn div_cx344_floor_ceiling_round_truncate_all_signs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 -3 3 -2 2 4 -2 -4 2 -2 2.0 3.0 2.0 2.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (floor 2.7) (floor -2.7)
      (ceiling 2.3) (ceiling -2.3)
      (round 2.5) (round 3.5) (round -2.5) (round -3.5)
      (truncate 2.7) (truncate -2.7)
      (ffloor 2.7) (fceiling 2.3)
      (fround 2.5) (ftruncate 2.7))
"##,
        expect,
    )
}

#[test]
fn div_cx344_mod_remainder_negative_divisor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 -1 1 -1 1 2 -2 -1 1.5 1.5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (% 7 3) (% -7 3) (% 7 -3) (% -7 -3)
      (mod 7 3) (mod -7 3) (mod 7 -3) (mod -7 -3)
      (mod 7.5 3) (mod -7.5 3))
"##,
        expect,
    )
}

#[test]
fn div_cx344_bignum_factorial_and_fibonacci() {
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
fn div_cx344_ash_lsh_logand_logior_logxor_with_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (1 1024 4294967296 18446744073709551616 340282366920938463463374607431768211456 128 -1 1298074214633706907132624082305024 1237940039285380274899124224 15 255 240 -1 8 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((big (expt 2 100)))
  (list (ash 1 0) (ash 1 10) (ash 1 32) (ash 1 64) (ash 1 128)
        (ash 256 -1) (ash -1 -1)
        (ash big 10) (ash big -10)
        (logand #xff #x0f) (logior #xf0 #x0f) (logxor #xff #x0f)
        (lognot #x00) (logcount 255) (logcount -1)))
"##,
        expect,
    )
}

#[test]
fn div_cx344_expt_log_sqrt_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable exp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (expt 2 10) (expt 2 0) (expt 2 -1) (expt 2 0.5) (expt 2 -0.5)
      (expt 10 20) (expt 0 0) (expt 1 100)
      (log 100) (log 100 10) (log exp) (log 1)
      (sqrt 16) (sqrt 2) (expt 8 1/3))
"##,
        expect,
    )
}

#[test]
fn div_cx344_number_to_string_string_to_number_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 1/3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (number-to-string 42)
      (number-to-string -42)
      (number-to-string 3.14)
      (number-to-string 1/3)
      (number-to-string (expt 2 64))
      (string-to-number "42")
      (string-to-number "3.14")
      (string-to-number "1/3")
      (string-to-number "0x1A")
      (string-to-number "not-a-number")
      (string-to-number "42abc")
      (string-to-number ""))
"##,
        expect,
    )
}

#[test]
fn div_cx344_ratio_arithmetic_full_reduction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 1/2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (+ 1/2 1/3) (- 5/6 1/2) (* 2/3 3/4) (/ 2/3 4/5)
      (+ 1/2 1/2) (* 6/4 2/3)
      (denominator 6/4) (numerator 6/4)
      (+ 1/2 0) (* 1/3 0))
"##,
        expect,
    )
}

#[test]
fn div_cx344_nan_inf_predicates_and_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((inf (/ 1.0 0.0))
      (neginf (/ -1.0 0.0))
      (nan (/ 0.0 0.0)))
  (list (numberp inf) (floatp inf)
        (= inf inf) (eq inf inf)
        (< neginf inf)
        (numberp nan) (= nan nan) (< nan 0)))
"##,
        expect,
    )
}

#[test]
fn div_cx344_number_arith_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(letrec ((fib (lambda (n) (if (< n 2) n (+ (funcall fib (1- n)) (funcall fib (- n 2)))))))
  (let ((f10 (funcall fib 10))
        (big (+ most-positive-fixnum (funcall fib 20)))
        (ratio 355/113))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert (format "Number mega: fib10=%d big=%s ratio=%s" f10 big ratio))
      (put-text-property 1 6 'face 'bold)
      (let ((m (set-marker (make-marker) 10))
            (ov (make-overlay 4 20)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 30)
        (let ((state (list f10 big ratio
                           (gcd f10 30) (lcm 12 f10)
                           (log big 2) (+ ratio 1/3)
                           (format "%d" big) (format "%x" big)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen()
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    )
}
