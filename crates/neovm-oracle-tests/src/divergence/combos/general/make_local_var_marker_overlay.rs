//! Combo: make-local-variable + setq + markers + overlays + undo + narrow.
//! Tests non-setq-local buffer-local variable interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_make_local_var_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mlv")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (make-local-variable 'my-mlv-var)
        (setq my-mlv-var 'local-val)
        (undo-boundary)
        (goto-char 6)
        (insert "XX-")
        (undo-boundary)
        (let ((v my-mlv-var)
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property 1 'seg))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list v mp os oe k s
                my-mlv-var
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_make_local_var_narrow_clone_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mlc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (make-local-variable 'my-mlc-var)
      (setq my-mlc-var 'base)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 6))
             (clone (clone-buffer "mlc-clone")))
        (with-current-buffer clone
          (setq my-mlc-var 'cloned)
          (narrow-to-region 6 10)
          (undo-boundary)
          (goto-char (point-min))
          (insert "XX")
          (undo-boundary)
          (let ((v my-mlc-var)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property (point-min) 'z))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list v mp os oe k bs
                  my-mlc-var
                  (marker-position m)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_make_local_var_multi_buffer_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "mm1"))
        (b2 (generate-new-buffer "mm2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (make-local-variable 'mm-shared)
      (setq mm-shared 'b1-val)
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 6)))
        (undo-boundary)
        (goto-char 1)
        (insert "XX-")
        (undo-boundary)
        (let ((v mm-shared)
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list v mp os oe s
                mm-shared
                (marker-position m)
                (buffer-string)))))
    (with-current-buffer b2
      (insert "CCCC-DDDD")
      (put-text-property 1 5 'z 'c)
      (put-text-property 6 10 'z 'd)
      (make-local-variable 'mm-shared)
      (setq mm-shared 'b2-val)
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'face 'italic))
             (m (make-marker))
             (_ (set-marker m 6)))
        (let ((v mm-shared)
              (mp (marker-position m))
              (s (buffer-string)))
          (list v mp s))))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_make_local_var_setq_default_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mls")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 8)))
        (make-local-variable 'mls-var)
        (setq mls-var 'local)
        (setq-default mls-var 'default)
        (undo-boundary)
        (goto-char 6)
        (insert "XX")
        (undo-boundary)
        (let ((lv mls-var)
              (dv (default-value 'mls-var))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list lv dv mp os oe s
                mls-var
                (default-value 'mls-var)
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_make_local_var_overlay_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "mlo")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (make-local-variable 'mlo-var)
      (setq mlo-var 'init)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 6 10)
        (undo-boundary)
        (setq mlo-var 'changed)
        (goto-char (point-min))
        (insert "XX")
        (undo-boundary)
        (let ((v mlo-var)
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'z))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list v mp os oe k bs
                mlo-var
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
