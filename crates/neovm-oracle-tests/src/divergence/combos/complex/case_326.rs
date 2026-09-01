//! Complex combo batch 326 — `eval`/`apply`/`funcall`/`closure` ultimate:
//! func-arity of lambdas/subrs, closure capture mutation, dynamic vs lexical
//! binding scoping, funcall on subr vs lambda vs macro, apply-partially.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx326_func_arity_of_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((2 . 2) (0 . many) (0 . 1) (1 . many) (0 . many) (1 . 1) (0 . many))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((fixed (lambda (a b) (+ a b)))
      (many (lambda (&rest args) args))
      (optional (lambda (&optional x) x))
      (complex (lambda (a &optional b &rest c) (list a b c))))
  (list (func-arity fixed)
        (func-arity many)
        (func-arity optional)
        (func-arity complex)
        (func-arity (symbol-function '+))
        (func-arity (symbol-function 'car))
        (func-arity (symbol-function 'list))))
"##,
        expect,
    )
}

#[test]
fn div_cx326_closure_capture_mutation_visible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-decf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((count 0))
    (let ((inc (lambda () (cl-incf count)))
          (dec (lambda () (cl-decf count)))
          (get (lambda () count)))
      (list (funcall get)
            (funcall inc)
            (funcall inc)
            (funcall inc)
            (funcall dec)
            (funcall get)
            count))))
"##,
        expect,
    )
}

#[test]
fn div_cx326_dynamic_vs_lexical_var_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (999 999)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defvar neo-cx326-dyn 0)
  (let ((lexical-binding nil))
    (let ((neo-cx326-dyn 100))
      (let ((captured (lambda () neo-cx326-dyn)))
        (let ((neo-cx326-dyn 999))
          (list (funcall captured) neo-cx326-dyn))))))
"##,
        expect,
    )
}

#[test]
fn div_cx326_apply_funcall_with_optional_and_rest() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 2 nil nil) (1 2 3 nil) (1 2 3 (4 5)) (1 2 nil nil) (1 2 3 (4 5)) 15)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((fn (lambda (a b &optional c &rest d) (list a b c d))))
  (list (funcall fn 1 2)
        (funcall fn 1 2 3)
        (funcall fn 1 2 3 4 5)
        (apply fn '(1 2))
        (apply fn 1 2 '(3 4 5))
        (apply '+ 1 2 '(3 4 5))))
"##,
        expect,
    )
}

#[test]
fn div_cx326_function_cells_and_indirect_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t :orig :orig)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defalias 'neo-cx326-orig (lambda () :orig))
(defalias 'neo-cx326-alias 'neo-cx326-orig)
(let* ((cell-orig (symbol-function 'neo-cx326-orig))
       (cell-alias (symbol-function 'neo-cx326-alias))
       (indirect-orig (indirect-function 'neo-cx326-orig))
       (indirect-alias (indirect-function 'neo-cx326-alias)))
  (list (eq cell-orig cell-alias)
        (eq cell-orig indirect-orig)
        (eq cell-orig indirect-alias)
        (funcall 'neo-cx326-orig)
        (funcall 'neo-cx326-alias)))
"##,
        expect,
    )
}

#[test]
fn div_cx326_eval_with_different_environments() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((x 100))
    (list (eval '(+ x 1))
          (eval '(+ x 1) t)
          (eval '(+ x 1) nil)
          (let ((y 50)) (eval '(+ x y) t))
          (eval '(let ((z 5)) (* x z))))))
"##,
        expect,
    )
}

#[test]
fn div_cx326_funcall_macro_should_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil (:err . invalid-function) (* 21 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defmacro neo-cx326-mac (x) `(* ,x 2))
(list (macrop 'neo-cx326-mac)
      (macrop (symbol-function 'neo-cx326-mac))
      (functionp 'neo-cx326-mac)
      (condition-case e (funcall 'neo-cx326-mac 5) (error (cons :err (car e))))
      (macroexpand '(neo-cx326-mac 21)))
"##,
        expect,
    )
}

#[test]
fn div_cx326_apply_partially_partial_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((add-then (apply-partially #'+ 1000))
           (concat-prefix (apply-partially #'concat "prefix-")))
      (list (funcall add-then 1 2 3)
            (funcall concat-prefix "alpha")
            (funcall concat-prefix "beta" "gamma")
            (length (funcall add-then 5))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx326_recursive_lambda_with_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 5 55)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (letrec ((fact (lambda (n acc1 acc2)
                  (if (= n 0) acc1
                    (funcall fact (1- n) acc2 (+ acc1 acc2))))))
    (list (funcall fact 0 0 1)
          (funcall fact 1 0 1)
          (funcall fact 5 0 1)
          (funcall fact 10 0 1))))
"##,
        expect,
    )
}

#[test]
fn div_cx326_eval_apply_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (letrec ((counter 0)
           (inc (lambda () (cl-incf counter)))
           (get (lambda () counter)))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Eval/apply mega test buffer content")
      (put-text-property 1 6 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (funcall inc) (funcall inc) (funcall inc)
        (let ((state (list (funcall get)
                           (func-arity inc)
                           (eval '(macroexpand '(if t :yes :no)) t)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen()
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    )
}
