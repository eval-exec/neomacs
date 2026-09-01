//! Deep combo: combine-after-change-calls × inhibit-modification-hooks ×
//! before-change-functions × after-change-functions × marker × overlay ×
//! textprop × undo × buffer-local × narrow.
//!
//! Stresses modification hooks: combining after-change calls, inhibiting
//! modification hooks, and hook interaction with markers/overlays/textprops.
//! Modification hooks are tricky because they must fire correctly during
//! edits and must interact correctly with the buffer's edit pipeline.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_combine_after_change_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-cac"))
        (hook-calls nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (m3 (copy-marker 15 nil))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (push (list beg end len) hook-calls))
                  nil t)
        (undo-boundary)
        (combine-after-change-calls
          (goto-char 5)
          (insert "XX")
          (goto-char 15)
          (insert "YY"))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 6 'grp)
                           (get-text-property 12 'grp)
                           (get-text-property 18 'grp)
                           (length hook-calls))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'grp)
                                (get-text-property 6 'grp)
                                (get-text-property 11 'grp)
                                (get-text-property 16 'grp))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_inhibit_modification_hooks_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-imh"))
        (before-calls nil)
        (after-calls nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (add-hook 'before-change-functions
                  (lambda (beg end)
                    (push (list beg end) before-calls))
                  nil t)
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (push (list beg end len) after-calls))
                  nil t)
        (undo-boundary)
        (let ((inhibit-modification-hooks t))
          (goto-char 5)
          (insert "XX"))
        (let ((after-inhibit (list (buffer-string)
                                   (marker-position m1)
                                   (marker-position m2)
                                   (overlay-start ov) (overlay-end ov)
                                   (length before-calls)
                                   (length after-calls)
                                   (get-text-property 1 'zone)
                                   (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after-inhibit restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_before_after_change_hooks_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-bac"))
        (before-calls nil)
        (after-calls nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 6 20)))
        (overlay-put ov 'zone 'middle)
        (add-hook 'before-change-functions
                  (lambda (beg end)
                    (push (list beg end) before-calls))
                  nil t)
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (push (list beg end len) after-calls))
                  nil t)
        (undo-boundary)
        (narrow-to-region 6 20)
        (goto-char (point-min))
        (insert "XX-")
        (widen)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (length before-calls)
                           (length after-calls)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 21 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 11 'sect)
                                (get-text-property 16 'sect)
                                (get-text-property 21 'sect))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_combine_after_change_narrow_buflocal_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-cacn"))
        (hook-calls nil))
    (with-current-buffer buf
      (make-local-variable 'cac-local)
      (setq cac-local 'buffer-specific)
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 6 20)))
        (overlay-put ov 'zone 'middle)
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (push (list beg end len) hook-calls))
                  nil t)
        (undo-boundary)
        (narrow-to-region 6 20)
        (combine-after-change-calls
          (goto-char (point-min))
          (insert "XX-")
          (goto-char (point-max))
          (insert "-YY"))
        (widen)
        (let ((after (list (buffer-string)
                           cac-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (length hook-calls)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 21 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                cac-local
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 11 'sect)
                                (get-text-property 16 'sect)
                                (get-text-property 21 'sect))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_inhibit_hooks_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ihbl"))
        (before-calls nil)
        (after-calls nil))
    (with-current-buffer buf
      (make-local-variable 'ih-local)
      (setq ih-local 'buffer-specific)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (add-hook 'before-change-functions
                  (lambda (beg end)
                    (push (list beg end) before-calls))
                  nil t)
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (push (list beg end len) after-calls))
                  nil t)
        (undo-boundary)
        (let ((inhibit-modification-hooks t))
          (goto-char 5)
          (insert "XX")
          (goto-char 13)
          (insert "YY"))
        (let ((after (list (buffer-string)
                           ih-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (length before-calls)
                           (length after-calls)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone)
                           (get-text-property 12 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                ih-local
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}
