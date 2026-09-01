//! Combo: overlay + marker + textprop + clone + buflocal.
//! Tests complex textprop+clone+buflocal interactions with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_textprop_clone_buflocal_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tcb")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "tcb-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (undo-boundary)
          (put-text-property 6 10 'kind 'changed)
          (put-text-property 6 10 'new-prop t)
          (goto-char 6)
          (insert (format "%s-" my-var))
          (undo-boundary)
          (let ((v my-var)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property 6 'kind))
                (np (get-text-property 6 'new-prop))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list v mp os oe k np s
                  my-var
                  (marker-position m)
                  (get-text-property 6 'kind)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_clone_buflocal_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tcn")))
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
             (clone (clone-buffer "tcn-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (narrow-to-region 6 10)
          (undo-boundary)
          (put-text-property (point-min) (point-max) 'z 'changed)
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
                  (get-text-property 6 'z)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_clone_buflocal_replace_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tcr")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "tcr-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (undo-boundary)
          (put-text-property 6 10 'kind 'changed)
          (goto-char 6)
          (re-search-forward "BBBB")
          (replace-match (format "%s-XX" my-var))
          (undo-boundary)
          (let ((v my-var)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property 6 'kind))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list v mp os oe k s
                  my-var
                  (marker-position m)
                  (get-text-property 6 'kind)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_clone_buflocal_setf_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tcs")))
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
             (clone (clone-buffer "tcs-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (undo-boundary)
          (put-text-property 6 10 'z 'changed)
          (setf (char-after 6) ?Z)
          (setf (marker-position m) 11)
          (undo-boundary)
          (let ((v my-var)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property 6 'z))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list v mp os oe k s
                  my-var
                  (marker-position m)
                  (get-text-property 6 'z)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_textprop_clone_buflocal_multi_buffer_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "tcm")))
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
             (c1 (clone-buffer "tcm-c1"))
             (c2 (clone-buffer "tcm-c2")))
        (with-current-buffer c1
          (setq-local my-var 'c1-val)
          (undo-boundary)
          (put-text-property 6 10 'z 'c1-changed)
          (goto-char 6)
          (insert (format "%s-" my-var))
          (undo-boundary))
        (with-current-buffer c2
          (setq-local my-var 'c2-val)
          (undo-boundary)
          (put-text-property 6 10 'z 'c2-changed)
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
                (with-current-buffer c1 (get-text-property 6 'z))
                (with-current-buffer c2 (get-text-property 6 'z))
                (with-current-buffer c1 (buffer-string))
                (with-current-buffer c2 (buffer-string)))))
      (kill-buffer c1)
      (kill-buffer c2)
      (kill-buffer buf)))"#,
        expect,
    );
}
