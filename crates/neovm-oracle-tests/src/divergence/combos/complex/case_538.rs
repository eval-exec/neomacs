/// Batch 538: compiler macros, function declarations, type declarations.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx538_compiler_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function define-compiler-macro)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (define-compiler-macro cx538-cm (&whole form arg)
    (if (numberp arg) (* arg 2) form))
  (cx538-cm 5))
"##,
        expect,
    );
}

#[test]
fn div_cx538_compiler_macro_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function define-compiler-macro)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (define-compiler-macro cx538-cm2 (&whole form a b)
    `(+ (* ,a 2) (* ,b 3)))
  (compiler-macroexpand '(cx538-cm2 5 6)))
"##,
        expect,
    );
}

#[test]
fn div_cx538_declaim_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function declaim)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (declaim (inline cx538-inline-fn))
  (defun cx538-inline-fn (x) (* x 3))
  (cx538-inline-fn 7))
"##,
        expect,
    );
}

#[test]
fn div_cx538_proclaim_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-proclaim)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-proclaim '(type (function (integer) integer) 1+))
  (1+ 5))
"##,
        expect,
    );
}

#[test]
fn div_cx538_declare_ftype() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-declare)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (defun cx538-ft (x) (* x 4))
  (cl-declare (ftype (function (number) number) cx538-ft))
  (cx538-ft 5))
"##,
        expect,
    );
}

#[test]
fn div_cx538_check_declare_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-check-type)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (cl-check-type 42 integer)
  'ok)
"##,
        expect,
    );
}

#[test]
fn div_cx538_check_declare_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (cl-check-type "hello" integer)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx538_assert_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-assert)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(cl-assert (equal 1 1) t "should be true")
"##,
        expect,
    );
}

#[test]
fn div_cx538_assert_fail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (cl-assert nil t "assertion failed")
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx538_multiple_value_bind() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function multiple-value-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(multiple-value-bind (a b c) (values 1 2 3) (+ a b c))
"##,
        expect,
    );
}

#[test]
fn div_cx538_multiple_value_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function multiple-value-call)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(multiple-value-call #'list (values 1 2 3))
"##,
        expect,
    );
}

#[test]
fn div_cx538_multiple_value_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function multiple-value-list)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(multiple-value-list (values 1 2 3))
"##,
        expect,
    );
}

#[test]
fn div_cx538_values_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function multiple-value-list)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(multiple-value-list (values 'a 'b))
"##,
        expect,
    );
}

#[test]
fn div_cx538_nth_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function nth-value)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(nth-value 0 (values 10 20 30))
"##,
        expect,
    );
}

#[test]
fn div_cx538_nth_value_second() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function nth-value)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(nth-value 1 (values 10 20 30))
"##,
        expect,
    );
}
