//! Combo: apply-partially + markers + overlays + textprop + undo.
//! Tests partial function application with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_apply_partially_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "app")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (defun my-insert-at (pos str)
          (goto-char pos)
          (insert str))
        (let ((my-ins (apply-partially 'my-insert-at 6)))
          (undo-boundary)
          (funcall my-ins "XX-")
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property 1 'seg))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe k s
                  (marker-position m)
                  (buffer-string))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_apply_partially_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "apn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8)))
        (defun my-narrow-ins (str)
          (goto-char (point-min))
          (insert str))
        (let ((my-ins (apply-partially 'my-narrow-ins)))
          (narrow-to-region 6 10)
          (undo-boundary)
          (funcall my-ins "XX")
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
                  (buffer-string))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_apply_partially_clone_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "apc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "apc-clone")))
        (defun my-cl-ins (pos str)
          (goto-char pos)
          (insert str))
        (let ((my-ins (apply-partially 'my-cl-ins 6)))
          (with-current-buffer clone
            (undo-boundary)
            (funcall my-ins "XX-")
            (undo-boundary)
            (let ((mp (marker-position m))
                  (os (overlay-start ov))
                  (oe (overlay-end ov))
                  (s (buffer-string)))
              (primitive-undo 1 buffer-undo-list)
              (list mp os oe s
                    (marker-position m)
                    (buffer-string))))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_apply_partially_multi_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun my-buf-ins (pos str)
    (goto-char pos)
    (insert str))
  (let ((my-ins (apply-partially 'my-buf-ins 1)))
    (let ((b1 (generate-new-buffer "ab1"))
          (b2 (generate-new-buffer "ab2")))
      (with-current-buffer b1
        (insert "AAAA-BBBB")
        (put-text-property 1 5 'z 'a)
        (put-text-property 6 10 'z 'b))
      (with-current-buffer b2
        (insert "CCCC-DDDD")
        (put-text-property 1 5 'z 'c)
        (put-text-property 6 10 'z 'd))
      (let ((r1 (with-current-buffer b1
                  (let* ((ov (make-overlay 1 10))
                         (_ (overlay-put ov 'face 'bold))
                         (m (make-marker))
                         (_ (set-marker m 6)))
                    (undo-boundary)
                    (funcall my-ins "XX-")
                    (undo-boundary)
                    (let ((mp (marker-position m))
                          (os (overlay-start ov))
                          (s (buffer-string)))
                      (primitive-undo 1 buffer-undo-list)
                      (list mp os s
                            (marker-position m)
                            (buffer-string))))))
            (r2 (with-current-buffer b2
                  (let* ((ov (make-overlay 1 10))
                         (_ (overlay-put ov 'face 'italic))
                         (m (make-marker))
                         (_ (set-marker m 6)))
                    (funcall my-ins "YY-")
                    (let ((mp (marker-position m))
                          (s (buffer-string)))
                      (list mp s))))))
        (list r1 r2))
      (kill-buffer b1)
      (kill-buffer b2))))"#,
        expect,
    );
}

#[test]
fn combo_apply_partially_overlay_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "apo")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 8)))
        (defun my-nar-ins (str)
          (goto-char (point-min))
          (insert str))
        (let ((my-ins (apply-partially 'my-nar-ins)))
          (narrow-to-region 6 10)
          (undo-boundary)
          (funcall my-ins "XX")
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
                  (buffer-string))))))
    (kill-buffer buf)))"#,
        expect,
    );
}
