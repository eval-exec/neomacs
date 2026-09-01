//! Oracle parity tests for function introspection: `defalias`,
//! `indirect-function`, `fset`, `symbol-function`, `fmakunbound`,
//! `macrop`, `subrp`, `primitive-function-p`, `functionp`.
//!
//! GNU src/eval.c, src/data.c: function cell manipulation and type
//! predicates are central to Emacs' Lisp-2 nature.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_defalias_creates_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defalias 'neovm--test-alias 'car)
  (functionp 'neovm--test-alias))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_indirect_function_follows_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defalias 'neovm--test-indirect 'car)
  (subrp (indirect-function 'neovm--test-indirect)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_indirect_function_on_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(subrp (indirect-function 'car))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_fset_sets_function_cell() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (fset 'neovm--test-fset (lambda () 42))
  (funcall 'neovm--test-fset))"#,
        expect,
    );
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_fmakunbound_clears_function_cell() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (fset 'neovm--test-fmu (lambda () 1))
  (fmakunbound 'neovm--test-fmu)
  (fboundp 'neovm--test-fmu))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_subrp_on_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(subrp (symbol-function 'car))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_subrp_on_lambda_is_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(subrp (lambda () 1))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_functionp_on_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(functionp 'car)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_functionp_on_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(functionp (lambda () 1))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_functionp_on_symbol_without_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (fmakunbound 'neovm--test-nofn) (functionp 'neovm--test-nofn))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_macrop_on_non_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(macrop 'car)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_commandp_on_non_interactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(commandp 'car)"#, expect);
    assert_ok_eq("nil", &o, &n);
}
