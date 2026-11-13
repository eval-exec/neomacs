//! Oracle parity for GNU's C-forwarded syntax variables. GNU reads the C
//! variable that `set_buffer_internal_2` (src/buffer.c:2342ff) swaps to the
//! current buffer's forwarded locals; a dynamic `let` writes the same
//! storage. `parse-sexp-lookup-properties` and
//! `parse-sexp-ignore-comments` are `DEFVAR_BOOL` (src/syntax.c:3764-3771),
//! `syntax-propertize--done` and `comment-end-can-be-escaped` are
//! `DEFVAR_BOOL/INT` followed by `Fmake_variable_buffer_local`
//! (src/syntax.c:3773-3777, 3811-3815). The expects are GNU 31.1 answers
//! recorded with `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_parse_sexp_lookup_properties_buffer_local_and_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (setq parse-sexp-lookup-properties nil)
  (insert "a")
  (put-text-property 1 2 'syntax-table (string-to-syntax "("))
  (setq-local parse-sexp-lookup-properties t)
  (let ((local-t (condition-case e (scan-lists 1 1 0) (error (car e)))))
    (setq-local parse-sexp-lookup-properties nil)
    (setq-default parse-sexp-lookup-properties t)
    (let ((default-t (condition-case e (scan-lists 1 1 0) (error (car e)))))
      (setq-default parse-sexp-lookup-properties nil)
      (setq-local parse-sexp-lookup-properties t)
      (let ((let-nil (condition-case e
                       (let ((parse-sexp-lookup-properties nil))
                         (scan-lists 1 1 0))
                       (error (car e)))))
        (list local-t default-t let-nil
              (condition-case e (scan-lists 1 1 0) (error (car e))))))))
"#;
    let expect = expect_test::expect![[r#""OK (scan-error nil nil scan-error)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_parse_sexp_ignore_comments_buffer_local_and_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "a ; )\nb")
  (setq parse-sexp-ignore-comments nil)
  (let ((local-nil (condition-case e (scan-lists 2 1 0) (error (car e)))))
    (setq-local parse-sexp-ignore-comments t)
    (let ((local-t (condition-case e (scan-lists 2 1 0) (error (car e)))))
      (setq-local parse-sexp-ignore-comments nil)
      (setq-default parse-sexp-ignore-comments t)
      (let ((default-t-local-nil
             (condition-case e (scan-lists 2 1 0) (error (car e)))))
        (setq-default parse-sexp-ignore-comments nil)
        (setq-local parse-sexp-ignore-comments t)
        (list local-nil local-t default-t-local-nil
              (condition-case e
                  (let ((parse-sexp-ignore-comments nil)) (scan-lists 2 1 0))
                (error (car e))))))))
"#;
    let expect = expect_test::expect![[r#""OK (scan-error nil scan-error scan-error)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_comment_end_can_be_escaped_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?/ ". 124b" st)
    (modify-syntax-entry ?* ". 23" st)
    (modify-syntax-entry ?\n "> b" st)
    (set-syntax-table st))
  (insert "/* a \\*/ b */")
  (setq comment-end-can-be-escaped nil)
  (let ((local-nil (progn (goto-char 1) (forward-comment 1) (point))))
    (setq-local comment-end-can-be-escaped t)
    (let ((local-t (progn (goto-char 1) (forward-comment 1) (point))))
      (setq-local comment-end-can-be-escaped nil)
      (setq-default comment-end-can-be-escaped t)
      (let ((default-t-local-nil
             (progn (goto-char 1) (forward-comment 1) (point))))
        (setq-default comment-end-can-be-escaped nil)
        (list local-nil local-t default-t-local-nil)))))
"#;
    let expect = expect_test::expect![[r#""OK (9 14 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_syntax_propertize_done_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (setq parse-sexp-lookup-properties t)
  (insert "a b c d e f g h i j")
  (let (log)
    (setq-local syntax-propertize-function
                (lambda (s e)
                  (push (list s e) log)
                  (setq-local syntax-propertize--done e)))
    (setq-local syntax-propertize--done 10)
    (syntax-propertize 10)
    (let ((skipped log))
      (syntax-propertize 15)
      (list skipped (nreverse log) syntax-propertize--done))))
"#;
    let expect = expect_test::expect![[r#""OK (nil ((1 20)) 20)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_parse_partial_sexp_buffer_local_propertize_done() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (setq parse-sexp-lookup-properties t)
  (insert "(a b)")
  (setq-local syntax-propertize-function
              (lambda (s e) (setq-local syntax-propertize--done e)))
  (condition-case e (progn (parse-partial-sexp 1 5) 'ok) (error (car e))))
"#;
    let expect = expect_test::expect![[r#""OK ok""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
