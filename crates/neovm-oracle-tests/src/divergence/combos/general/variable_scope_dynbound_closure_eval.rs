//! Deep combo: dynamic binding + let scoping + closures + eval + symbol-value.
//! Tests variable binding semantics with closures and dynamic scope.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_let_star_sequential_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (1 11 11)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((x 1)\n\
         (y (+ x 10))\n\
         (z (* y x)))\n\
         (list x y z)))", expect);
}

#[test]
fn deficiency_let_parallel_binding_old_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (2 1)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((x 1) (y 2))\n\
         (let ((x y) (y x))\n\
         (list x y))))", expect);
}

#[test]
fn deficiency_closure_captures_at_creation_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (0 1 2 3 4)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((closures nil))\n\
         (dotimes (i 5)\n\
         (push (lambda () i) closures))\n\
         (mapcar (lambda (f) (funcall f)) (nreverse closures))))", expect);
}

#[test]
fn deficiency_dynbind_with_symbol_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (global global (local local))""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar dyn-test 'global)\n\
         (list dyn-test\n\
         (symbol-value 'dyn-test)\n\
         (let ((dyn-test 'local))\n\
         (list dyn-test (symbol-value 'dyn-test)))))", expect);
}

#[test]
fn deficiency_setq_in_let_body_visible_to_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (30 30)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((x 10))\n\
         (setq x 20)\n\
         (let ((f (lambda () x)))\n\
         (setq x 30)\n\
         (list (funcall f) x))))", expect);
}

#[test]
fn deficiency_nested_let_shadows_correctly() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (inner)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((x 'outer))\n\
         (let ((x 'mid))\n\
         (let ((x 'inner))\n\
         (list x)))))", expect);
}

#[test]
fn deficiency_function_cell_vs_value_cell() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (original redefined nil)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defun my-fn () 'original)\n\
         (let ((result1 (my-fn)))\n\
         (fset 'my-fn (lambda () 'redefined))\n\
         (let ((result2 (my-fn)))\n\
         (fmakunbound 'my-fn)\n\
         (list result1 result2\n\
         (fboundp 'my-fn)))))", expect);
}

#[test]
fn deficiency_buffer_local_let_shadow_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (let-bound let-bound)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar blsc-var 'default)\n\
         (setq blsc-var 'global-set)\n\
         (let ((blsc-var 'let-bound))\n\
         (list blsc-var\n\
         (symbol-value 'blsc-var))))", expect);
}

#[test]
fn deficiency_default_value_with_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK t""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar dvbl-var 42)\n\
         (make-variable-buffer-local 'dvbl-var)\n\
         (let ((buf (generate-new-buffer \"dvb\")))\n\
         (with-current-buffer buf\n\
         (setq dvbl-var 99))\n\
         (list dvbl-var\n\
         (default-value 'dvbl-var)\n\
         (buffer-local-value 'dvbl-var buf)\n\
         (with-current-buffer buf dvbl-var))\n\
         (kill-buffer buf)))", expect);
}

#[test]
fn deficiency_setq_default_affects_new_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK t""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar sdan-var 'init)\n\
         (make-variable-buffer-local 'sdan-var)\n\
         (setq-default sdan-var 'new-default)\n\
         (let ((buf (generate-new-buffer \"sda\")))\n\
         (with-current-buffer buf\n\
         (list sdan-var\n\
         (default-value 'sdan-var)))\n\
         (kill-buffer buf)))", expect);
}
