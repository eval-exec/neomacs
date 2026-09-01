//! Deep combo: vector × char-table × make-vector × vconcat ×
//! marker × overlay × text-prop × undo × buffer-local × narrow ×
//! insert × delete × replace-match.
//!
//! Stresses vector and char-table interactions with buffer state:
//! vectors as buffer-local values, char-tables for syntax, and
//! vector operations during edits. Vectors are tricky because they
//! involve complex data structures that must be preserved correctly
//! across buffer operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_vector_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-vec"))
        (vec (vector 1 2 3 4 5)))
    (with-current-buffer buf
      (make-local-variable 'my-vec)
      (setq my-vec vec)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (overlay-put ov 'vector vec)
        (undo-boundary)
        (aset my-vec 0 99)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (aref my-vec 0)
                           (aref my-vec 1)
                           (length my-vec)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (overlay-get ov 'vector)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (aref my-vec 0)
                                (aref my-vec 1)
                                (length my-vec)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (overlay-get ov 'vector)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_vconcat_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-vcon"))
        (v1 (vector 'a 'b 'c))
        (v2 (vector 'd 'e 'f)))
    (with-current-buffer buf
      (make-local-variable 'concat-vec)
      (setq concat-vec (vconcat v1 v2))
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
        (insert (format "-<%S>-" concat-vec))
        (let ((after (list (buffer-string)
                           (length concat-vec)
                           (aref concat-vec 0)
                           (aref concat-vec 3)
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
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_make_vector_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mvec")))
    (with-current-buffer buf
      (make-local-variable 'mv-vec)
      (setq mv-vec (make-vector 5 'init))
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (aset mv-vec 2 'updated)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (aref mv-vec 0)
                           (aref mv-vec 2)
                           (aref mv-vec 4)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (aref mv-vec 0)
                                (aref mv-vec 2)
                                (aref mv-vec 4)
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
fn combo_vector_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-vecn"))
        (vec (vector 10 20 30 40 50)))
    (with-current-buffer buf
      (make-local-variable 'nar-vec)
      (setq nar-vec vec)
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
        (aset nar-vec 0 999)
        (goto-char (point-min))
        (insert "XX-")
        (widen)
        (let ((after (list (buffer-string)
                           (aref nar-vec 0)
                           (aref nar-vec 1)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 21 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (aref nar-vec 0)
                                (aref nar-vec 1)
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
fn combo_char_table_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ctab"))
        (ct (make-char-table 'test-table nil)))
    (aset ct ?a 'lower)
    (aset ct ?A 'upper)
    (aset ct ?0 'digit)
    (with-current-buffer buf
      (make-local-variable 'my-ct)
      (setq my-ct ct)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (aset my-ct ?b 'new-val)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (aref my-ct ?a)
                           (aref my-ct ?A)
                           (aref my-ct ?b)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (aref my-ct ?a)
                                (aref my-ct ?A)
                                (aref my-ct ?b)
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
