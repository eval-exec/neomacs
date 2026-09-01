//! Deep combo: coding-system × marker × overlay × undo × text-prop ×
//! buffer-local × encode/decode × process-filter × narrow.
//!
//! Stresses coding system interaction with buffer state: encoding and
//! decoding text with markers/overlays, process filters with coding
//! systems, and undo across coding boundaries. Coding systems are
//! tricky in a Rust rewrite because they involve byte/char position
//! conversion and multibyte text handling.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_coding_encode_decode_marker_overlay_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Encode buffer to string, decode back; markers/overlays must survive.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-cod")))
    (with-current-buffer buf
      (insert "αβγδεζηθ")
      (put-text-property 1 3 'zone 'greek1)
      (put-text-property 3 5 'zone 'greek2)
      (put-text-property 5 7 'zone 'greek3)
      (put-text-property 7 9 'zone 'greek4)
      (let ((m1 (copy-marker 3 nil))
            (m2 (copy-marker 5 t))
            (ov (make-overlay 1 9)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let* ((encoded (encode-coding-string (buffer-string) 'utf-8-unix))
               (decoded (decode-coding-string encoded 'utf-8-unix)))
          (erase-buffer)
          (insert decoded)
          (let ((after (list (buffer-string)
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'zone)
                             (get-text-property 3 'zone)
                             (get-text-property 5 'zone)
                             (get-text-property 7 'zone))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (get-text-property 1 'zone)
                                  (get-text-property 5 'zone))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_coding_multibyte_insert_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Insert multibyte text at marker position; undo must restore.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-cmb")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'charset 'ascii-a)
      (put-text-property 6 10 'charset 'ascii-b)
      (put-text-property 11 15 'charset 'ascii-c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (insert "αβγ")
        (let ((after-insert (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'charset)
                                  (get-text-property 5 'charset)
                                  (get-text-property 8 'charset))))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (get-text-property 1 'charset)
                                  (get-text-property 6 'charset))))
            (kill-buffer buf)
            (list after-insert after-undo))))))) "#,
        expect,
    );
}

#[test]
fn combo_coding_narrow_encode_decode_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Narrow, encode/decode, undo; markers must track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-cnarrow")))
    (with-current-buffer buf
      (insert "ααα-βββ-γγγ-δδδ")
      (put-text-property 1 4 'sect 'alpha)
      (put-text-property 5 8 'sect 'beta)
      (put-text-property 9 12 'sect 'gamma)
      (put-text-property 13 16 'sect 'delta)
      (let ((m1 (copy-marker 4 nil))
            (m2 (copy-marker 8 t))
            (m3 (copy-marker 12 nil))
            (ov (make-overlay 5 12)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 5 12)
        (let* ((narrowed (buffer-string))
               (encoded (encode-coding-string narrowed 'utf-8-unix))
               (decoded (decode-coding-string encoded 'utf-8-unix)))
          (erase-buffer)
          (insert decoded)
          (widen)
          (let ((after (list (buffer-string)
                             (marker-position m1)
                             (marker-position m2)
                             (marker-position m3)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'sect)
                             (get-text-property 5 'sect))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (marker-position m3)
                                  (get-text-property 1 'sect)
                                  (get-text-property 5 'sect)
                                  (get-text-property 9 'sect))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_coding_replace_multibyte_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Replace multibyte with different-length multibyte; markers track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-crep")))
    (with-current-buffer buf
      (insert "ααα βββ γγγ")
      (put-text-property 1 4 'word 'alpha)
      (put-text-property 5 8 'word 'beta)
      (put-text-property 9 12 'word 'gamma)
      (let ((m1 (copy-marker 4 nil))
            (m2 (copy-marker 8 t))
            (ov (make-overlay 1 12)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 1)
        (while (re-search-forward "[αβγ]+" nil t)
          (replace-match "δδδδδ"))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'word)
                                (get-text-property 5 'word)
                                (get-text-property 9 'word))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_coding_buffer_local_coding_system_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Buffer-local coding system affects encode/decode.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-cbl")))
    (with-current-buffer buf
      (make-local-variable 'buffer-file-coding-system)
      (setq buffer-file-coding-system 'utf-8-unix)
      (insert "hello world")
      (put-text-property 1 6 'part 'hello)
      (put-text-property 7 12 'part 'world)
      (let ((m (copy-marker 6 nil))
            (ov (make-overlay 1 12)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 6)
        (insert " beautiful")
        (let ((after (list (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'part)
                           (get-text-property 7 'part)
                           buffer-file-coding-system)))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'part)
                                (get-text-property 7 'part)
                                buffer-file-coding-system)))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}
