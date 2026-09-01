//! Strict combo oracle probes, batch 116: window-configuration roundtrip with
//! markers/point/dedicated state, indirect-buffer narrowing+markers, field
//! motion with text-property fields, and save-excursion/save-restriction
//! deep nesting.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_t0_window_config_roundtrip_markers_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((b1 (get-buffer-create " *probe-wc-a*"))
      (b2 (get-buffer-create " *probe-wc-b*"))
      (m (make-marker)))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b1)
        (with-current-buffer b1 (insert "line one\nline two\nline three"))
        (set-marker m 10 b1)
        (goto-char 10)
        (let ((cfg (current-window-configuration))
              (w2 (split-window nil nil 'below)))
          (set-window-buffer w2 b2)
          (with-current-buffer b2 (insert "other buffer content"))
          (select-window w2)
          (set-window-configuration cfg)
          (list (count-windows)
                (eq (current-buffer) b1)
                (point)
                (marker-position m)
                (buffer-name (window-buffer (selected-window))))))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))
    (delete-other-windows)))
"####,
    );
}

#[test]
fn div_t0_indirect_buffer_narrow_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let* ((base (get-buffer-create " *probe-ind-base*"))
       (ind (make-indirect-buffer base " *probe-ind*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (insert "0123456789ABCDEFGHIJ")
          (setq-local probe-var 'in-base))
        (with-current-buffer ind
          (narrow-to-region 5 15)
          (list (buffer-string)
                (point-min)
                (point-max)
                (eq (buffer-base-buffer ind) base)
                (buffer-base-buffer base)))
        (with-current-buffer base
          (list (point-min)
                (point-max)
                (buffer-string)
                (widen)
                (buffer-string))))
    (when (buffer-live-p ind) (kill-buffer ind))
    (when (buffer-live-p base) (kill-buffer base))))
"####,
    );
}

#[test]
fn div_t0_field_motion_text_property_fields() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(with-temp-buffer
  (insert "AAAA  BBBB  CCCC")
  (put-text-property 1 4 'field 'f1)
  (put-text-property 7 10 'field 'f2)
  (put-text-property 13 16 'field 'f3)
  (goto-char 2)
  (list (field-beginning 2)
        (field-end 2)
        (field-string 2)
        (constrain-to-field 15 2)
        (line-beginning-position)
        (field-beginning 8)
        (field-end 8)))
"####,
    );
}

#[test]
fn div_t0_save_excursion_restriction_deep_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(with-temp-buffer
  (insert "0123456789ABCDEFGHIJ")
  (let (result)
    (save-excursion
      (save-restriction
        (narrow-to-region 5 15)
        (goto-char 8)
        (push (list (point) (point-min) (point-max) (buffer-string)) result)
        (save-excursion
          (goto-char (point-min))
          (push (point) result))
        (push (list (point) (buffer-string)) result)))
    (push (list (point) (point-min) (point-max) (buffer-string)) result)
    (nreverse result)))
"####,
    );
}

#[test]
fn div_t0_window_dedicated_persist_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((b (get-buffer-create " *probe-ded*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (set-window-dedicated-p nil 'test)
        (let ((cfg (current-window-configuration))
              (ded-before (window-dedicated-p)))
          (set-window-dedicated-p nil nil)
          (set-window-configuration cfg)
          (list ded-before
                (window-dedicated-p)
                (count-windows))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"####,
    );
}
