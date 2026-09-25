//! Oracle parity for the data `scan-sexps` signals (GNU `scan_lists`,
//! src/syntax.c): `(scan-error MESSAGE LAST-GOOD FROM)`, where LAST-GOOD is
//! the last character examined at the starting depth and FROM where the scan
//! stopped. Backward, an open paren that ends the containing expression is
//! both; an unbalanced sexp stops at the accessible start.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_scan_sexps_error_data_both_directions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
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
    let expect = expect_test::expect![[
        r#""OK ((\"Containing expression ends prematurely\" 1 1) (\"Containing expression ends prematurely\" 6 6) (\"Containing expression ends prematurely\" 6 6) (\"Containing expression ends prematurely\" 11 11) (\"Containing expression ends prematurely\" 3 3) (\"Containing expression ends prematurely\" 1 1) (\"Containing expression ends prematurely\" 1 1) (\"Unbalanced parentheses\" 4 1) (\"Unbalanced parentheses\" 10 1) (\"Unbalanced parentheses\" 3 1) (\"Unbalanced parentheses\" 7 1) (\"Containing expression ends prematurely\" 11 12) (\"Containing expression ends prematurely\" 8 9) (\"Unbalanced parentheses\" 1 5) (\"Unbalanced parentheses\" 1 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_backward_sexp_command_error_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
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
    let expect = expect_test::expect![[
        r#""OK ((\"Containing expression ends prematurely\" 16 16) (\"Containing expression ends prematurely\" 10 10) (\"Containing expression ends prematurely\" 26 27))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
