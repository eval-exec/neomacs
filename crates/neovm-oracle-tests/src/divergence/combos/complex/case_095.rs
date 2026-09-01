//! Complex combo batch 95 — closure / oclosure / generator / coroutine
//! capture, `make-oclosure` with slots, `generator-yield` semantics, and
//! deferred execution.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx95_oclosure_basic_slots_and_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((lexical-binding t))
      (oclosure-define neo-cx95-counter
        "doc"
        (current :initform 0))
      (let ((c (neo-cx95-counter :current 5)))
        (list (neo-cx95-counter--current c)
              (oclosure-p c)
              (oclosure-type c))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx95_closure_p_lambda_p_and_function_p_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lex (let ((lexical-binding t)) (lambda () :lex)))
      (dyn (let ((lexical-binding nil)) (lambda () :dyn)))
      (named (defalias 'neo-cx95-named (lambda () :named))))
  (list (functionp lex)
        (functionp dyn)
        (functionp named)
        (subrp (symbol-function 'car))
        (byte-code-function-p (symbol-function 'car))
        (closurep (symbol-function 'neo-cx95-named))))
"##,
        expect,
    );
}

#[test]
fn div_cx95_generator_basic_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (cl-defmacro neo-cx95-iter (&rest body)
        `(lambda (generator-final-yield) ,@body))
      (let ((iter (let ((lexical-binding t))
                    (lambda-gen
                     (let ((i 0))
                       (while (< i 5)
                         (iter-yield (cl-incf i))))))))
        (list (funcall iter)
              (funcall iter)
              (funcall iter)
              (funcall iter)
              (funcall iter))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx95_closure_capture_in_loop_does_not_share() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 2 3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((closures nil))
    (dotimes (i 5)
      (push (lambda () i) closures))
    (mapcar #'funcall (nreverse closures))))
"##,
        expect,
    );
}

#[test]
fn div_cx95_closure_mutation_visible_across_callers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 2 3 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((count 0))
    (let ((inc (lambda () (cl-incf count)))
          (get (lambda () count)))
      (list (funcall get)
            (funcall inc)
            (funcall inc)
            (funcall inc)
            (funcall get)
            count))))
"##,
        expect,
    );
}

#[test]
fn div_cx95_make_interpreted_closure_vs_bytecompiled() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((x 42))
    (let ((interpreted (lambda () x))
          (via-eval (eval '(lambda () x) t)))
      (list (funcall interpreted)
            (funcall via-eval)
            (closurep interpreted)
            (closurep via-eval)))))
"##,
        expect,
    );
}

#[test]
fn div_cx95_closure_environment_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((lexical-binding t)
          (x 1)
          (y 2))
      (let ((f (lambda () (+ x y))))
        (list (closurep f)
              (closure--function-environment f))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx95_apply_closure_via_funcall_with_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 0 60 106)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((f (lambda (&rest args) (apply #'+ args))))
    (list (funcall f 1 2 3)
          (funcall f)
          (apply f '(10 20 30))
          (apply f 100 '(1 2 3)))))
"##,
        expect,
    );
}

#[test]
fn div_cx95_recursive_closure_with_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 5 55 6765)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (letrec ((fib (lambda (n acc1 acc2)
                  (if (= n 0) acc1
                    (funcall fib (1- n) acc2 (+ acc1 acc2))))))
    (list (funcall fib 0 0 1)
          (funcall fib 1 0 1)
          (funcall fib 5 0 1)
          (funcall fib 10 0 1)
          (funcall fib 20 0 1))))
"##,
        expect,
    );
}

#[test]
fn div_cx95_defun_inline_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 105 10 110)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (defun neo-cx95-make-adder (n)
    (lambda (x) (+ x n)))
  (let ((add5 (neo-cx95-make-adder 5))
        (add10 (neo-cx95-make-adder 10)))
    (list (funcall add5 0)
          (funcall add5 100)
          (funcall add10 0)
          (funcall add10 100))))
"##,
        expect,
    );
}

#[test]
fn div_cx95_closure_with_opt_and_rest_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable base)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((base 100)
        (f (lambda (a &optional b &rest c)
             (list a b c base))))
    (list (funcall f 1)
          (funcall f 1 2)
          (funcall f 1 2 3 4 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx95_closures_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (letrec ((state nil)
           (record (lambda (msg) (push (list msg) state)))
           (counter 0)
           (inc (lambda () (cl-incf counter) (funcall record :inc))))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Closure test buffer content")
      (put-text-property 1 7 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 3 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (funcall inc)
        (funcall inc)
        (funcall inc)
        (let ((snapshot (list counter (nreverse state)
                              (buffer-string)
                              (marker-position m)
                              (overlay-start ov) (overlay-end ov)
                              (text-properties-at 1))))
          (undo)
          (widen)
          (list snapshot (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
