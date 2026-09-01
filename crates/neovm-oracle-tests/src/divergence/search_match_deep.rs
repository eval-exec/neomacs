//! Divergence tests: goto-char, search, match-data edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_goto_char_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 32)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (goto-char 1)
  (list (point))
  (goto-char 12)
  (list (point))
  (goto-char 6)
  (list (point) (char-after))) "#,
        expect,
    );
}

#[test]
fn divergence_search_forward_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 4 9 12 9 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "foo bar foo baz foo")
  (goto-char 1)
  (search-forward "foo")
  (let ((p1 (match-beginning 0))
        (p2 (match-end 0)))
    (search-forward "foo")
    (let ((p3 (match-beginning 0))
          (p4 (match-end 0)))
      (search-backward "foo")
      (list p1 p2 p3 p4 (match-beginning 0) (match-end 0))))) "#,
        expect,
    );
}

#[test]
fn divergence_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 7 4 7 \"123\" \"123\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "abc123def456")
  (goto-char 1)
  (re-search-forward "\\([0-9]+\\)")
  (list (match-beginning 0)
        (match-end 0)
        (match-beginning 1)
        (match-end 1)
        (match-string 0)
        (match-string 1))) "#,
        expect,
    );
}

#[test]
fn divergence_match_data_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"2024-01-15\" \"2024\" \"01\" \"15\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "2024-01-15")
  (goto-char 1)
  (re-search-forward "\\([0-9]+\\)-\\([0-9]+\\)-\\([0-9]+\\)")
  (list (match-string 0)
        (match-string 1)
        (match-string 2)
        (match-string 3))) "#,
        expect,
    );
}

#[test]
fn divergence_re_search_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 6 \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "hello world")
  (goto-char 1)
  (list (re-search-forward "[0-9]+" nil t)
        (re-search-forward "[a-z]+" nil t)
        (match-string 0))) "#,
        expect,
    );
}

#[test]
fn divergence_replace_match_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"baz bar foo\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "foo bar foo")
  (goto-char 1)
  (re-search-forward "foo")
  (replace-match "baz")
  (buffer-string)) "#,
        expect,
    );
}

#[test]
fn divergence_re_search_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "aaa bbb aaa bbb aaa")
  (goto-char (point-max))
  (re-search-backward "bbb")
  (list (match-beginning 0) (match-end 0))
  (re-search-backward "bbb")
  (list (match-beginning 0) (match-end 0))) "#,
        expect,
    );
}

#[test]
fn divergence_case_fold_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((case-fold-search t))
    (goto-char 1)
    (list (search-forward "hello" nil t)
          (progn (goto-char 1) (search-forward "HELLO" nil t))))
  (let ((case-fold-search nil))
    (goto-char 1)
    (list (search-forward "hello" nil t)
          (search-forward "Hello" nil t)))) "#,
        expect,
    );
}

#[test]
fn divergence_word_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 6 nil 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "hello world helloworld")
  (goto-char 1)
  (list (word-search-forward "hello" nil t)
        (point)
        (word-search-forward "hello" nil t)
        (point))) "#,
        expect,
    );
}

#[test]
fn divergence_save_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "abc def ghi")
  (goto-char 1)
  (re-search-forward "[a-z]+")
  (let ((m (match-data)))
    (save-match-data
      (re-search-forward "[a-z]+"))
    (list (equal m (match-data))
          (match-string 0)))) "#,
        expect,
    );
}
