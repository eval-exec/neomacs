//! Oracle parity tests for `apply`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_apply_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 10""#]];
    let (oracle_sum, neovm_sum) =
        crate::common::eval_oracle_and_neovm_expect("(apply '+ '(1 2 3 4))", expect);
    assert_ok_eq("10", &oracle_sum, &neovm_sum);

    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    let (oracle_list, neovm_list) =
        crate::common::eval_oracle_and_neovm_expect("(apply 'list 1 2 '(3 4))", expect);
    assert_ok_eq("(1 2 3 4)", &oracle_list, &neovm_list);

    let expect = expect_test::expect![[r#""OK [1 2 3]""#]];
    let (oracle_vec, neovm_vec) =
        crate::common::eval_oracle_and_neovm_expect("(apply 'vector 1 '(2 3))", expect);
    assert_ok_eq("[1 2 3]", &oracle_vec, &neovm_vec);
}

#[test]
fn oracle_prop_apply_wrong_type_error_for_last_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp 2)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(apply '+ 1 2)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_apply_empty_tail_and_runtime_function_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    let (oracle_sum, neovm_sum) =
        crate::common::eval_oracle_and_neovm_expect("(apply '+ nil)", expect);
    assert_ok_eq("0", &oracle_sum, &neovm_sum);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle_list, neovm_list) =
        crate::common::eval_oracle_and_neovm_expect("(apply 'list nil)", expect);
    assert_ok_eq("nil", &oracle_list, &neovm_list);

    let expect = expect_test::expect![[r#""OK 1""#]];
    let (oracle_car, neovm_car) =
        crate::common::eval_oracle_and_neovm_expect("(apply #'car '((1 2)))", expect);
    assert_ok_eq("1", &oracle_car, &neovm_car);
}

#[test]
fn oracle_prop_apply_lambda_optional_and_rest_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form_full = "(apply (lambda (a &optional b &rest xs) (list a b xs)) '(1 2 3 4))";
    let expect = expect_test::expect![[r#""OK (1 2 (3 4))""#]];
    let (oracle_full, neovm_full) = crate::common::eval_oracle_and_neovm_expect(form_full, expect);
    assert_ok_eq("(1 2 (3 4))", &oracle_full, &neovm_full);

    let form_short = "(apply (lambda (a &optional b &rest xs) (list a b xs)) '(1))";
    let expect = expect_test::expect![[r#""OK (1 nil nil)""#]];
    let (oracle_short, neovm_short) =
        crate::common::eval_oracle_and_neovm_expect(form_short, expect);
    assert_ok_eq("(1 nil nil)", &oracle_short, &neovm_short);
}

#[test]
fn oracle_prop_apply_improper_tail_error_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (wrong-type-argument listp 2)""#];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (apply 'list '(1 . 2)) (error err))",
        expect,
    );
}

#[test]
fn oracle_prop_apply_nil_t_and_special_form_call_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (void-function nil)""#];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (apply nil nil) (error err))",
        expect,
    );
    let expect = expect_test::expect![r#""OK (void-function t)""#];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (apply t nil) (error err))",
        expect,
    );
    let expect = expect_test::expect![r#""OK (invalid-function #<subr if>)""#];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (apply 'if '(t 1 2)) (error err))",
        expect,
    );
}

#[test]
fn oracle_prop_apply_autoload_object_error_payload_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(condition-case err (apply '(autoload \"x\" nil nil nil) '(3)) (wrong-type-argument (list (car err) (nth 1 err) (autoloadp (nth 2 err)))))";
    let expect = expect_test::expect![[r#""OK (wrong-type-argument symbolp t)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(wrong-type-argument symbolp t)", &oracle, &neovm);
}

#[test]
fn oracle_prop_apply_keyword_function_cell_controls_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 1""#];
    crate::common::assert_oracle_parity_expect(
        "(let ((orig (symbol-function :k))) (unwind-protect (progn (fset :k 'car) (apply :k '((1 2)))) (fset :k orig)))",
        expect,
    );
    let expect = expect_test::expect![r#""OK (invalid-function :k)""#];
    crate::common::assert_oracle_parity_expect(
        "(let ((orig (symbol-function :k))) (unwind-protect (progn (fset :k 1) (condition-case err (apply :k nil) (error err))) (fset :k orig)))",
        expect,
    );
}

#[test]
fn oracle_prop_apply_zero_args_error_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (wrong-number-of-arguments apply 0)""#];
    crate::common::assert_oracle_parity_expect("(condition-case err (apply) (error err))", expect);
}

