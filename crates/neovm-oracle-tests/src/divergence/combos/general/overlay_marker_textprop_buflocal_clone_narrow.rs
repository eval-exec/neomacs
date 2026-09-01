//! Combo: overlay + marker + textprop + buflocal + clone + narrow + replace.
//! Tests 7 subsystems in different order.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_overlay_marker_textprop_buflocal_clone_narrow_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "omb")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (put-text-property 16 20 'kind 'd)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "omb-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (narrow-to-region 6 15)
          (undo-boundary)
          (put-text-property (point-min) (point-max) 'kind 'changed)
          (goto-char (point-min))
          (re-search-forward "BBBB")
          (replace-match (format "%s-XX" my-var))
          (undo-boundary)
          (let ((v my-var)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property (point-min) 'kind))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list v mp os oe k bs
                  my-var
                  (marker-position m)
                  (overlay-start ov)
                  (overlay-end ov)
                  (get-text-property 6 'kind)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_marker_textprop_buflocal_clone_narrow_setf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "oms")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "oms-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (narrow-to-region 6 15)
          (undo-boundary)
          (put-text-property (point-min) (point-max) 'z 'changed)
          (setf (char-after (point-min)) ?Z)
          (setf (marker-position m) 11)
          (goto-char (point-min))
          (insert (format "%s-" my-var))
          (undo-boundary)
          (let ((v my-var)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property (point-min) 'z))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list v mp os oe k bs
                  my-var
                  (marker-position m)
                  (overlay-start ov)
                  (overlay-end ov)
                  (get-text-property 6 'z)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_marker_textprop_buflocal_clone_narrow_multi_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "omo")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (put-text-property 21 25 'z 'e)
      (setq-local my-var 'base)
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "omo-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (narrow-to-region 6 20)
          (undo-boundary)
          (put-text-property (point-min) (point-max) 'z 'changed)
          (setf (char-after (point-min)) ?Z)
          (goto-char (point-min))
          (insert (format "%s-" my-var))
          (goto-char (point-max))
          (insert "-end")
          (undo-boundary)
          (let ((v my-var)
                (mp (marker-position m))
                (os1 (overlay-start ov1))
                (oe1 (overlay-end ov1))
                (os2 (overlay-start ov2))
                (oe2 (overlay-end ov2))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list v mp os1 oe1 os2 oe2 bs
                  my-var
                  (marker-position m)
                  (overlay-start ov1)
                  (overlay-end ov2)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_marker_textprop_buflocal_clone_narrow_textprop_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "omt")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (put-text-property 16 20 'kind 'd)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "omt-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (narrow-to-region 6 15)
          (undo-boundary)
          (put-text-property (point-min) (point-max) 'kind 'changed)
          (put-text-property (point-min) (point-max) 'new-prop t)
          (setf (char-after (point-min)) ?Z)
          (goto-char (point-min))
          (re-search-forward "BBBB")
          (replace-match (format "%s-XX" my-var))
          (undo-boundary)
          (let ((v my-var)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property (point-min) 'kind))
                (np (get-text-property (point-min) 'new-prop))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list v mp os oe k np bs
                  my-var
                  (marker-position m)
                  (overlay-start ov)
                  (overlay-end ov)
                  (get-text-property 6 'kind)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_overlay_marker_textprop_buflocal_clone_narrow_setf_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "omx")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "omx-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (narrow-to-region 6 15)
          (undo-boundary)
          (put-text-property (point-min) (point-max) 'z 'changed)
          (setf (char-after (point-min)) ?Z)
          (setf (marker-position m) 11)
          (goto-char (point-min))
          (re-search-forward "BBBB")
          (replace-match (format "%s-XX" my-var))
          (undo-boundary)
          (let ((v my-var)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property (point-min) 'z))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list v mp os oe k bs
                  my-var
                  (marker-position m)
                  (overlay-start ov)
                  (overlay-end ov)
                  (get-text-property 6 'z)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}
