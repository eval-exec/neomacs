//! Oracle parity tests for equality primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, eval_oracle_and_neovm, run_neovm_eval};

#[test]
fn oracle_prop_eq_symbol_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(eq 'x 'x)";
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_eq!(oracle.as_str(), "OK t");
    assert_eq!(neovm.as_str(), "OK t");
    assert_eq!(neovm, oracle);
}

#[test]
fn oracle_prop_eq_symbol_distinct() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(eq 'x 'y)";
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_eq!(oracle.as_str(), "OK nil");
    assert_eq!(neovm.as_str(), "OK nil");
    assert_eq!(neovm, oracle);
}

#[test]
fn oracle_prop_eq_wrong_arity_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(eq 1)";
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

#[test]
fn oracle_prop_eq_float_corner_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(list (eq 1.0 1.0) (let ((x 1.0)) (eq x x)) (eq 0.0 -0.0) (eql 0.0 -0.0))";
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_eq!(oracle.as_str(), "OK (nil t nil nil)");
    assert_eq!(neovm.as_str(), "OK (nil t nil nil)");
    assert_eq!(neovm, oracle);
}

/// Test that `eq` on floats flowing through `setq` returns t when
/// the value is bit-identical (the pattern that caused the macroexpand
/// infinite-loop bug in faces.el).
#[test]
fn oracle_prop_eq_float_through_setq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // (let ((x 1.0)) (eq x (setq x x)))  — setq returns the same float
    let form = r####"(let ((x 1.0)) (eq x (setq x x)))"####;
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_eq!(oracle.as_str(), "OK t");
    assert_eq!(neovm.as_str(), "OK t");
    assert_eq!(neovm, oracle);
}

/// Test eq on floats returned from a function call (non-literal, non-symbol
/// AST expression).
#[test]
fn oracle_prop_eq_float_from_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(let ((x 1.5)) (eq x (identity x)))"####;
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_eq!(oracle.as_str(), "OK t");
    assert_eq!(neovm.as_str(), "OK t");
    assert_eq!(neovm, oracle);
}

/// Test the macroexpand-like pattern: eq on a variable vs a setq that
/// assigns the same value from a function return.
#[test]
fn oracle_prop_eq_float_macroexpand_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulates: (not (eq form (setq new-form (macroexpand-1 form env))))
    // where macroexpand-1 returns the same float unchanged.
    let form = r####"
        (let ((form 65535.0)
              (new-form nil))
          (list
           (eq form (setq new-form (identity form)))
           (eq form new-form)))
    "####;
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_eq!(oracle.as_str(), "OK (t t)");
    assert_eq!(neovm.as_str(), "OK (t t)");
    assert_eq!(neovm, oracle);
}

/// Two distinct float literals should not be eq (different allocations).
#[test]
fn oracle_prop_eq_float_distinct_literals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(eq 3.14 3.14)";
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_eq!(oracle.as_str(), "OK nil");
    assert_eq!(neovm.as_str(), "OK nil");
    assert_eq!(neovm, oracle);
}

/// Different float values should never be eq.
#[test]
fn oracle_prop_eq_float_different_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 1.0) (y 2.0)) (eq x y))";
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_eq!(oracle.as_str(), "OK nil");
    assert_eq!(neovm.as_str(), "OK nil");
    assert_eq!(neovm, oracle);
}

/// Calling `eq` through funcall should preserve float identity semantics.
#[test]
fn oracle_prop_eq_float_via_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(funcall 'eq 1.0 1.0)";
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_eq!(oracle.as_str(), "OK nil");
    assert_eq!(neovm.as_str(), oracle.as_str());
}

/// `eq` compares identity, not numeric value, for computed floats.
#[test]
fn oracle_prop_eq_computed_floats_not_identical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(eq (+ 0.5 0.5) (- 2.0 1.0))";
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_eq!(oracle.as_str(), "OK nil");
    assert_eq!(neovm.as_str(), oracle.as_str());
}

/// Two separate calls returning equal float payloads are not `eq`.
#[test]
fn oracle_prop_eq_function_returned_floats_not_identical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((f (lambda () (+ 0.5 0.5)))) (eq (funcall f) (funcall f)))";
    let (oracle, neovm) = eval_oracle_and_neovm(form);

    assert_eq!(oracle.as_str(), "OK nil");
    assert_eq!(neovm.as_str(), oracle.as_str());
}

// ── NeoVM-only tests (no GNU Emacs required) ──────────────────────────

