//! Combo: text property changes during undo/redo + markers + overlays.
//! Tests text property changes during undo operations with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_textprop_change_undo_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tcu")))
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
        (put-text-property 6 10 'kind 'changed)
        (put-text-property 6 10 'new-prop t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property 6 'kind))
              (np (get-text-property 6 'new-prop))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe k np s
                (marker-position m)
                (get-text-property 6 'kind)
                (get-text-property 6 'new-prop)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_change_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tcn")))
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
        (put-text-property (point-min) (point-max) 'z 'changed)
        (put-text-property (point-min) (point-max) 'new t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'z))
              (nw (get-text-property (point-min) 'new))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe k nw bs
                (marker-position m)
                (get-text-property 6 'z)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_change_clone_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tcc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "tcc-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (put-text-property 6 10 'kind 'changed)
          (put-text-property 6 10 'new-prop t)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property 6 'kind))
                (np (get-text-property 6 'new-prop))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe k np s
                  (marker-position m)
                  (get-text-property 6 'kind)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_change_multi_zone_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tcm")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (put-text-property 1 10 'z 'first-half)
        (put-text-property 11 20 'z 'second-half)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (k1 (get-text-property 1 'z))
              (k11 (get-text-property 11 'z))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe1 os2 oe2 k1 k11 s
                (marker-position m)
                (get-text-property 1 'z)
                (get-text-property 11 'z)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_change_overlay_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tco")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 6 10)
        (undo-boundary)
        (put-text-property (point-min) (point-max) 'z 'changed)
        (put-text-property (point-min) (point-max) 'new t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'z))
              (nw (get-text-property (point-min) 'new))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe k nw bs
                (marker-position m)
                (get-text-property 6 'z)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
