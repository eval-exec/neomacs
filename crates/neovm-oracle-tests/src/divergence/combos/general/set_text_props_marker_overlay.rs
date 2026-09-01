//! Combo: set-text-properties + markers + overlays + undo + narrow.
//! Tests bulk text property operations with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_set_text_props_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "stp")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'old 'yes)
      (put-text-property 6 10 'old 'yes)
      (put-text-property 11 15 'old 'yes)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (set-text-properties 6 10 (list 'new 'replaced 'count 1))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (old6 (get-text-property 6 'old))
              (new6 (get-text-property 6 'new))
              (cnt (get-text-property 6 'count))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe old6 new6 cnt s
                (marker-position m)
                (get-text-property 6 'old)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_set_text_props_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "stn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 6 10)
        (undo-boundary)
        (set-text-properties (point-min) (point-max)
                             (list 'zone 'replaced 'new t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'zone))
              (nw (get-text-property (point-min) 'new))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe k nw bs
                (marker-position m)
                (get-text-property 6 'zone)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_set_text_props_clone_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "stc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'orig)
      (put-text-property 6 10 'kind 'orig)
      (put-text-property 11 15 'kind 'orig)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "stc-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (set-text-properties 6 10 (list 'kind 'changed 'modified t))
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property 6 'kind))
                (mod (get-text-property 6 'modified))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe k mod s
                  (marker-position m)
                  (get-text-property 6 'kind)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_set_text_props_multi_zone_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "smz")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (set-text-properties 1 10 (list 'zone 'first-half 'part 1))
        (set-text-properties 11 20 (list 'zone 'second-half 'part 2))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (k1 (get-text-property 1 'zone))
              (p1 (get-text-property 1 'part))
              (k11 (get-text-property 11 'zone))
              (p11 (get-text-property 11 'part))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe1 os2 oe2 k1 p1 k11 p11 s
                (marker-position m)
                (get-text-property 1 'zone)
                (get-text-property 11 'zone)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_set_text_props_overlay_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "sou")))
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
        (set-text-properties (point-min) (point-max)
                             (list 'z 'replaced 'new-prop t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'z))
              (np (get-text-property (point-min) 'new-prop))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe k np bs
                (marker-position m)
                (get-text-property 6 'z)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
