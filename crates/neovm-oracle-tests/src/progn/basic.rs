//! Oracle parity tests for `progn`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;
use std::sync::OnceLock;

use super::ast::arb_progn_form;
use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

fn oracle_progn_proptest_failure_path() -> &'static str {
    static PATH: OnceLock<&'static str> = OnceLock::new();
    PATH.get_or_init(|| {
        let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
        Box::leak(
            format!("{target_dir}/proptest-regressions/emacs_core/oracle/progn.txt")
                .into_boxed_str(),
        )
    })
}

#[test]
fn oracle_prop_progn_returns_last_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(progn 1 2 3)", expect);
    assert_ok_eq("3", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_empty_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(progn)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_single_form_returns_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(let ((x 42)) (progn x))", expect);
    assert_ok_eq("42", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_observes_left_to_right_side_effect_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form =
        "(let ((xs nil)) (progn (setq xs (cons 'a xs)) (setq xs (cons 'b xs)) (nreverse xs)))";
    let expect = expect_test::expect![[r#""OK (a b)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(a b)", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_error_short_circuits_later_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 0)) (condition-case nil (progn (setq x 1) (car 1) (setq x 2)) (error x)))";
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("1", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_nested_blocks_return_outer_last() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn 10 (progn 20 30) 40)";
    let expect = expect_test::expect![[r#""OK 40""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("40", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_with_defun_body_and_multiple_steps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn (defun neovm--pg-step (x) (progn (setq x (+ x 1)) (setq x (+ x 2)) x)) (unwind-protect (neovm--pg-step 5) (fmakunbound 'neovm--pg-step)))";
    let expect = expect_test::expect![[r#""OK 8""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("8", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_with_macro_expanding_to_progn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn (defmacro neovm--pg-inc2 (sym) (list 'progn (list 'setq sym (list '+ sym 1)) (list 'setq sym (list '+ sym 1)) sym)) (unwind-protect (let ((x 0)) (neovm--pg-inc2 x)) (fmakunbound 'neovm--pg-inc2)))";
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("2", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_multiple_funcall_sequencing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // With lexical binding, the lambda is created before `x` is bound in
    // the `let`, so `x` is not captured — both GNU Emacs and NeoVM signal
    // void-variable.
    let form = "(let ((x 0) (f (lambda () (setq x (1+ x)) x))) (progn (list (funcall f) (funcall f) (funcall f) x)))";
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(neovm, oracle, "neovm and oracle should match");
}

#[test]
fn oracle_prop_progn_multiple_macro_layers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn (defmacro neovm--pg-inner (sym n) (list 'setq sym (list '+ sym n))) (defmacro neovm--pg-outer (sym a b) (list 'progn (list 'neovm--pg-inner sym a) (list 'neovm--pg-inner sym b) sym)) (unwind-protect (let ((x 1)) (neovm--pg-outer x 4 7)) (fmakunbound 'neovm--pg-inner) (fmakunbound 'neovm--pg-outer)))";
    let expect = expect_test::expect![[r#""OK 12""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("12", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_macroexpand_shape_from_builder_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn (defmacro neovm--pg-build (&rest body) (cons 'progn body)) (unwind-protect (macroexpand '(neovm--pg-build 1 (funcall + 2 3) 4)) (fmakunbound 'neovm--pg-build)))";
    let expect = expect_test::expect![[r#""OK (progn 1 (funcall + 2 3) 4)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(progn 1 (funcall + 2 3) 4)", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_nested_progn_inside_macro_then_funcalls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn (defmacro neovm--pg-wrap (&rest body) (cons 'progn body)) (defmacro neovm--pg-inc (sym) (list 'setq sym (list '+ sym 1))) (unwind-protect (let ((x 0)) (neovm--pg-wrap (neovm--pg-inc x) (neovm--pg-inc x) (list x (funcall (lambda (z) (+ z 10)) x)))) (fmakunbound 'neovm--pg-wrap) (fmakunbound 'neovm--pg-inc)))";
    let expect = expect_test::expect![[r#""OK (2 12)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(2 12)", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_function_position_with_side_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 1)) (funcall (progn (setq x (+ x 4)) (lambda (y) (+ x y))) 3))";
    let expect = expect_test::expect![[r#""OK 8""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("8", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_throw_skips_remaining_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(catch 'neovm--pg-tag (progn 1 (throw 'neovm--pg-tag 42) 99))";
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("42", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_unwind_cleanup_happens_on_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 0)) (list (catch 'neovm--pg-tag (progn (unwind-protect (throw 'neovm--pg-tag 7) (setq x 11)) 99)) x))";
    let expect = expect_test::expect![[r#""OK (7 11)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(7 11)", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_nested_error_not_overwritten_by_later_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(condition-case nil (progn 1 (progn (car 1) 2) 3) (wrong-type-argument 'caught))";
    let expect = expect_test::expect![[r#""OK caught""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("caught", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_macro_and_defun_multiple_funcall_flow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn (defmacro neovm--pg-call3 (fn arg) (list 'progn (list 'funcall fn arg) (list 'funcall fn arg) (list 'funcall fn arg))) (defun neovm--pg-bump-cell (cell) (setcar cell (1+ (car cell))) (car cell)) (unwind-protect (let ((cell (list 0))) (list (neovm--pg-call3 'neovm--pg-bump-cell cell) (car cell))) (fmakunbound 'neovm--pg-call3) (fmakunbound 'neovm--pg-bump-cell)))";
    let expect = expect_test::expect![[r#""OK (3 3)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(3 3)", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_macroexpand_nested_progn_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn (defmacro neovm--pg-nested-shape (a b) (list 'progn a (list 'progn b))) (unwind-protect (macroexpand '(neovm--pg-nested-shape (setq x 1) (setq x 2))) (fmakunbound 'neovm--pg-nested-shape)))";
    let expect = expect_test::expect![[r#""OK (progn (setq x 1) (progn (setq x 2)))""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(progn (setq x 1) (progn (setq x 2)))", &oracle, &neovm);
}

#[test]
fn oracle_prop_progn_nonlocal_exit_with_cleanup_and_post_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 'start)) (list (catch 'neovm--pg-tag (progn (setq x 'entered) (unwind-protect (throw 'neovm--pg-tag 'boom) (setq x 'cleaned)) 'tail)) x))";
    let expect = expect_test::expect![[r#""OK (boom cleaned)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config({
        let mut config = proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES);
        config.failure_persistence = Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(
                oracle_progn_proptest_failure_path(),
            ),
        ));
        config
    })]

    #[test]
    fn oracle_prop_progn_returns_last(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(progn {} {})", a, b);
        let expected = b.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_progn_three_forms_returns_last(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
        c in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(progn {} {} {})", a, b, c);
        let expected = c.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_progn_mutation_chain_matches_arithmetic(
        initial in -100_000i64..100_000i64,
        add in -100_000i64..100_000i64,
        sub in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((x {})) (progn (setq x (+ x {})) (setq x (- x {})) x))",
            initial, add, sub
        );
        let expected = (initial + add - sub).to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_progn_error_keeps_last_successful_side_effect(
        initial in -100_000i64..100_000i64,
        first in -100_000i64..100_000i64,
        second in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((x {})) (condition-case nil (progn (setq x {}) (car 1) (setq x {})) (error x)))",
            initial, first, second
        );
        let expected = first.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_progn_macro_generated_mutation_chain(
        initial in -100_000i64..100_000i64,
        add_a in -100_000i64..100_000i64,
        add_b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn (defmacro neovm--pg-prop (sym da db) (list 'progn (list 'setq sym (list '+ sym da)) (list 'setq sym (list '+ sym db)) sym)) (unwind-protect (let ((x {})) (neovm--pg-prop x {} {})) (fmakunbound 'neovm--pg-prop)))",
            initial, add_a, add_b
        );
        let expected = (initial + add_a + add_b).to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_progn_defun_and_multiple_funcalls_accumulate(
        start in -100_000i64..100_000i64,
        d1 in -100_000i64..100_000i64,
        d2 in -100_000i64..100_000i64,
        d3 in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn (defun neovm--pg-acc (x d) (progn (setq x (+ x d)) x)) (unwind-protect (let ((x {})) (progn (setq x (neovm--pg-acc x {})) (setq x (neovm--pg-acc x {})) (setq x (neovm--pg-acc x {})) x)) (fmakunbound 'neovm--pg-acc)))",
            start, d1, d2, d3
        );
        let expected = (start + d1 + d2 + d3).to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_progn_function_position_property(
        base in -100_000i64..100_000i64,
        delta in -100_000i64..100_000i64,
        arg in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((x {})) (funcall (progn (setq x (+ x {})) (lambda (y) (+ x y))) {}))",
            base, delta, arg
        );
        let expected = (base + delta + arg).to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_progn_throw_returns_payload(
        thrown in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(catch 'neovm--pg-tag (progn 1 (throw 'neovm--pg-tag {}) 2))",
            thrown
        );
        let expected = thrown.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_progn_unwind_cleanup_persists_after_throw(
        thrown in -100_000i64..100_000i64,
        cleanup in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((x 0)) (list (catch 'neovm--pg-tag (progn (unwind-protect (throw 'neovm--pg-tag {}) (setq x {})) 99)) x))",
            thrown, cleanup
        );
        let expected = format!("({} {})", thrown, cleanup);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_progn_ast_parity(
        form in arb_progn_form(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        assert_oracle_parity(&form);
    }
}
