//! Strict combo oracle probes, batch 106: undo + marker + text-property
//! interaction — undo restores markers/text-props, indirect-buffer undo,
//! undo-limits, and marker-insertion-type during undo.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_s0_undo_restores_markers_and_textprops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(let ((m (make-marker))
      (result nil))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "abcdef")
    (set-marker m 4)
    (add-text-properties 2 5 '(face bold))
    (setq result (list (marker-position m) (get-text-property 3 'face)))
    (undo-boundary)
    (delete-region 2 4)
    (setq result (append result (list (marker-position m) (buffer-string))))
    (undo)
    (append result (list (marker-position m) (get-text-property 3 'face) (buffer-string)))))
"####,
        expect,
    );
}

#[test]
fn div_s0_undo_marker_insertion_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 4 \"abXcdef\" (1 1 \"\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(let ((m1 (make-marker))
      (m2 (make-marker)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "abcdef")
    (set-marker m1 3)
    (set-marker-insertion-type m1 nil)
    (set-marker m2 3)
    (set-marker-insertion-type m2 t)
    (goto-char 3)
    (undo-boundary)
    (insert "X")
    (list (marker-position m1) (marker-position m2)
          (buffer-string)
          (progn (undo) (list (marker-position m1) (marker-position m2) (buffer-string))))))
"####,
        expect,
    );
}

#[test]
fn div_s0_undo_buffer_undo_list_length_and_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-count-if)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "a")
  (undo-boundary)
  (insert "b")
  (undo-boundary)
  (insert "c")
  (let ((len (length buffer-undo-list))
        (boundaries (cl-count-if (lambda (e) (null e)) buffer-undo-list)))
    (undo)
    (list len boundaries (length buffer-undo-list) (buffer-string))))
"####,
        expect,
    )
}

#[test]
fn div_s0_undo_redo_with_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\" world\" nil \"\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello world")
  (add-text-properties 1 5 '(face bold))
  (undo-boundary)
  (delete-region 1 6)
  (let ((after-delete (buffer-string))
        (props-remain (text-properties-at 1)))
    (undo)
    (let ((after-undo (buffer-string))
          (props-restored (get-text-property 1 'face)))
      (list after-delete props-remain after-undo props-restored))))
"####,
        expect,
    );
}
