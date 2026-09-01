//! Strict combo oracle probes, batch 119: buffer modification hooks with
//! invisible text + narrowing + overlays combo, marker tracking through
//! undo with indirect buffers, and text-property merge edge cases
//! (front-sticky + rear-nonsticky on same boundary).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_t3_buffer_mod_hooks_invisible_narrow_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((log nil))
  (with-temp-buffer
    (insert "visible1 INVISIBLE visible2")
    (let ((o (make-overlay 9 17)))
      (overlay-put o 'invisible t))
    (add-text-properties 1 8 '(face bold))
    (add-hook 'before-change-functions
              (lambda (beg end) (push (list 'before beg end) log))
              nil t)
    (add-hook 'after-change-functions
              (lambda (beg end len) (push (list 'after beg end len) log))
              nil t)
    (narrow-to-region 1 25)
    (goto-char 20)
    (insert "X")
    (delete-region 2 5)
    (let ((inhibit-modification-hooks t)
          (insert "Y"))
      (push 'inhibited log))
    (list (buffer-string)
          (buffer-substring-no-properties 1 20)
          (length (nreverse log))
          (point-min)
          (point-max))))
"####,
    );
}

#[test]
fn div_t3_marker_undo_indirect_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let* ((base (get-buffer-create " *probe-mu-base*"))
       (ind (make-indirect-buffer base " *probe-mu-ind*"))
       (m-base (make-marker))
       (m-ind (make-marker)))
  (unwind-protect
      (progn
        (with-current-buffer base
          (buffer-enable-undo)
          (insert "abcdefghijklmno")
          (set-marker m-base 5)
          (undo-boundary))
        (with-current-buffer ind
          (set-marker m-ind 10)
          (narrow-to-region 3 13)
          (goto-char 7)
          (undo-boundary)
          (delete-region 5 8)
          (list (marker-position m-base)
                (marker-position m-ind)
                (buffer-string)
                (eq (marker-buffer m-base) base)
                (eq (marker-buffer m-ind) ind)))
        (with-current-buffer base
          (undo)
          (list (marker-position m-base)
                (buffer-string))))
    (when (buffer-live-p ind) (kill-buffer ind))
    (when (buffer-live-p base) (kill-buffer base))))
"####,
    );
}

#[test]
fn div_t3_text_property_merge_boundary_stickiness() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(with-temp-buffer
  (insert "abcdefghijklmno")
  (add-text-properties 1 5 '(face bold front-sticky nil rear-nonsticky (face)))
  (add-text-properties 6 10 '(face italic front-sticky (face) rear-nonsticky nil))
  (add-text-properties 11 15 '(face underline))
  (goto-char 5)
  (insert "X")
  (goto-char 11)
  (insert "Y")
  (list (buffer-string)
        (text-properties-at 4)
        (text-properties-at 6)
        (text-properties-at 7)
        (text-properties-at 11)
        (text-properties-at 13)
        (get-text-property 6 'face)
        (get-text-property 7 'face)))
"####,
    );
}

#[test]
fn div_t3_overlay_invisible_search_replace_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(with-temp-buffer
  (insert "find REPLACE1 hidden REPLACE2 find")
  (let ((o (make-overlay 16 24)))
    (overlay-put o 'invisible t))
  (goto-char 1)
  (let (positions)
    (while (search-forward "REPLACE" nil t)
      (push (match-beginning 0) positions)
      (replace-match "DONE"))
    (list (nreverse positions)
          (buffer-string)
          (buffer-substring 1 30)
          (overlays-in 1 30)
          (length (overlays-in 1 30)))))
"####,
    );
}