#[test]
fn oracle_prop_apply_single_arg_error_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (wrong-type-argument listp +)""#];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (apply '+) (error err))",
        expect,
    );
}

#[test]
fn oracle_prop_apply_non_list_tail_error_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (wrong-type-argument listp [1 2])""#];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (apply 'list [1 2]) (error err))",
        expect,
    );
}

#[test]
fn oracle_prop_apply_argument_evaluation_order_and_single_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 0)) (list (apply 'list (prog1 'a (setq x (1+ x))) (prog1 'b (setq x (1+ x))) (prog1 '(c d) (setq x (1+ x)))) x))";
    let expect = expect_test::expect![[r#""OK ((a b c d) 3)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("((a b c d) 3)", &oracle, &neovm);
}

#[test]
fn oracle_prop_apply_subr_object_ignores_symbol_rebinding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((orig (symbol-function 'car))) (unwind-protect (progn (fset 'car (lambda (&rest _) 'shadow)) (apply orig '((1 2)))) (fset 'car orig)))";
    let expect = expect_test::expect![[r#""OK 1""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("1", &oracle, &neovm);
}

#[test]
fn oracle_prop_apply_forwards_keyword_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid argument list\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(apply 'sort (list (list 3 1 2) #'< :key #'identity))",
        expect,
    );
}

#[test]
fn oracle_prop_apply_lambda_expression_function_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 7""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(apply '(lambda (x y) (+ x y)) '(3 4))",
        expect,
    );
    assert_ok_eq("7", &oracle, &neovm);
}

#[test]
fn oracle_prop_apply_symbol_uses_current_function_cell() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((sym 'neovm--apply-temp)) (fset sym (lambda (&rest xs) (apply '+ xs))) (unwind-protect (let ((first (apply sym '(1 2 3)))) (fset sym (lambda (&rest xs) (length xs))) (list first (apply sym '(1 2 3)))) (fmakunbound sym)))";
    let expect = expect_test::expect![[r#""OK (6 3)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(6 3)", &oracle, &neovm);
}

#[test]
fn oracle_prop_apply_append_with_nil_tail_is_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(apply 'append '(1 2) nil)", expect);
    assert_ok_eq("(1 2)", &oracle, &neovm);
}

#[test]
fn oracle_prop_apply_dotted_parameter_lambda_parity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (invalid-function (closure (t) (a b . rest) (list a b rest)))""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(apply (lambda (a b . rest) (list a b rest)) '(1 2 3 4))",
        expect,
    );
    assert_err_kind(&oracle, &neovm, "invalid-function");
}

#[test]
fn oracle_prop_apply_non_callable_list_error_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (invalid-function (1 2 3))""#];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (apply '(1 2 3) '(4)) (error err))",
        expect,
    );
}

#[test]
fn oracle_prop_apply_lambda_wrong_arity_error_kind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) (a b) (+ a b)) 1)""#
    ]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(apply (lambda (a b) (+ a b)) '(1))", expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

#[test]
fn oracle_prop_apply_prefix_args_with_empty_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(apply 'list 1 2 nil)", expect);
    assert_ok_eq("(1 2)", &oracle, &neovm);
}

#[test]
fn oracle_prop_apply_runtime_generated_tail_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((xs '(2 3))) (apply '+ 1 (append xs nil)))";
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("6", &oracle, &neovm);
}

#[test]
fn oracle_prop_apply_unfbound_symbol_error_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(condition-case err (let ((sym (make-symbol \"neovm-apply-unbound\"))) (apply sym nil)) (error err))";
    let expect = expect_test::expect![r#""OK (void-function neovm-apply-unbound)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_apply_splices_last_list_argument(
        a in -10_000i64..10_000i64,
        b in -10_000i64..10_000i64,
        c in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(apply 'list {} (list {} {}))", a, b, c);
        let expected = format!("({} {} {})", a, b, c);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_apply_mixed_prefix_and_spread_sum(
        a in -10_000i64..10_000i64,
        b in -10_000i64..10_000i64,
        c in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(apply '+ {} (list {} {}))", a, b, c);
        let expected = (a + b + c).to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_apply_list_prefix_and_nested_values(
        a in -10_000i64..10_000i64,
        b in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(apply 'list {} (list (list {}) (list {})))", a, a, b);
        let expected = format!("({} ({}) ({}))", a, a, b);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
