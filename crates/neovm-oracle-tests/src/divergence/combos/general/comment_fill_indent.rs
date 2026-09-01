//! Divergence tests: comment + fill + indentation + region combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_comment_region_uncomment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    crate::common::assert_oracle_parity(
        r#"(progn
  (insert "line1\nline2\nline3\n")
  (comment-region 1 18)
  (let ((commented (buffer-string)))
    (uncomment-region 1 (point-max))
    (list commented
          (buffer-string)
          (string-match ";" commented)
          (>= (length commented) 18)
          (string-match "line1" (buffer-string))
          (>= (length (buffer-string)) 17)))) "#,
    );
}

#[test]
fn divergence_indent_rigidly() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"    line1\\n      line2\\n\tline3\\n\" \"  line1\\n    line2\\n      line3\\n\" 0 10 0 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "line1\n  line2\n    line3\n")
  (indent-rigidly 1 28 4)
  (let ((s1 (buffer-string)))
    (indent-rigidly 1 28 -2)
    (let ((s2 (buffer-string)))
      (list s1 s2
            (string-match "    line1" s1)
            (string-match "      line2" s1)
            (string-match "  line1" s2)
            (string-match "    line2" s2))))) "#,
        expect,
    );
}

#[test]
fn divergence_delete_indentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"  hello world\" nil 2 8 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "  hello\n  world")
  (goto-char 9)
  (delete-indentation)
  (list (buffer-string)
        (= (buffer-size) 11)
        (string-match "hello" (buffer-string))
        (string-match "world" (buffer-string))
        (not (string-match "\n" (buffer-string))))) "#,
        expect,
    );
}

#[test]
fn divergence_thing_at_point_line_sentence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Second sentence.\" (18 . 34) t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "First sentence.  Second sentence.  Third sentence.")
  (goto-char 20)
  (let ((sentence (thing-at-point 'sentence))
        (bounds (bounds-of-thing-at-point 'sentence)))
    (list sentence
          bounds
          (stringp sentence)
          (> (length sentence) 0)
          (consp bounds)
          (<= (car bounds) 20)
          (>= (cdr bounds) 20)))) "#,
        expect,
    );
}

#[test]
fn divergence_justify_current_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"This is a short test line\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((fill-column 30))
    (insert "This is a short test line")
    (justify-current-line 'left nil t)
    (list (buffer-string)
          (= (buffer-size) 25)
          (string= (buffer-string) "This is a short test line")))) "#,
        expect,
    );
}

#[test]
fn divergence_comment_kill_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    crate::common::assert_oracle_parity(
        r#"(progn
  (insert ";; a comment\ncode here\n;; another comment\n")
  (goto-char 1)
  (let ((c1 (comment-search-forward (line-end-position) t)))
    (goto-char (line-beginning-position))
    (comment-kill 1)
    (list c1
          (buffer-string)
          (string-match "code here" (buffer-string))
          (string-match "another" (buffer-string))
          (>= (length (buffer-string)) 10)))) "#,
    );
}

#[test]
fn divergence_indent_line_to() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 49)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "hello\nworld\ntest\n")
  (goto-char 1)
  (indent-line-to 4)
  (forward-line 1)
  (indent-line-to 8)
  (forward-line 1)
  (indent-line-to 0)
  (list (buffer-string)
        (string-match "    hello" (buffer-string))
        (string-match "        world" (buffer-string))
        (string-match "^test" (buffer-string))))) "#,
        expect,
    );
}

#[test]
fn divergence_move_to_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 5 nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "short\nmedium length\nlonger line here\n")
  (goto-char 1)
  (move-to-column 10)
  (let ((c1 (current-column)))
    (end-of-line)
    (move-to-column 5)
    (let ((c2 (current-column)))
      (list c1 c2
            (= c1 10)
            (= c2 5)
            (= (current-column) 5))))) "#,
        expect,
    );
}

#[test]
fn divergence_comment_pad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\";\" \"\") t t \"code\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "code")
  (let ((style (list comment-start comment-end)))
    (list style
          (stringp comment-start)
          (or (null comment-end) (string= comment-end ""))
          (buffer-string)
          (string= (buffer-string) "code")))) "#,
        expect,
    );
}

#[test]
fn divergence_region_active_mark() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 8 t t \"CDEFG\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (push-mark 3)
  (goto-char 8)
  (let ((start (region-beginning))
        (end (region-end))
        (active (region-active-p)))
    (list start end
          (= start 3)
          (= end 8)
          (buffer-substring start end)
          (string= (buffer-substring start end) "CDEFG")))) "#,
        expect,
    );
}
