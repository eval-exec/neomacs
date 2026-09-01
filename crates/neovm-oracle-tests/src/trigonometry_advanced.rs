//! Advanced oracle parity tests for trigonometric and mathematical functions.
//!
//! Tests sin/cos/tan with special values, asin/acos/atan full ranges,
//! atan2 quadrants, exp/log/sqrt edge cases, log with base argument,
//! isnan/isinf checks, numerical integration (Simpson's rule),
//! and coordinate transforms (polar <-> cartesian).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// sin/cos/tan with special values (0, pi/2, pi, 2*pi)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trig_special_values_sin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // sin(0)=0, sin(pi/2)=1, sin(pi)~0, sin(3pi/2)=-1, sin(2pi)~0
    let form = "(let ((pi float-pi)
                      (eps 1e-10))
                  (list
                    (< (abs (sin 0)) eps)
                    (< (abs (- (sin (/ pi 2.0)) 1.0)) eps)
                    (< (abs (sin pi)) eps)
                    (< (abs (- (sin (* 1.5 pi)) -1.0)) eps)
                    (< (abs (sin (* 2.0 pi))) eps)))";
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_trig_special_values_cos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // cos(0)=1, cos(pi/2)~0, cos(pi)=-1, cos(3pi/2)~0, cos(2pi)=1
    let form = "(let ((pi float-pi)
                      (eps 1e-10))
                  (list
                    (< (abs (- (cos 0) 1.0)) eps)
                    (< (abs (cos (/ pi 2.0))) eps)
                    (< (abs (- (cos pi) -1.0)) eps)
                    (< (abs (cos (* 1.5 pi))) eps)
                    (< (abs (- (cos (* 2.0 pi)) 1.0)) eps)))";
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_trig_special_values_tan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // tan(0)=0, tan(pi/4)=1, tan(pi)~0, tan(-pi/4)=-1
    let form = "(let ((pi float-pi)
                      (eps 1e-10))
                  (list
                    (< (abs (tan 0)) eps)
                    (< (abs (- (tan (/ pi 4.0)) 1.0)) eps)
                    (< (abs (tan pi)) eps)
                    (< (abs (- (tan (/ pi -4.0)) -1.0)) eps)))";
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// asin/acos/atan with full valid ranges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_inverse_trig_full_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test asin/acos across range [-1, 1] at 0.25 increments
    // Verify asin(x) + acos(x) = pi/2 for each value
    let form = "(let ((half-pi (/ float-pi 2.0))
                      (eps 1e-10)
                      (all-ok t))
                  (dolist (x '(-1.0 -0.75 -0.5 -0.25 0.0 0.25 0.5 0.75 1.0))
                    (let ((sum (+ (asin x) (acos x))))
                      (unless (< (abs (- sum half-pi)) eps)
                        (setq all-ok nil))))
                  ;; Also verify atan range: atan maps R -> (-pi/2, pi/2)
                  (dolist (x '(-100.0 -10.0 -1.0 0.0 1.0 10.0 100.0))
                    (let ((a (atan x)))
                      (unless (and (> a (- half-pi))
                                   (< a half-pi))
                        (setq all-ok nil))))
                  all-ok)";
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// atan with two arguments (atan2): all four quadrants + axes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_atan2_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // atan2(y,x) returns angle in (-pi, pi]
    // Test all four quadrants, both axes, and origin-adjacent
    let form = "(let ((pi float-pi)
                      (eps 1e-10))
                  (list
                    ;; Q1: y>0, x>0 => 0 < angle < pi/2
                    (< (abs (- (atan 1.0 1.0) (/ pi 4.0))) eps)
                    ;; Q2: y>0, x<0 => pi/2 < angle < pi
                    (< (abs (- (atan 1.0 -1.0) (* 3.0 (/ pi 4.0)))) eps)
                    ;; Q3: y<0, x<0 => -pi < angle < -pi/2
                    (< (abs (- (atan -1.0 -1.0) (* -3.0 (/ pi 4.0)))) eps)
                    ;; Q4: y<0, x>0 => -pi/2 < angle < 0
                    (< (abs (- (atan -1.0 1.0) (/ pi -4.0))) eps)
                    ;; Positive y-axis: atan2(1,0) = pi/2
                    (< (abs (- (atan 1.0 0.0) (/ pi 2.0))) eps)
                    ;; Negative y-axis: atan2(-1,0) = -pi/2
                    (< (abs (- (atan -1.0 0.0) (/ pi -2.0))) eps)
                    ;; Positive x-axis: atan2(0,1) = 0
                    (< (abs (atan 0.0 1.0)) eps)
                    ;; Negative x-axis: atan2(0,-1) = pi
                    (< (abs (- (atan 0.0 -1.0) pi)) eps)))";
    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// exp/log/sqrt with edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_exp_log_sqrt_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // exp(0)=1, log(1)=0, sqrt(0)=0, sqrt(1)=1
    // log with base: log(8,2)=3, log(1000,10)=3, log(e^5)=5
    // sqrt(x)^2 = x for various x
    let form = "(let ((eps 1e-10))
                  (list
                    ;; exp edge cases
                    (< (abs (- (exp 0) 1.0)) eps)
                    (< (abs (- (exp 1) (exp 1.0))) eps)
                    ;; log edge cases
                    (< (abs (log 1)) eps)
                    (< (abs (- (log (exp 1.0)) 1.0)) eps)
                    ;; log with base argument
                    (< (abs (- (log 8 2) 3.0)) eps)
                    (< (abs (- (log 1000 10) 3.0)) eps)
                    (< (abs (- (log 27 3) 3.0)) eps)
                    (< (abs (- (log 256 2) 8.0)) eps)
                    ;; sqrt edge cases
                    (< (abs (sqrt 0)) eps)
                    (< (abs (- (sqrt 1) 1.0)) eps)
                    ;; sqrt(x)^2 = x
                    (< (abs (- (expt (sqrt 7.0) 2) 7.0)) eps)
                    (< (abs (- (expt (sqrt 144.0) 2) 144.0)) eps)
                    ;; large exp/log roundtrip
                    (< (abs (- (log (exp 20.0)) 20.0)) eps)))";
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(t t t t t t t t t t t t t)", &o, &n);
}

