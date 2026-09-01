//! Combo: defvar + let (dynamic binding) + markers + overlays + undo + narrow.
//! Tests dynamic binding interactions with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_dynamic_let_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dyn-test-var 'default)
  (let ((buf (generate-new-buffer "dlm")))
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
        (let ((dyn-test-var 'let-bound))
          (goto-char 6)
          (insert "XX-")
          (list dyn-test-var
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-string)
                (get-text-property 1 'seg))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_dynamic_let_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dyn-narrow-var 'base)
  (let ((buf (generate-new-buffer "dnm")))
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
        (let ((dyn-narrow-var 'narrowed))
          (goto-char (point-min))
          (insert "XX")
          (list dyn-narrow-var
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-substring (point-min) (point-max))
                (get-text-property (point-min) 'z))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_dynamic_let_clone_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable clone)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dyn-clone-var 'shared)
  (let ((buf (generate-new-buffer "dlc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "dlc-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (let ((dyn-clone-var 'cloned))
            (goto-char 6)
            (insert "XX-")
            (list dyn-clone-var
                  (marker-position m)
                  (overlay-start ov)
                  (overlay-end ov)
                  (buffer-string)
                  (get-text-property 1 'seg))))))
    (kill-buffer clone)
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_dynamic_let_multi_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dyn-multi-var 'global)
  (let ((b1 (generate-new-buffer "dm1"))
        (b2 (generate-new-buffer "dm2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b))
    (with-current-buffer b2
      (insert "CCCC-DDDD")
      (put-text-property 1 5 'z 'c)
      (put-text-property 6 10 'z 'd))
    (let ((r1 (with-current-buffer b1
                (let ((dyn-multi-var 'b1-scope))
                  (list dyn-multi-var
                        (get-text-property 1 'z)))))
          (r2 (with-current-buffer b2
                (let ((dyn-multi-var 'b2-scope))
                  (list dyn-multi-var
                        (get-text-property 1 'z))))))
      (list r1 r2 dyn-multi-var))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_dynamic_let_overlay_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dyn-let-ov-var 'init)
  (let ((buf (generate-new-buffer "dlo")))
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
        (let ((dyn-let-ov-var 'inner))
          (goto-char (point-min))
          (insert "XX")
          (undo-boundary)
          (list dyn-let-ov-var
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)
                (buffer-substring (point-min) (point-max))
                (get-text-property (point-min) 'z))))))
    (kill-buffer buf)))"#,
        expect,
    );
}
