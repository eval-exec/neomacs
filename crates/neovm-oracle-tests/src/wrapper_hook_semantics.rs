//! Oracle parity tests for GNU wrapper-hook semantics.
//!
//! GNU implements `with-wrapper-hook` in `lisp/subr.el` by expanding through
//! `subr--with-wrapper-hook-no-warnings` into a recursive `letrec`.  Wrapper
//! functions receive a continuation and may call it zero, one, or many times;
//! a local hook containing `t` splices in the global hook tail.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_wrapper_hook_macroexpansion_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(macroexpand
 '(subr--with-wrapper-hook-no-warnings neovm--wwh-hook (x y)
    (list x y)))
"#;

    let expect = expect_test::expect![[
        r#""OK (let (runrestofhook) (setq runrestofhook (lambda (funs global args) (if (consp funs) (if (eq t (car funs)) (funcall runrestofhook (append global (cdr funs)) nil args) (apply (car funs) (apply-partially (lambda (funs global &rest args) (funcall runrestofhook funs global args)) (cdr funs) global) args)) (apply (lambda (x y) (list x y)) args)))) (funcall runrestofhook neovm--wwh-hook (if (local-variable-p 'neovm--wwh-hook) (default-value 'neovm--wwh-hook)) (list x y)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_wrapper_hook_order_reentry_and_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil))
  (let ((hook
         (list
          (lambda (fun x)
            (push (list 'a-before x) log)
            (let ((result (funcall fun (1+ x))))
              (push (list 'a-after result) log)
              (+ result 10)))
          (lambda (fun x)
            (push (list 'b-before x) log)
            (let ((left (funcall fun (* x 2)))
                  (right (funcall fun (* x 3))))
              (push (list 'b-after left right) log)
              (+ left right)))))))
    (list
     (subr--with-wrapper-hook-no-warnings hook (x)
       (push (list 'body x) log)
       x)
     (nreverse log)
     (let ((log nil)
           (replace-hook
            (list
             (lambda (_fun x)
               (push (list 'replace x) log)
               'replacement)
             (lambda (fun x)
               (push (list 'never x) log)
               (funcall fun x)))))
       (list
        (subr--with-wrapper-hook-no-warnings replace-hook (x)
          (push (list 'body x) log)
          'body)
        (nreverse log))))))
"#;

    let expect = expect_test::expect![[r#""ERR (void-variable hook)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_wrapper_hook_local_t_splices_global_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neovm--wwh-hook nil)
  (let ((log nil))
    (unwind-protect
        (progn
          (setq neovm--wwh-hook
                (list (lambda (fun x)
                        (push (list 'global-before x) log)
                        (let ((result (funcall fun (+ x 100))))
                          (push (list 'global-after result) log)
                          result))))
          (with-temp-buffer
            (setq-local neovm--wwh-hook
                        (list
                         (lambda (fun x)
                           (push (list 'local-before x) log)
                           (let ((result (funcall fun (1+ x))))
                             (push (list 'local-after result) log)
                             result))
                         t))
            (list
             (subr--with-wrapper-hook-no-warnings neovm--wwh-hook (x)
               (push (list 'body x) log)
               x)
             (nreverse log))))
      (makunbound 'neovm--wwh-hook))))
"#;

    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
