//! Mixed-type arithmetic/comparison oracle rows (GNU 31.1).
//!
//! Two divergences found by a 20332-row generated matrix (10166 forms x
//! eval + byte-compiled):
//!
//! 1. GNU `arithcompare` coerces BOTH operands with `coerce_marker`
//!    (src/data.c:2705-2709, called at :2720-2721) before any type check.
//!    A detached marker therefore signals "Marker does not point anywhere"
//!    even when the other operand is not a number.  Neomacs validated the
//!    non-number first and never touched the marker.
//!
//! 2. GNU `Fmod` runs `check_number_coerce_marker` on both operands before
//!    choosing the float or integer path (src/data.c:3448-3458), so a
//!    bignum against a non-number reports `number-or-marker-p`.  Neomacs
//!    reached the integer path and reported `integer-or-marker-p`.
//!
//! The matrix's other divergence family (NaN compared with nil/t/string/
//! symbol) is owned by the arithmetic.rs NaN-arm fix and is covered there.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn arithcompare_detached_marker_coercion_precedes_operand_type_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Both orders. A non-number in the first position is the case Neo used
    // to reject before it ever read the marker. A detached marker in the
    // first position must signal the same error.
    for form in [
        "(let ((m (make-marker))) (= nil m))",
        "(let ((m (make-marker))) (= m nil))",
        "(let ((m (make-marker))) (< \"x\" m))",
        "(let ((m (make-marker))) (< m \"x\"))",
        "(let ((m (make-marker))) (>= 'foo m))",
        "(let ((m (make-marker))) (>= m 'foo))",
        "(let ((m (make-marker))) (/= t m))",
        "(let ((m (make-marker))) (/= m t))",
        "(let ((m (make-marker))) (> m nil))",
        "(let ((m (make-marker))) (<= nil m))",
        // Bytecode `Beqlsign` uses the same slow arm as the subr.
        "(let ((m (make-marker))) (funcall (byte-compile (lambda () (= nil m)))))",
    ] {
        let expect = expect_test::expect![[r#""ERR (error \"Marker does not point anywhere\")""#]];
        crate::common::assert_oracle_parity_expect(form, expect);
    }
}

#[test]
fn arithcompare_coerces_a_detached_marker_before_the_nan_arm() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU `coerce_marker` runs before the float/NaN arm, so a detached
    // marker compared with a NaN signals the marker error. NaN compared
    // with nil/t/string/symbol is a different arm and is not pinned here.
    for form in [
        "(let ((m (make-marker))) (= 0.0e+NaN m))",
        "(let ((m (make-marker))) (= m 0.0e+NaN))",
        "(let ((m (make-marker))) (/= 0.0e+NaN m))",
        "(let ((m (make-marker))) (/= m 0.0e+NaN))",
    ] {
        let expect = expect_test::expect![[r#""ERR (error \"Marker does not point anywhere\")""#]];
        crate::common::assert_oracle_parity_expect(form, expect);
    }
}

#[test]
fn arithcompare_live_marker_still_reports_the_non_number_operand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((b (generate-new-buffer \" *arithcompare-live-marker*\")))
           (unwind-protect
               (let ((m (set-marker (make-marker) 1 b)))
                 (= nil m))
             (kill-buffer b)))",
        expect,
    );
}

#[test]
fn mod_checks_number_or_marker_before_integer_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let nil_expect =
        expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect("(mod (expt 2 70) nil)", nil_expect);
    let nil_expect =
        expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect("(mod nil (expt 2 70))", nil_expect);

    let str_expect =
        expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"5\")""#]];
    crate::common::assert_oracle_parity_expect("(mod (expt 2 70) \"5\")", str_expect);

    let sym_expect =
        expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p foo)""#]];
    crate::common::assert_oracle_parity_expect("(mod 'foo (expt 2 70))", sym_expect);

    let t_expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p t)""#]];
    crate::common::assert_oracle_parity_expect("(mod (expt 2 70) t)", t_expect);
    let t_expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p t)""#]];
    crate::common::assert_oracle_parity_expect("(mod t (expt 2 70))", t_expect);

    let ok_expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect("(mod (expt 2 70) 3)", ok_expect);
}
