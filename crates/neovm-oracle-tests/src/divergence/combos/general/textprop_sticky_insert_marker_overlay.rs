//! Combo: text property sticky + insert + markers + overlays + undo.
//! Tests text property inheritance on insert operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_textprop_sticky_insert_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tsi")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (goto-char 5)
        (insert "-XX-")
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k1 (get-text-property 1 'kind))
              (k5 (get-text-property 5 'kind))
              (k6 (get-text-property 6 'kind))
              (k10 (get-text-property 10 'kind))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe k1 k5 k6 k10 s
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_sticky_narrow_insert_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tni")))
    (with-current-buffer buf
      (insert "alpha-beta-gamma")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 11 'zone 'b)
      (put-text-property 12 17 'zone 'c)
      (let* ((ov (make-overlay 7 11))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 9)))
        (narrow-to-region 7 11)
        (undo-boundary)
        (goto-char (point-min))
        (insert "XX")
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'zone))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe k bs
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_sticky_clone_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tci")))
    (with-current-buffer buf
      (insert "one-two-three")
      (put-text-property 1 4 'part 'first)
      (put-text-property 5 8 'part 'second)
      (put-text-property 9 14 'part 'third)
      (let* ((ov (make-overlay 5 8))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 6))
             (clone (clone-buffer "tci-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (goto-char 4)
          (insert "-X-")
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property 4 'part))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe k s
                  (marker-position m)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_sticky_multi_insert_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tmi")))
    (with-current-buffer buf
      (insert "AAA-BBB-CCC")
      (put-text-property 1 4 'z 'a)
      (put-text-property 5 8 'z 'b)
      (put-text-property 9 12 'z 'c)
      (let* ((ov1 (make-overlay 1 8))
             (ov2 (make-overlay 5 12))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 6)))
        (undo-boundary)
        (goto-char 4)
        (insert "-XX-")
        (goto-char 9)
        (insert "-YY-")
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (k4 (get-text-property 4 'z))
              (k9 (get-text-property 9 'z))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe1 os2 oe2 k4 k9 s
                (marker-position m)
                (overlay-start ov1)
                (overlay-end ov2)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_insert_undo_marker_overlay_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tiu")))
    (with-current-buffer buf
      (insert "aaaa-bbbb-cccc")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 6 10)
        (undo-boundary)
        (goto-char (point-min))
        (insert "XX")
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'seg))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe k bs
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
