//! Complex combo batch 221 — `align` / `align-regexp` / `sort-fields` /
//! `sort-numeric-fields` / `comment-region` / `uncomment-region` /
//! `comment-or-uncomment-region` / `comment-box` operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx221_align_region_columns_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a\t= 1\\nbb\t= 22\\nccc\t= 333\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "a = 1\nbb = 22\nccc = 333\n")
      (align-regexp (point-min) (point-max) "\\(\\s-*\\)=")
      (buffer-string))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx221_comment_region_uncomment_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"
(with-temp-buffer
  (insert "line one\nline two\nline three\n")
  (comment-region (point-min) (point-max))
  (let ((commented (buffer-string)))
    (uncomment-region (point-min) (point-max))
    (let ((uncommented (buffer-string)))
      (list commented uncommented))))
"##,
    );
}

#[test]
fn div_cx221_comment_or_uncomment_region_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"
(with-temp-buffer
  (insert "line one\nline two\n")
  (comment-or-uncomment-region (point-min) (point-max))
  (let ((after-first (buffer-string)))
    (comment-or-uncomment-region (point-min) (point-max))
    (let ((after-second (buffer-string)))
      (list after-first after-second))))
"##,
    );
}

#[test]
fn div_cx221_sort_lines_numeric_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"1\\n2\\n3\\n10\\n20\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "10\n2\n1\n20\n3\n")
  (sort-numeric-fields 1 (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx221_sort_lines_alpha() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"apple\\nbanana\\ncherry\\ndate\\nelderberry\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "banana\napple\ncherry\ndate\nelderberry\n")
  (sort-lines nil (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx221_sort_fields_by_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"b 1 y\\nc 2 z\\na 3 x\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "a 3 x\nb 1 y\nc 2 z\n")
      (sort-fields 2 (point-min) (point-max))
      (buffer-string))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx221_sort_columns_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"bravo charlie delta\\nzebra alpha mike\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "zebra alpha mike\nbravo charlie delta\n")
      (sort-columns nil (point-min) (point-max))
      (buffer-string))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx221_delete_duplicate_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"alpha\\nbeta\\ngamma\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "alpha\nbeta\nalpha\ngamma\nbeta\nalpha\n")
      (delete-duplicate-lines (point-min) (point-max))
      (buffer-string))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx221_comment_box_wrap_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "text to box\nsecond line\n")
      (comment-box (point-min) (point-max) 2)
      (buffer-string))
  (error (list :errored (car e))))
"##,
    );
}

#[test]
fn div_cx221_align_sort_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "b = 2\na = 1\nc = 3\n")
      (put-text-property 1 5 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 3 12)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 1 18)
        (sort-lines nil (point-min) (point-max))
        (let ((state (list (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
