//! Divergence tests: funcall, apply, mapcar with various arg types.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_funcall_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 (1 2 3) 25 \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (funcall '+ 1 2)
  (funcall 'list 1 2 3)
  (funcall (lambda (x) (* x x)) 5)
  (funcall 'concat "a" "b" "c")) "#,
        expect,
    );
}

#[test]
fn divergence_apply_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 10 nil [1 2 3])""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (apply '+ '(1 2 3))
  (apply '+ 1 2 '(3 4))
  (apply 'list nil)
  (apply 'vector '(1 2 3))) "#,
        expect,
    );
}

#[test]
fn divergence_funcall_composed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 (a b c) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (funcall (apply-partially '+ 1) 2)
  (funcall (apply-partially 'list 'a) 'b 'c)
  (fboundp 'apply-partially)) "#,
        expect,
    );
}

#[test]
fn divergence_higher_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (a (b c) b)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((fns (list #'car #'cdr #'cadr)))
  (list (funcall (nth 0 fns) '(a b c))
        (funcall (nth 1 fns) '(a b c))
        (funcall (nth 2 fns) '(a b c)))) "#,
        expect,
    );
}

#[test]
fn divergence_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 13 15)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((make-adder
       (lambda (n)
         (lambda (x) (+ x n)))))
  (let ((add5 (funcall make-adder 5))
        (add10 (funcall make-adder 10)))
    (list (funcall add5 3)
          (funcall add10 3)
          (funcall add5 10)))) "#,
        expect,
    );
}

#[test]
fn divergence_lambda_varargs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3) (1 2 3) (1 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (funcall (lambda (&rest args) args) 1 2 3)
  (funcall (lambda (x &rest args) (cons x args)) 1 2 3)
  (funcall (lambda (&optional x y) (list x y)) 1 2)) "#,
        expect,
    );
}

#[test]
fn divergence_funcall_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((3 9 27) \"HELLO\" \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (funcall (lambda (x) (list x (* x x) (* x x x))) 3)
  (funcall (lambda (s) (upcase s)) "hello")
  (funcall (lambda (s) (downcase s)) "HELLO")) "#,
        expect,
    );
}

#[test]
fn divergence_recursive_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 120 3628800)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(letrec ((fact (lambda (n)
                       (if (<= n 1) 1 (* n (funcall fact (1- n)))))))
  (list (funcall fact 1)
        (funcall fact 5)
        (funcall fact 10))) "#,
        expect,
    );
}

#[test]
fn divergence_function_quote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t interpreted-function symbol)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (functionp (lambda (x) x))
  (functionp 'car)
  (type-of (lambda (x) x))
  (type-of 'car)) "#,
        expect,
    );
}

#[test]
fn divergence_advice_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 10 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-fn-adv-xxx (x) (* x 2))
  (let ((orig (symbol-function 'test-fn-adv-xxx)))
    (list (funcall orig 5)
          (funcall 'test-fn-adv-xxx 5)
          (eq orig (symbol-function 'test-fn-adv-xxx))))) "#,
        expect,
    );
}
