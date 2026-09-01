//! Divergence tests: advice-deep, nadvice, and function recomposition edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_advice_add_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (12 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-adv-fn (x) (* x 2))
  (advice-add 'my-adv-fn :around
              (lambda (fn &rest args)
                (apply fn (mapcar #'1+ args))))
  (let ((result (my-adv-fn 5)))
    (advice-remove 'my-adv-fn
                   (lambda (fn &rest args)
                     (apply fn (mapcar #'1+ args))))
    (list result (my-adv-fn 5))))"#,
        expect,
    );
}

#[test]
fn divergence_advice_before_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((after 5) (before 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-adv-log nil)
  (defun my-adv-target (x) (+ x 10))
  (advice-add 'my-adv-target :before
              (lambda (x) (push (list 'before x) my-adv-log)))
  (advice-add 'my-adv-target :after
              (lambda (x) (push (list 'after x) my-adv-log)))
  (my-adv-target 5)
  (let ((result my-adv-log))
    (advice-remove 'my-adv-target
                   (lambda (x) (push (list 'before x) my-adv-log)))
    (advice-remove 'my-adv-target
                   (lambda (x) (push (list 'after x) my-adv-log)))
    result))"#,
        expect,
    );
}

#[test]
fn divergence_advice_filter_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-filter-fn (x) x)
  (advice-add 'my-filter-fn :filter-args
              (lambda (args) (list (1+ (car args)))))
  (let ((result (my-filter-fn 9)))
    (advice-remove 'my-filter-fn
                   (lambda (args) (list (1+ (car args)))))
    result))"#,
        expect,
    );
}

#[test]
fn divergence_advice_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (overridden original)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-override-fn () 'original)
  (advice-add 'my-override-fn :override (lambda () 'overridden))
  (let ((result (my-override-fn)))
    (advice-remove 'my-override-fn (lambda () 'overridden))
    (list result (my-override-fn))))"#,
        expect,
    );
}

#[test]
fn divergence_nadvice_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'nadvice--advice-declarity)
  (fboundp 'advice--p)
  (fboundp 'advice--cd*r))"#,
        expect,
    );
}

#[test]
fn divergence_function_advice_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-advp-test () t)
  (list (advice--p (symbol-function 'my-advp-test))
        (advice--p (symbol-function 'car))))"#,
        expect,
    );
}

#[test]
fn divergence_multiple_advice_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (original a b)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-multi-adv-result nil)
  (defun my-multi-adv-target () (push 'original my-multi-adv-result))
  (advice-add 'my-multi-adv-target :before (lambda () (push 'a my-multi-adv-result)))
  (advice-add 'my-multi-adv-target :before (lambda () (push 'b my-multi-adv-result)))
  (my-multi-adv-target)
  my-multi-adv-result)"#,
        expect,
    );
}

#[test]
fn divergence_advice_interactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t (interactive nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-int-adv-fn () (interactive) nil)
  (advice-add 'my-int-adv-fn :after (lambda ()))
  (list (commandp 'my-int-adv-fn)
        (interactive-form 'my-int-adv-fn)))"#,
        expect,
    );
}
