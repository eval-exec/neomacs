//! Deep combo: hash-table × record × make-hash-table × puthash ×
//! gethash × remhash × clrhash × maphash × marker × overlay ×
//! text-prop × undo × buffer-local × narrow × insert × delete.
//!
//! Stresses hash-table and record interactions with buffer state:
//! hash-tables as buffer-local values, records in overlays, and
//! hash-table operations during edits. Hash-tables are tricky because
//! they involve complex data structures that must be preserved correctly
//! across buffer operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_hash_table_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-htbl"))
        (ht (make-hash-table :test 'equal)))
    (puthash "key1" 'val1 ht)
    (puthash "key2" 'val2 ht)
    (with-current-buffer buf
      (make-local-variable 'my-hash)
      (setq my-hash ht)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (overlay-put ov 'hash ht)
        (undo-boundary)
        (puthash "key3" 'val3 my-hash)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (gethash "key1" my-hash)
                           (gethash "key2" my-hash)
                           (gethash "key3" my-hash)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (overlay-get ov 'hash)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (gethash "key1" my-hash)
                                (gethash "key2" my-hash)
                                (gethash "key3" my-hash)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (overlay-get ov 'hash)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_record_overlay_marker_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-rec")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15))
            (rec (record 'my-type 'data '(a b c))))
        (overlay-put ov 'scope 'all)
        (overlay-put ov 'record rec)
        (undo-boundary)
        (goto-char 5)
        (insert "XX")
        (let* ((ov-rec (overlay-get ov 'record))
               (after (list (buffer-string)
                            (aref ov-rec 0)
                            (aref ov-rec 1)
                            (aref ov-rec 2)
                            (marker-position m1)
                            (marker-position m2)
                            (overlay-start ov) (overlay-end ov)
                            (get-text-property 1 'zone)
                            (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let* ((ov-rec-restored (overlay-get ov 'record))
                 (restored (list (buffer-string)
                                 (aref ov-rec-restored 0)
                                 (aref ov-rec-restored 1)
                                 (aref ov-rec-restored 2)
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
fn combo_hash_table_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-htn"))
        (ht (make-hash-table :test 'equal)))
    (puthash "x" 1 ht)
    (puthash "y" 2 ht)
    (with-current-buffer buf
      (make-local-variable 'narrow-hash)
      (setq narrow-hash ht)
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
        (puthash "z" 3 narrow-hash)
        (goto-char (point-min))
        (insert "XX-")
        (widen)
        (let ((after (list (buffer-string)
                           (gethash "x" narrow-hash)
                           (gethash "y" narrow-hash)
                           (gethash "z" narrow-hash)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 21 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (gethash "x" narrow-hash)
                                (gethash "y" narrow-hash)
                                (gethash "z" narrow-hash)
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
fn combo_maphash_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-maph"))
        (ht (make-hash-table :test 'equal)))
    (puthash "a" 1 ht)
    (puthash "b" 2 ht)
    (puthash "c" 3 ht)
    (with-current-buffer buf
      (make-local-variable 'map-hash)
      (setq map-hash ht)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((sum 0))
          (maphash (lambda (k v) (setq sum (+ sum v))) map-hash)
          (goto-char 5)
          (insert (format "-<%d>-" sum)))
        (let ((after (list (buffer-string)
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
fn combo_clrhash_remhash_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-clrh"))
        (ht (make-hash-table :test 'equal)))
    (puthash "x" 10 ht)
    (puthash "y" 20 ht)
    (with-current-buffer buf
      (make-local-variable 'clr-hash)
      (setq clr-hash ht)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (remhash "x" clr-hash)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (gethash "x" clr-hash)
                           (gethash "y" clr-hash)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (gethash "x" clr-hash)
                                (gethash "y" clr-hash)
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
