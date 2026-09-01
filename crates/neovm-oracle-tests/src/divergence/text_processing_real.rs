//! Divergence tests: real text processing behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_re_search_forward_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 13 5 8 \"bbb\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"aaa bbb ccc aaa ddd\")
  (goto-char 1)
  (re-search-forward \"aaa\")
  (let ((pos1 (match-beginning 0)))
    (re-search-forward \"aaa\")
    (let ((pos2 (match-beginning 0)))
      (re-search-backward \"bbb\")
      (list pos1 pos2
            (match-beginning 0)
            (match-end 0)
            (match-string 0))))) ",
        expect,
    );
}

#[test]
fn divergence_replace_regexp_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"fooNUMbarNUMbaz\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"foo123bar456baz\")
  (goto-char 1)
  (while (re-search-forward \"[0-9]+\" nil t)
    (replace-match \"NUM\"))
  (buffer-string)) ",
        expect,
    );
}

#[test]
fn divergence_query_replace_no_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"hello world hello\")
  (goto-char 1)
  (perform-replace \"hello\" \"goodbye\" nil nil nil nil nil nil
                   (point-min) (point-max))
  (buffer-string)) ",
        expect,
    );
}

#[test]
fn divergence_extract_words() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"The\" \"quick\" \"brown\" \"fox\" \"jumps\" \"over\" \"the\" \"lazy\" \"dog\") 9 4 \"quick\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(let ((s \"The quick brown fox jumps over the lazy dog\"))
  (list (split-string s \" +\" t)
        (length (split-string s \" +\" t))
        (string-match \"quick\" s)
        (substring s (match-beginning 0) (match-end 0)))) ",
        expect,
    );
}

#[test]
fn divergence_thing_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello\" (1 . 6) 6 \"hello\" (1 . 6))""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"hello world\")
  (goto-char 3)
  (list (thing-at-point 'word)
        (bounds-of-thing-at-point 'word)
        (goto-char 6)
        (thing-at-point 'word)
        (bounds-of-thing-at-point 'word))) ",
        expect,
    );
}

#[test]
fn divergence_forward_word_backward_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 6 12 7)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"hello world foo bar\")
  (goto-char 1)
  (let ((p1 (point)))
    (forward-word 1)
    (let ((p2 (point)))
      (forward-word 1)
      (let ((p3 (point)))
        (backward-word 1)
        (let ((p4 (point)))
          (list p1 p2 p3 p4)))))) ",
        expect,
    );
}

#[test]
fn divergence_kill_ring_yank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"first\" \" second thirdfirst\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"first second third\")
  (goto-char 1)
  (set-mark 1)
  (goto-char 6)
  (kill-region (mark) (point))
  (let ((killed (current-kill 0)))
    (goto-char (point-max))
    (yank)
    (list killed (buffer-string)))) ",
        expect,
    );
}

#[test]
fn divergence_comment_region_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\";; line1\\n;; line2\\n;; line3\" \"line1\\nline2\\nline3\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (setq comment-start \";; \")
  (setq comment-end \"\")
  (insert \"line1\\nline2\\nline3\")
  (comment-region 1 18)
  (let ((s1 (buffer-string)))
    (uncomment-region 1 (point-max))
    (list s1 (buffer-string)))) ",
        expect,
    );
}

#[test]
fn divergence_indent_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 0 4 0 0 19 22)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (setq indent-line-function #'indent-to-left-margin)
  (insert \"  hello\\n    world\\nfoo\")
  (goto-char 1)
  (list (current-indentation)
        (forward-line 1)
        (current-indentation)
        (forward-line 1)
        (current-indentation)
        (line-beginning-position)
        (line-end-position))) ",
        expect,
    );
}

#[test]
fn divergence_whitespace_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"hello   \\nworld  \\nfoo bar\" \"hello\\nworld\\nfoo bar\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"hello   \\nworld  \\nfoo bar\")
  (let ((before (buffer-string)))
    (whitespace-cleanup)
    (list before (buffer-string)))) ",
        expect,
    );
}
