//! Divergence tests: macro + compile-time + runtime interaction combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_macro_generates_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 10 51)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-safe-div-xxx (a b)
    (let ((err-sym (make-symbol "err")))
      `(condition-case ,err-sym
           (/ ,a ,b)
         (arith-error 'div-error))))
  (list (test-safe-div-xxx 10 3)
        (test-safe-div-xxx 10 0)
        (= (test-safe-div-xxx 10 5) 2)
        (eq (test-safe-div-xxx 10 0) 'div-error)))) "#,
        expect,
    );
}

#[test]
fn divergence_eval_when_compile_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5) t (3 4 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-ewc-xxx nil)
  (eval-and-compile
    (setq test-ewc-xxx '(1 2 3)))
  (eval-when-compile
    (setq test-ewc-xxx (append test-ewc-xxx '(4 5))))
  (list test-ewc-xxx
        (>= (length test-ewc-xxx) 3)
        (member 3 test-ewc-xxx))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_inspects_env() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (interpreted 'no)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-if-compiled-xxx (then &rest else)
    (if (byte-code-function-p (symbol-function 'car))
        (macroexp-progn (cons then else))
      (macroexp-progn else)))
  (list (test-if-compiled-xxx 'compiled 'interpreted)
        (macroexpand '(test-if-compiled-xxx 'yes 'no)))) "#,
        expect,
    );
}

#[test]
fn divergence_nested_macro_expansion_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((list 'wrapped (test-wrap-xxx 42)) (list 'wrapped (list 'wrapped 42)) (wrapped (wrapped 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-wrap-xxx (expr) `(list 'wrapped ,expr))
  (defmacro test-double-xxx (expr) `(test-wrap-xxx (test-wrap-xxx ,expr)))
  (list (macroexpand '(test-double-xxx 42))
        (macroexpand-all '(test-double-xxx 42))
        (test-double-xxx 42))) "#,
        expect,
    );
}

#[test]
fn divergence_compiler_macro_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function inline-leteval)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (define-inline test-fast-square-xxx (x)
    (inline-leteval (x)
      (inline-quote (* ,x ,x))))
  (defun test-use-fast-xxx (n)
    (+ (test-fast-square-xxx n) (test-fast-square-xxx (1+ n))))
  (list (test-use-fast-xxx 3)
        (= (test-use-fast-xxx 3) (+ 9 16))
        (test-use-fast-xxx 0)
        (= (test-use-fast-xxx 0) 1))) "#,
        expect,
    );
}

#[test]
fn divergence_defmacro_with_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 9 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-let1-xxx (binding &rest body)
    (pcase binding
      (`(,var ,val)
       `(let ((,var ,val)) ,@body))))
  (test-let1-xxx (x (+ 1 2))
    (list x (* x x) (= x 3)))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_with_gensym_preventing_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 10 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-swap-xxx (a b)
    (let ((tmp (make-symbol "tmp")))
      `(let ((,tmp ,a))
         (setq ,a ,b)
         (setq ,b ,tmp))))
  (let ((tmp 10) (other 20))
    (test-swap-xxx tmp other)
    (list tmp other (equal (list tmp other) '(20 10))))) "#,
        expect,
    );
}

#[test]
fn divergence_eval_form_with_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (result nil (if (> 5 3) (progn 'result)))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-when-xxx (cond &rest body)
    `(if ,cond (progn ,@body)))
  (let ((forms '((test-when-xxx t 'yes 'ok)
                  (test-when-xxx nil 'no)
                  (when t 'builtin-when)))))
  (list (eval '(test-when-xxx (> 5 3) 'result))
        (eval '(test-when-xxx nil 'not-this))
        (macroexpand-all '(test-when-xxx (> 5 3) 'result)))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_generates_defun_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 t 14 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-define-ops-xxx (name &rest args)
    (let ((fn-name (intern (format "test-op-%s-xxx" name))))
      `(progn
         (defun ,fn-name (x) (,name x ,@args))
         ',fn-name)))
  (test-define-ops-xxx + 10)
  (test-define-ops-xxx * 2)
  (list (test-op-+-xxx 5)
        (= (test-op-+-xxx 5) 15)
        (test-op-*-xxx 7)
        (= (test-op-*-xxx 7) 14))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_with_backquote_splice_and_unquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 \"hello\" (a b) t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-bind-and-run-xxx (bindings &rest body)
    `(let ,(mapcar (lambda (b) (list (car b) (cadr b))) bindings)
       ,@(mapcar (lambda (b) `(message "bound %s to %S" ',(car b) ,(car b))) bindings)
       ,@body))
  (test-bind-and-run-xxx ((x 42) (y "hello") (z '(a b)))
    (list x y z (string= y "hello") (= x 42)))) "#,
        expect,
    );
}
