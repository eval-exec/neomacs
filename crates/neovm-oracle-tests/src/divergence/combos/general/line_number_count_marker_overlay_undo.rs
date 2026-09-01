//! Deep combo: line-number-at-pos × what-line × count-lines ×
//! line-beginning-position × line-end-position ×
//! marker × overlay × textprop × undo × buffer-local × narrow.
//!
//! Stresses line number/position queries with buffer state: getting
//! line numbers, line boundaries, and counting lines while preserving
//! markers, overlays, text properties, and undo state. Line queries
//! are tricky because they depend on buffer content and must correctly
//! interact with narrowing and buffer modifications.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_line_number_at_pos_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-lnp")))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4\nline5")
      (put-text-property 1 6 'line 'l1)
      (put-text-property 7 12 'line 'l2)
      (put-text-property 13 18 'line 'l3)
      (put-text-property 19 24 'line 'l4)
      (put-text-property 25 30 'line 'l5)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((ln1 (line-number-at-pos 1))
              (ln3 (line-number-at-pos 13))
              (ln5 (line-number-at-pos 25)))
          (goto-char 7)
          (insert "INSERTED\n")
          (let ((after (list (buffer-string)
                             ln1 ln3 ln5
                             (line-number-at-pos 1)
                             (line-number-at-pos 13)
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
                                  (get-text-property 7 'line)
                                  (get-text-property 13 'line)
                                  (get-text-property 19 'line)
                                  (get-text-property 25 'line))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_count_lines_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-cl")))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4\nline5")
      (put-text-property 1 6 'line 'l1)
      (put-text-property 7 12 'line 'l2)
      (put-text-property 13 18 'line 'l3)
      (put-text-property 19 24 'line 'l4)
      (put-text-property 25 30 'line 'l5)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((cl (count-lines 1 30)))
          (goto-char 13)
          (insert "EXTRA\n")
          (let ((after (list (buffer-string)
                             cl
                             (count-lines 1 (point-max))
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'line)
                             (get-text-property 7 'line)
                             (get-text-property 13 'line))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (count-lines 1 (point-max))
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'line)
                                  (get-text-property 7 'line)
                                  (get-text-property 13 'line)
                                  (get-text-property 19 'line)
                                  (get-text-property 25 'line))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_line_beginning_end_position_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-lbe")))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4\nline5")
      (put-text-property 1 6 'line 'l1)
      (put-text-property 7 12 'line 'l2)
      (put-text-property 13 18 'line 'l3)
      (put-text-property 19 24 'line 'l4)
      (put-text-property 25 30 'line 'l5)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 10)
        (let ((bol (line-beginning-position))
              (eol (line-end-position)))
          (goto-char 7)
          (insert "XX")
          (let ((after (list (buffer-string)
                             bol eol
                             (line-beginning-position)
                             (line-end-position)
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
                                  (get-text-property 7 'line)
                                  (get-text-property 13 'line)
                                  (get-text-property 19 'line)
                                  (get-text-property 25 'line))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_line_queries_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-lqn")))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4\nline5\nline6")
      (put-text-property 1 6 'line 'l1)
      (put-text-property 7 12 'line 'l2)
      (put-text-property 13 18 'line 'l3)
      (put-text-property 19 24 'line 'l4)
      (put-text-property 25 30 'line 'l5)
      (put-text-property 31 36 'line 'l6)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 7 30)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 7 30)
        (let ((ln (line-number-at-pos (point-min)))
              (cl (count-lines (point-min) (point-max)))
              (bol (line-beginning-position))
              (eol (line-end-position)))
          (goto-char (point-min))
          (insert "XX-")
          (widen)
          (let ((after (list (buffer-string)
                             ln cl bol eol
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'line)
                             (get-text-property 7 'line)
                             (get-text-property 13 'line)
                             (get-text-property 19 'line)
                             (get-text-property 25 'line)
                             (get-text-property 31 'line))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'line)
                                  (get-text-property 7 'line)
                                  (get-text-property 13 'line)
                                  (get-text-property 19 'line)
                                  (get-text-property 25 'line)
                                  (get-text-property 31 'line))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_line_queries_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-lqbl")))
    (with-current-buffer buf
      (make-local-variable 'lq-local)
      (setq lq-local 'buffer-specific)
      (insert "line1\nline2\nline3\nline4")
      (put-text-property 1 6 'line 'l1)
      (put-text-property 7 12 'line 'l2)
      (put-text-property 13 18 'line 'l3)
      (put-text-property 19 24 'line 'l4)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 24)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((ln (line-number-at-pos 13))
              (cl (count-lines 1 24))
              (bol (line-beginning-position 3))
              (eol (line-end-position 3)))
          (goto-char 7)
          (insert "XX")
          (let ((after (list (buffer-string)
                             ln cl bol eol
                             lq-local
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'line)
                             (get-text-property 7 'line)
                             (get-text-property 13 'line))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  lq-local
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'line)
                                  (get-text-property 7 'line)
                                  (get-text-property 13 'line)
                                  (get-text-property 19 'line))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}
