//! Oracle parity tests for closure/oclosure-related behavior.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_closure_primitives_are_consistent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list (fboundp 'closurep) (fboundp 'make-closure) (fboundp 'make-interpreted-closure))",
        expect,
    );
}

#[test]
fn oracle_prop_closurep_on_common_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list (closurep 1) (closurep 'x) (closurep '(lambda (x) x)))",
        expect,
    );
}

#[test]
fn oracle_prop_make_interpreted_closure_basic_callable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((f (make-interpreted-closure '(x) '((+ x 1)) nil))) (list (closurep f) (funcall f 41)))";
    let expect = expect_test::expect![[r#""OK (t 42)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(t 42)", &oracle, &neovm);
}

#[test]
fn oracle_interpreted_closure_is_not_a_list_cell() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU eval.c creates interpreted functions as PVEC_CLOSURE objects.
    // data.c therefore treats them as non-cons values: safe accessors return
    // nil and strict accessors signal.  A quoted lambda remains an ordinary
    // cons, which guards against "fixing" this by rejecting lambda syntax too.
    let form = r#"
(let ((closure
       (make-interpreted-closure nil '((quote ok)) nil))
      (quoted-lambda '(lambda () (quote ok))))
  (list
   (closurep closure)
   (listp closure)
   (consp closure)
   (car-safe closure)
   (cdr-safe closure)
   (condition-case error-data
       (car closure)
     (error (car error-data)))
   (condition-case error-data
       (cdr closure)
     (error (car error-data)))
   (condition-case error-data
       (nthcdr 1 closure)
     (error (car error-data)))
   (car-safe quoted-lambda)
   (consp (cdr-safe quoted-lambda))
   (eq (nthcdr 1 quoted-lambda)
       (cdr quoted-lambda))))
"#;
    let expect = expect_test::expect![[
        r#""OK (t nil nil nil nil wrong-type-argument wrong-type-argument wrong-type-argument lambda t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_make_interpreted_closure_lexenv_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 9""#]];
    // LEXENV argument should provide a lexical binding for `x`.
    crate::common::assert_oracle_parity_expect(
        "(let ((f (make-interpreted-closure '() '(x) '((x . 9))))) (funcall f))",
        expect,
    );
}

#[test]
fn oracle_prop_make_closure_invalid_argument_shape_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (make-closure nil) (error (car err)))",
        expect,
    );
    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    crate::common::assert_oracle_parity_expect(
        "(condition-case err (make-closure 1 2 3) (error (car err)))",
        expect,
    );
}

#[test]
fn oracle_prop_oclosure_macros_presence_matches_oracle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list (fboundp 'oclosure-define) (fboundp 'oclosure-lambda))",
        expect,
    );
}

#[test]
fn oracle_prop_oclosure_define_creates_callable_type_and_accessor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
      (oclosure-define neovm-oracle-oclosure-define-test slot)
      (list
        (fboundp 'neovm-oracle-oclosure-define-test--slot)
        (fboundp 'neovm-oracle-oclosure-define-test--internal-p)
        (condition-case nil
            (not (null (cl--find-class 'neovm-oracle-oclosure-define-test)))
          (error nil))))";
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_oclosure_macroexpand_when_available() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In this minimal harness these may be unavailable; when available,
    // macroexpand should still match between oracle and neovm.
    let form = "(if (and (fboundp 'oclosure-lambda) (fboundp 'macroexpand)) (macroexpand '(oclosure-lambda neovm--oc-test (self) self)) 'oclosure-unavailable)";
    let expect = expect_test::expect![[r#""ERR (error \"Unknown class: nil\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_make_interpreted_closure_arithmetic_payload(
        base in -100_000i64..100_000i64,
        arg in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((f (make-interpreted-closure '(x) '((+ x {})) nil))) (funcall f {}))",
            base, arg
        );
        let expected = (base + arg).to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
