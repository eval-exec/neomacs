//! Combo: remove-text-properties + add-text-properties + markers + overlays + undo.
//! Tests text property removal and addition with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_remove_text_props_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments put-text-property 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rtp")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a 'color 'red)
      (put-text-property 6 10 'kind 'b 'color 'blue)
      (put-text-property 11 15 'kind 'c 'color 'green)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (remove-text-properties 6 10 '(kind nil color nil))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k6 (get-text-property 6 'kind))
              (c6 (get-text-property 6 'color))
              (k1 (get-text-property 1 'kind))
              (c1 (get-text-property 1 'color))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe k6 c6 k1 c1 s
                (marker-position m)
                (get-text-property 6 'kind)
                (get-text-property 6 'color)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_add_text_props_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "atp")))
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
        (add-text-properties (point-min) (point-max)
                             (list 'new t 'extra 'val))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'zone))
              (nw (get-text-property (point-min) 'new))
              (ex (get-text-property (point-min) 'extra))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe k nw ex bs
                (marker-position m)
                (get-text-property 6 'new)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_remove_add_text_props_clone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rac")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a 'status 'active)
      (put-text-property 6 10 'kind 'b 'status 'active)
      (put-text-property 11 15 'kind 'c 'status 'active)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "rac-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (remove-text-properties 6 10 '(status nil))
          (add-text-properties 6 10 (list 'status 'modified 'changed t))
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property 6 'kind))
                (st (get-text-property 6 'status))
                (ch (get-text-property 6 'changed))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe k st ch s
                  (marker-position m)
                  (get-text-property 6 'status)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_remove_text_props_multi_zone_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments put-text-property 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rmz")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a 'p 1)
      (put-text-property 6 10 'z 'b 'p 2)
      (put-text-property 11 15 'z 'c 'p 3)
      (put-text-property 16 20 'z 'd 'p 4)
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (remove-text-properties 1 10 '(p nil))
        (remove-text-properties 11 20 '(z nil))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (p1 (get-text-property 1 'p))
              (z11 (get-text-property 11 'z))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe1 os2 oe2 p1 z11 s
                (marker-position m)
                (get-text-property 1 'p)
                (get-text-property 11 'z)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_add_remove_text_props_overlay_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "aru")))
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
        (remove-text-properties (point-min) (point-max) '(z nil))
        (add-text-properties (point-min) (point-max) (list 'z 'new 'added t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'z))
              (ad (get-text-property (point-min) 'added))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe k ad bs
                (marker-position m)
                (get-text-property 6 'z)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
