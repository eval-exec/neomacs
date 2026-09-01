//! Divergence tests: real macro & advice behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_macroexpand_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defmacro test-when-xxx (cond &rest body)
    \\`(if ,cond (progn ,@body)))
  (list (macroexpand '(test-when-xxx (> 5 3) 'yes 'ok))
        (macroexpand-all '(test-when-xxx (> 5 3) 'yes 'ok))
        (macroexpand '(test-when-xxx nil 'no)))) ",
        expect,
    );
}

#[test]
fn divergence_nested_macro_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defmacro test-add1-xxx (x) \\`(+ 1 ,x))
  (defmacro test-add2-xxx (x) \\`(+ (test-add1-xxx ,x) 1))
  (list (macroexpand '(test-add2-xxx 5))
        (macroexpand-all '(test-add2-xxx 5)))) ",
        expect,
    );
}

#[test]
fn divergence_advice_add_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 10 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-advice-fn-xxx (x) (* x 2))
  (advice-add 'test-advice-fn-xxx :filter-return
               (lambda (r) (+ r 10)))
  (let ((r1 (test-advice-fn-xxx 5)))
    (advice-remove 'test-advice-fn-xxx
                    (lambda (r) (+ r 10)))
    (let ((r2 (test-advice-fn-xxx 5)))
      (list r1 r2
            (advice-member-p (lambda (r) (+ r 10))
                             'test-advice-fn-xxx))))) ",
        expect,
    );
}

#[test]
fn divergence_advice_before_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (21 (\"before:7\" \"fn:7\" \"after:7\") #[128 \"��\u{2}\\\"��\u{3}\\\"��\" [#[(x) ((push (format \"after:%d\" x) test-advice-log-xxx)) (t)] #[128 \"��\u{2}\\\"���\u{2}\\\"�\" [#[(x) ((push (format \"before:%d\" x) test-advice-log-xxx)) (t)] #[(x) ((push (format \"fn:%d\" x) test-advice-log-xxx) (* x 3)) (t)] :before nil apply] 4 advice] :after nil apply] 5 advice])""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-advice-log-xxx nil)
  (defun test-advice-fn2-xxx (x)
    (push (format \"fn:%d\" x) test-advice-log-xxx)
    (* x 3))
  (advice-add 'test-advice-fn2-xxx :before
               (lambda (x)
                 (push (format \"before:%d\" x) test-advice-log-xxx)))
  (advice-add 'test-advice-fn2-xxx :after
               (lambda (x)
                 (push (format \"after:%d\" x) test-advice-log-xxx)))
  (let ((result (test-advice-fn2-xxx 7)))
    (list result
          (nreverse test-advice-log-xxx)
          (advice-member-p nil 'test-advice-fn2-xxx)))) ",
        expect,
    );
}

#[test]
fn divergence_defsubst_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (105 97 (closure (t) (x) (+ x 100)) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defsubst test-subst-xxx (x) (+ x 100))
  (list (test-subst-xxx 5)
        (test-subst-xxx -3)
        (symbol-function 'test-subst-xxx)
        (subrp (symbol-function 'test-subst-xxx)))) ",
        expect,
    );
}

#[test]
fn deficiency_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((closures nil))
  (dolist (i '(1 2 3))
    (push (let ((n i)) (lambda () n)) closures))
  (list (mapcar #'funcall (nreverse closures))
        (length closures))) ",
        expect,
    );
}

#[test]
fn divergence_inline_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function inline-leteval)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (define-inline test-inline-xxx (x)
    (inline-leteval (x)
      (inline-quote (+ ,x 1))))
  (list (test-inline-xxx 5)
        (test-inline-xxx 0)
        (test-inline-xxx -1))) ",
        expect,
    );
}

#[test]
fn divergence_compiled_function_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (compiled-function-p (lambda (x) x))
  (compiled-function-p #'car)
  (compiled-function-p 'car)
  (compiled-function-p nil)
  (byte-code-function-p #'car)) ",
        expect,
    );
}

#[test]
fn divergence_gv_generalized() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-decf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((lst '(1 2 3 4 5)))
  (cl-incf (nth 2 lst))
  (list lst
        (nth 2 lst)
        (cl-decf (nth 0 lst))
        lst)) ",
        expect,
    );
}

#[test]
fn divergence_setf_on_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 20 (x 99 y 20) 99 99)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((sym (make-symbol \"test\")))
  (setf (get sym 'x) 10)
  (setf (get sym 'y) 20)
  (list (get sym 'x)
        (get sym 'y)
        (symbol-plist sym)
        (setf (get sym 'x) 99)
        (get sym 'x))) ",
        expect,
    );
}
