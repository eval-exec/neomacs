//! Oracle parity tests for GNU `func-arity` semantics.
//!
//! GNU implements `func-arity` in `src/eval.c`: it follows symbol function
//! indirection, unwraps `(macro . FUNCTION)`, delegates subrs to `subr-arity`,
//! parses lambda arglists directly, and signals before trying to load direct
//! autoload conses whose FUNNAME would not be a symbol.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_func_arity_lambda_and_macro_arglist_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (func-arity '(lambda (a b) nil))
 (func-arity '(lambda (a &optional b c) nil))
 (func-arity '(lambda (&optional a &rest b c) nil))
 (func-arity '(macro lambda (a &optional b) nil))
 (condition-case err
     (func-arity '(lambda . bogus))
   (error (list (car err) (cdr err))))
 (condition-case err
     (func-arity '(lambda (a . b) nil))
   (error (list (car err) (cdr err))))
 (condition-case err
     (func-arity '(macro bogus))
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((2 . 2) (1 . 3) (0 . many) (1 . 2) (invalid-function ((lambda . bogus))) (invalid-function ((lambda (a . b) nil))) (invalid-function ((macro bogus))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_func_arity_symbol_indirection_and_subr_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (progn
      (fset 'neomacs--oracle-fa-subr 'car)
      (fset 'neomacs--oracle-fa-lambda
            '(lambda (a &optional b &rest c) nil))
      (fset 'neomacs--oracle-fa-alias1 'neomacs--oracle-fa-alias2)
      (fset 'neomacs--oracle-fa-alias2
            '(lambda (a b &optional c) nil))
      (list
       (func-arity 'neomacs--oracle-fa-subr)
       (func-arity (symbol-function 'neomacs--oracle-fa-subr))
       (func-arity 'neomacs--oracle-fa-lambda)
       (func-arity 'neomacs--oracle-fa-alias1)
       (func-arity 'neomacs--oracle-fa-alias2)
       (func-arity 'if)))
  (dolist (sym '(neomacs--oracle-fa-subr
                 neomacs--oracle-fa-lambda
                 neomacs--oracle-fa-alias1
                 neomacs--oracle-fa-alias2))
    (when (fboundp sym)
      (fmakunbound sym))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((1 . 1) (1 . 1) (1 . many) (2 . 3) (2 . 3) (2 . unevalled))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_func_arity_error_ordering_for_non_functions_and_autoloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (func-arity nil)
   (error (list (car err) (cdr err))))
 (condition-case err
     (func-arity 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (func-arity "not-a-function")
   (error (list (car err) (cdr err))))
 (condition-case err
     (func-arity '(autoload "missing-func-arity-file" nil nil nil))
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((void-function (nil)) (invalid-function (42)) (invalid-function (\"not-a-function\")) (wrong-type-argument (symbolp (autoload \"missing-func-arity-file\" nil nil nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
