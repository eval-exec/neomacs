//! Divergence tests: real buffer editing behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_insert_delete_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"The  slowquick brown fox\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"The quick brown fox\")
  (goto-char 5)
  (insert \" VERY\")
  (list (buffer-string) (point))
  (delete-region 5 10)
  (list (buffer-string) (point))
  (goto-char 5)
  (insert \" slow\")
  (buffer-string)) ",
        expect,
    );
}

#[test]
fn divergence_buffer_substring_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold bold nil italic italic nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'face 'italic)
  (list (get-text-property 1 'face)
        (get-text-property 3 'face)
        (get-text-property 5 'face)
        (get-text-property 6 'face)
        (get-text-property 8 'face)
        (get-text-property 10 'face))) ",
        expect,
    );
}

#[test]
fn divergence_replace_match_backref() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello\" \"world\" 1 6 7 12)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"hello world\")
  (goto-char 1)
  (re-search-forward \"\\\\([a-z]+\\\\) \\\\([a-z]+\\\\)\")
  (list (match-string 1)
        (match-string 2)
        (match-beginning 1)
        (match-end 1)
        (match-beginning 2)
        (match-end 2))) ",
        expect,
    );
}

#[test]
fn divergence_narrow_edit_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ABxyCDEFGHIJ\" 1 13)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (narrow-to-region 3 7)
  (goto-char (point-min))
  (insert \"xy\")
  (list (buffer-string) (point-min) (point-max))
  (widen)
  (list (buffer-string) (point-min) (point-max))) ",
        expect,
    );
}

#[test]
fn divergence_overlay_layered_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold italic 1 10 2)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (put-text-property 1 11 'face 'default)
  (let ((ov1 (make-overlay 1 6))
        (ov2 (make-overlay 4 11)))
    (overlay-put ov1 'face 'bold)
    (overlay-put ov2 'face 'italic)
    (overlay-put ov1 'priority 1)
    (overlay-put ov2 'priority 10)
    (list (overlay-get ov1 'face)
          (overlay-get ov2 'face)
          (overlay-get ov1 'priority)
          (overlay-get ov2 'priority)
          (length (overlays-in 1 11))))) ",
        expect,
    );
}

#[test]
fn divergence_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 11 38)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"AAA\")
  (undo-boundary)
  (insert \"BBB\")
  (undo-boundary)
  (insert \"CCC\")
  (let ((s1 (buffer-string)))
    (primitive-undo 1 buffer-undo-list)
    (let ((s2 (buffer-string)))
      (primitive-undo 1 buffer-undo-list)
      (list s1 s2 (buffer-string)))))) ",
        expect,
    );
}

#[test]
fn divergence_marker_after_multiple_edits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 6 6 \"123CDEFGHXYZIJ\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (let ((m (make-marker)))
    (set-marker m 5 (current-buffer))
    (goto-char 3)
    (insert \"123\")
    (let ((p1 (marker-position m)))
      (delete-region 1 3)
      (let ((p2 (marker-position m)))
        (goto-char 10)
        (insert \"XYZ\")
        (list p1 p2 (marker-position m)
              (buffer-string)))))) ",
        expect,
    );
}

#[test]
fn divergence_textprop_after_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"foo quux baz\" 0 4 (test-prop original) 8 12 (test-prop original)) original nil original original)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"foo bar baz\")
  (put-text-property 1 12 'test-prop 'original)
  (goto-char 1)
  (search-forward \"bar\")
  (replace-match \"quux\")
  (list (buffer-string)
        (get-text-property 1 'test-prop)
        (get-text-property 5 'test-prop)
        (get-text-property 9 'test-prop)
        (get-text-property 12 'test-prop))) ",
        expect,
    );
}

#[test]
fn divergence_case_change_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 9 38)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"hello WORLD\")
  (narrow-to-region 1 5)
  (upcase-region (point-min) (point-max))
  (let ((s1 (buffer-string)))
    (widen)
    (let ((s2 (buffer-string)))
      (downcase-region 7 12)
      (list s1 s2 (buffer-string)))))) ",
        expect,
    );
}

#[test]
fn divergence_rectangle_extract() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"BC\") 1 (\"AB\"))""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGH\\nIJKLMNOP\\nQRSTUVWX\")
  (list (extract-rectangle 2 4)
        (length (extract-rectangle 2 4))
        (extract-rectangle 1 3))) ",
        expect,
    );
}
