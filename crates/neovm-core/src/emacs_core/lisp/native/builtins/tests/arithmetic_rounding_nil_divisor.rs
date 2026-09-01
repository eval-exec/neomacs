//! Tests for GNU-faithful handling of an explicit `nil` divisor in the
//! rounding builtins (`floor`/`ceiling`/`round`/`truncate`), and the
//! cl-lib wrappers (`cl-floor`/`cl-ceiling`/`cl-truncate`) which forward
//! an unsupplied `&optional` divisor (i.e. `nil`) straight through.
//!
//! GNU's `rounding_driver` (src/floatfns.c) checks `if (NILP (d))` FIRST,
//! treating a nil (or omitted) divisor as the single-arg form:
//!   `return FLOATP (n) ? double_to_integer (double_round (...)) : n;`
//! NeoMacs previously gated the single-arg path on `args.len() == 1`, so
//! an explicitly-passed nil divisor fell into the 2-arg path and failed
//! the `numberp` check with `(wrong-type-argument numberp nil)`.
//!
//! Oracle values were produced with GNU Emacs:
//!   emacs --batch --eval '(prin1 ...)'

use crate::emacs_core::{Context, format_eval_result};

/// Evaluate `src` and return the printed result (mirrors `prin1`),
/// prefixed with `OK`/`ERR` by `format_eval_result`.
fn eval_one(src: &str) -> String {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    format_eval_result(&ev.eval_str(src))
}

// -----------------------------------------------------------------------
// Explicit nil divisor on the primitive rounding builtins. GNU treats a
// nil divisor as omitted (single-arg form), NOT as a number to divide by.
// -----------------------------------------------------------------------

#[test]
fn floor_float_with_nil_divisor() {
    // GNU: (floor 5.5 nil) => 5
    assert_eq!(eval_one("(floor 5.5 nil)"), "OK 5");
}

#[test]
fn ceiling_float_with_nil_divisor() {
    // GNU: (ceiling 5.5 nil) => 6
    assert_eq!(eval_one("(ceiling 5.5 nil)"), "OK 6");
}

#[test]
fn round_int_with_nil_divisor() {
    // GNU: (round 5 nil) => 5 (non-float numerator returned unchanged)
    assert_eq!(eval_one("(round 5 nil)"), "OK 5");
}

#[test]
fn truncate_float_with_nil_divisor() {
    // GNU: (truncate 5.5 nil) => 5
    assert_eq!(eval_one("(truncate 5.5 nil)"), "OK 5");
}

#[test]
fn floor_int_with_nil_divisor() {
    // GNU: (floor 5 nil) => 5
    assert_eq!(eval_one("(floor 5 nil)"), "OK 5");
}

#[test]
fn round_float_with_nil_divisor_uses_bankers_rounding() {
    // GNU: (round 2.5 nil) => 2 (round half to even)
    assert_eq!(eval_one("(round 2.5 nil)"), "OK 2");
}

// -----------------------------------------------------------------------
// A real (non-nil) divisor must still divide — no regression.
// -----------------------------------------------------------------------

#[test]
fn floor_float_with_real_divisor_unchanged() {
    // GNU: (floor 5.5 2) => 2
    assert_eq!(eval_one("(floor 5.5 2)"), "OK 2");
}

#[test]
fn round_no_divisor_bankers_rounding_unchanged() {
    // GNU: (round 2.5) => 2 ; (round 3.5) => 4 ; (round 0.5) => 0
    assert_eq!(eval_one("(round 2.5)"), "OK 2");
    assert_eq!(eval_one("(round 3.5)"), "OK 4");
    assert_eq!(eval_one("(round 0.5)"), "OK 0");
}

#[test]
fn rounding_non_numeric_operands_signal_numberp_in_gnu_check_order() {
    assert_eq!(
        eval_one(
            r#"(mapcar
                 (lambda (form)
                   (condition-case err
                       (eval form t)
                     (error err)))
                 '((ceiling 128 "8")
                   (floor 128 "8")
                   (round 128 "8")
                   (truncate 128 "8")
                   (ceiling "128" "8")))"#
        ),
        r#"OK ((wrong-type-argument numberp "8") (wrong-type-argument numberp "8") (wrong-type-argument numberp "8") (wrong-type-argument numberp "8") (wrong-type-argument numberp "128"))"#
    );
}

// -----------------------------------------------------------------------
// cl-lib wrappers forward their unsupplied &optional divisor as nil
// straight to the primitive `(floor x y)` etc. We exercise the *exact*
// GNU cl-extra.el wrapper bodies inline as lambdas (the bare test
// `Context` has no `cl-lib` load path / `defun`), proving the primitive
// nil-divisor fix is what makes the cl-lib wrappers work.
//
// GNU cl-extra.el:
//   (defun cl-floor (x &optional y)
//     (let ((q (floor x y))) (list q (- x (if y (* y q) q)))))
//   (defun cl-ceiling (x &optional y)
//     (let ((res (cl-floor x y)))
//       (if (= (cadr res) 0) res
//         (list (1+ (car res)) (- (cadr res) (or y 1))))))
//   (defun cl-truncate (x &optional y)
//     (if (eq (>= x 0) (or (null y) (>= y 0))) (cl-floor x y) (cl-ceiling x y)))

#[test]
fn cl_floor_single_arg() {
    // GNU: (cl-floor 3.7) => (3 0.7000000000000002)
    // `y` is the unsupplied &optional => nil, forwarded to `(floor x y)`.
    assert_eq!(
        eval_one(
            "(funcall (lambda (x &optional y) \
                        (let ((q (floor x y))) (list q (- x (if y (* y q) q))))) \
                      3.7)"
        ),
        "OK (3 0.7000000000000002)"
    );
}

#[test]
fn cl_ceiling_single_arg() {
    // GNU: (cl-ceiling 3.7) => (4 -0.2999999999999998)
    assert_eq!(
        eval_one(
            "(let ((cl-floor (lambda (x &optional y) \
                               (let ((q (floor x y))) (list q (- x (if y (* y q) q))))))) \
               (funcall (lambda (x &optional y) \
                          (let ((res (funcall cl-floor x y))) \
                            (if (= (car (cdr res)) 0) res \
                              (list (1+ (car res)) (- (car (cdr res)) (or y 1)))))) \
                        3.7))"
        ),
        "OK (4 -0.2999999999999998)"
    );
}

#[test]
fn cl_truncate_single_arg() {
    // GNU: (cl-truncate 3.7) => (3 0.7000000000000002)
    // For x>=0 with nil y, cl-truncate delegates to cl-floor.
    assert_eq!(
        eval_one(
            "(let ((cl-floor (lambda (x &optional y) \
                               (let ((q (floor x y))) (list q (- x (if y (* y q) q)))))) \
                   (cl-ceiling (lambda (x &optional y) x))) \
               (funcall (lambda (x &optional y) \
                          (if (eq (>= x 0) (or (null y) (>= y 0))) \
                              (funcall cl-floor x y) (funcall cl-ceiling x y))) \
                        3.7))"
        ),
        "OK (3 0.7000000000000002)"
    );
}
