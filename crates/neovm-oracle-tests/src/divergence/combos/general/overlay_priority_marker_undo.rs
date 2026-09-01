//! Deep combo: overlay priority × overlay start/end × overlay properties ×
//! marker × text-prop × undo × buffer-local × narrow × insert × delete.
//!
//! Stresses overlay priority and ordering: overlapping overlays with
//! different priorities, overlay property lookup order, and overlay
//! position tracking through edits. Overlay priority is tricky because
//! it affects which overlay's properties are visible at a given position.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_overlay_priority_overlap_marker_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-op")))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAAAAAAAAAA")
      (put-text-property 1 21 'base 'all)
      (let ((ov-low  (make-overlay 1 20))
            (ov-mid  (make-overlay 5 15))
            (ov-high (make-overlay 8 12))
            (m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t)))
        (overlay-put ov-low 'prio 'low)
        (overlay-put ov-low 'priority 1)
        (overlay-put ov-mid 'prio 'mid)
        (overlay-put ov-mid 'priority 2)
        (overlay-put ov-high 'prio 'high)
        (overlay-put ov-high 'priority 3)
        (undo-boundary)
        (goto-char 10)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-get (car (overlays-at 1)) 'prio)
                           (overlay-get (car (overlays-at 6)) 'prio)
                           (overlay-get (car (overlays-at 9)) 'prio)
                           (overlay-get (car (overlays-at 17)) 'prio)
                           (get-text-property 1 'base)
                           (get-text-property 10 'base))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-get (car (overlays-at 1)) 'prio)
                                (overlay-get (car (overlays-at 6)) 'prio)
                                (overlay-get (car (overlays-at 9)) 'prio)
                                (overlay-get (car (overlays-at 17)) 'prio)
                                (get-text-property 1 'base)
                                (get-text-property 10 'base)
                                (get-text-property 15 'base))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_overlay_priority_delete_region_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-opdel")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (put-text-property 21 25 'grp 'e)
      (let ((ov1 (make-overlay 1 10))
            (ov2 (make-overlay 6 20))
            (ov3 (make-overlay 11 25))
            (m1 (copy-marker 5 nil))
            (m2 (copy-marker 15 t)))
        (overlay-put ov1 'zone 'first)
        (overlay-put ov1 'priority 1)
        (overlay-put ov2 'zone 'middle)
        (overlay-put ov2 'priority 2)
        (overlay-put ov3 'zone 'last)
        (overlay-put ov3 'priority 3)
        (undo-boundary)
        (delete-region 6 20)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (and (overlay-start ov1) (overlay-end ov1))
                           (and (overlay-start ov2) (overlay-end ov2))
                           (and (overlay-start ov3) (overlay-end ov3))
                           (overlay-get (car (overlays-at 1)) 'zone)
                           (overlay-get (car (overlays-at 6)) 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (and (overlay-start ov1) (overlay-end ov1))
                                (and (overlay-start ov2) (overlay-end ov2))
                                (and (overlay-start ov3) (overlay-end ov3))
                                (overlay-get (car (overlays-at 1)) 'zone)
                                (overlay-get (car (overlays-at 8)) 'zone)
                                (overlay-get (car (overlays-at 15)) 'zone)
                                (overlay-get (car (overlays-at 22)) 'zone))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_overlay_priority_narrow_insert_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-opnar")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((ov1 (make-overlay 1 15))
            (ov2 (make-overlay 6 25))
            (m1 (copy-marker 5 nil))
            (m2 (copy-marker 15 t)))
        (overlay-put ov1 'zone 'left)
        (overlay-put ov1 'priority 1)
        (overlay-put ov2 'zone 'right)
        (overlay-put ov2 'priority 2)
        (undo-boundary)
        (narrow-to-region 6 20)
        (goto-char (point-min))
        (insert "XX-")
        (widen)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov1) (overlay-end ov1)
                           (overlay-start ov2) (overlay-end ov2)
                           (overlay-get (car (overlays-at 1)) 'zone)
                           (overlay-get (car (overlays-at 8)) 'zone)
                           (overlay-get (car (overlays-at 20)) 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov1) (overlay-end ov1)
                                (overlay-start ov2) (overlay-end ov2)
                                (overlay-get (car (overlays-at 1)) 'zone)
                                (overlay-get (car (overlays-at 8)) 'zone)
                                (overlay-get (car (overlays-at 15)) 'zone)
                                (overlay-get (car (overlays-at 22)) 'zone))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_overlay_priority_buffer_local_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-opbl")))
    (with-current-buffer buf
      (make-local-variable 'op-local)
      (setq op-local 'buffer-specific)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((ov1 (make-overlay 1 10))
            (ov2 (make-overlay 6 15))
            (m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t)))
        (overlay-put ov1 'zone 'left)
        (overlay-put ov1 'priority 1)
        (overlay-put ov2 'zone 'right)
        (overlay-put ov2 'priority 2)
        (undo-boundary)
        (goto-char 5)
        (insert "-XX-")
        (let ((after (list (buffer-string)
                           op-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov1) (overlay-end ov1)
                           (overlay-start ov2) (overlay-end ov2)
                           (overlay-get (car (overlays-at 1)) 'zone)
                           (overlay-get (car (overlays-at 8)) 'zone)
                           (overlay-get (car (overlays-at 13)) 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                op-local
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov1) (overlay-end ov1)
                                (overlay-start ov2) (overlay-end ov2)
                                (overlay-get (car (overlays-at 1)) 'zone)
                                (overlay-get (car (overlays-at 8)) 'zone)
                                (overlay-get (car (overlays-at 13)) 'zone))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_overlay_priority_replace_match_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-oprep")))
    (with-current-buffer buf
      (insert "AAAA-XXXXX-BBBB-XXXXX-CCCC")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 11 'grp 'x1)
      (put-text-property 12 16 'grp 'b)
      (put-text-property 17 22 'grp 'x2)
      (put-text-property 23 27 'grp 'c)
      (let ((ov1 (make-overlay 1 16))
            (ov2 (make-overlay 6 27))
            (m1 (copy-marker 5 nil))
            (m2 (copy-marker 16 t)))
        (overlay-put ov1 'zone 'left)
        (overlay-put ov1 'priority 1)
        (overlay-put ov2 'zone 'right)
        (overlay-put ov2 'priority 2)
        (undo-boundary)
        (goto-char 1)
        (while (re-search-forward "XXXXX" nil t)
          (replace-match "XX"))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov1) (overlay-end ov1)
                           (overlay-start ov2) (overlay-end ov2)
                           (overlay-get (car (overlays-at 1)) 'zone)
                           (overlay-get (car (overlays-at 8)) 'zone)
                           (overlay-get (car (overlays-at 18)) 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov1) (overlay-end ov1)
                                (overlay-start ov2) (overlay-end ov2)
                                (overlay-get (car (overlays-at 1)) 'zone)
                                (overlay-get (car (overlays-at 8)) 'zone)
                                (overlay-get (car (overlays-at 15)) 'zone)
                                (overlay-get (car (overlays-at 22)) 'zone))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}
