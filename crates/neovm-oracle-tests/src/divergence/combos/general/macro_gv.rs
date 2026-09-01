//! Divergence tests: defmacro + cl-macs + backquote + eval-when-compile combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_defmacro_backquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (10 nil 20 nil \"(let ((tmp ,a))\\n       (setq ,a ,b)\\n       (setq ,b tmp)))\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-macro-swap (a b)
    "(let ((tmp ,a))
       (setq ,a ,b)
       (setq ,b tmp)))")
  (let ((x 10) (y 20))
    (test-macro-swap x y)
    (list x (= x 20)
          y (= y 10)
          (macroexpand '(test-macro-swap x y))
          (listp (macroexpand '(test-macro-swap x y)))))) "#,
        expect,
    );
}

#[test]
fn deficiency_defmacro_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-macro-with (place val)
    "(setf ,place ,val)")
  (let ((buf (generate-new-buffer "test-macro-buf")))
    (with-current-buffer buf
      (insert "ORIGINAL")
      (put-text-property 1 8 'status 'initial)
      (let ((ov (make-overlay 1 8)))
        (overlay-put ov 'state 'clean)
        (undo-boundary)
        (goto-char 1)
        (test-macro-with (buffer-string) (concat "MODIFIED-" (buffer-string)))
        (let ((s (buffer-string)))
          (list s
                (string= s "MODIFIED-ORIGINAL")
                (= (buffer-size) 16))))))
    (kill-buffer buf))) "#,
        expect,
    );
}

#[test]
fn deficiency_cl_defmacro_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defmacro test-cl-dstruct ((a b &optional c) &rest body)
    "(list ,a ,b ,c (progn ,@body)))
  (list (test-cl-dstruct (1 2) "extra")
        (equal (test-cl-dstruct (1 2) "extra") '(1 2 nil "extra"))
        (test-cl-dstruct (10 20 30) "more" "stuff")
        (equal (test-cl-dstruct (10 20 30) "more" "stuff") '(10 20 30 ("more" "stuff"))))) "#,
        expect,
    );
}

#[test]
fn deficiency_gv_generalized_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a b X d Y) t (99 t 1 t))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-gv-list '(a b c d e))
  (setf (nth 2 test-gv-list) 'X)
  (setf (nth 4 test-gv-list) 'Y)
  (list test-gv-list
        (equal test-gv-list '(a b X d Y))
        (let ((h (make-hash-table :test 'equal)))
          (puthash "key" 42 h)
          (setf (gethash "key" h) 99)
          (list (gethash "key" h) (= (gethash "key" h) 99)
                (hash-table-count h) (= (hash-table-count h) 1))))) "#,
        expect,
    );
}

#[test]
fn deficiency_gv_with_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"BYEO WORLD\" 5 9 (case upper)) nil \"BYEO PLANET\" nil nil en t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "HELLO WORLD")
  (put-text-property 1 5 'case 'upper)
  (put-text-property 7 11 'case 'upper)
  (let ((ov (make-overlay 1 11)))
    (overlay-put ov 'lang 'en)
    (setf (buffer-substring 1 5) "BYE")
    (let ((s1 (buffer-string)))
      (setf (buffer-substring 5 11) " PLANET")
      (let ((s2 (buffer-string)))
        (list s1 (string= s1 "BYE WORLD")
              s2 (string= s2 "BYE PLANET")
              (= (buffer-size) 10)
              (overlay-get ov 'lang) (eq (overlay-get ov 'lang) 'en)))))) "#,
        expect,
    );
}

#[test]
fn deficiency_eval_and_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument number-or-marker-p \"(funcall (lambda (y) (+ y (test-inline-helper ,x))) 10)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-inline-helper (x) (+ x 1))
  (defmacro test-inline-macro (x)
    "(funcall (lambda (y) (+ y (test-inline-helper ,x))) 10)")
  (list (test-inline-macro 5) (= (test-inline-macro 5) 16)
        (test-inline-macro 10) (= (test-inline-macro 10) 21)
        (test-inline-macro 0) (= (test-inline-macro 0) 11)))) "#,
        expect,
    );
}

#[test]
fn deficiency_cl_letf_with_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "BEFORE")
  (put-text-property 1 6 'stage 'initial)
  (let ((ov (make-overlay 1 6)))
    (overlay-put ov 'phase 1)
    (cl-letf (((buffer-string)) "DURING")
      (list (buffer-string)
            (string= (buffer-string) "BEFORE")))
    (list (buffer-string) (string= (buffer-string) "BEFORE")
          (get-text-property 1 'stage) (eq (get-text-property 1 'stage) 'initial)
          (overlay-get ov 'phase) (= (overlay-get ov 'phase) 1)))) "#,
        expect,
    );
}

#[test]
fn deficiency_compiled_vs_interpreted_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((x 10)
        (f1 (lambda (n) (+ n x)))
        (f2 (function (lambda (n) (+ n x)))))
    (list (funcall f1 5) (= (funcall f1 5) 15)
          (funcall f2 5) (= (funcall f2 5) 15)
          (functionp f1) (functionp f2)
          (eq (type-of f1) (type-of f2))))) "#,
        expect,
    );
}

#[test]
fn deficiency_backquote_nested_splice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((begin (\\,@ a) middle (\\,@ b) (\\,@ c) end) nil nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((a '(1 2 3))
        (b '(4 5))
        (c '(6 7 8)))
    (let ((result '(begin ,@a middle ,@b ,@c end)))
      (list result
            (equal result '(begin 1 2 3 middle 4 5 6 7 8 end))
            (= (length result) 10)
            (equal (nth 0 result) 'begin)
            (equal (last result) '(end)))))) "#,
        expect,
    );
}

#[test]
fn deficiency_macroexpand_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function \\,)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-mx-when (cond &rest body)
    "(if ,cond (progn ,@body)))
  (defmacro test-mx-unless (cond &rest body)
    "(if (not ,cond) (progn ,@body)))
  (let ((w (macroexpand-all '(test-mx-when t (princ 1) (princ 2)))))
    (list w
          (listp w)
          (eq (car w) 'if)
          (let ((u (macroexpand-all '(test-mx-unless nil (princ 3)))))
            (list u (listp u) (eq (car u) 'if)))))) "#,
        expect,
    );
}
