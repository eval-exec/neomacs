//! Divergence tests: let-binding, dynamic binding, lexical scope edge.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_let_parallel() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x 1) (y 2))
  (list x y (+ x y))) "#,
        expect,
    );
}

#[test]
fn divergence_let_star() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((x 1) (y (1+ x)) (z (+ x y)))
  (list x y z)) "#,
        expect,
    );
}

#[test]
fn divergence_let_parallel_vs_sequential() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x 10))
  (let ((x 20) (y x))
    (list x y))) "#,
        expect,
    );
}

#[test]
fn divergence_let_star_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x 10))
  (let* ((x 20) (y x))
    (list x y))) "#,
        expect,
    );
}

#[test]
fn divergence_dynamic_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(defvar test-dynamic-var-xxx 10)
(let ((test-dynamic-var-xxx 20))
  (list test-dynamic-var-xxx
        (default-value 'test-dynamic-var-xxx))) "#,
        expect,
    );
}

#[test]
fn divergence_lexical_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((counter 0))
  (let ((inc (lambda () (setq counter (1+ counter)))))
    (list (funcall inc)
          (funcall inc)
          (funcall inc)
          counter))) "#,
        expect,
    );
}

#[test]
fn divergence_closure_over_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((closures nil))
  (dotimes (i 3)
    (push (let ((x i)) (lambda () x)) closures))
  (list (funcall (nth 0 closures))
        (funcall (nth 1 closures))
        (funcall (nth 2 closures)))) "#,
        expect,
    );
}

#[test]
fn divergence_nested_let_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((a 1))
  (let ((b (1+ a)))
    (let ((c (1+ b)))
      (let ((d (1+ c)))
        (list a b c d))))) "#,
        expect,
    );
}

#[test]
fn divergence_unwind_protect_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x 0))
  (ignore-errors
    (setq x 1)
    (error "oops")
    (setq x 2))
  (list x)) "#,
        expect,
    );
}

#[test]
fn divergence_setq_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'setq-default)
  (fboundp 'default-value)
  (fboundp 'set-default)
  (fboundp 'default-boundp)
  (fboundp 'makunbound)) "#,
        expect,
    );
}
