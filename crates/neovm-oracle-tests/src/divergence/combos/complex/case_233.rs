//! Complex combo batch 233 — `calc` deep: `math-eval`, `calc-eval` with
//! algebraic simplification, `math-read-expr`, radix conversions, and
//! symbolic computation availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx233_calc_eval_basic_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"3\" \"12\" \"3.33333333333\" \"1024\" \"4\" \"3628800\" \"6\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calc)
      (list (calc-eval "1 + 2")
            (calc-eval "3 * 4")
            (calc-eval "10 / 3")
            (calc-eval "2^10")
            (calc-eval "sqrt(16)")
            (calc-eval "10!")
            (calc-eval "gcd(12, 18)")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx233_calc_eval_algebraic_simplification() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"5 x\" \"(a + b)^2\" \"0\" \"0\" \"1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calc)
      (list (calc-eval "2x + 3x")
            (calc-eval "(a + b)^2")
            (calc-eval "sin(0)")
            (calc-eval "ln(1)")
            (calc-eval "exp(0)")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx233_calc_radix_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"16#FF\" \"16#10\" \"16#A\" \"16#40\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calc)
      (let ((calc-number-radix 16))
        (list (calc-eval "255")
              (calc-eval "16")
              (let ((calc-number-radix 2)) (calc-eval "10"))
              (let ((calc-number-radix 8)) (calc-eval "64")))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx233_calc_matrix_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"[[19, 22], [43, 50]]\" \"-2\" \"[5, 7, 9]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calc)
      (list (calc-eval "[1, 2; 3, 4] * [5, 6; 7, 8]")
            (calc-eval "det([1, 2; 3, 4])")
            (calc-eval "[1, 2, 3] + [4, 5, 6]")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx233_calc_fraction_and_rational() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"0.5\" \"0.5\" \"0.0833333333333\" \"1.5\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calc)
      (list (calc-eval "1 / 3 + 1 / 6")
            (calc-eval "2 / 4")
            (calc-eval "1 / 2 * 2 / 3")
            (calc-eval "6 / 4")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx233_calc_eval_trigonometric() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"0\" \"1\" \"0\" \"asin(1)\" \"acos(0)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calc)
      (list (calc-eval "sin(0)")
            (calc-eval "cos(0)")
            (calc-eval "tan(0)")
            (calc-eval "asin(1)")
            (calc-eval "acos(0)")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx233_math_read_expr_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'math-read-expr)
          (fboundp 'math-evaluate-expr)
          (fboundp 'calc-do-alg-entry)
          (boundp 'calc-language))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx233_calc_modes_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'calc-angle-mode)
          (boundp 'calc-complex-mode)
          (boundp 'calc-infinite-mode)
          (boundp 'calc-symbolic-mode)
          (boundp 'calc-display-just))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx233_calc_log_and_exponential() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"2\" \"2\" \"1.\" \"2.71828182846\" \"1000\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calc)
      (list (calc-eval "log(100, 10)")
            (calc-eval "log10(100)")
            (calc-eval "ln(exp(1))")
            (calc-eval "exp(1)")
            (calc-eval "10^3")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx233_calc_with_marker_overlay_undo_narrow_mega() {
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
          (insert (format "Calc mega: %s" result))
          (put-text-property 1 5 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (let ((state (list result
                               (calc-eval "42")
                               (buffer-string)
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
