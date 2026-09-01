//! Oracle parity tests for trigonometric functions: `sin`, `cos`, `tan`,
//! `asin`, `acos`, `atan`, `exp`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// tan
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tan_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(tan 0)", expect);
    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(tan 0.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.5574077246549023""#]];
    crate::common::assert_oracle_parity_expect("(tan 1.0)", expect);
}

#[test]
fn oracle_prop_tan_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // tan(x) = sin(x)/cos(x)
    let form = "(let ((x 0.7))
                  (let ((via-tan (tan x))
                        (via-div (/ (sin x) (cos x))))
                    (< (abs (- via-tan via-div)) 1e-10)))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

// ---------------------------------------------------------------------------
// asin / acos
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_asin_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(asin 0)", expect);
    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(asin 0.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.5707963267948966""#]];
    crate::common::assert_oracle_parity_expect("(asin 1.0)", expect);
    let expect = expect_test::expect![[r#""OK -1.5707963267948966""#]];
    crate::common::assert_oracle_parity_expect("(asin -1.0)", expect);
    let expect = expect_test::expect![[r#""OK 0.5235987755982989""#]];
    crate::common::assert_oracle_parity_expect("(asin 0.5)", expect);
}

#[test]
fn oracle_prop_acos_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1.5707963267948966""#]];
    crate::common::assert_oracle_parity_expect("(acos 0)", expect);
    let expect = expect_test::expect![[r#""OK 1.5707963267948966""#]];
    crate::common::assert_oracle_parity_expect("(acos 0.0)", expect);
    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(acos 1.0)", expect);
    let expect = expect_test::expect![[r#""OK 3.141592653589793""#]];
    crate::common::assert_oracle_parity_expect("(acos -1.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.0471975511965979""#]];
    crate::common::assert_oracle_parity_expect("(acos 0.5)", expect);
}

#[test]
fn oracle_prop_asin_acos_complementary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // asin(x) + acos(x) = pi/2
    let form = "(let ((x 0.3))
                  (let ((sum (+ (asin x) (acos x)))
                        (half-pi (/ float-pi 2.0)))
                    (< (abs (- sum half-pi)) 1e-10)))";
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_asin_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // sin(asin(x)) = x for -1 <= x <= 1
    let form = "(let ((x 0.7))
                  (< (abs (- (sin (asin x)) x)) 1e-10))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_acos_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 0.4))
                  (< (abs (- (cos (acos x)) x)) 1e-10))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

// ---------------------------------------------------------------------------
// atan (1 and 2 argument forms)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_atan_one_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(atan 0)", expect);
    let expect = expect_test::expect![[r#""OK 0.7853981633974483""#]];
    crate::common::assert_oracle_parity_expect("(atan 1.0)", expect);
    let expect = expect_test::expect![[r#""OK -0.7853981633974483""#]];
    crate::common::assert_oracle_parity_expect("(atan -1.0)", expect);
    let expect = expect_test::expect![[r#""OK 0.4636476090008061""#]];
    crate::common::assert_oracle_parity_expect("(atan 0.5)", expect);
}

#[test]
fn oracle_prop_atan_two_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0.7853981633974483""#]];
    // atan with 2 args is atan2(y, x)
    crate::common::assert_oracle_parity_expect("(atan 1.0 1.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.5707963267948966""#]];
    crate::common::assert_oracle_parity_expect("(atan 1.0 0.0)", expect);
    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(atan 0.0 1.0)", expect);
    let expect = expect_test::expect![[r#""OK -2.356194490192345""#]];
    crate::common::assert_oracle_parity_expect("(atan -1.0 -1.0)", expect);
    let expect = expect_test::expect![[r#""OK 3.141592653589793""#]];
    crate::common::assert_oracle_parity_expect("(atan 0.0 -1.0)", expect);
}

#[test]
fn oracle_prop_atan_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // tan(atan(x)) = x
    let form = "(let ((x 2.5))
                  (< (abs (- (tan (atan x)) x)) 1e-10))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_atan2_quadrants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // atan2 distinguishes quadrants
    let form = "(let ((q1 (atan 1.0 1.0))
                      (q2 (atan 1.0 -1.0))
                      (q3 (atan -1.0 -1.0))
                      (q4 (atan -1.0 1.0)))
                  (list (> q1 0) (> q2 0) (< q3 0) (< q4 0)))";
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(t t t t)", &o, &n);
}

// ---------------------------------------------------------------------------
// exp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_exp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1.0""#]];
    crate::common::assert_oracle_parity_expect("(exp 0)", expect);
    let expect = expect_test::expect![[r#""OK 2.718281828459045""#]];
    crate::common::assert_oracle_parity_expect("(exp 1)", expect);
    let expect = expect_test::expect![[r#""OK 0.36787944117144233""#]];
    crate::common::assert_oracle_parity_expect("(exp -1)", expect);
}

#[test]
fn oracle_prop_exp_log_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // exp(log(x)) = x for x > 0
    let form = "(let ((x 3.7))
                  (< (abs (- (exp (log x)) x)) 1e-10))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_log_exp_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // log(exp(x)) = x
    let form = "(let ((x 2.3))
                  (< (abs (- (log (exp x)) x)) 1e-10))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

// ---------------------------------------------------------------------------
// Complex: Pythagorean identity / Euler formula
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trig_pythagorean_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // sin^2(x) + cos^2(x) = 1 for various x
    let form = "(let ((result t))
                  (dolist (x '(0.0 0.5 1.0 1.5 2.0 3.14159 -1.0))
                    (let ((sum (+ (* (sin x) (sin x))
                                  (* (cos x) (cos x)))))
                      (unless (< (abs (- sum 1.0)) 1e-10)
                        (setq result nil))))
                  result)";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_trig_double_angle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // sin(2x) = 2*sin(x)*cos(x)
    let form = "(let ((x 0.8))
                  (let ((lhs (sin (* 2.0 x)))
                        (rhs (* 2.0 (sin x) (cos x))))
                    (< (abs (- lhs rhs)) 1e-10)))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_trig_complex_computation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute a rotation matrix application
    let form = "(let ((angle (/ float-pi 4.0))
                      (px 3.0) (py 4.0))
                  (let ((c (cos angle)) (s (sin angle)))
                    (let ((rx (- (* c px) (* s py)))
                          (ry (+ (* s px) (* c py))))
                      ;; Distance should be preserved: sqrt(rx^2+ry^2)=sqrt(px^2+py^2)
                      (let ((orig-dist (sqrt (+ (* px px) (* py py))))
                            (new-dist (sqrt (+ (* rx rx) (* ry ry)))))
                        (< (abs (- orig-dist new-dist)) 1e-10)))))";
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_trig_taylor_sin_approximation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Taylor series: sin(x) ≈ x - x^3/6 + x^5/120 for small x
    let form = "(let ((x 0.1))
                  (let ((approx (- x
                                   (/ (expt x 3) 6.0)
                                   (- (/ (expt x 5) 120.0)))))
                    (< (abs (- (sin x) approx)) 1e-10)))";
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("t", &o, &n);
}
