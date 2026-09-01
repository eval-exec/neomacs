//! Deep combo: move-to-column × current-column × indent-to ×
//! tab-to-tab-stop × tab-width × indent-tabs-mode ×
//! marker × overlay × textprop × undo × buffer-local × narrow.
//!
//! Stresses column-based operations with buffer state: moving to
//! specific columns, getting current column, indenting to columns,
//! and tab handling. Column operations are tricky because they depend
//! on character widths and tab settings and must correctly interact
//! with markers, overlays, text properties, and undo state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_move_to_column_current_column_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mcc")))
    (with-current-buffer buf
      (insert "AAAA\tBBBB\tCCCC")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (let ((col1 (current-column)))
          (forward-char 5)
          (let ((col2 (current-column)))
            (move-to-column 12)
            (let ((col3 (current-column))
                  (pos3 (point)))
              (goto-char 6)
              (insert "XX")
              (let ((after (list (buffer-string)
                                 col1 col2 col3 pos3
                                 (marker-position m1)
                                 (marker-position m2)
                                 (overlay-start ov) (overlay-end ov)
                                 (get-text-property 1 'grp)
                                 (get-text-property 6 'grp))))
                (primitive-undo 1 buffer-undo-list)
                (let ((restored (list (buffer-string)
                                      (marker-position m1)
                                      (marker-position m2)
                                      (overlay-start ov) (overlay-end ov)
                                      (get-text-property 1 'grp)
                                      (get-text-property 6 'grp)
                                      (get-text-property 11 'grp))))
                  (kill-buffer buf)
                  (list after restored)))))))))) "#,
        expect,
    );
}

#[test]
fn combo_indent_to_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-it")))
    (with-current-buffer buf
      (insert "AAAA\nBBBB\nCCCC\nDDDD")
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
        (indent-to 8)
        (goto-char 12)
        (indent-to 16)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 6 'grp)
                           (get-text-property 14 'grp))))
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
fn combo_tab_width_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-tw")))
    (with-current-buffer buf
      (setq tab-width 4)
      (insert "AAAA\tBBBB\tCCCC")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (let ((col-tab (progn (forward-char 5) (current-column))))
          (setq tab-width 8)
          (goto-char 1)
          (let ((col-tab8 (progn (forward-char 5) (current-column))))
            (goto-char 6)
            (insert "XX")
            (let ((after (list (buffer-string)
                               tab-width col-tab col-tab8
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'grp)
                               (get-text-property 6 'grp))))
              (primitive-undo 1 buffer-undo-list)
              (let ((restored (list (buffer-string)
                                    (marker-position m1)
                                    (marker-position m2)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'grp)
                                    (get-text-property 6 'grp)
                                    (get-text-property 11 'grp))))
                (kill-buffer buf)
                (list after restored))))))))) "#,
        expect,
    );
}

#[test]
fn combo_column_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-coln")))
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
        (undo-boundary)
        (narrow-to-region 6 20)
        (goto-char (point-min))
        (let ((col (current-column)))
          (move-to-column 5)
          (let ((col5 (current-column))
                (pos5 (point)))
            (indent-to 10)
            (widen)
            (let ((after (list (buffer-string)
                               col col5 pos5
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
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
                (list after restored)))))))))) "#,
        expect,
    );
}

#[test]
fn combo_column_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-colbl")))
    (with-current-buffer buf
      (make-local-variable 'col-local)
      (setq col-local 'buffer-specific)
      (make-local-variable 'tab-width)
      (setq tab-width 4)
      (insert "AAAA\tBBBB\tCCCC")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (let ((col (current-column)))
          (forward-char 5)
          (let ((col5 (current-column)))
            (goto-char 6)
            (insert "XX")
            (let ((after (list (buffer-string)
                               col col5
                               col-local tab-width
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'grp)
                               (get-text-property 6 'grp))))
              (primitive-undo 1 buffer-undo-list)
              (let ((restored (list (buffer-string)
                                    col-local tab-width
                                    (marker-position m1)
                                    (marker-position m2)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'grp)
                                    (get-text-property 6 'grp)
                                    (get-text-property 11 'grp))))
                (kill-buffer buf)
                (list after restored)))))))))) "#,
        expect,
    );
}
