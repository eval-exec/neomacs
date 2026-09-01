//! Combo: overlay invisible property + markers + textprop + undo + narrow.
//! Tests overlay display properties (invisible) with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_overlay_invisible_marker_textprop_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "oin")))
    (with-current-buffer buf
      (insert "visible-HIDDEN-visible")
      (put-text-property 1 8 'zone 'vis1)
      (put-text-property 9 15 'zone 'hid)
      (put-text-property 16 23 'zone 'vis2)
      (let* ((ov (make-overlay 9 15))
             (_ (overlay-put ov 'invisible t))
             (m (make-marker))
             (_ (set-marker m 12))
             (invis '((t))))
        (narrow-to-region 1 23)
        (goto-char (point-min))
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (inv (overlay-get ov 'invisible))
              (k1 (get-text-property 1 'zone))
              (k2 (get-text-property 9 'zone))
              (bs (buffer-string)))
          (widen)
          (list mp os oe inv k1 k2 bs))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_invisible_undo_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "oiu")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'invisible t))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (goto-char 6)
        (insert "XX-")
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (inv (overlay-get ov 'invisible))
              (k (get-text-property 6 'kind))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe inv k s
                (buffer-string)
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_priority_invisible_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "opi")))
    (with-current-buffer buf
      (insert "aaa bbb ccc ddd")
      (put-text-property 1 4 'p 1)
      (put-text-property 5 8 'p 2)
      (put-text-property 9 12 'p 3)
      (put-text-property 13 16 'p 4)
      (let* ((ov1 (make-overlay 1 8))
             (ov2 (make-overlay 5 12))
             (ov3 (make-overlay 9 16))
             (_ (overlay-put ov1 'invisible t))
             (_ (overlay-put ov2 'priority 5))
             (_ (overlay-put ov2 'invisible nil))
             (_ (overlay-put ov3 'priority 10))
             (_ (overlay-put ov3 'invisible t))
             (m (make-marker))
             (_ (set-marker m 10)))
        (let ((mp (marker-position m))
              (inv1 (overlay-get ov1 'invisible))
              (inv2 (overlay-get ov2 'invisible))
              (inv3 (overlay-get ov3 'invisible))
              (p1 (overlay-get ov1 'priority))
              (p2 (overlay-get ov2 'priority))
              (p3 (overlay-get ov3 'priority))
              (k (get-text-property 5 'p)))
          (list mp inv1 inv2 inv3 p1 p2 p3 k))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_invisible_clone_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "oic")))
    (with-current-buffer buf
      (insert "SHOW-HIDE-SHOW")
      (put-text-property 1 5 'vis 'yes)
      (put-text-property 6 10 'vis 'no)
      (put-text-property 11 15 'vis 'yes)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'invisible t))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "oic-clone")))
        (with-current-buffer clone
          (narrow-to-region 1 15)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (inv (overlay-get ov 'invisible))
                (v1 (get-text-property 1 'vis))
                (v2 (get-text-property 6 'vis))
                (bs (buffer-string)))
            (widen)
            (list mp os oe inv v1 v2 bs))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_invisible_textprop_undo_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "oit")))
    (with-current-buffer buf
      (insert "aaaBBBcccDDDeee")
      (put-text-property 1 4 'zone 'low)
      (put-text-property 4 7 'zone 'up)
      (put-text-property 7 10 'zone 'low2)
      (put-text-property 10 13 'zone 'up2)
      (put-text-property 13 16 'zone 'low3)
      (let* ((ov1 (make-overlay 4 7))
             (ov2 (make-overlay 10 13))
             (_ (overlay-put ov1 'invisible t))
             (_ (overlay-put ov2 'invisible t))
             (m (make-marker))
             (_ (set-marker m 5)))
        (narrow-to-region 1 16)
        (undo-boundary)
        (goto-char 4)
        (insert "XX")
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (k (get-text-property 1 'zone))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os1 oe1 os2 oe2 k bs
                (buffer-string)
                (marker-position m)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
