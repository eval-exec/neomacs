//! Combo: buflocal + marker + overlay + textprop + undo + multi-buffer.
//! Tests complex multi-buffer interactions with buffer-local variables.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_buflocal_multi_buffer_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "bm1"))
        (b2 (generate-new-buffer "bm2"))
        (b3 (generate-new-buffer "bm3")))
    (with-current-buffer b1
      (insert "AAAA-BBBB")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (setq-local my-var 'b1-val))
    (with-current-buffer b2
      (insert "CCCC-DDDD")
      (put-text-property 1 5 'z 'c)
      (put-text-property 6 10 'z 'd)
      (setq-local my-var 'b2-val))
    (with-current-buffer b3
      (insert "EEEE-FFFF")
      (put-text-property 1 5 'z 'e)
      (put-text-property 6 10 'z 'f)
      (setq-local my-var 'b3-val))
    (let* ((ov1 (with-current-buffer b1
                  (let ((ov (make-overlay 1 10)))
                    (overlay-put ov 'face 'bold) ov)))
           (ov2 (with-current-buffer b2
                  (let ((ov (make-overlay 1 10)))
                    (overlay-put ov 'face 'italic) ov)))
           (m1 (with-current-buffer b1
                 (let ((m (make-marker))) (set-marker m 6) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker))) (set-marker m 6) m))))
      (with-current-buffer b1
        (undo-boundary)
        (goto-char 1)
        (insert (format "%s-" my-var))
        (undo-boundary))
      (with-current-buffer b2
        (undo-boundary)
        (goto-char 1)
        (insert (format "%s-" my-var))
        (undo-boundary))
      (let ((mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2))
            (v1 (buffer-local-value 'my-var b1))
            (v2 (buffer-local-value 'my-var b2))
            (v3 (buffer-local-value 'my-var b3))
            (s1 (with-current-buffer b1 (buffer-string)))
            (s2 (with-current-buffer b2 (buffer-string))))
        (with-current-buffer b1
          (primitive-undo 1 buffer-undo-list))
        (with-current-buffer b2
          (primitive-undo 1 buffer-undo-list))
        (list mp1 mp2 os1 oe1 os2 oe2 v1 v2 v3 s1 s2
              (marker-position m1)
              (marker-position m2)
              (with-current-buffer b1 (buffer-string))
              (with-current-buffer b2 (buffer-string)))))
    (kill-buffer b1)
    (kill-buffer b2)
    (kill-buffer b3)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_multi_buffer_narrow_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "bn1"))
        (b2 (generate-new-buffer "bn2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local my-var 'b1-val))
    (with-current-buffer b2
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'z 'd)
      (put-text-property 6 10 'z 'e)
      (put-text-property 11 15 'z 'f)
      (setq-local my-var 'b2-val))
    (let* ((ov1 (with-current-buffer b1
                  (let ((ov (make-overlay 6 10)))
                    (overlay-put ov 'face 'bold) ov)))
           (ov2 (with-current-buffer b2
                  (let ((ov (make-overlay 6 10)))
                    (overlay-put ov 'face 'italic) ov)))
           (m1 (with-current-buffer b1
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker))) (set-marker m 8) m))))
      (with-current-buffer b1
        (narrow-to-region 6 10)
        (undo-boundary)
        (goto-char (point-min))
        (insert (format "%s-" my-var))
        (undo-boundary)
        (widen))
      (with-current-buffer b2
        (narrow-to-region 6 10)
        (undo-boundary)
        (goto-char (point-min))
        (insert (format "%s-" my-var))
        (undo-boundary)
        (widen))
      (let ((mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2))
            (v1 (buffer-local-value 'my-var b1))
            (v2 (buffer-local-value 'my-var b2))
            (s1 (with-current-buffer b1 (buffer-string)))
            (s2 (with-current-buffer b2 (buffer-string))))
        (list mp1 mp2 os1 oe1 os2 oe2 v1 v2 s1 s2)))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_multi_buffer_clone_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "bmc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (c1 (clone-buffer "bmc-c1"))
             (c2 (clone-buffer "bmc-c2")))
        (with-current-buffer c1
          (setq-local my-var 'c1-val)
          (undo-boundary)
          (goto-char 6)
          (insert (format "%s-" my-var))
          (undo-boundary))
        (with-current-buffer c2
          (setq-local my-var 'c2-val)
          (undo-boundary)
          (goto-char 6)
          (insert (format "%s-" my-var))
          (undo-boundary))
        (let ((v1 (buffer-local-value 'my-var c1))
              (v2 (buffer-local-value 'my-var c2))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov)))
          (with-current-buffer c1
            (primitive-undo 1 buffer-undo-list))
          (with-current-buffer c2
            (primitive-undo 1 buffer-undo-list))
          (list v1 v2 mp os oe
                (buffer-local-value 'my-var c1)
                (buffer-local-value 'my-var c2)
                (with-current-buffer c1 (buffer-string))
                (with-current-buffer c2 (buffer-string)))))
      (kill-buffer c1)
      (kill-buffer c2)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_multi_buffer_replace_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "br1"))
        (b2 (generate-new-buffer "br2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local my-var 'b1-val))
    (with-current-buffer b2
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'z 'd)
      (put-text-property 6 10 'z 'e)
      (put-text-property 11 15 'z 'f)
      (setq-local my-var 'b2-val))
    (let* ((ov1 (with-current-buffer b1
                  (let ((ov (make-overlay 1 15)))
                    (overlay-put ov 'face 'bold) ov)))
           (ov2 (with-current-buffer b2
                  (let ((ov (make-overlay 1 15)))
                    (overlay-put ov 'face 'italic) ov)))
           (m1 (with-current-buffer b1
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker))) (set-marker m 8) m))))
      (with-current-buffer b1
        (undo-boundary)
        (goto-char 1)
        (re-search-forward "AAAA")
        (replace-match (format "%s-AAAA" my-var))
        (undo-boundary))
      (with-current-buffer b2
        (undo-boundary)
        (goto-char 1)
        (re-search-forward "DDDD")
        (replace-match (format "%s-DDDD" my-var))
        (undo-boundary))
      (let ((mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2))
            (v1 (buffer-local-value 'my-var b1))
            (v2 (buffer-local-value 'my-var b2))
            (s1 (with-current-buffer b1 (buffer-string)))
            (s2 (with-current-buffer b2 (buffer-string))))
        (with-current-buffer b1
          (primitive-undo 1 buffer-undo-list))
        (with-current-buffer b2
          (primitive-undo 1 buffer-undo-list))
        (list mp1 mp2 os1 oe1 os2 oe2 v1 v2 s1 s2
              (marker-position m1)
              (marker-position m2)
              (with-current-buffer b1 (buffer-string))
              (with-current-buffer b2 (buffer-string)))))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_multi_buffer_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "bt1"))
        (b2 (generate-new-buffer "bt2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local my-var 'b1-val))
    (with-current-buffer b2
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'z 'd)
      (put-text-property 6 10 'z 'e)
      (put-text-property 11 15 'z 'f)
      (setq-local my-var 'b2-val))
    (let* ((ov1 (with-current-buffer b1
                  (let ((ov (make-overlay 6 10)))
                    (overlay-put ov 'face 'bold) ov)))
           (ov2 (with-current-buffer b2
                  (let ((ov (make-overlay 6 10)))
                    (overlay-put ov 'face 'italic) ov)))
           (m1 (with-current-buffer b1
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker))) (set-marker m 8) m))))
      (with-current-buffer b1
        (undo-boundary)
        (put-text-property 6 10 'z 'changed)
        (undo-boundary))
      (with-current-buffer b2
        (undo-boundary)
        (put-text-property 6 10 'z 'changed)
        (undo-boundary))
      (let ((mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2))
            (v1 (buffer-local-value 'my-var b1))
            (v2 (buffer-local-value 'my-var b2))
            (k1 (with-current-buffer b1 (get-text-property 6 'z)))
            (k2 (with-current-buffer b2 (get-text-property 6 'z))))
        (with-current-buffer b1
          (primitive-undo 1 buffer-undo-list))
        (with-current-buffer b2
          (primitive-undo 1 buffer-undo-list))
        (list mp1 mp2 os1 oe1 os2 oe2 v1 v2 k1 k2
              (with-current-buffer b1 (get-text-property 6 'z))
              (with-current-buffer b2 (get-text-property 6 'z)))))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}
