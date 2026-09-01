//! Oracle parity tests for `eval`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;
use std::sync::OnceLock;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

fn oracle_eval_proptest_failure_path() -> &'static str {
    static PATH: OnceLock<&'static str> = OnceLock::new();
    PATH.get_or_init(|| {
        let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
        Box::leak(
            format!("{target_dir}/proptest-regressions/emacs_core/oracle/eval.txt")
                .into_boxed_str(),
        )
    })
}

#[test]
fn oracle_prop_eval_lexical_flag_controls_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    let (oracle_default, neovm_default) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((f (eval '(let ((x 1)) (lambda () x))))) (funcall f))",
        expect,
    );
    assert_err_kind(&oracle_default, &neovm_default, "void-variable");

    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    let (oracle_nil, neovm_nil) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((f (eval '(let ((x 1)) (lambda () x)) nil))) (funcall f))",
        expect,
    );
    assert_err_kind(&oracle_nil, &neovm_nil, "void-variable");

    let expect = expect_test::expect![[r#""OK 1""#]];
    let (oracle_lex, neovm_lex) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((f (eval '(let ((x 1)) (lambda () x)) t))) (funcall f))",
        expect,
    );
    assert_ok_eq("1", &oracle_lex, &neovm_lex);
}

#[test]
fn oracle_prop_eval_nil_resets_dynamic_mode_after_lexical_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((_ (eval '(let ((x 7)) (lambda () x)) t))) (condition-case nil (let ((f (eval '(let ((x 9)) (lambda () x)) nil))) (funcall f)) (void-variable 'dynamic)))";
    let expect = expect_test::expect![[r#""OK dynamic""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("dynamic", &oracle, &neovm);
}

#[test]
fn oracle_prop_eval_wrong_arity_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments eval 0)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(eval)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

#[test]
fn oracle_prop_eval_lexenv_list_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(eval '(list x y) '((x . 1) (y . 2)))",
        expect,
    );
    assert_ok_eq("(1 2)", &oracle, &neovm);
}

#[test]
fn oracle_prop_eval_lexenv_shadowing_outer_dynamic_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(let ((x 10)) (eval 'x '((x . 3))))", expect);
    assert_ok_eq("3", &oracle, &neovm);
}

#[test]
fn oracle_prop_eval_lexenv_duplicate_binding_first_wins() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(eval 'x '((x . 1) (x . 2)))", expect);
    assert_ok_eq("1", &oracle, &neovm);
}

#[test]
fn oracle_prop_eval_lexenv_binding_with_implicit_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(eval 'x '((x)))", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_prop_eval_lexenv_captured_by_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 99""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((f (eval '(lambda () x) '((x . 99))))) (funcall f))",
        expect,
    );
    let expect = expect_test::expect![[r#""OK 99""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((f (eval '(lambda () x) '((x . 99))))) (let ((x 3)) (funcall f)))",
        expect,
    );
}

#[test]
fn oracle_prop_eval_macro_expansion_with_lexenv() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 9""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(eval '(when x y) '((x . t) (y . 9)))",
        expect,
    );
    assert_ok_eq("9", &oracle, &neovm);
}

#[test]
fn oracle_prop_eval_lexenv_argument_shape_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp (x . 1))""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(eval 'x '(x . 1))", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_eval_error_does_not_leak_lexical_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((_ (condition-case nil (eval '(+ 1 \"x\") t) (error 'err)))) (let ((f (eval '(let ((x 9)) (lambda () x)) nil))) (condition-case nil (funcall f) (void-variable 'dynamic))))";
    let expect = expect_test::expect![[r#""OK dynamic""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("dynamic", &oracle, &neovm);
}

#[test]
fn oracle_prop_eval_nested_mode_switch_with_inner_lexical_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 7""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((f (eval '(eval '(let ((x 7)) (lambda () x)) t) nil))) (funcall f))",
        expect,
    );
    assert_ok_eq("7", &oracle, &neovm);
}

#[test]
fn oracle_prop_eval_dynamic_setq_side_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1""#]];
    // Under lexical binding, `(eval '(setq x 2))` evaluates in a null lexical
    // environment, so `setq` sets the global/dynamic `x`, not the lexical `x`.
    // The outer `let` still sees its lexical `x` = 1.
    // GNU Emacs returns OK 1; NeoVM should match.
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(let ((x 1)) (eval '(setq x 2)) x)", expect);
    assert_eq!(neovm, oracle, "neovm and oracle should match");
}

#[test]
fn oracle_prop_eval_quote_and_function_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK x""#]];
    let (oracle_quote, neovm_quote) =
        crate::common::eval_oracle_and_neovm_expect("(let ((x 1)) (eval '(quote x)))", expect);
    assert_ok_eq("x", &oracle_quote, &neovm_quote);

    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle_fn, neovm_fn) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((f (eval '(function (lambda (x) (+ x 1)))))) (funcall f 41))",
        expect,
    );
    assert_ok_eq("42", &oracle_fn, &neovm_fn);
}

#[test]
fn oracle_prop_eval_error_passthrough_via_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK caught""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(condition-case nil (eval '(car 1)) (wrong-type-argument 'caught))",
        expect,
    );
    assert_ok_eq("caught", &oracle, &neovm);
}

proptest! {
    #![proptest_config({
        let mut config = proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES);
        config.failure_persistence = Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(
                oracle_eval_proptest_failure_path(),
            ),
        ));
        config
    })]

    #[test]
    fn oracle_prop_eval_lexenv_integer_addition(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(eval '(+ x y) '((x . {}) (y . {})))", a, b);
        let expected = (a + b).to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_eval_lexenv_shadows_outer_dynamic_binding(
        outer in -100_000i64..100_000i64,
        inner in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(let ((x {})) (eval 'x '((x . {}))))", outer, inner);
        let expected = inner.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_eval_runtime_constructed_form_addition(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(eval (list '+ {} {}))", a, b);
        let expected = (a + b).to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_eval_dynamic_setq_updates_binding(
        initial in -100_000i64..100_000i64,
        updated in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        // Under lexical binding, (eval '(setq x ...)) sets the dynamic x,
        // not the lexical x from the outer let.  The result is `initial`.
        let form = format!("(let ((x {})) (eval '(setq x {})) x)", initial, updated);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_eq!(neovm, oracle, "neovm and oracle should match");
    }
}