// ---------------------------------------------------------------------------
// isnan / special float checks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_isnan_and_special_float_computation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Check isnan on results of computations, and verify finite results
    let form = "(list
                  ;; Normal results are not NaN
                  (isnan (sin 1.0))
                  (isnan (cos 1.0))
                  (isnan (exp 1.0))
                  (isnan (sqrt 4.0))
                  (isnan (log 2.0))
                  ;; 0.0e+NaN is NaN
                  (isnan 0.0e+NaN)
                  ;; Arithmetic with NaN propagates
                  (isnan (+ 0.0e+NaN 1.0))
                  (isnan (* 0.0e+NaN 0.0))
                  ;; Verify floatp for special values
                  (floatp 1.0e+INF)
                  (floatp -1.0e+INF)
                  (floatp 0.0e+NaN))";
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: numerical integration via Simpson's rule
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_simpsons_rule_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Integrate sin(x) from 0 to pi using Simpson's rule => should be 2.0
    // Simpson's rule: (h/3) * [f(a) + 4*f(a+h) + 2*f(a+2h) + ... + f(b)]
    // Using n=100 subintervals (must be even)
    let form = "(progn
  (fset 'neovm--test-simpsons
    (lambda (f a b n)
      (let* ((h (/ (- b a) (float n)))
             (sum (+ (funcall f a) (funcall f b)))
             (i 1))
        (while (< i n)
          (let ((x (+ a (* i h))))
            (if (= (% i 2) 0)
                (setq sum (+ sum (* 2 (funcall f x))))
              (setq sum (+ sum (* 4 (funcall f x))))))
          (setq i (1+ i)))
        (* (/ h 3.0) sum))))
  (unwind-protect
      (let ((result (funcall 'neovm--test-simpsons 'sin 0.0 float-pi 100)))
        ;; integral of sin(x) from 0 to pi = 2.0
        (list (< (abs (- result 2.0)) 1e-8)
              ;; Also integrate x^2 from 0 to 1 = 1/3
              (let ((sq (lambda (x) (* x x))))
                (< (abs (- (funcall 'neovm--test-simpsons sq 0.0 1.0 100)
                           (/ 1.0 3.0)))
                   1e-8))
              ;; Integrate cos(x) from 0 to pi/2 = 1.0
              (< (abs (- (funcall 'neovm--test-simpsons 'cos 0.0 (/ float-pi 2.0) 100)
                         1.0))
                 1e-8)))
    (fmakunbound 'neovm--test-simpsons)))";
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: polar <-> cartesian coordinate transforms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_polar_cartesian_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert cartesian->polar->cartesian and verify roundtrip
    // polar: r = sqrt(x^2+y^2), theta = atan2(y,x)
    // cartesian: x = r*cos(theta), y = r*sin(theta)
    let form = "(progn
  (fset 'neovm--test-cart-to-polar
    (lambda (x y)
      (let ((r (sqrt (+ (* x x) (* y y))))
            (theta (atan y x)))
        (cons r theta))))
  (fset 'neovm--test-polar-to-cart
    (lambda (r theta)
      (cons (* r (cos theta))
            (* r (sin theta)))))
  (unwind-protect
      (let ((eps 1e-10)
            (all-ok t))
        ;; Test several points in different quadrants
        (dolist (pt '((3.0 . 4.0)
                      (-2.0 . 5.0)
                      (-3.0 . -7.0)
                      (6.0 . -1.0)
                      (0.0 . 1.0)
                      (1.0 . 0.0)))
          (let* ((x (car pt))
                 (y (cdr pt))
                 (polar (funcall 'neovm--test-cart-to-polar x y))
                 (back (funcall 'neovm--test-polar-to-cart (car polar) (cdr polar)))
                 (dx (abs (- (car back) x)))
                 (dy (abs (- (cdr back) y))))
            (unless (and (< dx eps) (< dy eps))
              (setq all-ok nil))))
        ;; Also verify specific known conversions:
        ;; (1,0) -> r=1, theta=0
        (let* ((p (funcall 'neovm--test-cart-to-polar 1.0 0.0)))
          (unless (and (< (abs (- (car p) 1.0)) eps)
                       (< (abs (cdr p)) eps))
            (setq all-ok nil)))
        ;; (0,1) -> r=1, theta=pi/2
        (let* ((p (funcall 'neovm--test-cart-to-polar 0.0 1.0)))
          (unless (and (< (abs (- (car p) 1.0)) eps)
                       (< (abs (- (cdr p) (/ float-pi 2.0))) eps))
            (setq all-ok nil)))
        ;; (3,4) -> r=5
        (let* ((p (funcall 'neovm--test-cart-to-polar 3.0 4.0)))
          (unless (< (abs (- (car p) 5.0)) eps)
            (setq all-ok nil)))
        all-ok)
    (fmakunbound 'neovm--test-cart-to-polar)
    (fmakunbound 'neovm--test-polar-to-cart)))";
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
