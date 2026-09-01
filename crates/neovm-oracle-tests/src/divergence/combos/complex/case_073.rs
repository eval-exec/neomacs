//! Complex combo batch 73 — `eval` / `apply` / `funcall` / closure deep:
//! function arity, dynamic vs lexical binding scoping, `func-arity`,
//! `closure` capture mutation, `funcall` on subr vs lambda vs macro.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx73_func_arity_of_lambdas_subrs_and_macros() {
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
  (list
   (func-arity fixed)
   (func-arity many)
   (func-arity optional)
   (func-arity complex)
   (func-arity (symbol-function '+))
   (func-arity (symbol-function 'car))
   (func-arity (symbol-function 'list))))
"##,
        expect,
    );
}

#[test]
fn div_cx73_closure_capture_mutation_lexical_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-decf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((counter 0))
    (let ((inc (lambda () (cl-incf counter)))
          (dec (lambda () (cl-decf counter)))
          (get (lambda () counter)))
      (list
       (funcall get)
       (funcall inc)
       (funcall inc)
       (funcall inc)
       (funcall dec)
       (funcall get)
       counter))))
"##,
        expect,
    );
}

#[test]
fn div_cx73_dynamic_vs_lexical_var_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (999 999)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defvar neo-cx73-dyn 0)
  (let ((lexical-binding nil))
    (let ((neo-cx73-dyn 100))
      (let ((lex-captured (lambda () neo-cx73-dyn)))
        (let ((neo-cx73-dyn 999))
          (list (funcall lex-captured) neo-cx73-dyn))))))
"##,
        expect,
    );
}

#[test]
fn div_cx73_apply_funcall_with_optional_and_rest() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 2 nil nil) (1 2 3 nil) (1 2 3 (4 5)) (1 2 nil nil) (1 2 3 (4 5)) (1 2 3 nil) 15)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((fn (lambda (a b &optional c &rest d) (list a b c d))))
  (list
   (funcall fn 1 2)
   (funcall fn 1 2 3)
   (funcall fn 1 2 3 4 5)
   (apply fn '(1 2))
   (apply fn 1 2 '(3 4 5))
   (apply fn 1 '(2 3))
   (apply '+ 1 2 '(3 4 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx73_function_cells_and_indirect_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t :orig :orig)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defalias 'neo-cx73-orig (lambda () :orig))
(defalias 'neo-cx73-alias 'neo-cx73-orig)
(let* ((cell-orig (symbol-function 'neo-cx73-orig))
       (cell-alias (symbol-function 'neo-cx73-alias))
       (indirect-orig (indirect-function 'neo-cx73-orig))
       (indirect-alias (indirect-function 'neo-cx73-alias)))
  (list (eq cell-orig cell-alias)
        (eq cell-orig indirect-orig)
        (eq cell-orig indirect-alias)
        (funcall 'neo-cx73-orig)
        (funcall 'neo-cx73-alias)))
"##,
        expect,
    );
}

#[test]
fn div_cx73_eval_with_different_lexical_environments() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((x 100))
    (list
     (eval '(+ x 1))
     (eval '(+ x 1) t)
     (eval '(+ x 1) nil)
     (let ((y 50)) (eval '(+ x y) t))
     (eval '(let ((z 5)) (* x z))) )))
"##,
        expect,
    );
}

#[test]
fn div_cx73_closure_environment_capture_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((lexical-binding t))
      (let ((a 1) (b 2) (c 3))
        (let ((f (lambda () (list a b c))))
          (let ((env (closure--function-environment f)))
            (list env)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx73_funcall_macro_should_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil (:err . invalid-function) (* 21 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defmacro neo-cx73-mac (x) `(* ,x 2))
(list
 (macrop 'neo-cx73-mac)
 (macrop (symbol-function 'neo-cx73-mac))
 (functionp 'neo-cx73-mac)
 (condition-case e (funcall 'neo-cx73-mac 5) (error (cons :err (car e))))
 (macroexpand '(neo-cx73-mac 21)))
"##,
        expect,
    );
}

#[test]
fn div_cx73_partial_application_with_apply_partially() {
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
    );
}

#[test]
fn div_cx73_helpful_recursive_lambda_in_letrec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 120 3628800)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (letrec ((fact (lambda (n) (if (= n 0) 1 (* n (funcall fact (1- n)))))))
    (list (funcall fact 0)
          (funcall fact 1)
          (funcall fact 5)
          (funcall fact 10))))
"##,
        expect,
    );
}

#[test]
fn div_cx73_mapcar_mapc_with_strings_and_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 98 99) (2 3 4) (1 2 3) (1 2 3) (1 1 2 2 3 3) \"<1>,<2>,<3>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (acc)
  (list
   (mapcar #'identity "abc")
   (mapcar #'1+ [1 2 3])
   (mapc (lambda (x) (push x acc)) '(1 2 3))
   (nreverse acc)
   (mapcan (lambda (x) (list x x)) '(1 2 3))
   (mapconcat (lambda (x) (format "<%d>" x)) '(1 2 3) ",")))
"##,
        expect,
    );
}

#[test]
fn div_cx73_apply_lambda_with_defun_kw_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defun)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defun neo-cx73-kwfn (a &key b (c 99))
  (list a b c))
(list
 (neo-cx73-kwfn 1 :b 2)
 (neo-cx73-kwfn 1 :b 2 :c 3)
 (neo-cx73-kwfn 1)
 (condition-case e (neo-cx73-kwfn 1 :unknown 5) (error (cons :err (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx73_closure_apply_eval_undo_marker_overlay_textprop_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function add-eval)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (letrec ((counter 0)
           (state-collector (lambda (msg)
                              (push (list msg counter) state-collector))))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Buffer text content here")
      (put-text-property 1 5 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (let* ((add-eval (lambda (form) (eval form t)))
               (apply-fn (lambda (f x) (funcall f x)))
               (counter-inc (add-eval '(cl-incf counter)))
               (before (apply-fn state-collector :before)))
          (delete-region 5 9)
          (insert "XX")
          (cl-incf counter)
          (let ((after (apply-fn state-collector :after)))
            (undo)
            (widen)
            (list counter before after
                  (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))))
"##,
        expect,
    );
}
