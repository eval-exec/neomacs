//! Deep combo: buffer-string × substring × buffer-substring ×
//! insert-buffer-substring × erase-buffer × marker × overlay ×
//! text-prop × undo × buffer-local × narrow × buffer-modified-p.
//!
//! Stresses buffer content extraction and insertion: getting buffer
//! content as string, inserting from other buffers, and erasing.
//! These operations are tricky because they must preserve markers,
//! overlays, and text properties correctly.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_buffer_substring_insert_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (generate-new-buffer " combo-bs1"))
        (buf2 (generate-new-buffer " combo-bs2")))
    (with-current-buffer buf1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((sub (buffer-substring 6 10)))
          (with-current-buffer buf2
            (insert "XXXX-")
            (insert sub)
            (insert "-YYYY"))
          (goto-char 10)
          (insert sub))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone)
                           (with-current-buffer buf2 (buffer-string)))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf1)
            (kill-buffer buf2)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_insert_buffer_substring_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((src (generate-new-buffer " combo-ibss"))
        (dst (generate-new-buffer " combo-ibsd")))
    (with-current-buffer src
      (insert "SRC-TEXT")
      (put-text-property 1 4 'src 's)
      (put-text-property 5 9 'src 't))
    (with-current-buffer dst
      (insert "DST-PREFIX-DST-SUFFIX")
      (put-text-property 1 10 'dst 'prefix)
      (put-text-property 11 21 'dst 'suffix)
      (let ((m1 (copy-marker 10 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 21)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 10)
        (insert-buffer-substring src 1 4)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'dst)
                           (get-text-property 10 'dst)
                           (get-text-property 14 'dst))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'dst)
                                (get-text-property 11 'dst))))
            (kill-buffer src)
            (kill-buffer dst)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_erase_buffer_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-eb")))
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
        (let ((before-erase (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'zone)
                                  (buffer-modified-p))))
          (erase-buffer)
          (let ((after-erase (list (buffer-string)
                                   (marker-position m1)
                                   (marker-position m2)
                                   (and (overlay-start ov) t)
                                   (buffer-modified-p))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'zone)
                                  (get-text-property 6 'zone)
                                  (get-text-property 11 'zone)
                                  (buffer-modified-p))))
              (kill-buffer buf)
              (list before-erase after-erase restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_buffer_string_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-bsn")))
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
        (let ((narrowed (buffer-string)))
          (goto-char (point-min))
          (insert "XX-")
          (widen)
          (let ((after (list (buffer-string)
                             narrowed
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
fn combo_buffer_modified_p_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-bmp")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (set-buffer-modified-p nil)
        (undo-boundary)
        (goto-char 5)
        (insert "XX")
        (let ((after-insert (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (buffer-modified-p)
                                  (get-text-property 1 'zone)
                                  (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (buffer-modified-p)
                                  (get-text-property 1 'zone)
                                  (get-text-property 6 'zone)
                                  (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after-insert after-undo))))))) "#,
        expect,
    );
}
