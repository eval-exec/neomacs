//! Strict combo oracle probes, batch 140: print-circle + print-number-table
//! combined, variable watcher + buffer-local combo, indent-relative,
//! electric-indent-mode behavior, and cl-typep with complex specifiers.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v4_print_circle_number_table_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((n 42)
       (s "shared")
       (shared (list n s))
       (tree (list shared shared))
       (mixed (list shared [n s] shared)))
  (list (let ((print-circle t) (print-number-table t)) (prin1-to-string tree))
        (let ((print-circle t) (print-number-table t)) (prin1-to-string mixed))
        (let ((print-number-table t) (prin1-to-string tree))
        (let ((print-circle t) (prin1-to-string tree))))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v4_variable_watcher_buffer_local_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (defvar probe-vw2 0)
  (let ((w (lambda (sym val op where)
             (push (list op (and (bufferp where) (eq where (current-buffer)))) log))))
    (add-variable-watcher 'probe-vw2 w)
    (unwind-protect
        (progn
          (setq probe-vw2 1)
          (with-temp-buffer
            (make-local-variable 'probe-vw2)
            (setq probe-vw2 2)
            (kill-local-variable 'probe-vw2))
          (setq-default probe-vw2 3)
          (makunbound 'probe-vw2)
          (defvar probe-vw2 4))
      (remove-variable-watcher 'probe-vw2 w))
    (list (nreverse log) probe-vw2 (default-value 'probe-vw2))))
"##;
    let expect = expect_test::expect![[
        r#""OK (((set nil) (set t) (makunbound t) (set nil) (makunbound nil) (set nil)) 4 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v4_indent_relative_and_electric_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (with-temp-buffer
        (insert "foo\nbar\nbaz")
        (goto-char 5)
        (let ((indent-line-function #'indent-relative))
          (indent-line-to 2))
        (buffer-string))
      (with-temp-buffer
        (insert "(defun foo ()\nbody)")
        (emacs-lisp-mode)
        (goto-char 16)
        (let ((electric-indent-mode nil))
          (newline)
          (buffer-string)))
      (with-temp-buffer
        (insert "(a\nb")
        (emacs-lisp-mode)
        (goto-char 4)
        (let ((electric-indent-mode nil))
          (newline-and-indent)
          (buffer-string))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"foo\\n  bar\\nbaz\" \"(defun foo ()\\nb\\nody)\" \"(a\\n\\n b\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v4_cl_typep_complex_specifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (cl-defstruct probe-typep-struct a)
  (let ((s (probe-typep-struct :a 1)))
    (list (cl-typep 42 'integer)
          (cl-typep 42 '(or string integer))
          (cl-typep "x" '(or string integer))
          (cl-typep 42 '(and number (not float)))
          (cl-typep 3.14 '(and number (not float)))
          (cl-typep '(1 2) '(satisfies (lambda (x) (> (length x) 1))))
          (cl-typep s 'probe-typep-struct)
          (cl-typep [1 2] 'vector)
          (cl-typep 42 '(integer 0 100))
          (cl-typep 150 '(integer 0 100)
          (cl-typep 42 '(member 1 2 42 3))
          (cl-typep 'x '(member a b c)))))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v4_overlay_modification_hooks_combo_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (with-temp-buffer
    (insert "0123456789")
    (let ((o (make-overlay 2 6)))
      (overlay-put o 'modification-hooks
                   (list (lambda (ov after beg end &optional len)
                           (push (list 'mod (if after 'after 'before) beg end len) log))))
      (overlay-put o 'insert-in-front-hooks
                   (list (lambda (ov after beg end &optional len)
                           (push (list 'front (if after 'after 'before) beg end len) log))))
      (overlay-put o 'insert-behind-hooks
                   (list (lambda (ov after beg end &optional len)
                           (push (list 'behind (if after 'after 'before) beg end len) log))))
      (goto-char 4)
      (insert "X")
      (goto-char 2)
      (insert "Y")
      (goto-char 8)
      (insert "Z")
      (delete-region 3 5))
    (list (buffer-string)
          (length (nreverse log))
          (eq (car (nreverse log)) (car (nreverse log))))))
"##;
    let expect = expect_test::expect![[r#""OK (\"0YX34Z56789\" 8 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
