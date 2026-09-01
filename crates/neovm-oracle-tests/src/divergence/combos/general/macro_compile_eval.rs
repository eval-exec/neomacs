//! Divergence tests: macro + compile-time + eval + defmacro combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_defmacro_gentemp_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 10 t 20 30 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-dm-xxx (var expr)
    (let ((tmp (make-symbol "tmp")))
      (list 'let (list (list tmp expr))
            (list 'setq var (list '+ var tmp))
            tmp)))
  (let ((counter 0))
    (list (test-dm-xxx counter 10)
          counter
          (= counter 10)
          (test-dm-xxx counter 20)
          counter
          (= counter 30)))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_nested_backquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 99 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-nbq-xxx (name val)
    (list 'defvar (intern (format "test-nbq-%s-xxx" name)) val))
  (test-nbq-xxx alpha 42)
  (test-nbq-xxx beta 99)
  (list test-nbq-alpha-xxx
        test-nbq-beta-xxx
        (= test-nbq-alpha-xxx 42)
        (= test-nbq-beta-xxx 99)
        (boundp 'test-nbq-alpha-xxx)
        (boundp 'test-nbq-beta-xxx))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_expands_to_defun_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 50 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-defn-xxx (name args &rest body)
    (list 'defun name args
          (list 'interactive)
          (cons 'progn body)))
  (test-defn-xxx test-mfn-xxx (n)
    (+ n 42))
  (list (commandp 'test-mfn-xxx)
        (fboundp 'test-mfn-xxx)
        (funcall 'test-mfn-xxx 8)
        (= (funcall 'test-mfn-xxx 8) 50))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_with_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (invalid-function (closure (t) ((a b c) &rest body) (list 'let (list (list 'x (list '+ a b c))) (cons 'progn body) 'x)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-des-xxx ((a b c) &rest body)
    (list 'let (list (list 'x (list '+ a b c)))
          (cons 'progn body)
          'x))
  (list (test-des-xxx (1 2 3) x)
        (= (test-des-xxx (10 20 30) x) 60)
        (test-des-xxx (5 5 5) (* x 2))
        (= (test-des-xxx (5 5 5) (* x 2)) 15))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_recursive_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 t 6 t 100 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-sum-xxx (&rest args)
    (if (null args) 0
      (list '+ (car args)
            (cons 'test-sum-xxx (cdr args)))))
  (list (test-sum-xxx)
        (= (test-sum-xxx) 0)
        (test-sum-xxx 1 2 3)
        (= (test-sum-xxx 1 2 3) 6)
        (test-sum-xxx 10 20 30 40)
        (= (test-sum-xxx 10 20 30 40) 100))) "#,
        expect,
    );
}

#[test]
fn divergence_eval_after_macro_def() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (49 t 121 t (* 5 5) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-ea-xxx (x) (list '* x x))
  (let ((result (eval '(test-ea-xxx 7))))
    (list result (= result 49)
          (eval '(test-ea-xxx 11))
          (= (eval '(test-ea-xxx 11)) 121)
          (macroexpand '(test-ea-xxx 5))
          (equal (macroexpand '(test-ea-xxx 5)) '(* 5 5))))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_winds_binding_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 t 2 2 t 4 4 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-wbs-xxx 0)
  (defmacro test-inc-xxx ()
    (cl-incf test-wbs-xxx)
    (list 'setq 'test-wbs-xxx (list '+ 'test-wbs-xxx 1)))
  (list test-wbs-xxx
        (= test-wbs-xxx 0)
        (test-inc-xxx)
        test-wbs-xxx
        (= test-wbs-xxx 2)
        (test-inc-xxx)
        test-wbs-xxx
        (= test-wbs-xxx 4))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_error_in_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-oddp)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-err-xxx (x)
    (if (cl-oddp x)
        (list '* x x)
      (error "expected odd number, got %d" x)))
  (list (test-err-xxx 3)
        (= (test-err-xxx 3) 9)
        (condition-case e
            (test-err-xxx 4)
          (error (cons (car e) (cdr e)))))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_symbol_macro_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function symbol-macrolet)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-sym-xxx (expr)
    (list 'symbol-macrolet (list (list 'it expr))
          '(list it (* it it) (+ it 1))))
  (list (test-sym-xxx 7)
        (equal (test-sym-xxx 7) '(7 49 8))
        (test-sym-xxx 10)
        (equal (test-sym-xxx 10) '(10 100 11)))) "#,
        expect,
    );
}

#[test]
fn divergence_inline_function_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (define-inline test-inl-xxx (x)
    (inline-letevals (x)
      (list 'progn
            (list 'setq 'test-inl-val-xxx x)
            'test-inl-val-xxx)))
  (defvar test-inl-val-xxx nil)
  (list (test-inl-xxx 42)
        (= test-inl-val-xxx 42)
        (test-inl-xxx 99)
        (= test-inl-val-xxx 99)
        (= (test-inl-xxx 5) 5)
        (= test-inl-val-xxx 5))) "#,
        expect,
    );
}
