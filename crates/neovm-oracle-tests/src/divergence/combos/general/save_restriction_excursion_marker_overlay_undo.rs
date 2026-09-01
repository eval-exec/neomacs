//! Deep combo: save-restriction × save-excursion × marker × overlay ×
//! text-prop × buffer-local × narrow × undo × regex × replace-match.
//!
//! Stresses save-restriction and save-excursion interaction: nested
//! save-restriction/save-excursion with markers, overlays, text properties,
//! and undo. These are tricky in a Rust rewrite because they involve
//! saving/restoring multiple pieces of state (point, mark, narrowing)
//! and must interact correctly with the edit pipeline.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_save_restriction_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // save-restriction with edit inside; markers/overlays track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-srn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (m3 (copy-marker 15 nil))
            (ov (make-overlay 1 25)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (save-restriction
          (narrow-to-region 6 20)
          (goto-char (point-min))
          (insert "XX-")
          (goto-char (point-max))
          (insert "-YY"))
        (let ((after-save (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 16 'sect)
                                (get-text-property 21 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (marker-position m3)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'sect)
                                  (get-text-property 6 'sect)
                                  (get-text-property 11 'sect)
                                  (get-text-property 16 'sect)
                                  (get-text-property 21 'sect))))
            (kill-buffer buf)
            (list after-save after-undo))))))) "#,
        expect,
    );
}

#[test]
fn combo_save_excursion_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // save-excursion with edit; point must be restored.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-se")))
    (with-current-buffer buf
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
        (save-excursion
          (insert "XX-")
          (goto-char (point-max))
          (insert "-YY"))
        (let ((after-save (list (buffer-string)
                                (point)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  (point)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'zone)
                                  (get-text-property 6 'zone)
                                  (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after-save after-undo))))))) "#,
        expect,
    );
}

#[test]
fn combo_save_restriction_nested_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Nested save-restriction with edits; markers track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-srnested")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (put-text-property 26 30 'sect 'f)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 15 t))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (save-restriction
          (narrow-to-region 6 25)
          (save-restriction
            (narrow-to-region 11 20)
            (goto-char (point-min))
            (insert "INNER-"))
          (goto-char (point-min))
          (insert "OUTER-"))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 11 'sect)
                                (get-text-property 16 'sect)
                                (get-text-property 21 'sect)
                                (get-text-property 26 'sect))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_save_excursion_save_restriction_nested_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Nested save-excursion + save-restriction with edits.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-se-srn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (save-excursion
          (save-restriction
            (narrow-to-region 6 15)
            (goto-char (point-min))
            (insert "XX-")
            (goto-char (point-max))
            (insert "-YY"))
          (goto-char 1)
          (insert "START-"))
        (let ((after (list (buffer-string)
                           (point)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone)
                           (get-text-property 11 'zone)
                           (get-text-property 16 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (point)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone)
                                (get-text-property 16 'zone))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_save_restriction_regex_replace_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 31 40)""#]];
    // save-restriction with regex replace-match inside; markers track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-srregex")))
    (with-current-buffer buf
      (insert "alpha:100 beta:200 gamma:300 delta:400")
      (put-text-property 1 10 'grp 'g1)
      (put-text-property 11 20 'grp 'g2)
      (put-text-property 21 30 'grp 'g3)
      (put-text-property 31 40 'grp 'g4)
      (let ((m1 (copy-marker 10 nil))
            (m2 (copy-marker 20 t))
            (m3 (copy-marker 30 nil))
            (ov (make-overlay 1 40)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (save-restriction
          (narrow-to-region 11 30)
          (goto-char (point-min))
          (while (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" nil t)
            (replace-match "\\1=\\2" t)))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 11 'grp)
                           (get-text-property 21 'grp)
                           (get-text-property 31 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'grp)
                                (get-text-property 11 'grp)
                                (get-text-property 21 'grp)
                                (get-text-property 31 'grp))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}
