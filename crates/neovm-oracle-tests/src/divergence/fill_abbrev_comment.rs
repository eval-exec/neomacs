//! Divergence tests: abbrev, auto-fill, fill-region, paragraph operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_abbrev_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'abbrev-mode)
  (fboundp 'define-abbrev)
  (fboundp 'expand-abbrev)
  (boundp 'abbrevs-changed)
  (boundp 'local-abbrev-table))"#,
        expect,
    );
}

#[test]
fn divergence_define_abbrev_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (define-abbrev-table 'my-test-abbrev-table
    '(("tst" "test" nil :case-fixed t)))
  (list (abbrev-symbol "tst" 'my-test-abbrev-table)
        (abbrev-expansion "tst" 'my-test-abbrev-table)))"#,
        expect,
    );
}

#[test]
fn divergence_fill_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"This is a long line\\nthat should be\\nfilled at some point\\nin the text.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "This is a long line that should be filled at some point in the text.")
  (let ((fill-column 20))
    (fill-region (point-min) (point-max))
    (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_fill_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "This is a very long paragraph that spans multiple sentences. It should be filled according to the fill column setting.")
  (let ((fill-column 30))
    (fill-paragraph nil)
    (length (split-string (buffer-string) "\n"))))"#,
        expect,
    );
}

#[test]
fn divergence_auto_fill_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'auto-fill-mode)
  (boundp 'auto-fill-function)
  (booleanp (boundp 'auto-fill-function)))"#,
        expect,
    );
}

#[test]
fn divergence_paragraph_commands() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (11 22 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Para one.\n\nPara two.\n\nPara three.")
  (goto-char 1)
  (forward-paragraph 1)
  (let ((p1 (point)))
    (forward-paragraph 1)
    (let ((p2 (point)))
      (backward-paragraph 1)
      (list p1 p2 (point)))))"#,
        expect,
    );
}

#[test]
fn divergence_sentence_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (stringp sentence-end)
  (fboundp 'sentence-end)
  (boundp 'sentence-end-double-space)
  (booleanp sentence-end-double-space))"#,
        expect,
    );
}

#[test]
fn divergence_comment_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\";; hello world\" \";; \" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq comment-start ";; ")
  (setq comment-end "")
  (insert "hello world")
  (comment-region 1 12)
  (list (buffer-string)
        comment-start
        comment-end))"#,
        expect,
    );
}

#[test]
fn divergence_uncomment_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\\\"\" 7 51)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq comment-start ";; ")
  (setq comment-end "")
  (insert ";; hello world")
  (uncomment-region 1 15)
  (list (buffer-string)
        (string= (buffer-string) "hello world")))#" ,
    );
}

#[test]
fn divergence_indent_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    assert_oracle_parity(
        r#"(list
  (fboundp 'indent-for-tab-command)
  (fboundp 'indent-region)
  (fboundp 'indent-relative)
  (fboundp 'indent-to))"#,
        expect,
    );
}
