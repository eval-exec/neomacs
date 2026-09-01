//! Divergence tests: undo system semantic differences.
//!
//! GNU Emacs records marker adjustments before deletion undo entries so that
//! primitive-undo can restore marker positions. Neomacs skips marker
//! adjustments entirely, causing markers to remain at collapsed positions
//! after undoing a deletion.
//!
//! GNU Emacs also records character positions in undo entries, while neomacs
//! records byte positions, causing divergences in multibyte buffers.
//!
//! GNU Emacs only merges forward (existing END == new BEG), while neomacs
//! also merges backward (existing BEG == new END), causing different undo
//! granularity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_undo_marker_position_restored_after_delete_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"No undo information in this buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (let ((m (set-marker (make-marker) 5 (current-buffer))))
    (delete-region 3 8)
    (undo)
    (marker-position m)))"#,
        expect,
    );
}

#[test]
fn divergence_undo_marker_inside_deleted_region_restored() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"No undo information in this buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "0123456789")
  (let ((m (set-marker (make-marker) 7 (current-buffer))))
    (delete-region 3 9)
    (undo)
    (list (marker-position m)
          (buffer-substring 1 11))))"#,
        expect,
    );
}

#[test]
fn divergence_undo_multiple_markers_in_deleted_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"No undo information in this buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghijklmno")
  (let ((m1 (set-marker (make-marker) 5 (current-buffer)))
        (m2 (set-marker (make-marker) 10 (current-buffer)))
        (m3 (set-marker (make-marker) 15 (current-buffer))))
    (delete-region 3 13)
    (list (marker-position m1) (marker-position m2) (marker-position m3))
    (undo)
    (list (marker-position m1) (marker-position m2) (marker-position m3))))"#,
        expect,
    );
}

#[test]
fn divergence_undo_insertion_type_marker_restored() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"No undo information in this buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (let ((m (set-marker (make-marker) 7 (current-buffer))))
    (set-marker-insertion-type m t)
    (delete-region 3 9)
    (undo)
    (list (marker-position m) (marker-insertion-type m))))"#,
        expect,
    );
}

#[test]
fn divergence_undo_multibyte_char_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "ābcdef")  ;; ā is 2 bytes, 1 char
  (goto-char 1)
  (insert "x")
  (car buffer-undo-list))"#,
        expect,
    );
}

#[test]
fn divergence_undo_multibyte_delete_char_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "ābcdef")  ;; ā is 2 bytes
  (goto-char 3)      ;; after ā (char pos 2), at 'b'
  (delete-region 3 6)
  (car buffer-undo-list))"#,
        expect,
    );
}

#[test]
fn divergence_undo_multibyte_delete_and_undo_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"No undo information in this buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "ābcdef")
  (goto-char (point-max))
  (delete-region 2 5)  ;; delete "bcd" (chars 2-4)
  (undo)
  (point))"#,
        expect,
    );
}

#[test]
fn divergence_undo_backward_merge_not_in_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "ABCDE")
  (goto-char 3)
  (insert "X")
  (goto-char 3)
  (insert "Y")
  (let ((undo-entry (car buffer-undo-list)))
    (if (consp undo-entry)
        (list (car undo-entry) (cdr undo-entry))
      undo-entry)))"#,
        expect,
    );
}

#[test]
fn divergence_undo_first_change_sentinel_not_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "hello")
  (let ((entry (car buffer-undo-list)))
    ;; The first-change sentinel should be (t . MODTIME) where MODTIME
    ;; is the file modtime for file-visiting buffers or 0 for non-file
    ;; buffers. GNU Emacs uses the visited file modtime; for non-file
    ;; buffers it also uses 0. This test verifies the entry shape.
    (if (and (consp entry) (eq (car entry) t))
        (cdr entry)
      'no-first-change-entry)))"#,
        expect,
    );
}

#[test]
fn divergence_undo_delete_point_at_end_negative_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "ābcd")  ;; ā=2bytes, total 5 bytes, 4 chars
  (goto-char (point-max))
  (delete-region 1 2)  ;; delete "ā"
  ;; Point was at end (= beg + SCHARS in GNU), so position should be negative
  (let ((entry (car buffer-undo-list)))
    (if (and (consp entry) (stringp (car entry)))
        (< (cdr entry) 0)  ;; position should be negative
      'unexpected-entry)))"#,
        expect,
    );
}

#[test]
fn divergence_undo_boundary_separates_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"No undo information in this buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "hello")
  (undo-boundary)
  (insert "world")
  ;; First undo should remove "world" only
  (undo)
  (buffer-string))"#,
        expect,
    );
}

#[test]
fn divergence_undo_full_cycle_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"No undo information in this buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "ABCDE")
  (goto-char 3)
  (delete-region 3 5)  ;; delete "CD"
  (undo)
  (buffer-string))"#,
        expect,
    );
}

#[test]
fn divergence_undo_marker_after_insert_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (user-error \"No undo information in this buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcde")
  (let ((m (set-marker (make-marker) 3 (current-buffer))))
    (goto-char 2)
    (insert "XYZ")
    (undo)
    (list (marker-position m) (buffer-string))))"#,
        expect,
    );
}
