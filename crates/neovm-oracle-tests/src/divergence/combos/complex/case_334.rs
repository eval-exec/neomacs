//! Complex combo batch 334 — `coding-system`/`charset` ultimate:
//! encode/decode with utf-8/latin-1/big5/utf-16, BOM check, category
//! matrix, char-charset for eight-bit, charset-plist completeness.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx334_encode_decode_roundtrip_all_major_codings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 18 18 t) (utf-8-unix 18 18 t) (latin-1 13 13 nil) (iso-8859-1 13 13 nil) (utf-16 28 28 t) (utf-16le 26 26 t) (utf-16be 26 26 t) (big5 15 15 nil) (gb2312 16 16 t) (no-conversion 18 18 nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((text "Hello café 世界"))
  (mapcar (lambda (cs)
            (condition-case e
                (let* ((enc (encode-coding-string text cs))
                      (dec (decode-coding-string enc cs)))
                 (list cs (length enc) (string-bytes enc) (string= text dec)))
              (error (list cs :err (car e)))))
          '(utf-8 utf-8-unix latin-1 iso-8859-1 utf-16 utf-16le utf-16be
            big5 gb2312 no-conversion)))
"##,
        expect,
    )
}

#[test]
fn div_cx334_encode_utf8_with_signature_bom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11 14 239 187 191 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((text "café世界")
       (plain (encode-coding-string text 'utf-8))
       (sig (encode-coding-string text 'utf-8-with-signature)))
  (list (string-bytes plain) (string-bytes sig)
        (aref sig 0) (aref sig 1) (aref sig 2)
        (string= (substring sig 3) plain)))
"##,
        expect,
    )
}

#[test]
fn div_cx334_decode_invalid_utf8_bytes_per_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 7 (ascii ascii ascii ascii ascii eight-bit unicode-bmp)) (latin-1 8 (ascii ascii ascii ascii ascii unicode-bmp unicode-bmp unicode-bmp)) (iso-8859-1 8 (ascii ascii ascii ascii ascii unicode-bmp unicode-bmp unicode-bmp)) (no-conversion 8 (ascii ascii ascii ascii ascii unicode-bmp unicode-bmp unicode-bmp)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((raw (unibyte-string #x68 #x65 #x6c #x6c #x6f #xff #xc3 #xa9)))
  (mapcar (lambda (cs)
            (condition-case e
                (let ((dec (decode-coding-string raw cs t)))
                  (list cs (length dec) (mapcar #'char-charset (string-to-list dec))))
              (error (list cs :err (car e)))))
          '(utf-8 latin-1 iso-8859-1 no-conversion)))
"##,
        expect,
    )
}

#[test]
fn div_cx334_char_charset_classification_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((128 eight-bit) (144 eight-bit) (160 eight-bit) (180 eight-bit) (200 eight-bit) (220 eight-bit) (240 eight-bit) (255 eight-bit))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (b)
          (let ((c (decode-char 'eight-bit b)))
            (list b (char-charset c))))
        '(128 144 160 180 200 220 240 255))
"##,
        expect,
    )
}

#[test]
fn div_cx334_coding_system_category_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 coding-category-utf-8) (utf-8-with-signature coding-category-utf-8-sig) (latin-1 coding-category-charset) (iso-8859-7 coding-category-charset) (emacs-mule coding-category-emacs-mule) (utf-16 coding-category-utf-16-auto) (utf-16be coding-category-utf-16-be-nosig) (utf-16le coding-category-utf-16-le-nosig) (big5 coding-category-big5) (no-conversion coding-category-raw-text) (raw-text coding-category-raw-text) (undecided coding-category-undecided) (binary coding-category-raw-text))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (list cs (condition-case e (coding-system-category cs) (error :err))))
        '(utf-8 utf-8-with-signature latin-1 iso-8859-7
          emacs-mule utf-16 utf-16be utf-16le big5
          no-conversion raw-text undecided binary))
"##,
        expect,
    )
}

#[test]
fn div_cx334_charset_plist_completeness() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((ascii 1 \"ASCII\" \"ASCII (ISO646 IRV)\" [0 127 0 0 0 0 0 0]) (unicode 3 \"Unicode\" \"Unicode (ISO10646)\" [0 255 0 255 0 16 0 0]) (eight-bit 1 \"Raw bytes\" \"Raw bytes 128-255\" [128 255 0 0 0 0 0 0]) (iso-8859-1 1 \"Latin-1\" \"Latin-1 (ISO/IEC 8859-1)\" [0 255 0 0 0 0 0 0]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (let ((p (charset-plist cs)))
            (list cs (plist-get p :dimension) (plist-get p :short-name)
                  (plist-get p :docstring) (plist-get p :code-space))))
        '(ascii unicode eight-bit iso-8859-1))
"##,
        expect,
    )
}

#[test]
fn div_cx334_current_bidi_paragraph_direction_all_scripts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (left-to-right right-to-left right-to-left left-to-right left-to-right)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer (insert "Hello world") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "مرحبا بالعالم") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "שלום עולם") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "你好世界") (current-bidi-paragraph-direction)))
"##,
        expect,
    )
}

#[test]
fn div_cx334_set_buffer_multibyte_toggle_data_loss() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx334-tog*")))
  (with-current-buffer buf
    (set-buffer-multibyte t)
    (insert "café 世界 0123456789ABCDEF0123456789")
    (let ((len-mb (buffer-size))
          (bytes-mb (string-bytes (buffer-string))))
      (set-buffer-multibyte nil)
      (let ((len-uni (buffer-size)))
        (set-buffer-multibyte t)
        (let ((len-back (buffer-size)))
        (prog1 (list len-mb bytes-mb len-uni len-back)
          (kill-buffer buf))))))
"##,
        expect,
    )
}

#[test]
fn div_cx334_string_make_unibyte_multibyte_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"café 世界\" \"caf� \u{16}L\" \"caf\\351 \u{16}L\" t nil t 7 7 7 12 7 8)""#
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
        (string-bytes mb) (string-bytes uni) (string-bytes back)))
"##,
        expect,
    )
}

#[test]
fn div_cx334_coding_charset_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((text "café 世界 😀 coding mega")
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
                         (length enc) (string-bytes enc) hash
                         (coding-system-category 'utf-8)
                         (charset-plist 'ascii)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    )
}
