//! Combo: overlay evaporate + multi-buffer + markers + undo.
//! Tests overlay evaporate behavior across multiple buffers.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_overlay_evaporate_multi_buffer_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "oe1"))
        (b2 (generate-new-buffer "oe2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c))
    (with-current-buffer b2
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'z 'd)
      (put-text-property 6 10 'z 'e)
      (put-text-property 11 15 'z 'f))
    (let* ((ov1 (with-current-buffer b1
                  (let ((ov (make-overlay 6 10)))
                    (overlay-put ov 'evaporate t)
                    (overlay-put ov 'face 'bold) ov)))
           (ov2 (with-current-buffer b2
                  (let ((ov (make-overlay 6 10)))
                    (overlay-put ov 'evaporate t)
                    (overlay-put ov 'face 'italic) ov)))
           (m1 (with-current-buffer b1
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker))) (set-marker m 8) m))))
      (with-current-buffer b1
        (undo-boundary)
        (delete-region 6 10)
        (undo-boundary))
      (let ((mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2))
            (s1 (with-current-buffer b1 (buffer-string)))
            (s2 (with-current-buffer b2 (buffer-string))))
        (with-current-buffer b1
          (primitive-undo 1 buffer-undo-list))
        (list mp1 mp2 os1 oe1 os2 oe2 s1 s2
              (marker-position m1)
              (with-current-buffer b1 (buffer-string))
              (overlay-start ov1)
              (overlay-end ov1))))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_evaporate_clone_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "oec")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "oec-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (delete-region 6 10)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (ev (overlay-get ov 'evaporate))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe ev s
                  (marker-position m)
                  (overlay-start ov)
                  (overlay-end ov)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_evaporate_multi_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "oem")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'evaporate t))
             (_ (overlay-put ov2 'evaporate t))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (delete-region 6 10)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe1 s
                (marker-position m)
                (overlay-start ov1)
                (overlay-end ov1)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_evaporate_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "oen")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 6 10)
        (undo-boundary)
        (delete-region (point-min) (point-max))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe bs
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_evaporate_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "oet")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (delete-region 6 10)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property 1 'z))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe k s
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
