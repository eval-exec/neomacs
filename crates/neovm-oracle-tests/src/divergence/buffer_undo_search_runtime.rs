//! Buffer/undo/search parity: primitive-undo chains, search-forward/backward,
//! re-search + replace-match, narrow/widen, line/column ops, looking-at/back.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn line_column_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 6 3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "line1\nline22\nline333\n")
  (goto-char (point-min)) (forward-line 1) (end-of-line)
  (list (line-number-at-pos) (current-column)
        (count-lines (point-min) (point-max))
        (progn (goto-char (point-max)) (line-number-at-pos))))"##,
        expect,
    );
}

#[test]
fn looking_at_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (goto-char 6)
  (list (looking-at " wor") (looking-back "hello" 1)
        (looking-at-p "[[:space:]]")))"##,
        expect,
    );
}

#[test]
fn narrow_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"3456\" 3 7 \"1234567890\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "1234567890")
  (narrow-to-region 3 7)
  (list (buffer-string) (point-min) (point-max)
        (progn (widen) (buffer-string))))"##,
        expect,
    );
}

#[test]
fn replace_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"aXbXcXdX\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "a1b2c3d4")
  (goto-char (point-min))
  (while (re-search-forward "[0-9]" nil t) (replace-match "X"))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn replace_string_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"the cAT sAT on the mAT\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "the cat sat on the mat")
  (goto-char (point-min))
  (while (search-forward "at" nil t) (replace-match "AT"))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn search_forward_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 12 17 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "foo bar foo baz foo")
  (goto-char (point-min))
  (list (search-forward "foo" nil t) (search-forward "foo" nil t)
        (progn (goto-char (point-max)) (search-backward "foo" nil t))
        (count-matches "foo" (point-min) (point-max))))"##,
        expect,
    );
}

#[test]
fn undo_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello") (undo-boundary) (insert " world") (undo-boundary)
  (primitive-undo 1 buffer-undo-list)
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn undo_redo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"AAABBB\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (buffer-enable-undo)
  (insert "AAA") (undo-boundary) (insert "BBB") (undo-boundary)
  (delete-region 1 4) (undo-boundary)
  (let ((l buffer-undo-list))
    (setq l (primitive-undo 1 l))
    (setq l (primitive-undo 1 l))
    (buffer-string)))"##,
        expect,
    );
}
