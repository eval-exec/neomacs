//! Deep combo: narrow-to-region × widen × save-restriction ×
//! restriction × buffer-narrowed-p × point-min × point-max ×
//! marker × overlay × textprop × undo × buffer-local × insert ×
//! delete × replace-match × regex.
//!
//! Stresses narrowing with buffer state: multiple nested narrow/widen
//! operations, buffer-narrowed-p checks, point-min/point-max in
//! narrowed buffers, and interaction with markers, overlays, text
//! properties, and undo. Narrowing is tricky because it changes the
//! accessible portion of the buffer and must correctly track all
//! buffer state through the restriction.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_nested_narrow_widen_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-nnw")))
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
            (m3 (copy-marker 25 nil))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        ;; Outer narrow
        (narrow-to-region 6 25)
        (let ((outer-pmin (point-min))
              (outer-pmax (point-max))
              (outer-narrowed (buffer-narrowed-p)))
          ;; Inner narrow
          (narrow-to-region 11 20)
          (let ((inner-pmin (point-min))
                (inner-pmax (point-max)))
            (goto-char (point-min))
            (insert "XX-")
            ;; Widen to outer
            (widen)
            (let ((after-inner (list (buffer-string)
                                     outer-pmin outer-pmax outer-narrowed
                                     inner-pmin inner-pmax
                                     (marker-position m1)
                                     (marker-position m2)
                                     (marker-position m3)
                                     (overlay-start ov) (overlay-end ov)
                                     (get-text-property 1 'sect)
                                     (get-text-property 6 'sect)
                                     (get-text-property 16 'sect)
                                     (get-text-property 21 'sect)
                                     (get-text-property 26 'sect))))
              ;; Widen fully
              (widen)
              (primitive-undo 1 buffer-undo-list)
              (let ((restored (list (buffer-string)
                                    (marker-position m1)
                                    (marker-position m2)
                                    (marker-position m3)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'sect)
                                    (get-text-property 6 'sect)
                                    (get-text-property 11 'sect)
                                    (get-text-property 16 'sect)
                                    (get-text-property 21 'sect)
                                    (get-text-property 26 'sect))))
                (kill-buffer buf)
                (list after-inner restored))))))))) "#,
        expect,
    );
}

#[test]
fn combo_save_restriction_nested_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-srnn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (put-text-property 26 30 'sect 'f)
      (put-text-property 31 35 'sect 'g)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 15 t))
            (m3 (copy-marker 25 nil))
            (ov (make-overlay 6 30)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (save-restriction
          (narrow-to-region 6 30)
          (save-restriction
            (narrow-to-region 11 25)
            (goto-char (point-min))
            (insert "INNER-"))
          (goto-char (point-min))
          (insert "OUTER-"))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 21 'sect)
                           (get-text-property 26 'sect)
                           (get-text-property 31 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 11 'sect)
                                (get-text-property 16 'sect)
                                (get-text-property 21 'sect)
                                (get-text-property 26 'sect)
                                (get-text-property 31 'sect))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_narrow_regex_replace_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-nrr")))
    (with-current-buffer buf
      (insert "alpha:100 beta:200 gamma:300 delta:400 epsilon:500")
      (put-text-property 1 10 'grp 'g1)
      (put-text-property 11 20 'grp 'g2)
      (put-text-property 21 30 'grp 'g3)
      (put-text-property 31 40 'grp 'g4)
      (put-text-property 41 51 'grp 'g5)
      (let ((m1 (copy-marker 10 nil))
            (m2 (copy-marker 20 t))
            (m3 (copy-marker 30 nil))
            (ov (make-overlay 11 40)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 11 40)
        (goto-char (point-min))
        (while (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" nil t)
          (replace-match "\\1=\\2" t))
        (widen)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 11 'grp)
                           (get-text-property 21 'grp)
                           (get-text-property 31 'grp)
                           (get-text-property 41 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'grp)
                                (get-text-property 11 'grp)
                                (get-text-property 21 'grp)
                                (get-text-property 31 'grp)
                                (get-text-property 41 'grp))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_narrow_delete_region_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 26 26)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ndr")))
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
            (m3 (copy-marker 25 nil))
            (ov (make-overlay 6 25)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 6 25)
        (delete-region (point-min) (+ (point-min) 5))
        (widen)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 26 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
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
fn combo_narrow_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-nbl")))
    (with-current-buffer buf
      (make-local-variable 'nbl-local)
      (setq nbl-local 'buffer-specific)
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
        (let ((narrowed-p (buffer-narrowed-p))
              (pmin (point-min))
              (pmax (point-max)))
          (goto-char (point-min))
          (insert (format "<%s>-" nbl-local))
          (widen)
          (let ((after (list (buffer-string)
                             narrowed-p pmin pmax
                             nbl-local
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'sect)
                             (get-text-property 6 'sect)
                             (get-text-property 16 'sect)
                             (get-text-property 21 'sect))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  nbl-local
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'sect)
                                  (get-text-property 6 'sect)
                                  (get-text-property 11 'sect)
                                  (get-text-property 16 'sect)
                                  (get-text-property 21 'sect))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}
