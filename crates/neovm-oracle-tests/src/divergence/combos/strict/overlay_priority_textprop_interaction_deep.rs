//! Strict combo oracle probes, batch 109: overlay priority ordering with
//! text-property interaction, overlay evaporate with text-property
//! preservation, and multiple-overlay merge at same position.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_s3_overlay_priority_and_face_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 2 3) (bold underline italic) 3 italic italic bold nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r####"
(with-temp-buffer
  (insert "abcdefghij")
  (let ((o1 (make-overlay 2 6))
        (o2 (make-overlay 4 8))
        (o3 (make-overlay 3 7)))
    (overlay-put o1 'priority 1)
    (overlay-put o2 'priority 3)
    (overlay-put o3 'priority 2)
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (overlay-put o3 'face 'underline)
    (list (mapcar (lambda (o) (overlay-get o 'priority)) (overlays-at 5))
          (mapcar (lambda (o) (overlay-get o 'face)) (overlays-at 5))
          (length (overlays-at 5))
          (get-char-property 5 'face)
          (get-char-property 4 'face)
          (get-char-property 2 'face)
          (get-char-property 8 'face))))
"####,
        expect,
    );
}

#[test]
fn div_s3_overlay_evaporate_with_textprops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(with-temp-buffer
  (insert "abcdef")
  (add-text-properties 2 5 '(face bold))
  (let ((o (make-overlay 3 4)))
    (overlay-put o 'priority 1)
    (overlay-put o 'face 'italic)
    (overlay-put o 'evaporate t)
    (delete-region 3 4)
    (list (buffer-string)
          (get-text-property 2 'face)
          (get-text-property 3 'face)
          (length (overlays-in 1 5))
          (null (overlay-buffer o))))
"####,
        expect,
    );
}

#[test]
fn div_s3_overlay_before_after_string_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument buffer-or-string-p face)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(with-temp-buffer
  (insert "abcdef")
  (let ((o (make-overlay 2 4)))
    (overlay-put o 'before-string (propertize "<" 'face 'bold))
    (overlay-put o 'after-string (propertize ">" 'face 'bold))
    (list (buffer-string)
          (overlay-get o 'before-string)
          (overlay-get o 'after-string)
          (get-text-property 0 (overlay-get o 'before-string) 'face))))
"####,
        expect,
    );
}

#[test]
fn div_s3_overlay_invisibility_with_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"visible INVISIBLE visible INVISIBLE visible\" \"visible INVISIBLE visible INVISIBLE visible\" (#<overlay in no buffer>) nil 8 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r####"
(with-temp-buffer
  (insert "visible INVISIBLE visible INVISIBLE visible")
  (let ((o1 (make-overlay 8 17))
        (o2 (make-overlay 26 35)))
    (overlay-put o1 'invisible t)
    (overlay-put o2 'invisible t)
    (list (buffer-substring 1 (point-max))
          (buffer-substring-no-properties 1 (point-max))
          (overlays-at 10)
          (overlays-at 20)
          (next-overlay-change 1)
          (next-single-char-property-change 1 'invisible))))
"####,
        expect,
    );
}
