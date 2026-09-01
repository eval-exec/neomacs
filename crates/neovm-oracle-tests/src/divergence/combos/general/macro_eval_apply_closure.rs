//! Divergence tests: macro expansion + eval + apply + funcall + closure.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_macro_expansion_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((5 -5 5 0) nil 6 t (a b c) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-men-xxx (op &rest args)
    `(,op ,@args))
  (defmacro test-men-chain-xxx (x &rest ops)
    `(list ,@(mapcar (lambda (op) `(,op ,x)) ops)))
  (let ((result (eval (macroexpand
                        '(test-men-chain-xxx 5 + - * /)))))
    (list result
          (equal result '(5 5 5 5))
          (eval (macroexpand '(test-men-xxx + 1 2 3)))
          (= (eval (macroexpand '(test-men-xxx + 1 2 3))) 6)
          (eval (macroexpand '(test-men-xxx list 'a 'b 'c)))
          (equal (eval (macroexpand '(test-men-xxx list 'a 'b 'c)))
                 '(a b c))))) "#,
        expect,
    );
}

#[test]
fn divergence_apply_funcall_partial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 t 6 t 20 t 20 t 60 t 21 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((add (lambda (a b c) (+ a b c)))
        (mul (lambda (a b) (* a b))))
    (list (funcall add 1 2 3)
          (= (funcall add 1 2 3) 6)
          (apply add '(1 2 3))
          (= (apply add '(1 2 3)) 6)
          (funcall mul 4 5)
          (= (funcall mul 4 5) 20)
          (apply mul 4 '(5))
          (= (apply mul 4 '(5)) 20)
          (funcall (apply-partially add 10) 20 30)
          (= (funcall (apply-partially add 10) 20 30) 60)
          (funcall (apply-partially mul 3) 7)
          (= (funcall (apply-partially mul 3) 7) 21)))) "#,
        expect,
    );
}

#[test]
fn divergence_closure_over_mutable_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((state '(nil)))
    (let ((push-fn (lambda (val)
                     (setcar state (cons val (car state)))))
          (pop-fn (lambda ()
                    (let ((val (car (car state))))
                      (setcar state (cdr (car state)))
                      val)))
          (peek-fn (lambda () (car (car state)))))
      (funcall push-fn 'a)
      (funcall push-fn 'b)
      (funcall push-fn 'c)
      (list (funcall peek-fn)
            (eq (funcall peek-fn) 'c)
            (funcall pop-fn)
            (eq (funcall pop-fn) 'b)
            (funcall peek-fn)
            (eq (funcall peek-fn) 'a)
            (funcall pop-fn)
            (eq (funcall pop-fn) 'a)
            (null (funcall peek-fn))))) "#,
        expect,
    );
}

#[test]
fn divergence_nested_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((outer 1))
    (let ((mid (let ((inner 2))
                 (list (lambda () (+ outer inner))
                       (lambda (x) (+ outer inner x))))))
      (let ((f1 (car mid))
            (f2 (cadr mid)))
        (setq outer 10)
        (list (funcall f1)
              (= (funcall f1) 3)
              (funcall f2 5)
              (= (funcall f2 5) 8)
              (setq outer 100)
              (funcall f1)
              (= (funcall f1) 3)
              (funcall f2 5)
              (= (funcall f2 5) 8))))) "#,
        expect,
    );
}

#[test]
fn divergence_defmacro_with_gensym() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-dwg-swap-xxx (a b)
    (let ((tmp (make-symbol "--tmp--")))
      `(let ((,tmp ,a))
         (setq ,a ,b)
         (setq ,b ,tmp))))
  (let ((x 10) (y 20))
    (eval (macroexpand '(test-dwg-swap-xxx x y)))
    (list x y
          (= x 20)
          (= y 10)
          (eval (macroexpand '(test-dwg-swap-xxx x y)))
          (= x 10)
          (= y 20)))) "#,
        expect,
    );
}

#[test]
fn divergence_eval_defun_and_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((12 7 -1) t (30 11 -1) t (56 15 -1) t t (lambda (x y) (list (* x y) (+ x y) (- x y))) nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (eval '(defun test-edf-xxx (x y)
           (list (* x y) (+ x y) (- x y))))
  (list (test-edf-xxx 3 4)
        (equal (test-edf-xxx 3 4) '(12 7 -1))
        (funcall 'test-edf-xxx 5 6)
        (equal (funcall 'test-edf-xxx 5 6) '(30 11 -1))
        (apply 'test-edf-xxx '(7 8))
        (equal (apply 'test-edf-xxx '(7 8)) '(56 15 -1))
        (functionp 'test-edf-xxx)
        (symbol-function 'test-edf-xxx)
        (byte-code-function-p (symbol-function 'test-edf-xxx))
        (not (byte-code-function-p (symbol-function 'test-edf-xxx))))) "#,
        expect,
    );
}

#[test]
fn divergence_closure_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((compose (lambda (f g)
                   (lambda (x) (funcall f (funcall g x)))))
        (add1 (lambda (x) (+ x 1)))
        (mul2 (lambda (x) (* x 2)))
        (sq (lambda (x) (* x x))))
    (let ((add1-mul2 (funcall compose mul2 add1))
          (mul2-add1 (funcall compose add1 mul2))
          (sq-add1 (funcall compose add1 sq)))
      (list (funcall add1-mul2 3)
            (= (funcall add1-mul2 3) 8)
            (funcall mul2-add1 3)
            (= (funcall mul2-add1 3) 7)
            (funcall sq-add1 4)
            (= (funcall sq-add1 4) 17)
            (funcall (funcall compose add1 add1) 5)
            (= (funcall (funcall compose add1 add1) 5) 7)))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_anaphoric() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 76)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-ma-when-xxx (test &rest body)
    `(let ((it ,test))
       (when it ,@body)))
  (list (eval (macroexpand '(test-ma-when-xxx (+ 1 2) (+ it 10))))
        (= (eval (macroexpand '(test-ma-when-xxx (+ 1 2) (+ it 10)))) 13)
        (eval (macroexpand '(test-ma-when-xxx nil (+ it 10))))
        (null (eval (macroexpand '(test-ma-when-xxx nil (+ it 10)))))
        (eval (macroexpand '(test-ma-when-xxx "hello" (length it))))
        (= (eval (macroexpand '(test-ma-when-xxx "hello" (length it)))) 5)
        (eval (macroexpand '(test-ma-when-xxx '(1 2 3) (car it))))
        (eq (eval (macroexpand '(test-ma-when-xxx '(1 2 3) (car it)))) 1)))) "#,
        expect,
    );
}

#[test]
fn divergence_recursive_lambda_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 t 1 t 120 t 3628800 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (letrec ((factorial
            (lambda (n)
              (if (<= n 1) 1
                (* n (funcall factorial (- n 1)))))))
    (list (funcall factorial 0)
          (= (funcall factorial 0) 1)
          (funcall factorial 1)
          (= (funcall factorial 1) 1)
          (funcall factorial 5)
          (= (funcall factorial 5) 120)
          (funcall factorial 10)
          (= (funcall factorial 10) 3628800)))) "#,
        expect,
    );
}

#[test]
fn divergence_eval_with_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((caught arith-error nil) t nil nil 3 t (type-error listp) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (eval '(condition-case err
                  (/ 10 0)
                (arith-error (list 'caught (car err) (cdr err)))))
        (equal (eval '(condition-case err
                         (/ 10 0)
                       (arith-error (list 'caught (car err)))))
               '(caught arith-error))
        (eval '(condition-case err
                  (car nil)
                (error 'caught-general)))
        (eq (eval '(condition-case err
                      (car nil)
                    (error 'caught-general)))
            'caught-general)
        (eval '(condition-case err
                  (+ 1 2)
                (error 'not-reached)))
        (= (eval '(condition-case err
                      (+ 1 2)
                    (error 'not-reached)))
           3)
        (eval '(condition-case err
                  (signal 'wrong-type-argument '(listp 5))
                (wrong-type-argument (list 'type-error (cadr err)))))
        (equal (eval '(condition-case err
                         (signal 'wrong-type-argument '(listp 5))
                       (wrong-type-argument (list 'type-error (cadr err)))))
               '(type-error 5)))) "#,
        expect,
    );
}
