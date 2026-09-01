//! Divergence tests: text property and overlay edge cases.
//!
//! Tests for text property stickiness, overlay evaporate,
//! property boundary semantics, and char-property precedence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_text_prop_empty_range_noop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdef")
  (put-text-property 3 3 'face 'bold)
  (list (get-text-property 3 'face)
        (get-text-property 2 'face)
        (get-text-property 4 'face)))"#,
        expect,
    );
}

#[test]
fn divergence_text_prop_at_buffer_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abc")
  (put-text-property 1 3 'face 'bold)
  (list (get-text-property 0 'face)
        (get-text-property 1 'face)
        (get-text-property 2 'face)
        (get-text-property 3 'face)))"#,
        expect,
    );
}

#[test]
fn divergence_next_property_change_at_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 4 7 nil nil 4 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (put-text-property 1 4 'face 'bold)
  (put-text-property 4 7 'face 'italic)
  (list (next-property-change 1)
        (next-property-change 3)
        (next-property-change 4)
        (next-property-change 7)
        (previous-property-change 4)
        (previous-property-change 7)
        (previous-property-change 1)))"#,
        expect,
    );
}

#[test]
fn divergence_next_single_property_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 5 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (put-text-property 1 4 'face 'bold)
  (put-text-property 5 8 'face 'italic)
  (list (next-single-property-change 1 'face)
        (next-single-property-change 4 'face)
        (next-single-property-change 5 'face)))"#,
        expect,
    );
}

#[test]
fn divergence_remove_text_properties_middle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold nil bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (put-text-property 1 8 'face 'bold)
  (remove-text-properties 3 6 '(face))
  (list (get-text-property 2 'face)
        (get-text-property 4 'face)
        (get-text-property 7 'face)))"#,
        expect,
    );
}

#[test]
fn divergence_set_text_properties_replace_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold italic italic nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (put-text-property 1 5 'face 'bold)
  (set-text-properties 3 7 '(face italic))
  (list (get-text-property 2 'face)
        (get-text-property 4 'face)
        (get-text-property 6 'face)
        (get-text-property 8 'face)))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_evaporate_on_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (let ((ov (make-overlay 3 6)))
    (overlay-put ov 'evaporate t)
    (overlay-put ov 'face 'bold)
    (delete-region 3 6)
    (list (overlay-buffer ov)
          (length (overlays-in 1 8)))))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_evaporate_after_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (let ((ov (make-overlay 3 6)))
    (overlay-put ov 'evaporate t)
    (move-overlay ov 4 4)
    (list (overlay-buffer ov)
          (length (overlays-in 1 8)))))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_front_advance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (let ((ov (make-overlay 3 6 nil t nil)))
    (goto-char 3)
    (insert "XXX")
    (list (overlay-start ov) (overlay-end ov))))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_rear_advance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (let ((ov (make-overlay 3 6 nil nil t)))
    (goto-char 6)
    (insert "XXX")
    (list (overlay-start ov) (overlay-end ov))))"#,
        expect,
    );
}

#[test]
fn divergence_get_char_property_overlay_vs_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (text-face overlay-face text-face text-face)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (put-text-property 1 8 'face 'text-face)
  (let ((ov (make-overlay 3 6)))
    (overlay-put ov 'face 'overlay-face)
    (list (get-text-property 4 'face)
          (get-char-property 4 'face)
          (get-char-property 2 'face)
          (get-char-property 7 'face))))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_priority_with_same_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK italic""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "abcdefgh")
  (let ((a (make-overlay 2 7))
        (b (make-overlay 2 5)))
    (overlay-put a 'face 'bold)
    (overlay-put a 'priority 1)
    (overlay-put b 'face 'italic)
    (overlay-put b 'priority 10)
    (get-char-property 3 'face)))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_cross_buffer_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 3 \" *test-ov-b*\" 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((buf-a (get-buffer-create " *test-ov-a*"))
        (buf-b (get-buffer-create " *test-ov-b*")))
  (unwind-protect
      (progn
        (with-current-buffer buf-a (insert "abcdef"))
        (with-current-buffer buf-b (insert "uvwxyz"))
        (let ((ov (make-overlay 2 4 buf-a)))
          (move-overlay ov 1 3 buf-b)
          (list (overlay-start ov)
                (overlay-end ov)
                (buffer-name (overlay-buffer ov))
                (with-current-buffer buf-a
                  (length (overlays-in 1 6))))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)))"#,
        expect,
    );
}

#[test]
fn divergence_object_intervals_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((0 3 (face bold)) (3 6 (face italic)) (6 8 nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s (copy-sequence "abcdefgh")))
  (put-text-property 0 3 'face 'bold s)
  (put-text-property 3 6 'face 'italic s)
  (object-intervals s))"#,
        expect,
    );
}
