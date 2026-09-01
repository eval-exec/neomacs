//! Combo: buffer-local variable interactions during clone + markers + overlays.
//! Tests complex buffer-local variable scenarios with cloning.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_buflocal_clone_multi_var_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "bcv")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (setq-local var1 'base1)
      (setq-local var2 'base2)
      (setq-local var3 'base3)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "bcv-clone")))
        (with-current-buffer clone
          (setq-local var1 'clone1)
          (setq-local var2 'clone2)
          (undo-boundary)
          (goto-char 6)
          (insert (format "%s-%s-" var1 var2))
          (undo-boundary)
          (let ((v1 var1)
                (v2 var2)
                (v3 var3)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list v1 v2 v3 mp os oe s
                  var1 var2 var3
                  (marker-position m)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_clone_narrow_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "bno")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "bno-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (narrow-to-region 6 10)
          (undo-boundary)
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
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_clone_multi_buffer_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "bcm")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local shared-var 'base)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (c1 (clone-buffer "bcm-c1"))
             (c2 (clone-buffer "bcm-c2")))
        (with-current-buffer c1
          (setq-local shared-var 'c1-val)
          (undo-boundary)
          (goto-char 6)
          (insert (format "%s-" shared-var))
          (undo-boundary))
        (with-current-buffer c2
          (setq-local shared-var 'c2-val)
          (undo-boundary)
          (goto-char 6)
          (insert (format "%s-" shared-var))
          (undo-boundary))
        (let ((v1 (buffer-local-value 'shared-var c1))
              (v2 (buffer-local-value 'shared-var c2))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov)))
          (with-current-buffer c1
            (primitive-undo 1 buffer-undo-list))
          (with-current-buffer c2
            (primitive-undo 1 buffer-undo-list))
          (list v1 v2 mp os oe
                (buffer-local-value 'shared-var c1)
                (buffer-local-value 'shared-var c2)
                (with-current-buffer c1 (buffer-string))
                (with-current-buffer c2 (buffer-string)))))
      (kill-buffer c1)
      (kill-buffer c2)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_clone_setq_default_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar bcv-sd-var 'global)
  (let ((buf (generate-new-buffer "bsd")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local bcv-sd-var 'buf-local)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "bsd-clone")))
        (with-current-buffer clone
          (setq bcv-sd-var 'clone-val)
          (setq-default bcv-sd-var 'new-global)
          (undo-boundary)
          (goto-char 6)
          (insert (format "%s-%s-" bcv-sd-var (default-value 'bcv-sd-var)))
          (undo-boundary)
          (let ((lv bcv-sd-var)
                (dv (default-value 'bcv-sd-var))
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list lv dv mp os oe s
                  bcv-sd-var
                  (default-value 'bcv-sd-var)
                  (marker-position m)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_clone_hook_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "bch")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local my-hook-var 'base)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "bch-clone")))
        (with-current-buffer clone
          (setq-local my-hook-var 'cloned)
          (undo-boundary)
          (goto-char 6)
          (insert (format "%s-" my-hook-var))
          (undo-boundary)
          (let ((v my-hook-var)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list v mp os oe s
                  my-hook-var
                  (marker-position m)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}
