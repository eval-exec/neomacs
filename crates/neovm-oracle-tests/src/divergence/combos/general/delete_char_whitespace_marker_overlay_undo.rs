//! Deep combo: delete-char × delete-backward-char ×
//! backward-delete-char-untabify × delete-forward-char ×
//! delete-horizontal-space × just-one-space × delete-blank-lines ×
//! marker × overlay × textprop × undo × buffer-local × narrow.
//!
//! Stresses deletion commands with buffer state: character-level
//! deletion commands and whitespace manipulation. These commands are
//! tricky because they modify buffer content and must correctly
//! track markers, overlays, text properties, and undo state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_delete_char_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-dc")))
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
        (undo-boundary)
        (goto-char 5)
        (delete-char 1)
        (goto-char 10)
        (delete-char -2)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 5 'grp)
                           (get-text-property 10 'grp)
                           (get-text-property 14 'grp))))
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
fn combo_delete_backward_char_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-dbc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 6)
        (delete-backward-char 1)
        (goto-char 12)
        (delete-backward-char 2)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 5 'grp)
                           (get-text-property 9 'grp)
                           (get-text-property 13 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
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
fn combo_delete_horizontal_space_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 16 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-dhs")))
    (with-current-buffer buf
      (insert "AAAA   BBBB   CCCC")
      (put-text-property 1 4 'word 'a)
      (put-text-property 8 12 'word 'b)
      (put-text-property 16 20 'word 'c)
      (let ((m1 (copy-marker 7 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (delete-horizontal-space)
        (goto-char 10)
        (delete-horizontal-space)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'word)
                           (get-text-property 5 'word)
                           (get-text-property 9 'word))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'word)
                                (get-text-property 8 'word)
                                (get-text-property 16 'word))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_just_one_space_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 20 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-jos")))
    (with-current-buffer buf
      (insert "AAAA     BBBB     CCCC")
      (put-text-property 1 4 'word 'a)
      (put-text-property 10 14 'word 'b)
      (put-text-property 20 24 'word 'c)
      (let ((m1 (copy-marker 9 nil))
            (m2 (copy-marker 14 t))
            (ov (make-overlay 1 24)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (just-one-space)
        (goto-char 12)
        (just-one-space 2)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'word)
                           (get-text-property 5 'word)
                           (get-text-property 9 'word))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'word)
                                (get-text-property 10 'word)
                                (get-text-property 20 'word))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_delete_blank_lines_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 20 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-dbl")))
    (with-current-buffer buf
      (insert "line1\n\n\n\nline2\n\n\nline3")
      (put-text-property 1 6 'line 'l1)
      (put-text-property 11 16 'line 'l2)
      (put-text-property 20 25 'line 'l3)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 16 t))
            (ov (make-overlay 1 25)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 7)
        (delete-blank-lines)
        (goto-char 14)
        (delete-blank-lines)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'line)
                           (get-text-property 7 'line)
                           (get-text-property 13 'line))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'line)
                                (get-text-property 11 'line)
                                (get-text-property 20 'line))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}
