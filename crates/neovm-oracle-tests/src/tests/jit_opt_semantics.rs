//! GNU-observable parity of the passes-off opt tier. Expectations are refreshed
//! only from the attested GNU executable. Warmups also exercise T1->T2 and OSR
//! under the opt/T2/stress oracle configuration.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_opt_heap_mutation_and_gc_roots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((f (byte-compile (lambda (xs n)
                    (while (> n 0)
                      (setcar xs (1+ (car xs)))
                      (setcdr xs (cons (car xs) (cdr xs)))
                      (setq n (1- n)))
                    (list (car xs) (length xs)))))
                   (x (list 0)))
              (dotimes (_ 200) (funcall f x 2))
              (garbage-collect)
              (funcall f x 3))"#;
    let expect = expect_test::expect![[r#""OK (403 404)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_opt_optional_rest_and_closure_instances() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((make (byte-compile (lambda (k)
                         (lambda (x &optional y &rest r)
                           (list (+ k x (or y 0)) r)))))
                   (a (funcall make 7)) (b (funcall make 100)))
              (dotimes (_ 200) (funcall a 1) (funcall b 2 3 4 5))
              (list (funcall a 8) (funcall b 8 9 10 11)
                    (eq (cadr (funcall a 0 nil 1)) (cadr (funcall a 0 nil 1)))))"#;
    let expect = expect_test::expect![[r#""OK ((15 nil) (117 (10 11)) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_opt_switch_float_bits_and_result_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((f (byte-compile (lambda (x)
                         (pcase x ('a -0.0) ('b 1.5) ('c 2.0) ('d 4.0) (_ 0.0)))))
                  (g (byte-compile (lambda (x) (let ((y (* x 2.0))) (list (eq y y) (eq y (* x 2.0)) (- y)))))))
              (dotimes (_ 300) (funcall f 'b) (funcall g 1.5))
              (list (mapcar f '(a b c d other)) (funcall g 0.0) (funcall g 1.5)))"#;
    let expect =
        expect_test::expect![[r#""OK ((-0.0 1.5 2.0 4.0 0.0) (t nil -0.0) (t nil -3.0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_opt_redefinition_and_signalling_call_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
              (fset 'opt-oracle-callee (byte-compile (lambda (x) (1+ x))))
              (fset 'opt-oracle-caller (byte-compile (lambda (x) (+ 1 (opt-oracle-callee x)))))
              (dotimes (_ 200) (opt-oracle-caller 2))
              (let ((first (opt-oracle-caller 2)))
                (fset 'opt-oracle-callee (byte-compile (lambda (x) (+ x 100))))
                (list first (opt-oracle-caller 2)
                  (condition-case err (opt-oracle-caller 'bad) (error (car err))))))"#;
    let expect = expect_test::expect![[r#""OK (4 103 wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_opt_dynamic_bindings_and_watchers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
              (defvar opt-oracle-v 1)
              (setq opt-oracle-v 1)
              (let ((events nil))
                (add-variable-watcher 'opt-oracle-v
                  (lambda (_sym new op _where) (push (list new op) events)))
                (unwind-protect
                  (let ((f (byte-compile (lambda (n)
                             (let ((opt-oracle-v n))
                               (setq opt-oracle-v (1+ opt-oracle-v)) opt-oracle-v)))))
                    (dotimes (_ 200) (funcall f 4))
                    (setq events nil)
                    (list (funcall f 9) opt-oracle-v (nreverse events)))
                  (remove-variable-watcher 'opt-oracle-v
                    (car (get-variable-watchers 'opt-oracle-v))))))"#;
    let expect = expect_test::expect![[r#""OK (10 1 ((9 let) (10 set) (1 unlet)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_opt_native_call_backtrace_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
      (fset 'opt-oracle-frame-callee
        (byte-compile (lambda (x) (backtrace-frames))))
      (fset 'opt-oracle-frame-caller
        (byte-compile (lambda (x) (opt-oracle-frame-callee x))))
      (dotimes (_ 200) (opt-oracle-frame-caller 7))
      (let ((frames (opt-oracle-frame-caller 23)) (out nil))
        (dolist (frame frames)
          (when (memq (nth 1 frame) '(opt-oracle-frame-callee opt-oracle-frame-caller))
            (push (list (nth 1 frame) (nth 2 frame)) out)))
        (nreverse out)))"#;
    let expect = expect_test::expect![[
        r#""OK ((opt-oracle-frame-callee (23)) (opt-oracle-frame-caller (23)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
