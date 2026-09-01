//! Divergence tests: real custom/hook/feature behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_defcustom_real() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (42 ((funcall #'(closure (t) nil 42))) ((funcall #'(closure (t) nil 42))) integer 42)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defcustom test-custom-var-xxx 42
    \"A test custom variable.\"
    :type 'integer
    :group 'test-group-xxx)
  (list test-custom-var-xxx
        (custom-variable-p 'test-custom-var-xxx)
        (get 'test-custom-var-xxx 'standard-value)
        (get 'test-custom-var-xxx 'custom-type)
        (default-value 'test-custom-var-xxx))) ",
        expect,
    );
}

#[test]
fn divergence_custom_set_and_save() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function customize-get-value)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defcustom test-cs-var-xxx 'default
    \"Test custom.\"
    :type 'symbol)
  (customize-set-variable 'test-cs-var-xxx 'modified)
  (list test-cs-var-xxx
        (customize-get-value 'test-cs-var-xxx)
        (eq test-cs-var-xxx 'modified))) ",
        expect,
    );
}

#[test]
fn divergence_add_remove_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 2)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (setq test-hook-xxx nil)
  (add-hook 'test-hook-xxx (lambda () (push 'a test-hook-xxx-run)))
  (add-hook 'test-hook-xxx (lambda () (push 'b test-hook-xxx-run)))
  (let ((len (length test-hook-xxx)))
    (remove-hook 'test-hook-xxx (lambda () (push 'a test-hook-xxx-run)))
    (list len (length test-hook-xxx)))) ",
        expect,
    );
}

#[test]
fn divergence_hook_depth_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 4) 5)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (setq test-depth-hook-xxx nil)
  (add-hook 'test-depth-hook-xxx (lambda () 'c) nil nil 50)
  (add-hook 'test-depth-hook-xxx (lambda () 'a) nil nil -50)
  (add-hook 'test-depth-hook-xxx (lambda () 'b) nil nil 0)
  (list (length test-depth-hook-xxx)
        (>= (length test-depth-hook-xxx) 3))) ",
        expect,
    );
}

#[test]
fn divergence_run_hooks_real() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-rh-log-xxx nil)
  (setq test-rh-hook-xxx nil)
  (add-hook 'test-rh-hook-xxx (lambda () (push 1 test-rh-log-xxx)))
  (add-hook 'test-rh-hook-xxx (lambda () (push 2 test-rh-log-xxx)))
  (run-hooks 'test-rh-hook-xxx)
  (nreverse test-rh-log-xxx)) ",
        expect,
    );
}

#[test]
fn divergence_provide_require_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t test-feature-a-xxx)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (provide 'test-feature-a-xxx)
  (list (featurep 'test-feature-a-xxx)
        (if (memq 'test-feature-a-xxx features) t nil)
        (require 'test-feature-a-xxx))) ",
        expect,
    );
}

#[test]
fn divergence_autoload_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (autoloadp 'car)
  (autoloadp 'undefined-fn-xxx)
  (functionp 'car)
  (functionp 'undefined-fn-xxx)
  (functionp (lambda (x) x))) ",
        expect,
    );
}

#[test]
fn deficiency_obarray_intern_soft() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((sym (intern-soft \"car\" obarray)))
  (list (symbolp sym)
        (eq sym 'car)
        (null (intern-soft \"nonexistent-sym-xxx\" obarray))
        (eq (intern \"test-sym-xxx\" obarray)
            (intern-soft \"test-sym-xxx\" obarray)))) ",
        expect,
    );
}

#[test]
fn divergence_defvar_real() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 \"Doc string.\" t 10)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-dv-xxx 10 \"Doc string.\")
  (list test-dv-xxx
        (documentation-property 'test-dv-xxx 'variable-documentation)
        (boundp 'test-dv-xxx)
        (default-value 'test-dv-xxx))) ",
        expect,
    );
}

#[test]
fn divergence_buffer_local_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 2)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (setq test-bl-hook-xxx nil)
  (add-hook 'test-bl-hook-xxx (lambda () 'global) nil t)
  (list (listp test-bl-hook-xxx)
        (local-variable-p 'test-bl-hook-xxx)
        (length test-bl-hook-xxx))) ",
        expect,
    );
}
