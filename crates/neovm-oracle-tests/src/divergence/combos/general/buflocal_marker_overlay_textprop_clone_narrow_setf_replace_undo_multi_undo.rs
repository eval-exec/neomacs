//! Combo: buflocal + marker + overlay + textprop + clone + narrow + setf + replace + undo + multi-buffer + undo.
//! Tests 11 subsystems in different order.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_buflocal_marker_overlay_textprop_clone_narrow_setf_replace_undo_multi_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ char-after\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "b1a"))
        (b2 (generate-new-buffer "b1b")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (setq-local my-var 'b1-base))
    (with-current-buffer b2
      (insert "EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'z 'e)
      (put-text-property 6 10 'z 'f)
      (put-text-property 11 15 'z 'g)
      (put-text-property 16 20 'z 'h)
      (setq-local my-var 'b2-base))
    (let* ((ov1 (with-current-buffer b1
                  (let ((ov (make-overlay 6 15)))
                    (overlay-put ov 'face 'bold) ov)))
           (ov2 (with-current-buffer b2
                  (let ((ov (make-overlay 6 15)))
                    (overlay-put ov 'face 'bold) ov)))
           (m1 (with-current-buffer b1
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (c1 (with-current-buffer b1 (clone-buffer "b1c")))
           (c2 (with-current-buffer b2 (clone-buffer "b1d"))))
      (with-current-buffer c1
        (setq-local my-var 'c1-cloned)
        (narrow-to-region 6 15)
        (undo-boundary)
        (put-text-property (point-min) (point-max) 'z 'changed)
        (setf (char-after (point-min)) ?Z)
        (setf (marker-position m1) 11)
        (goto-char (point-min))
        (re-search-forward "BBBB")
        (replace-match (format "%s-XX" my-var))
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (undo-boundary)
        (goto-char (point-min))
        (insert "YY-")
        (undo-boundary))
      (with-current-buffer c2
        (setq-local my-var 'c2-cloned)
        (narrow-to-region 6 15)
        (undo-boundary)
        (put-text-property (point-min) (point-max) 'z 'changed)
        (setf (char-after (point-min)) ?Y)
        (setf (marker-position m2) 11)
        (goto-char (point-min))
        (re-search-forward "FFFF")
        (replace-match (format "%s-YY" my-var))
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (undo-boundary)
        (goto-char (point-min))
        (insert "ZZ-")
        (undo-boundary))
      (let ((v1 (buffer-local-value 'my-var c1))
            (v2 (buffer-local-value 'my-var c2))
            (mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2)))
        (with-current-buffer c1
          (primitive-undo 1 buffer-undo-list))
        (with-current-buffer c2
          (primitive-undo 1 buffer-undo-list))
        (list v1 v2 mp1 mp2 os1 oe1 os2 oe2
              (buffer-local-value 'my-var c1)
              (buffer-local-value 'my-var c2)
              (with-current-buffer c1 (buffer-string))
              (with-current-buffer c2 (buffer-string)))))
    (kill-buffer c1)
    (kill-buffer c2)
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_marker_overlay_textprop_clone_narrow_replace_setf_undo_multi_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ char-after\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "c1a"))
        (b2 (generate-new-buffer "c1b")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (put-text-property 16 20 'kind 'd)
      (setq-local my-var 'b1-base))
    (with-current-buffer b2
      (insert "EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'kind 'e)
      (put-text-property 6 10 'kind 'f)
      (put-text-property 11 15 'kind 'g)
      (put-text-property 16 20 'kind 'h)
      (setq-local my-var 'b2-base))
    (let* ((ov1 (with-current-buffer b1
                  (let ((ov (make-overlay 6 15)))
                    (overlay-put ov 'face 'highlight) ov)))
           (ov2 (with-current-buffer b2
                  (let ((ov (make-overlay 6 15)))
                    (overlay-put ov 'face 'highlight) ov)))
           (m1 (with-current-buffer b1
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (c1 (with-current-buffer b1 (clone-buffer "c1c")))
           (c2 (with-current-buffer b2 (clone-buffer "c1d"))))
      (with-current-buffer c1
        (setq-local my-var 'c1-cloned)
        (narrow-to-region 6 15)
        (undo-boundary)
        (put-text-property (point-min) (point-max) 'kind 'changed)
        (goto-char (point-min))
        (re-search-forward "BBBB")
        (replace-match (format "%s-XX" my-var))
        (setf (char-after (point-min)) ?Z)
        (setf (marker-position m1) 11)
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (undo-boundary)
        (goto-char (point-min))
        (insert "YY-")
        (undo-boundary))
      (with-current-buffer c2
        (setq-local my-var 'c2-cloned)
        (narrow-to-region 6 15)
        (undo-boundary)
        (put-text-property (point-min) (point-max) 'kind 'changed)
        (goto-char (point-min))
        (re-search-forward "FFFF")
        (replace-match (format "%s-YY" my-var))
        (setf (char-after (point-min)) ?Y)
        (setf (marker-position m2) 11)
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (undo-boundary)
        (goto-char (point-min))
        (insert "ZZ-")
        (undo-boundary))
      (let ((v1 (buffer-local-value 'my-var c1))
            (v2 (buffer-local-value 'my-var c2))
            (mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2)))
        (with-current-buffer c1
          (primitive-undo 1 buffer-undo-list))
        (with-current-buffer c2
          (primitive-undo 1 buffer-undo-list))
        (list v1 v2 mp1 mp2 os1 oe1 os2 oe2
              (buffer-local-value 'my-var c1)
              (buffer-local-value 'my-var c2)
              (with-current-buffer c1 (get-text-property 6 'kind))
              (with-current-buffer c2 (get-text-property 6 'kind))
              (with-current-buffer c1 (buffer-string))
              (with-current-buffer c2 (buffer-string)))))
    (kill-buffer c1)
    (kill-buffer c2)
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_marker_overlay_textprop_clone_narrow_multi_overlay_undo_multi_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ char-after\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "d1a"))
        (b2 (generate-new-buffer "d1b")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (put-text-property 21 25 'z 'e)
      (setq-local my-var 'b1-base))
    (with-current-buffer b2
      (insert "FFFF-GGGG-HHHH-IIII-JJJJ")
      (put-text-property 1 5 'z 'f)
      (put-text-property 6 10 'z 'g)
      (put-text-property 11 15 'z 'h)
      (put-text-property 16 20 'z 'i)
      (put-text-property 21 25 'z 'j)
      (setq-local my-var 'b2-base))
    (let* ((ov1a (with-current-buffer b1
                   (let ((ov (make-overlay 1 10)))
                     (overlay-put ov 'priority 1) ov)))
           (ov1b (with-current-buffer b1
                   (let ((ov (make-overlay 11 20)))
                     (overlay-put ov 'priority 2) ov)))
           (ov2a (with-current-buffer b2
                   (let ((ov (make-overlay 1 10)))
                     (overlay-put ov 'priority 1) ov)))
           (ov2b (with-current-buffer b2
                   (let ((ov (make-overlay 11 20)))
                     (overlay-put ov 'priority 2) ov)))
           (m1 (with-current-buffer b1
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (c1 (with-current-buffer b1 (clone-buffer "d1c")))
           (c2 (with-current-buffer b2 (clone-buffer "d1d"))))
      (with-current-buffer c1
        (setq-local my-var 'c1-cloned)
        (narrow-to-region 6 20)
        (undo-boundary)
        (put-text-property (point-min) (point-max) 'z 'changed)
        (setf (char-after (point-min)) ?Z)
        (setf (marker-position m1) 11)
        (goto-char (point-min))
        (insert (format "%s-" my-var))
        (goto-char (point-max))
        (insert "-end")
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (undo-boundary)
        (goto-char (point-min))
        (insert "REDO-")
        (undo-boundary))
      (with-current-buffer c2
        (setq-local my-var 'c2-cloned)
        (narrow-to-region 6 20)
        (undo-boundary)
        (put-text-property (point-min) (point-max) 'z 'changed)
        (setf (char-after (point-min)) ?Y)
        (setf (marker-position m2) 11)
        (goto-char (point-min))
        (insert (format "%s-" my-var))
        (goto-char (point-max))
        (insert "-end")
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (undo-boundary)
        (goto-char (point-min))
        (insert "REDO-")
        (undo-boundary))
      (let ((v1 (buffer-local-value 'my-var c1))
            (v2 (buffer-local-value 'my-var c2))
            (mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1a (overlay-start ov1a))
            (oe1a (overlay-end ov1a))
            (os1b (overlay-start ov1b))
            (oe1b (overlay-end ov1b)))
        (with-current-buffer c1
          (primitive-undo 1 buffer-undo-list))
        (with-current-buffer c2
          (primitive-undo 1 buffer-undo-list))
        (list v1 v2 mp1 mp2 os1a oe1a os1b oe1b
              (buffer-local-value 'my-var c1)
              (buffer-local-value 'my-var c2)
              (with-current-buffer c1 (buffer-string))
              (with-current-buffer c2 (buffer-string)))))
    (kill-buffer c1)
    (kill-buffer c2)
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_marker_overlay_textprop_clone_narrow_textprop_replace_undo_multi_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ char-after\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "e1a"))
        (b2 (generate-new-buffer "e1b")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (put-text-property 16 20 'kind 'd)
      (setq-local my-var 'b1-base))
    (with-current-buffer b2
      (insert "EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'kind 'e)
      (put-text-property 6 10 'kind 'f)
      (put-text-property 11 15 'kind 'g)
      (put-text-property 16 20 'kind 'h)
      (setq-local my-var 'b2-base))
    (let* ((ov1 (with-current-buffer b1
                  (let ((ov (make-overlay 6 15)))
                    (overlay-put ov 'face 'region) ov)))
           (ov2 (with-current-buffer b2
                  (let ((ov (make-overlay 6 15)))
                    (overlay-put ov 'face 'region) ov)))
           (m1 (with-current-buffer b1
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (c1 (with-current-buffer b1 (clone-buffer "e1c")))
           (c2 (with-current-buffer b2 (clone-buffer "e1d"))))
      (with-current-buffer c1
        (setq-local my-var 'c1-cloned)
        (narrow-to-region 6 15)
        (undo-boundary)
        (put-text-property (point-min) (point-max) 'kind 'changed)
        (put-text-property (point-min) (point-max) 'new-prop t)
        (setf (char-after (point-min)) ?Z)
        (goto-char (point-min))
        (re-search-forward "BBBB")
        (replace-match (format "%s-XX" my-var))
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (undo-boundary)
        (goto-char (point-min))
        (insert "YY-")
        (undo-boundary))
      (with-current-buffer c2
        (setq-local my-var 'c2-cloned)
        (narrow-to-region 6 15)
        (undo-boundary)
        (put-text-property (point-min) (point-max) 'kind 'changed)
        (put-text-property (point-min) (point-max) 'new-prop t)
        (setf (char-after (point-min)) ?Y)
        (goto-char (point-min))
        (re-search-forward "FFFF")
        (replace-match (format "%s-YY" my-var))
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (undo-boundary)
        (goto-char (point-min))
        (insert "ZZ-")
        (undo-boundary))
      (let ((v1 (buffer-local-value 'my-var c1))
            (v2 (buffer-local-value 'my-var c2))
            (mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2)))
        (with-current-buffer c1
          (primitive-undo 1 buffer-undo-list))
        (with-current-buffer c2
          (primitive-undo 1 buffer-undo-list))
        (list v1 v2 mp1 mp2 os1 oe1 os2 oe2
              (buffer-local-value 'my-var c1)
              (buffer-local-value 'my-var c2)
              (with-current-buffer c1 (get-text-property 6 'kind))
              (with-current-buffer c2 (get-text-property 6 'kind))
              (with-current-buffer c1 (buffer-string))
              (with-current-buffer c2 (buffer-string)))))
    (kill-buffer c1)
    (kill-buffer c2)
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_buflocal_marker_overlay_textprop_clone_narrow_setf_replace_undo_multi_undo_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ char-after\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (generate-new-buffer "f1a"))
        (b2 (generate-new-buffer "f1b")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (setq-local my-var 'b1-base))
    (with-current-buffer b2
      (insert "EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'z 'e)
      (put-text-property 6 10 'z 'f)
      (put-text-property 11 15 'z 'g)
      (put-text-property 16 20 'z 'h)
      (setq-local my-var 'b2-base))
    (let* ((ov1 (with-current-buffer b1
                  (let ((ov (make-overlay 6 15)))
                    (overlay-put ov 'face 'highlight) ov)))
           (ov2 (with-current-buffer b2
                  (let ((ov (make-overlay 6 15)))
                    (overlay-put ov 'face 'highlight) ov)))
           (m1 (with-current-buffer b1
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (m2 (with-current-buffer b2
                 (let ((m (make-marker))) (set-marker m 8) m)))
           (c1 (with-current-buffer b1 (clone-buffer "f1c")))
           (c2 (with-current-buffer b2 (clone-buffer "f1d"))))
      (with-current-buffer c1
        (setq-local my-var 'c1-cloned)
        (narrow-to-region 6 15)
        (undo-boundary)
        (put-text-property (point-min) (point-max) 'z 'changed)
        (setf (char-after (point-min)) ?Z)
        (setf (marker-position m1) 11)
        (goto-char (point-min))
        (re-search-forward "BBBB")
        (replace-match (format "%s-XX" my-var))
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (undo-boundary)
        (goto-char (point-min))
        (insert "YY-")
        (undo-boundary))
      (with-current-buffer c2
        (setq-local my-var 'c2-cloned)
        (narrow-to-region 6 15)
        (undo-boundary)
        (put-text-property (point-min) (point-max) 'z 'changed)
        (setf (char-after (point-min)) ?Y)
        (setf (marker-position m2) 11)
        (goto-char (point-min))
        (re-search-forward "FFFF")
        (replace-match (format "%s-YY" my-var))
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (undo-boundary)
        (goto-char (point-min))
        (insert "ZZ-")
        (undo-boundary))
      (let ((v1 (buffer-local-value 'my-var c1))
            (v2 (buffer-local-value 'my-var c2))
            (mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2)))
        (with-current-buffer c1
          (primitive-undo 1 buffer-undo-list))
        (with-current-buffer c2
          (primitive-undo 1 buffer-undo-list))
        (list v1 v2 mp1 mp2 os1 oe1 os2 oe2
              (buffer-local-value 'my-var c1)
              (buffer-local-value 'my-var c2)
              (with-current-buffer c1 (buffer-string))
              (with-current-buffer c2 (buffer-string)))))
    (kill-buffer c1)
    (kill-buffer c2)
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}
