//! Combo: marker + overlay + textprop + undo + setf + clone.
//! Tests complex setf+clone interactions with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_setf_clone_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "scm")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "scm-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (setf (char-after 1) ?Z)
          (setf (marker-position m) 11)
          (setf (overlay-end ov) 15)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property 1 'kind))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe k s
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
fn combo_setf_clone_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "scn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "scn-clone")))
        (with-current-buffer clone
          (narrow-to-region 6 10)
          (undo-boundary)
          (setf (char-after (point-min)) ?Z)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property (point-min) 'z))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list mp os oe k bs
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
fn combo_setf_clone_multi_setf_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "scx")))
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
             (_ (set-marker m 8))
             (clone (clone-buffer "scx-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (setf (char-after 1) ?Z)
          (setf (char-after 6) ?Y)
          (setf (marker-position m) 13)
          (setf (overlay-start ov1) 1)
          (setf (overlay-end ov1) 11)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os1 (overlay-start ov1))
                (oe1 (overlay-end ov1))
                (os2 (overlay-start ov2))
                (oe2 (overlay-end ov2))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os1 oe1 os2 oe2 s
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
fn combo_setf_clone_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "sct")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "sct-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (setf (get-text-property 1 'kind) 'changed)
          (setf (char-after 6) ?Z)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property 1 'kind))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe k s
                  (marker-position m)
                  (get-text-property 1 'kind)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_setf_clone_buflocal_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "scb")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "scb-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (undo-boundary)
          (setf (char-after 1) ?Z)
          (setf (marker-position m) 11)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (v my-var)
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe v s
                  (marker-position m)
                  my-var
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}