/// Same float through setq must be eq (neovm-only, no oracle needed).
#[test]
fn neovm_eq_float_through_setq() {
    let neovm =
        run_neovm_eval(r#"(let ((x 1.0)) (eq x (setq x x)))"#).expect("neovm eval should run");
    assert_eq!(neovm.as_str(), "OK t");
}

/// Two distinct float literals must not be eq (neovm-only).
#[test]
fn neovm_eq_float_literal_distinct() {
    let neovm = run_neovm_eval("(eq 1.0 1.0)").expect("neovm eval should run");
    assert_eq!(neovm.as_str(), "OK nil");
}

/// Same variable holding a float must be eq to itself (neovm-only).
#[test]
fn neovm_eq_float_same_variable() {
    let neovm = run_neovm_eval(r#"(let ((x 3.14)) (eq x x))"#).expect("neovm eval should run");
    assert_eq!(neovm.as_str(), "OK t");
}

/// Float through identity function must be eq (neovm-only).
#[test]
fn neovm_eq_float_through_identity() {
    let neovm =
        run_neovm_eval(r#"(let ((x 2.5)) (eq x (identity x)))"#).expect("neovm eval should run");
    assert_eq!(neovm.as_str(), "OK t");
}

/// Macroexpand-like pattern: eq on variable vs setq from function return
/// (neovm-only). This is the exact pattern that caused the infinite loop.
#[test]
fn neovm_eq_float_macroexpand_pattern() {
    let neovm = run_neovm_eval(
        r#"
        (let ((form 65535.0)
              (new-form nil))
          (list
           (eq form (setq new-form (identity form)))
           (eq form new-form)))
        "#,
    )
    .expect("neovm eval should run");
    assert_eq!(neovm.as_str(), "OK (t t)");
}

/// Different float values must not be eq (neovm-only).
#[test]
fn neovm_eq_float_different_values() {
    let neovm =
        run_neovm_eval(r#"(let ((x 1.0) (y 2.0)) (eq x y))"#).expect("neovm eval should run");
    assert_eq!(neovm.as_str(), "OK nil");
}

/// 0.0 and -0.0 have different bit patterns, should not be eq (neovm-only).
#[test]
fn neovm_eq_float_zero_vs_neg_zero() {
    let neovm = run_neovm_eval("(eq 0.0 -0.0)").expect("neovm eval should run");
    assert_eq!(neovm.as_str(), "OK nil");
}

// ── Debug: charscript args-out-of-range investigation ─────────────────

#[test]
fn debug_charscript_nth() {
    let neovm = run_neovm_eval(
        r#"(let ((elt '(0 127 latin)))
             (list (car elt) (nth 1 elt) (nth 2 elt)))"#,
    )
    .expect("neovm eval should run");
    assert_eq!(neovm.as_str(), "OK (0 127 latin)");
}

#[test]
fn debug_charscript_cons_range() {
    let neovm = run_neovm_eval(
        r#"(let ((elt '(0 127 latin)))
             (cons (car elt) (nth 1 elt)))"#,
    )
    .expect("neovm eval should run");
    assert_eq!(neovm.as_str(), "OK (0 . 127)");
}

#[test]
fn debug_charscript_set_char_table_range() {
    let neovm = run_neovm_eval(
        r#"(let ((elt '(0 127 latin)))
             (set-char-table-range char-script-table
               (cons (car elt) (nth 1 elt)) (nth 2 elt)))"#,
    )
    .expect("neovm eval should run");
    assert_eq!(neovm.as_str(), "OK latin");
}

#[test]
fn debug_charscript_hex_literals() {
    let neovm = run_neovm_eval(
        r#"(let ((elt '(#x0000 #x007F latin)))
             (list (car elt) (nth 1 elt)
                   (cons (car elt) (nth 1 elt))))"#,
    )
    .expect("neovm eval should run");
    assert_eq!(neovm.as_str(), "OK (0 127 (0 . 127))");
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_eq_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(eq {} {})", a, b);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_eql_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(eql {} {})", a, b);
        let expected = if a == b { "OK t" } else { "OK nil" };
        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), expected);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_equal_operator(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(equal (list {} {}) (list {} {}))", a, b, a, b);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(oracle.as_str(), "OK t");
        prop_assert_eq!(neovm.as_str(), "OK t");
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_equal_operator_distinct_lists(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
        c in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));
        prop_assume!(a != c || b != c);

        let form = format!("(equal (list {} {}) (list {} {}))", a, b, c, c);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);

        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }
}
