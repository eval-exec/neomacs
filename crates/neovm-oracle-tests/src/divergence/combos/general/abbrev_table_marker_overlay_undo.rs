//! Deep combo: abbrev × abbrev-table × define-abbrev × abbrev-table-p ×
//! abbrev-table-name × marker × overlay × textprop × undo × buffer-local ×
//! narrow.
//!
//! Stresses abbrev system with buffer state: defining abbreviations,
//! expanding them, and managing abbrev tables while preserving markers,
//! overlays, text properties, and undo state. The abbrev system is
//! complex because it involves hook-based expansion that modifies buffer
//! content and must interact correctly with the buffer's edit pipeline.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_define_abbrev_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-dabbrev"))
        (table (make-abbrev-table)))
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
        (define-abbrev table "myabbrev" "EXPANDED" nil)
        (goto-char 5)
        (insert (format "-<%s>-" (abbrev-table-p table)))
        (let ((after (list (buffer-string)
                           (abbrev-table-p table)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
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
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_abbrev_table_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-atn"))
        (table (make-abbrev-table)))
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
        (define-abbrev table "myab" "EXPANDED" nil)
        (goto-char (point-min))
        (insert (format "<%s>-" (abbrev-table-p table)))
        (widen)
        (let ((after (list (buffer-string)
                           (abbrev-table-p table)
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
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_abbrev_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-abl"))
        (table (make-abbrev-table)))
    (with-current-buffer buf
      (make-local-variable 'abbrev-local)
      (setq abbrev-local 'buffer-specific)
      (define-abbrev table "myab" "EXPANDED" nil)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (insert (format "-<%s:%s>-" abbrev-local (abbrev-table-p table)))
        (let ((after (list (buffer-string)
                           abbrev-local
                           (abbrev-table-p table)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                abbrev-local
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_abbrev_table_name_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-atname"))
        (table (make-abbrev-table)))
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
        (define-abbrev table "myab" "EXPANDED" nil :name "my-abbrev")
        (goto-char 5)
        (insert (format "-<%s>-" (abbrev-table-p table)))
        (let ((after (list (buffer-string)
                           (abbrev-table-p table)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
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
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_abbrev_multi_buffer_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (generate-new-buffer " combo-abm1"))
        (buf2 (generate-new-buffer " combo-abm2"))
        (table (make-abbrev-table)))
    (define-abbrev table "myab" "EXPANDED" nil)
    (with-current-buffer buf1
      (make-local-variable 'ab-local)
      (setq ab-local 'buf1)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c))
    (with-current-buffer buf2
      (make-local-variable 'ab-local)
      (setq ab-local 'buf2)
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'd)
      (put-text-property 6 10 'zone 'e)
      (put-text-property 11 15 'zone 'f))
    (let ((results nil))
      (with-current-buffer buf1
        (let ((m (copy-marker 5 nil))
              (ov (make-overlay 1 15)))
          (overlay-put ov 'scope 'all)
          (undo-boundary)
          (goto-char 5)
          (insert (format "<%s:%s>-" ab-local (abbrev-table-p table)))
          (push (list (buffer-string) ab-local
                      (marker-position m)
                      (overlay-start ov) (overlay-end ov))
                results)
          (primitive-undo 1 buffer-undo-list)
          (push (list (buffer-string) ab-local
                      (marker-position m)
                      (overlay-start ov) (overlay-end ov))
                results)))
      (with-current-buffer buf2
        (push (list (buffer-string) ab-local) results))
      (kill-buffer buf1)
      (kill-buffer buf2)
      (list (nreverse results)))) "#,
        expect,
    );
}
