//! Divergence tests: subrp, bytecode, compiled-function deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_subrp_builtin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (subrp (symbol-function 'car))
  (subrp (symbol-function 'cons))
  (subrp (symbol-function 'list))
  (subrp (symbol-function '+))
  (subrp (symbol-function 'lambda))
  (subrp 'not-a-function))"#,
        expect,
    );
}

#[test]
fn divergence_compiled_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((fn (symbol-function 'car)))
  (list (compiled-function-p fn)
        (subrp fn)
        (or (subrp fn) (compiled-function-p fn))))"#,
        expect,
    );
}

#[test]
fn divergence_byte_code_function_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (byte-code-function-p (symbol-function 'car))
  (byte-code-function-p (symbol-function 'list))
  (byte-code-function-p (lambda (x) x)))"#,
        expect,
    );
}

#[test]
fn divergence_function_type_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (subr interpreted-function subr)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (type-of (symbol-function 'car))
  (type-of (lambda (x) x))
  (type-of (symbol-function 'list)))"#,
        expect,
    );
}

#[test]
fn divergence_interactive_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t (interactive \"nNum: \"))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((fn (lambda (x) (interactive "nNum: ") x)))
  (list (functionp fn)
        (commandp fn)
        (interactive-form fn)))"#,
        expect,
    );
}

#[test]
fn divergence_closure_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (interpreted-function t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x 42)
        (fn (lambda () x)))
  (list (type-of fn)
        (functionp fn)
        (closurep fn)))"#,
        expect,
    );
}

#[test]
fn divergence_function_documentation_builtin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t \"Return a newly created list with specified arguments as elements.\\nAllows any number of arguments, including zero.\\n\\n(fn &rest OBJECTS)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (stringp (documentation 'car))
  (stringp (documentation 'cons))
  (documentation 'list))"#,
        expect,
    );
}

#[test]
fn divergence_advice_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'advice-info)
  (fboundp 'advice-map-tree)
  (fboundp 'advice--tweak))"#,
        expect,
    );
}

#[test]
fn divergence_func_arity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 . 1) (0 . many) (0 . many) (1 . many))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (func-arity 'car)
  (func-arity 'list)
  (func-arity '+)
  (func-arity 'format))"#,
        expect,
    );
}

#[test]
fn divergence_help_function_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'help-function-arglist)
  (fboundp 'help-split-fundoc)
  (fboundp 'describe-function))"#,
        expect,
    );
}
