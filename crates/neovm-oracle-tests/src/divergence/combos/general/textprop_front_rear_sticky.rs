//! Combo: text property front-sticky/rear-sticky + insert + markers + overlays + undo.
//! Tests sticky text property behavior at buffer boundaries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_textprop_front_sticky_insert_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tfs")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'front-sticky t)
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'rear-nonsticky t)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
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
              (k5 (get-text-property 5 'zone))
              (k6 (get-text-property 6 'zone))
              (k10 (get-text-property 10 'zone))
              (fs (get-text-property 1 'front-sticky))
              (rn (get-text-property 6 'rear-nonsticky))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe k5 k6 k10 fs rn s
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_rear_sticky_insert_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "trs")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'rear-sticky t)
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'front-nonsticky t)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
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
              (k (get-text-property (point-min) 'zone))
              (rs (get-text-property 1 'rear-sticky))
              (fn (get-text-property 6 'front-nonsticky))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe k rs fn bs
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_sticky_clone_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tsc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'front-sticky t)
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'rear-nonsticky t)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "tsc-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (goto-char 5)
          (insert "-XX-")
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k5 (get-text-property 5 'zone))
                (fs (get-text-property 1 'front-sticky))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe k5 fs s
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
  (let ((buf (generate-new-buffer "tsm")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'front-sticky t)
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'rear-nonsticky t)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 6 15))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (goto-char 5)
        (insert "-XX-")
        (goto-char 10)
        (insert "-YY-")
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (k5 (get-text-property 5 'zone))
              (k10 (get-text-property 10 'zone))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe1 os2 oe2 k5 k10 s
                (marker-position m)
                (overlay-start ov1)
                (overlay-end ov2)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_sticky_narrow_clone_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tsn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'front-sticky t)
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'rear-nonsticky t)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "tsn-clone")))
        (with-current-buffer clone
          (narrow-to-region 6 10)
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
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}
