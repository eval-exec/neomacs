//! Divergence tests: complex search + marker + undo combinations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_markers_after_regex_replace_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"VAL:20 VAL:40 VAL:60 END\" 1 8 21 21)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"NUM:10 NUM:20 NUM:30 END\")
  (let ((m1 (make-marker)) (m2 (make-marker)) (m3 (make-marker)))
    (set-marker m1 5)
    (set-marker m2 13)
    (set-marker m3 21)
    (goto-char 1)
    (while (re-search-forward \"NUM:\\\\([0-9]+\\\\)\" nil t)
      (replace-match (format \"VAL:%d\" (* 2 (string-to-number (match-string 1)))) t))
    (list (buffer-string)
          (marker-position m1) (marker-position m2) (marker-position m3)
          (point)))) ",
        expect,
    );
}

#[test]
fn divergence_undo_marker_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 1 3 \"AB123CDE\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDE\")
  (let ((m (make-marker)))
    (set-marker m 3)
    (undo-boundary)
    (goto-char 3)
    (insert \"123\")
    (let ((p1 (marker-position m)))
      (undo-boundary)
      (delete-region 1 3)
      (let ((p2 (marker-position m)))
        (primitive-undo 1 buffer-undo-list)
        (list p1 p2 (marker-position m) (buffer-string)))))) ",
        expect,
    );
}

#[test]
fn divergence_match_data_with_markers_swap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"foo\" nil) \"foo\" \"bar\" 1 8)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"foo-bar-baz-qux\")
  (goto-char 1)
  (re-search-forward \"\\\\([a-z]+\\\\)-\\\\([a-z]+\\\\)\")
  (let ((saved-match (match-data t)))
    (list (match-string 1) (match-string 2))
    (goto-char 1)
    (re-search-forward \"\\\\([a-z]+\\\\)\")
    (let ((new-match (list (match-string 1) (match-string 2))))
      (set-match-data saved-match)
      (list new-match
            (match-string 1) (match-string 2)
            (match-beginning 0) (match-end 0))))) ",
        expect,
    );
}

#[test]
fn divergence_narrowed_search_replace_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((\"XXXX-XXXX-XXX\" 14) \"AAA-XXXX-XXXX-XXXD-EEEE\" 14)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"AAA-BBBB-CCCC-DDDD-EEEE\")
  (let ((m (make-marker)))
    (set-marker m 14)
    (narrow-to-region 5 18)
    (goto-char (point-min))
    (while (re-search-forward \"\\\\(B+\\\\|C+\\\\|D+\\\\)\" nil t)
      (replace-match (make-string (length (match-string 1)) ?X) t))
    (let ((result (list (buffer-string) (marker-position m))))
      (widen)
      (list result (buffer-string) (marker-position m))))) ",
        expect,
    );
}

#[test]
fn divergence_multiline_backref_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (2 \"start\\nline1 NUM=420\\nline2 NUM=990\\nend\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"start\\nline1 VALUE=42\\nline2 VALUE=99\\nend\")
  (goto-char 1)
  (let ((count 0))
    (while (re-search-forward \"VALUE=\\\\([0-9]+\\\\)\" nil t)
      (cl-incf count)
      (replace-match (format \"NUM=%d\" (* 10 (string-to-number (match-string 1)))) t))
    (list count (buffer-string)))) ",
        expect,
    );
}

#[test]
fn divergence_save_excursion_restriction_marker_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((12 9 12) \"BBB-CCC-DDD-\" 9 5 17)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"AAA-BBB-CCC-DDD-EEE\")
  (let ((m (make-marker)))
    (set-marker m 9)
    (narrow-to-region 5 17)
    (let ((result
           (save-excursion
             (save-restriction
               (widen)
               (goto-char 1)
               (re-search-forward \"CCC\")
               (list (point) (match-beginning 0) (match-end 0))))))
      (list result
            (buffer-string)
            (marker-position m)
            (point-min) (point-max))))) ",
        expect,
    );
}

#[test]
fn divergence_kill_yank_marker_preservation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ABCGHIJ\" \"ABCGHIJDEF\" 4 4 4 4)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (let ((m1 (make-marker)) (m2 (make-marker)))
    (set-marker m1 4)
    (set-marker m2 7)
    (undo-boundary)
    (kill-region 4 7)
    (let ((s1 (buffer-string))
          (p1 (marker-position m1))
          (p2 (marker-position m2)))
      (goto-char (point-max))
      (undo-boundary)
      (yank)
      (list s1 (buffer-string) p1 p2
            (marker-position m1) (marker-position m2))))) ",
        expect,
    );
}

#[test]
fn divergence_marker_insertion_type_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 3 t nil \"ABXYZCDE\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDE\")
  (let ((m1 (make-marker)) (m2 (make-marker)))
    (set-marker m1 3)
    (set-marker-insertion-type m1 t)
    (set-marker m2 3)
    (set-marker-insertion-type m2 nil)
    (goto-char 3)
    (insert \"XYZ\")
    (list (marker-position m1)
          (marker-position m2)
          (marker-insertion-type m1)
          (marker-insertion-type m2)
          (buffer-string)))) ",
        expect,
    );
}

#[test]
fn divergence_regex_word_boundaries_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"hello world\\nfoo bar\\nbaz qux\")
  (let ((matches nil))
    (goto-char 1)
    (while (re-search-forward \"\\\\\\\\<\\\\([a-z]+\\\\)\\\\\\\\>\" nil t)
      (push (list (match-string 1) (match-beginning 0) (line-number-at-pos (match-beginning 0))) matches))
    (nreverse matches))) ", expect);
}

#[test]
fn divergence_overlay_dont_track_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 3 10 \"ABCD123EFGHIJ\" tracked)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (let ((m (set-marker (make-marker) 5))
        (ov (make-overlay 3 7)))
    (overlay-put ov 'test 'tracked)
    (set-marker-insertion-type m t)
    (goto-char 5)
    (insert \"123\")
    (list (marker-position m)
          (overlay-start ov) (overlay-end ov)
          (buffer-string)
          (overlay-get ov 'test)))) ",
        expect,
    );
}
