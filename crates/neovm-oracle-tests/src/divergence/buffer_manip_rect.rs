//! Divergence tests: buffer manipulation edge cases, kill-region, rectangles.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_insert_before_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 \"ABXYCDE\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDE")
  (let ((m (point-marker)))
    (goto-char 3)
    (insert-before-markers "XY")
    (list (marker-position m)
          (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_insert_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"XXXXX---\" 9 1 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert-char ?X 5)
  (insert-char ?- 3)
  (list (buffer-string)
        (point)
        (point-min)
        (point-max)))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_substring_vs_buffer_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello\" \"Hello\" \"Hello World!\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World!")
  (list (buffer-substring 1 6)
        (buffer-substring-no-properties 1 6)
        (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_delete_region_vs_delete_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (delete-region 3 6)
  (list (buffer-string)
        (point))
  (delete-char 2)
  (list (buffer-string)
        (point)))"#,
        expect,
    );
}

#[test]
fn divergence_delete_duplicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ABBCCCDDDDEEEEE\" 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABBCCCDDDDEEEEE")
  (goto-char 1)
  (while (and (not (eobp)) (eq (char-after) (char-after (1+ (point)))))
    (delete-char 1))
  (list (buffer-string)
        (point)))"#,
        expect,
    );
}

#[test]
fn divergence_kill_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\" World\" \"Hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (kill-region 1 6)
  (list (buffer-string)
        (car kill-ring)))"#,
        expect,
    );
}

#[test]
fn divergence_yank_after_kill() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello World\" 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (kill-region 1 6)
  (goto-char 1)
  (yank)
  (list (buffer-string)
        (point)))"#,
        expect,
    );
}

#[test]
fn divergence_kill_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"line2\\nline3\" \"\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "line1\nline2\nline3")
  (goto-char 1)
  (kill-line)
  (list (buffer-string)
        (point)
        (car kill-ring))
  (kill-line)
  (list (buffer-string)
        (car kill-ring)))"#,
        expect,
    );
}

#[test]
fn divergence_kill_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\" foo bar\" 1 \"hello world\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "hello world foo bar")
  (goto-char 1)
  (kill-word 2)
  (list (buffer-string)
        (point)
        (car kill-ring)))"#,
        expect,
    );
}

#[test]
fn divergence_transpose_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"abcdef\" 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "abcdef")
  (goto-char 3)
  (transpose-chars 1)
  (list (buffer-string)
        (point))
  (transpose-chars -1)
  (list (buffer-string)
        (point)))"#,
        expect,
    );
}

#[test]
fn divergence_transpose_words() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"world hello\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "hello world")
  (goto-char 6)
  (transpose-words 1)
  (buffer-string))"#,
        expect,
    );
}

#[test]
fn divergence_extract_rectangle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"BCD\") 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ\nKLMNOPQRST\nUVWXYZ1234")
  (list (extract-rectangle 2 5)
        (string-rectangle 2 5 "XX")))"#,
        expect,
    );
}

#[test]
fn divergence_delete_rectangle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"AEFGHIJ\\nKLMNOPQRST\\nUVWXYZ1234\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ\nKLMNOPQRST\nUVWXYZ1234")
  (delete-rectangle 2 5)
  (buffer-string))"#,
        expect,
    );
}
