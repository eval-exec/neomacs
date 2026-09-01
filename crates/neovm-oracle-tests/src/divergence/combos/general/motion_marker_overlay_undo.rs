//! Deep combo: goto-char × forward-char × backward-char × forward-line ×
//! beginning-of-line × end-of-line × marker × overlay × text-prop ×
//! undo × buffer-local × narrow × delete-region × insert.
//!
//! Stresses motion commands with buffer state: cursor movement commands
//! must interact correctly with markers, overlays, and text properties.
//! Motion commands are tricky because they update point and must track
//! marker positions correctly.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_goto_char_forward_backward_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-gcfb")))
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
        (goto-char 6)
        (forward-char 4)
        (let ((pos-fwd (point)))
          (backward-char 2)
          (let ((pos-bwd (point)))
            (goto-char (point-max))
            (let ((pos-max (point)))
              (goto-char (point-min))
              (let ((pos-min (point))
                    (after (list (buffer-string)
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (overlay-start ov) (overlay-end ov)
                                 (get-text-property 1 'grp)
                                 (get-text-property 6 'grp)
                                 (get-text-property 11 'grp)
                                 (get-text-property 16 'grp))))
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
                  (list pos-fwd pos-bwd pos-max pos-min after restored))))))))) "#,
        expect,
    );
}

#[test]
fn combo_forward_line_beginning_end_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-flbe")))
    (with-current-buffer buf
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
        (goto-char 1)
        (forward-line 2)
        (let ((pos-fl2 (point)))
          (beginning-of-line)
          (let ((pos-bol (point)))
            (end-of-line)
            (let ((pos-eol (point))
                  (after (list (buffer-string)
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'line)
                               (get-text-property 7 'line)
                               (get-text-property 13 'line)
                               (get-text-property 19 'line))))
              (primitive-undo 1 buffer-undo-list)
              (let ((restored (list (buffer-string)
                                    (marker-position m1)
                                    (marker-position m2)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'line)
                                    (get-text-property 7 'line)
                                    (get-text-property 13 'line)
                                    (get-text-property 19 'line))))
                (kill-buffer buf)
                (list pos-fl2 pos-bol pos-eol after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_forward_char_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-fcn")))
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
        (forward-char 4)
        (let ((pos-fwd (point)))
          (backward-char 2)
          (let ((pos-bwd (point)))
            (insert "XX")
            (widen)
            (let ((after (list (buffer-string)
                               pos-fwd pos-bwd
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
                (list after restored))))))))) "#,
        expect,
    );
}

#[test]
fn combo_goto_char_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-gcbl")))
    (with-current-buffer buf
      (make-local-variable 'motion-local)
      (setq motion-local 'buffer-specific)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 6)
        (insert "XX")
        (let ((after (list (buffer-string)
                           motion-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone)
                           (get-text-property 12 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                motion-local
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

#[test]
fn combo_forward_line_delete_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-fldel")))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4\nline5")
      (put-text-property 1 6 'line 'l1)
      (put-text-property 7 12 'line 'l2)
      (put-text-property 13 18 'line 'l3)
      (put-text-property 19 24 'line 'l4)
      (put-text-property 25 30 'line 'l5)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (m3 (copy-marker 18 nil))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 7)
        (forward-line 1)
        (let ((pos-fl (point)))
          (beginning-of-line)
          (let ((pos-bol (point)))
            (end-of-line)
            (delete-region pos-bol (point))
            (let ((after (list (buffer-string)
                               pos-fl pos-bol
                               (marker-position m1)
                               (marker-position m2)
                               (marker-position m3)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'line)
                               (get-text-property 7 'line))))
              (primitive-undo 1 buffer-undo-list)
              (let ((restored (list (buffer-string)
                                    (marker-position m1)
                                    (marker-position m2)
                                    (marker-position m3)
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
