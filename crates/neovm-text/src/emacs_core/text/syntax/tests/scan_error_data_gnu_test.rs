//! The GNU oracle forms of `neovm-oracle-tests` `syntax/scan_error_data.rs`,
//! run in process against the answers GNU 31.1 gave there
//! (`NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`).
use crate::test_utils::{oracle_expect_transcript, runtime_startup_eval_one};

#[test]
fn scan_sexps_error_data_both_directions() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(let (out)
  (dolist (case '(("(foo (bar) baz)" 6 -3)
                  ("(foo ( bar) baz)" 11 -3)
                  ("(foo (  'bar) baz)" 13 -3)
                  ("(foo ; c\n (bar) baz)" 12 -3)
                  ("x (foo bar) y" 8 -3)
                  ("(a b)" 3 -2)
                  ("((a) b)" 6 -3)
                  ("a b)" 5 -1)
                  ("a (b c) d)" 11 -1)
                  ("ab\"" 4 -1)
                  ("a \"b c) d" 9 -2)
                  ("(foo (bar ) baz)" 7 3)
                  ("(a  b  )" 3 3)
                  ("(a b" 1 1)
                  ("\"ab" 1 1)))
    (with-temp-buffer
      (emacs-lisp-mode)
      (insert (car case))
      (push (condition-case err
                (scan-sexps (nth 1 case) (nth 2 case))
              (scan-error (cdr err)))
            out)))
  (nreverse out))
"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(
            r#""OK ((\"Containing expression ends prematurely\" 1 1) (\"Containing expression ends prematurely\" 6 6) (\"Containing expression ends prematurely\" 6 6) (\"Containing expression ends prematurely\" 11 11) (\"Containing expression ends prematurely\" 3 3) (\"Containing expression ends prematurely\" 1 1) (\"Containing expression ends prematurely\" 1 1) (\"Unbalanced parentheses\" 4 1) (\"Unbalanced parentheses\" 10 1) (\"Unbalanced parentheses\" 3 1) (\"Unbalanced parentheses\" 7 1) (\"Containing expression ends prematurely\" 11 12) (\"Containing expression ends prematurely\" 8 9) (\"Unbalanced parentheses\" 1 5) (\"Unbalanced parentheses\" 1 4))""#
        )
    );
}

#[test]
fn backward_sexp_command_error_data() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun f (x)\n  (list x 'y))")
  (list (condition-case err (progn (goto-char 20) (backward-sexp 4) (point))
          (scan-error (cdr err)))
        (condition-case err (progn (goto-char 11) (backward-sexp 2) (point))
          (scan-error (cdr err)))
        (condition-case err (progn (goto-char 20) (forward-sexp 4) (point))
          (scan-error (cdr err)))))
"#;
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(
            r#""OK ((\"Containing expression ends prematurely\" 16 16) (\"Containing expression ends prematurely\" 10 10) (\"Containing expression ends prematurely\" 26 27))""#
        )
    );
}
