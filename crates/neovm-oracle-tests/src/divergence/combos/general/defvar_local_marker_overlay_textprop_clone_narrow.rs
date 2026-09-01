//! Combo: defvar-local + marker + overlay + textprop + clone + narrow + undo.
//! Tests buffer-local defvar interactions with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_defvar_local_marker_overlay_textprop_clone_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dvl-test-var 'global-default)
  (let ((buf (generate-new-buffer "dvl")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (setq-local dvl-test-var 'buf-local)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "dvl-clone")))
        (with-current-buffer clone
          (setq-local dvl-test-var 'clone-local)
          (narrow-to-region 6 15)
          (undo-boundary)
          (put-text-property (point-min) (point-max) 'z 'changed)
          (setf (char-after (point-min)) ?Z)
          (setf (marker-position m) 11)
          (goto-char (point-min))
          (insert (format "%s-" dvl-test-var))
          (undo-boundary)
          (let ((v dvl-test-var)
                (dv (default-value 'dvl-test-var))
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property (point-min) 'z))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list v dv mp os oe k bs
                  dvl-test-var
                  (default-value 'dvl-test-var)
                  (marker-position m)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_defvar_local_multi_buffer_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dvl-multi-var 'global)
  (let ((b1 (generate-new-buffer "dm1"))
        (b2 (generate-new-buffer "dm2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local dvl-multi-var 'b1-val))
    (with-current-buffer b2
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'z 'd)
      (put-text-property 6 10 'z 'e)
      (put-text-property 11 15 'z 'f)
      (setq-local dvl-multi-var 'b2-val))
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
        (goto-char 6)
        (insert (format "%s-" dvl-multi-var))
        (undo-boundary))
      (with-current-buffer b2
        (undo-boundary)
        (goto-char 6)
        (insert (format "%s-" dvl-multi-var))
        (undo-boundary))
      (let ((v1 (buffer-local-value 'dvl-multi-var b1))
            (v2 (buffer-local-value 'dvl-multi-var b2))
            (dv (default-value 'dvl-multi-var))
            (mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2)))
        (with-current-buffer b1
          (primitive-undo 1 buffer-undo-list))
        (with-current-buffer b2
          (primitive-undo 1 buffer-undo-list))
        (list v1 v2 dv mp1 mp2 os1 oe1 os2 oe2
              (buffer-local-value 'dvl-multi-var b1)
              (buffer-local-value 'dvl-multi-var b2)
              (with-current-buffer b1 (buffer-string))
              (with-current-buffer b2 (buffer-string)))))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_defvar_local_setf_replace_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ char-after\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dvl-setf-var 'global)
  (let ((buf (generate-new-buffer "dvs")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (setq-local dvl-setf-var 'buf-local)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (setf (char-after 6) ?Z)
        (setf (marker-position m) 11)
        (goto-char 6)
        (re-search-forward "BBBB")
        (replace-match (format "%s-XX" dvl-setf-var))
        (undo-boundary)
        (let ((v dvl-setf-var)
              (dv (default-value 'dvl-setf-var))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list v dv mp os oe s
                dvl-setf-var
                (default-value 'dvl-setf-var)
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_defvar_local_narrow_clone_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dvl-nc-var 'global)
  (let ((buf (generate-new-buffer "dcn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (setq-local dvl-nc-var 'buf-local)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "dcn-clone")))
        (with-current-buffer clone
          (setq-local dvl-nc-var 'clone-local)
          (narrow-to-region 6 15)
          (undo-boundary)
          (put-text-property (point-min) (point-max) 'z 'changed)
          (setf (char-after (point-min)) ?Z)
          (setf (marker-position m) 11)
          (goto-char (point-min))
          (insert (format "%s-" dvl-nc-var))
          (undo-boundary)
          (let ((v dvl-nc-var)
                (dv (default-value 'dvl-nc-var))
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property (point-min) 'z))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list v dv mp os oe k bs
                  dvl-nc-var
                  (default-value 'dvl-nc-var)
                  (marker-position m)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_defvar_local_multi_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ char-after\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dvl-mo-var 'global)
  (let ((buf (generate-new-buffer "dmo")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (put-text-property 21 25 'z 'e)
      (setq-local dvl-mo-var 'buf-local)
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (setf (char-after 6) ?Z)
        (setf (marker-position m) 11)
        (goto-char 6)
        (insert (format "%s-" dvl-mo-var))
        (undo-boundary)
        (let ((v dvl-mo-var)
              (dv (default-value 'dvl-mo-var))
              (mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list v dv mp os1 oe1 os2 oe2 s
                dvl-mo-var
                (default-value 'dvl-mo-var)
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
