//! Oracle parity tests for GNU `subr.el` conditional binding macros.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_subr_conditional_binding_macro_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el implements these through internal--build-bindings.  This
    // fixes short-circuit order, symbol-only bindings, ignored `_` bindings,
    // old if-let/when-let single-binding compatibility, and malformed binding
    // error payloads.
    let form = r#"(list
 (let ((trace nil))
   (if-let* ((a (progn (push 'a trace) 1))
             (b (progn (push 'b trace) 2)))
       (list 'then a b (nreverse trace))
     (list 'else (nreverse trace))))
 (let ((trace nil))
   (if-let* ((a (progn (push 'a trace) 1))
             (b (progn (push 'b trace) nil))
             (c (progn (push 'c trace) 3)))
       (list 'then a b c trace)
     (list 'else a b (nreverse trace))))
 (let ((trace nil))
   (and-let* (((progn (push 'test trace) 'value)))
     (list 'body (nreverse trace))))
 (let ((trace nil))
   (and-let* ((_ (progn (push 'ignored trace) 'value)))
     (list 'body (nreverse trace))))
 (let ((x 7))
   (if-let* (x) (list 'symbol x) 'no))
 (and-let* ((x 7)))
 (and-let* ())
 (when-let* ())
 (if-let (x 42) (list 'old-single x) 'no)
 (when-let (x 42) (list 'old-when x))
 (let ((xs '(1 2 nil 3)) (out nil))
   (while-let ((x (pop xs)))
     (push x out))
   (nreverse out))
 (let ((gensym-counter 0))
   (mapcar (lambda (form)
             (condition-case e (macroexpand form)
               (error (list 'error (car e) (cdr e)))))
           '((if-let* ((x 1 2)) x)
             (if-let* ((x 1) (y 2)) (list x y))
             (and-let* ((x 1)))
             (while-let ((x (pop xs))) x)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((then 1 2 (a b)) (else 1 nil (a b)) (body (test)) (body (ignored)) (symbol 7) 7 t nil (old-single 42) (old-when 42) (1 2) ((error error (\"`let' bindings can have only one value-form\" x 1 2)) (let* ((x (and t 1)) (y (and x 2))) (if y (list x y))) (let* ((x (and t 1))) x) (catch 'done0 (while t (if-let* ((x (pop xs))) (progn x) (throw 'done0 nil))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
