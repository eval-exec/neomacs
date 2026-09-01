//! Complex combo batch 198 — `coding-system` encode/decode matrix across
//! ALL supported and unsupported codings, with multibyte payload.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx198_encode_decode_roundtrip_utf8_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 19 27 27)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((text "Hello café 世界 😀 end")
       (enc (encode-coding-string text 'utf-8))
       (dec (decode-coding-string enc 'utf-8-unix)))
  (list (string= text dec)
        (equal text dec)
        (length text)
        (string-bytes enc)
        (length enc)))
"##,
        expect,
    );
}

#[test]
fn div_cx198_encode_with_signature_bom_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11 14 239 187 191 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((text "café世界")
       (plain (encode-coding-string text 'utf-8))
       (sig (encode-coding-string text 'utf-8-with-signature)))
  (list (length plain)
        (length sig)
        (aref sig 0) (aref sig 1) (aref sig 2)
        (string= (substring sig 3) plain)))
"##,
        expect,
    );
}

#[test]
fn div_cx198_decode_invalid_bytes_per_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 7 9 (ascii ascii ascii ascii ascii eight-bit unicode-bmp)) (latin-1 8 11 (ascii ascii ascii ascii ascii unicode-bmp unicode-bmp unicode-bmp)) (iso-8859-1 8 11 (ascii ascii ascii ascii ascii unicode-bmp unicode-bmp unicode-bmp)) (no-conversion 8 8 (ascii ascii ascii ascii ascii unicode-bmp unicode-bmp unicode-bmp)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((raw (unibyte-string #x68 #x65 #x6c #x6c #x6f #xff #xc3 #xa9)))
  (mapcar (lambda (cs)
            (condition-case e
                (let ((dec (decode-coding-string raw cs t)))
                  (list cs (length dec) (string-bytes dec)
                        (mapcar #'char-charset (string-to-list dec))))
              (error (list cs :err (car e)))))
          '(utf-8 latin-1 iso-8859-1 no-conversion)))
"##,
        expect,
    );
}

#[test]
fn div_cx198_encode_coding_region_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\\377\\376\" 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string #x68 #x65 #x6c #x6c #x6f #xff #xfe))
  (set-buffer-multibyte t)
  (encode-coding-region (point-min) (point-max) 'utf-8-unix (current-buffer))
  (list (buffer-string) (buffer-size)))
"##,
        expect,
    );
}

#[test]
fn div_cx198_decode_coding_region_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-number-of-arguments decode-coding-region 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string #xe4 #xb8 #x96 #xe7 #x95 #x8c #x00 #x41))
  (set-buffer-multibyte t)
  (decode-coding-region (point-min) (point-max) 'utf-8-unix (current-buffer) t)
  (list (buffer-string) (buffer-size) (string-bytes (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx198_coding_system_plist_query_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 utf-8 85 utf-8 t) (utf-8-with-signature utf-8-with-signature 85 nil nil) (latin-1 iso-latin-1 49 iso-8859-1 t) (iso-8859-9 iso-latin-5 57 iso-8859-9 t) (utf-16 utf-16 85 utf-16 nil) (utf-16le utf-16le 85 utf-16le nil) (utf-16be utf-16be 85 utf-16be nil) (big5 chinese-big5 66 big5 t) (gb2312 chinese-iso-8bit 99 gb2312 t) (no-conversion no-conversion 61 nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (let ((p (coding-system-plist cs)))
            (list cs
                  (plist-get p :name)
                  (plist-get p :mnemonic)
                  (plist-get p :mime-charset)
                  (plist-get p :ascii-compatible-p))))
        '(utf-8 utf-8-with-signature latin-1 iso-8859-9
          utf-16 utf-16le utf-16be big5 gb2312 no-conversion))
"##,
        expect,
    );
}

#[test]
fn div_cx198_set_buffer_file_coding_system_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8-unix)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-file-coding-system 'utf-8-unix)
  (list (buffer-local-value 'buffer-file-coding-system (current-buffer))))
"##,
        expect,
    );
}

#[test]
fn div_cx198_string_make_unibyte_then_multibyte_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"café 世界\" \"caf� \u{16}L\" \"caf\\351 \u{16}L\" t nil t 7 7 7 12 7 8 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((mb "café 世界")
       (uni (string-make-unibyte mb))
       (back (string-make-multibyte uni)))
  (list mb uni back
        (multibyte-string-p mb)
        (multibyte-string-p uni)
        (multibyte-string-p back)
        (length mb) (length uni) (length back)
        (string-bytes mb) (string-bytes uni) (string-bytes back)
        (equal mb back)))
"##,
        expect,
    );
}

#[test]
fn div_cx198_coding_system_aliases_and_parents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 :err void-function) (utf-8-unix :err void-function) (latin-1 :err void-function) (iso-8859-1 :err void-function))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (condition-case e
              (list cs (coding-system-aliases cs)
                    (coding-system-parent cs))
            (error (list cs :err (car e)))))
        '(utf-8 utf-8-unix latin-1 iso-8859-1))
"##,
        expect,
    );
}

#[test]
fn div_cx198_coding_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((text "café 世界 😀 hello")
       (enc (encode-coding-string text 'utf-8))
       (dec (decode-coding-string enc 'utf-8-unix))
       (hash (secure-hash 'sha256 enc)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert dec)
    (put-text-property 1 4 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 3 12)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 14)
      (let ((state (list (string= text dec)
                         (length enc) (string-bytes enc)
                         hash
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
