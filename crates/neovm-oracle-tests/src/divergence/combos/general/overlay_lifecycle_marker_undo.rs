//! Combo: overlay make/delete + markers + textprop + undo + narrow.
//! Tests overlay lifecycle (create/delete) with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_overlay_delete_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "odm")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (delete-overlay ov)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (k (get-text-property 1 'seg))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os k s
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_delete_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "odn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 6 10)
        (undo-boundary)
        (delete-overlay ov)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (k (get-text-property (point-min) 'z))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os k bs
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_create_delete_clone_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "ocd")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov1 (make-overlay 1 10))
             (_ (overlay-put ov1 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 6))
             (clone (clone-buffer "ocd-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (let ((ov2 (make-overlay 6 15)))
            (overlay-put ov2 'face 'italic)
            (overlay-put ov2 'priority 5))
          (delete-overlay ov1)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os1 (overlay-start ov1))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os1 s
                  (marker-position m)
                  (overlay-start ov1)
                  (overlay-end ov1)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_delete_multi_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "omd")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov2 'face 'italic))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (delete-overlay ov1)
        (undo-boundary)
        (delete-overlay ov2)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (os2 (overlay-start ov2))
              (s (buffer-string)))
          (primitive-undo 2 buffer-undo-list)
          (list mp os1 os2 s
                (marker-position m)
                (overlay-start ov1)
                (overlay-end ov1)
                (overlay-start ov2)
                (overlay-end ov2)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_delete_textprop_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "odt")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 1 15)
        (undo-boundary)
        (delete-overlay ov)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (k (get-text-property 6 'z))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os k bs
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
