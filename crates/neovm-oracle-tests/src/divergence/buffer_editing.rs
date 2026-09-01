//! Divergence tests: buffer editing semantics.
//!
//! Tests for insert/delete point movement, marker behavior, narrowing
//! interactions, and buffer state management.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_insert_before_markers_moves_non_insertion_type_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (let ((m1 (set-marker (make-marker) 5 (current-buffer)))
        (m2 (set-marker (make-marker) 5 (current-buffer))))
    (set-marker-insertion-type m1 t)
    (set-marker-insertion-type m2 nil)
    (goto-char 5)
    (insert-before-markers "XYZ")
    (list (marker-position m1) (marker-position m2))))"#,
        expect,
    );
}

#[test]
fn divergence_delete_region_marker_at_exclusive_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 \"abhij\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (let ((m (set-marker (make-marker) 8 (current-buffer))))
    (delete-region 3 8)
    (list (marker-position m) (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_delete_region_marker_at_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 \"abhij\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (let ((m (set-marker (make-marker) 3 (current-buffer))))
    (delete-region 3 8)
    (list (marker-position m) (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_delete_region_point_movement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (goto-char 10)
  (delete-region 3 8)
  (point))"#,
        expect,
    );
}

#[test]
fn divergence_delete_region_point_inside_deleted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (goto-char 5)
  (delete-region 3 8)
  (point))"#,
        expect,
    );
}

#[test]
fn divergence_insert_point_movement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcde")
  (goto-char 3)
  (insert "XYZ")
  (point))"#,
        expect,
    );
}

#[test]
fn divergence_replace_buffer_contents_preserves_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 \"abWXYZghij\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (let ((m (set-marker (make-marker) 5 (current-buffer))))
    (goto-char 3)
    (delete-region 3 7)
    (goto-char 3)
    (insert "WXYZ")
    (list (marker-position m) (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_narrow_marker_outside_visible() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 5 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (let ((m (set-marker (make-marker) 2 (current-buffer))))
    (narrow-to-region 5 9)
    (list (marker-position m)
          (point-min)
          (point-max))))"#,
        expect,
    );
}

#[test]
fn divergence_save_excursion_restores_point_and_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (#<buffer *scratch*> 1 \"*scratch*\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((buf1 (get-buffer-create " *test-buf1*"))
        (buf2 (get-buffer-create " *test-buf2*")))
  (unwind-protect
      (progn
        (with-current-buffer buf1 (insert "AAA"))
        (with-current-buffer buf2 (insert "BBB"))
        (with-current-buffer buf1
          (goto-char 3)
          (save-excursion
            (set-buffer buf2)
            (goto-char 3)))
        (list (current-buffer)
              (point)
              (buffer-name (current-buffer))))
    (kill-buffer buf1)
    (kill-buffer buf2)))"#,
        expect,
    );
}

#[test]
fn divergence_save_restriction_restores_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (narrow-to-region 4 7)
  (save-restriction
    (widen)
    (list (point-min) (point-max)))
  (list (point-min) (point-max)))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_modification_tick_after_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (let ((tick1 (buffer-modified-tick))
        (chars-tick1 (buffer-chars-modified-tick)))
    (insert "hello")
    (let ((tick2 (buffer-modified-tick))
          (chars-tick2 (buffer-chars-modified-tick)))
      (list (> tick2 tick1) (> chars-tick2 chars-tick1)))))"#,
        expect,
    );
}

#[test]
fn divergence_indirect_buffer_marker_sharing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((base (get-buffer-create " *test-ind-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base (insert "abcdefgh"))
        (let ((ind (make-indirect-buffer base " *test-ind-ind*")))
          (unwind-protect
              (with-current-buffer ind
                (let ((m (set-marker (make-marker) 5 base)))
                  (with-current-buffer base
                    (goto-char 5)
                    (insert "XXX"))
                  (marker-position m)))
            (kill-buffer ind))))
    (kill-buffer base)))"#,
        expect,
    );
}

#[test]
fn divergence_delete_region_insertion_type_marker_collapsed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 4 \"abcij\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefghij")
  (let ((m-front (set-marker (make-marker) 7 (current-buffer)))
        (m-back  (set-marker (make-marker) 7 (current-buffer))))
    (set-marker-insertion-type m-front t)
    (set-marker-insertion-type m-back nil)
    (delete-region 4 9)
    (list (marker-position m-front)
          (marker-position m-back)
          (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_multibyte_insert_char_count_vs_byte_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 4 1 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "ābcd")  ;; ā=2 bytes, 1 char; total 5 bytes, 4 chars
  (list (point) (buffer-size) (position-bytes 1) (position-bytes 2)))"#,
        expect,
    );
}

#[test]
fn divergence_multibyte_delete_and_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 \"ābef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "ābcdef")  ;; ā=2bytes
  (goto-char 3)      ;; at 'b', char pos 3
  (delete-char 2)    ;; delete "cd"
  (list (point) (buffer-string)))"#,
        expect,
    );
}
