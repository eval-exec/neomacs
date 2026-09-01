//! Complex combo batch 398 — `coding-system`/`charset` registry ultimate:
//! coding-system-p/type/mnemonic/category/aliases/plist matrix across all
//! major codings, charset-dimension/chars/plist, decode/encode roundtrip.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx398_coding_system_p_matrix_all_codings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 t) (utf-8-unix t) (utf-8-with-signature t) (latin-1 t) (iso-8859-1 t) (iso-8859-9 t) (utf-16 t) (utf-16le t) (utf-16be t) (big5 t) (gb2312 t) (no-conversion t) (undecided t) (binary t) (invalid-cs nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (list cs (coding-system-p cs)))
        '(utf-8 utf-8-unix utf-8-with-signature
          latin-1 iso-8859-1 iso-8859-9
          utf-16 utf-16le utf-16be
          big5 gb2312 no-conversion
          undecided binary invalid-cs))
"##,
        expect,
    )
}

#[test]
fn div_cx398_coding_system_type_mnemonic_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 utf-8 85) (utf-8-unix utf-8 85) (latin-1 charset 49) (iso-8859-9 charset 57) (utf-16 utf-16 85) (big5 big5 66) (gb2312 iso-2022 99))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (list cs
                (condition-case e (coding-system-type cs) (error :err))
                (condition-case e (coding-system-mnemonic cs) (error :err))))
        '(utf-8 utf-8-unix latin-1 iso-8859-9 utf-16 big5 gb2312))
"##,
        expect,
    )
}

#[test]
fn div_cx398_coding_system_category_matrix() {
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
fn div_cx398_coding_system_aliases_and_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 (utf-8 mule-utf-8 cp65001) (:ascii-compatible-p t :category coding-category-utf-8 :name utf-8 :docstring \"UTF-8 (no signature (BOM))\" :coding-type utf-8 :mnemonic 85 :charset-list (unicode) :mime-charset utf-8)) (utf-8-unix (utf-8-unix mule-utf-8-unix cp65001-unix) (:ascii-compatible-p t :category coding-category-utf-8 :name utf-8 :docstring \"UTF-8 (no signature (BOM))\" :coding-type utf-8 :mnemonic 85 :charset-list (unicode) :mime-charset utf-8)) (latin-1 (iso-latin-1 iso-8859-1 latin-1) (:ascii-compatible-p t :category coding-category-charset :name iso-latin-1 :docstring \"ISO 2022 based 8-bit encoding for Latin-1 (MIME:ISO-8859-1).\" :coding-type charset :mnemonic 49 :charset-list (iso-8859-1) :mime-charset iso-8859-1)) (iso-8859-9 (iso-latin-5 iso-8859-9 latin-5) (:ascii-compatible-p t :category coding-category-charset :name iso-latin-5 :docstring \"ISO 2022 based 8-bit encoding for Latin-5 (MIME:ISO-8859-9).\" :coding-type charset :mnemonic 57 :charset-list (iso-8859-9) :mime-charset iso-8859-9)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (list cs
                (condition-case e (coding-system-aliases cs) (error :err))
                (condition-case e (coding-system-plist cs) (error :err))))
        '(utf-8 utf-8-unix latin-1 iso-8859-9))
"##,
        expect,
    )
}

#[test]
fn div_cx398_encode_decode_roundtrip_all_major_codings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 18 t) (utf-8-unix 18 t) (latin-1 13 nil) (iso-8859-1 13 nil) (utf-16 28 t) (utf-16le 26 t) (utf-16be 26 t) (big5 15 nil) (gb2312 16 t) (no-conversion 18 nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((text "Hello café 世界"))
  (mapcar (lambda (cs)
            (condition-case e
                (let* ((enc (encode-coding-string text cs))
                      (dec (decode-coding-string enc cs)))
                 (list cs (string-bytes enc) (string= text dec)))
              (error (list cs :err (car e)))))
          '(utf-8 utf-8-unix latin-1 iso-8859-1 utf-16 utf-16le utf-16be
            big5 gb2312 no-conversion)))
"##,
        expect,
    )
}

#[test]
fn div_cx398_encode_utf8_with_signature_bom() {
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
fn div_cx398_decode_invalid_bytes_per_coding() {
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
                  (list cs (length dec)
                        (mapcar #'char-charset (string-to-list dec))))
              (error (list cs :err (car e)))))
          '(utf-8 latin-1 iso-8859-1 no-conversion)))
"##,
        expect,
    )
}

#[test]
fn div_cx398_charset_plist_and_dimension_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((ascii 1 \"ASCII\" \"ASCII (ISO646 IRV)\" [0 127 0 0 0 0 0 0]) (unicode 3 \"Unicode\" \"Unicode (ISO10646)\" [0 255 0 255 0 16 0 0]) (eight-bit 1 \"Raw bytes\" \"Raw bytes 128-255\" [128 255 0 0 0 0 0 0]) (iso-8859-1 1 \"Latin-1\" \"Latin-1 (ISO/IEC 8859-1)\" [0 255 0 0 0 0 0 0]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (let ((p (charset-plist cs)))
            (list cs (plist-get p :dimension)
                  (plist-get p :short-name)
                  (plist-get p :docstring)
                  (plist-get p :code-space))))
        '(ascii unicode eight-bit iso-8859-1))
"##,
        expect,
    )
}

#[test]
fn div_cx398_current_bidi_paragraph_direction_all_scripts() {
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
fn div_cx398_coding_charset_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((text "café 世界 😀 coding mega")
       (enc (encode-coding-string text 'utf-8))
       (hash (secure-hash 'sha256 enc)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert enc)
    (put-text-property 1 4 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 3 12)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 14)
      (let ((state (list (string= text (decode-coding-string enc 'utf-8-unix))
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
