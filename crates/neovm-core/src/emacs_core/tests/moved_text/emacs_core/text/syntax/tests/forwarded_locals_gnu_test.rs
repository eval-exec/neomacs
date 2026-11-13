//! In-process twin of `neovm-oracle-tests` `syntax/forwarded_locals.rs`:
//! GNU's C-forwarded syntax variables are read from the CURRENT BUFFER's
//! forwarded binding and the current dynamic binding, not from the variable's
//! default. The expectations are the GNU 31.1 `--batch -Q` answers (the same
//! forms and expect literals are recorded in the oracle module, where
//! `NEOVM_ORACLE_MODE=refresh` writes them from GNU).
//!
//! GNU source: `DEFVAR_BOOL parse-sexp-lookup-properties` and
//! `parse-sexp-ignore-comments` (src/syntax.c:3764-3771), `DEFVAR_INT
//! syntax-propertize--done` + `Fmake_variable_buffer_local` (src/syntax.c:
//! 3773-3777), `DEFVAR_BOOL comment-end-can-be-escaped` +
//! `Fmake_variable_buffer_local` (src/syntax.c:3811-3815). The scanners read
//! the C variables, which `set_buffer_internal_2` (src/buffer.c:2342ff) swaps
//! to the current buffer's forwarded locals; a dynamic `let` binding writes
//! the same C storage. The pre-split Neomacs read was `builtin_var_value`;
//! after the stage-3 TextHost split these sites read
//! `VariableView::value` (the DEFAULT), which turns every buffer-local and
//! `let` binding of these variables invisible to the scanner.

use crate::test_utils::{oracle_expect_transcript, runtime_startup_eval_one};

#[test]
fn parse_sexp_lookup_properties_reads_the_current_buffer_and_dynamic_binding() {
    crate::test_utils::init_test_tracing();
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
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(r#""OK (scan-error nil nil scan-error)""#)
    );
}

#[test]
fn parse_sexp_ignore_comments_reads_the_current_buffer_and_dynamic_binding() {
    crate::test_utils::init_test_tracing();
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
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(r#""OK (scan-error nil scan-error scan-error)""#)
    );
}

#[test]
fn comment_end_can_be_escaped_reads_the_current_buffer() {
    crate::test_utils::init_test_tracing();
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
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(r#""OK (9 14 9)""#)
    );
}

#[test]
fn syntax_propertize_done_reads_the_current_buffer() {
    crate::test_utils::init_test_tracing();
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
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(r#""OK (nil ((1 20)) 20)""#)
    );
}

#[test]
fn parse_partial_sexp_accepts_a_buffer_local_propertize_done() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(with-temp-buffer
  (setq parse-sexp-lookup-properties t)
  (insert "(a b)")
  (setq-local syntax-propertize-function
              (lambda (s e) (setq-local syntax-propertize--done e)))
  (condition-case e (progn (parse-partial-sexp 1 5) 'ok) (error (car e))))
"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(r#""OK ok""#)
    );
}
