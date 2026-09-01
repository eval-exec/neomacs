//! Deep combo: marker insertion-type × insert × delete × undo ×
//! overlay × text-prop × buffer-local × narrow × regex × replace-match.
//!
//! Stresses marker insertion-type edge cases: markers with different
//! insertion types interacting with inserts, deletes, and replacements.
//! Marker insertion-type is tricky because it controls whether a marker
//! advances when text is inserted at its position.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_marker_insertion_type_insert_delete_undo_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mit")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m-nil (copy-marker 5 nil))
            (m-t   (copy-marker 10 t))
            (m-nil2 (copy-marker 15 nil))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (insert "XX")
        (goto-char 12)
        (insert "YY")
        (let ((after-insert (list (buffer-string)
                                  (marker-position m-nil)
                                  (marker-position m-t)
                                  (marker-position m-nil2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'grp)
                                  (get-text-property 6 'grp)
                                  (get-text-property 12 'grp)
                                  (get-text-property 18 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  (marker-position m-nil)
                                  (marker-position m-t)
                                  (marker-position m-nil2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'grp)
                                  (get-text-property 6 'grp)
                                  (get-text-property 11 'grp)
                                  (get-text-property 16 'grp))))
            (kill-buffer buf)
            (list after-insert after-undo))))))) "#,
        expect,
    );
}

#[test]
fn combo_marker_insertion_type_delete_region_undo_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mitdel")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (put-text-property 21 25 'grp 'e)
      (let ((m-nil (copy-marker 5 nil))
            (m-t   (copy-marker 10 t))
            (m-nil2 (copy-marker 15 nil))
            (m-t2  (copy-marker 20 t))
            (ov (make-overlay 1 25)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (delete-region 6 20)
        (let ((after-delete (list (buffer-string)
                                  (marker-position m-nil)
                                  (marker-position m-t)
                                  (marker-position m-nil2)
                                  (marker-position m-t2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'grp)
                                  (get-text-property 6 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  (marker-position m-nil)
                                  (marker-position m-t)
                                  (marker-position m-nil2)
                                  (marker-position m-t2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'grp)
                                  (get-text-property 6 'grp)
                                  (get-text-property 11 'grp)
                                  (get-text-property 16 'grp)
                                  (get-text-property 21 'grp))))
            (kill-buffer buf)
            (list after-delete after-undo))))))) "#,
        expect,
    );
}

#[test]
fn combo_marker_insertion_type_replace_match_undo_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mitrep")))
    (with-current-buffer buf
      (insert "AAAA-XXXXX-BBBB-XXXXX-CCCC")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 11 'grp 'x1)
      (put-text-property 12 16 'grp 'b)
      (put-text-property 17 22 'grp 'x2)
      (put-text-property 23 27 'grp 'c)
      (let ((m-nil (copy-marker 5 nil))
            (m-t   (copy-marker 12 t))
            (m-nil2 (copy-marker 16 nil))
            (ov (make-overlay 1 27)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (while (re-search-forward "XXXXX" nil t)
          (replace-match "XX"))
        (let ((after-replace (list (buffer-string)
                                   (marker-position m-nil)
                                   (marker-position m-t)
                                   (marker-position m-nil2)
                                   (overlay-start ov) (overlay-end ov)
                                   (get-text-property 1 'grp)
                                   (get-text-property 6 'grp)
                                   (get-text-property 10 'grp)
                                   (get-text-property 14 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  (marker-position m-nil)
                                  (marker-position m-t)
                                  (marker-position m-nil2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'grp)
                                  (get-text-property 6 'grp)
                                  (get-text-property 12 'grp)
                                  (get-text-property 17 'grp)
                                  (get-text-property 23 'grp))))
            (kill-buffer buf)
            (list after-replace after-undo))))))) "#,
        expect,
    );
}

#[test]
fn combo_marker_insertion_type_narrow_insert_undo_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mitnar")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((m-nil (copy-marker 5 nil))
            (m-t   (copy-marker 10 t))
            (m-nil2 (copy-marker 15 nil))
            (ov (make-overlay 6 20)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 6 20)
        (goto-char (point-min))
        (insert "XX-")
        (goto-char (point-max))
        (insert "-YY")
        (widen)
        (let ((after (list (buffer-string)
                           (marker-position m-nil)
                           (marker-position m-t)
                           (marker-position m-nil2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 21 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m-nil)
                                (marker-position m-t)
                                (marker-position m-nil2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 11 'sect)
                                (get-text-property 16 'sect)
                                (get-text-property 21 'sect))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_marker_insertion_type_buffer_local_insert_undo_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mitbl")))
    (with-current-buffer buf
      (make-local-variable 'mit-local)
      (setq mit-local 'buffer-specific)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m-nil (copy-marker 5 nil))
            (m-t   (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (insert "-XX-")
        (goto-char 14)
        (insert "-YY-")
        (let ((after (list (buffer-string)
                           mit-local
                           (marker-position m-nil)
                           (marker-position m-t)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone)
                           (get-text-property 12 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                mit-local
                                (marker-position m-nil)
                                (marker-position m-t)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}
