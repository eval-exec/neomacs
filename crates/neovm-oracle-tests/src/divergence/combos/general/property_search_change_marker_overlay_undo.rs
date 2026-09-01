//! Deep combo: text-property-search-forward × text-property-search-backward ×
//! next-single-property-change × previous-single-property-change ×
//! next-property-change × previous-property-change ×
//! marker × overlay × textprop × undo × buffer-local × narrow.
//!
//! Stresses text property search/scan operations with buffer state:
//! searching for text properties, finding property boundaries, and
//! navigating by property changes while preserving markers, overlays,
//! and undo state. These operations are complex because they involve
//! interval tree traversal that must interact correctly with the
//! buffer's edit pipeline.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_next_single_property_change_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-nspc")))
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
        (let ((change1 (next-single-property-change 1 'grp))
              (change2 (next-single-property-change 6 'grp))
              (change3 (next-single-property-change 11 'grp)))
          (goto-char 5)
          (insert "XX")
          (let ((after (list (buffer-string)
                             change1 change2 change3
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'grp)
                             (get-text-property 6 'grp)
                             (get-text-property 12 'grp)
                             (get-text-property 18 'grp))))
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
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_previous_single_property_change_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-pspc")))
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
        (let ((change1 (previous-single-property-change 20 'grp))
              (change2 (previous-single-property-change 15 'grp))
              (change3 (previous-single-property-change 10 'grp)))
          (goto-char 10)
          (insert "XX")
          (let ((after (list (buffer-string)
                             change1 change2 change3
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'grp)
                             (get-text-property 6 'grp)
                             (get-text-property 12 'grp)
                             (get-text-property 18 'grp))))
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
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_narrow_property_change_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-npc")))
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
        (let ((changes nil)
              (pos (point-min)))
          (while (< pos (point-max))
            (let ((next (next-single-property-change pos 'sect nil (point-max))))
              (push next changes)
              (setq pos next)))
          (widen)
          (goto-char 5)
          (insert "XX-")
          (let ((after (list (buffer-string)
                             (nreverse changes)
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
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_property_search_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-psrch")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 6 20)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 6 20)
        (goto-char (point-min))
        (let ((results nil))
          (while (< (point) (point-max))
            (let* ((match (text-property-search-forward 'zone))
                   (found (and match (prop-match-beginning match)))
                   (val (and match (prop-match-value match))))
              (when match
                (push (list found val) results)
                (goto-char (prop-match-end match)))))
          (widen)
          (goto-char 5)
          (insert "XX")
          (let ((after (list (buffer-string)
                             (nreverse results)
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'zone)
                             (get-text-property 6 'zone)
                             (get-text-property 16 'zone)
                             (get-text-property 21 'zone))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'zone)
                                  (get-text-property 6 'zone)
                                  (get-text-property 11 'zone)
                                  (get-text-property 16 'zone)
                                  (get-text-property 21 'zone))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_property_search_backward_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function text-property-search-backward)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-psb")))
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
        (goto-char (point-max))
        (let ((results nil))
          (while (> (point) (point-min))
            (let* ((match (text-property-search-backward 'grp))
                   (found (and match (prop-match-beginning match)))
                   (val (and match (prop-match-value match))))
              (when match
                (push (list found val) results)
                (goto-char (prop-match-beginning match)))))
          (goto-char 5)
          (insert "XX")
          (let ((after (list (buffer-string)
                             results
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'grp)
                             (get-text-property 6 'grp)
                             (get-text-property 12 'grp)
                             (get-text-property 18 'grp))))
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
              (list after restored)))))))) "#,
        expect,
    );
}
