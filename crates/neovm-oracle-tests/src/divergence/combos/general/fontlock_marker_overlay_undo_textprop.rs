//! Deep combo: font-lock × marker × overlay × undo × text-prop ×
//! buffer-local × jit-lock × syntax-propertize × narrow.
//!
//! Stresses font-lock interaction with buffer state: fontification
//! after edits, font-lock overlays with markers, and undo of
//! fontified regions. Font-lock is tricky in a Rust rewrite because
//! it involves lazy evaluation, syntax tables, and text properties.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_fontlock_insert_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Insert into fontified buffer; markers/overlays track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-fl")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(defun hello ()\n  (message \"hi\"))")
      (let ((m1 (copy-marker 7 nil))
            (m2 (copy-marker 16 t))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 7)
        (insert "-world")
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'face)
                           (syntax-ppss 7))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (syntax-ppss 7))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_fontlock_replace_keyword_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Replace keyword; font-lock must re-fontify.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-flrep")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(defun test-func ()\n  (message \"test\"))")
      (let ((m1 (copy-marker 7 nil))
            (m2 (copy-marker 18 t))
            (ov (make-overlay 1 36)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (re-search-forward "defun" nil t)
        (replace-match "defmacro")
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_fontlock_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Narrow, edit, undo; font-lock state must survive.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-flnarrow")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(defun alpha ()\n  (message \"a\"))\n\n(defun beta ()\n  (message \"b\"))")
      (let ((m1 (copy-marker 16 nil))
            (m2 (copy-marker 32 t))
            (ov (make-overlay 1 60)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (narrow-to-region 33 60)
        (goto-char (point-min))
        (insert "(setq x 1)\n")
        (widen)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_fontlock_buffer_local_major_mode_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Buffer-local major mode affects fontification.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-flmode")))
    (with-current-buffer buf
      (python-mode)
      (insert "def hello():\n    print('hi')")
      (let ((m1 (copy-marker 4 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 28)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 4)
        (insert "_world")
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'face)
                           (syntax-ppss 4))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (syntax-ppss 4))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_fontlock_delete_region_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Delete region in fontified buffer; undo restores.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-fldel")))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(defun test ()\n  (let ((x 1))\n    (message \"%s\" x)))")
      (let ((m1 (copy-marker 15 nil))
            (m2 (copy-marker 30 t))
            (ov (make-overlay 1 50)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (delete-region 15 30)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'face)
                                (syntax-ppss 15))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}
