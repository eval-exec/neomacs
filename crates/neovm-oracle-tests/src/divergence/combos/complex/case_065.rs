//! Complex combo batch 65 — UTF-8 / multibyte deep dives targeting known
//! weak areas of the Neomacs string model: eight-bit byte classification,
//! `set-buffer-multibyte` raw-byte promotion, composition property format,
//! bidi auto-detection, and the `:charset` text property on file read.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx65_char_charset_classification_matrix_per_byte_0x80_0xff() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((128 eight-bit 128) (144 eight-bit 144) (160 eight-bit 160) (180 eight-bit 180) (200 eight-bit 200) (220 eight-bit 220) (240 eight-bit 240) (255 eight-bit 255))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (b)
          (let ((c (decode-char 'eight-bit b)))
            (list b (char-charset c) (encode-char c 'eight-bit))))
        '(128 144 160 180 200 220 240 255))
"##,
        expect,
    );
}

#[test]
fn div_cx65_string_make_multibyte_each_byte_eightbit_byte_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((128 1 2 unicode-bmp \"\u{80}\") (160 1 2 unicode-bmp \"\u{a0}\") (192 1 2 unicode-bmp \"À\") (255 1 2 unicode-bmp \"ÿ\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (b)
          (let* ((s (string-make-multibyte (string b)))
                 (first-char (aref s 0)))
            (list b (length s) (string-bytes s)
                  (char-charset first-char)
                  (char-to-string first-char))))
        '(#x80 #xa0 #xc0 #xff))
"##,
        expect,
    );
}

#[test]
fn div_cx65_decode_invalid_utf8_then_set_buffer_multibyte_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (4 9 (unicode-bmp eight-bit ascii unicode-bmp) 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((raw (unibyte-string #xe4 #xb8 #x96 #xff #x00 #xe7 #x95 #x8c))
       (decoded (decode-coding-string raw 'utf-8-unix t)))
  (list (length decoded)
        (string-bytes decoded)
        (mapcar #'char-charset (string-to-list decoded))
        (length (encode-coding-string decoded 'utf-8-unix))))
"##,
        expect,
    );
}

#[test]
fn div_cx65_buffer_set_multibyte_round_trip_with_raw_bytes_data_loss() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx65-tog*")))
  (with-current-buffer buf
    (set-buffer-multibyte nil)
    (erase-buffer)
    (insert (unibyte-string #x80 #x81 #x82 #x41 #x42 #xc3 #xa9))
    (let ((len-unibyte (buffer-size))
          (bytes-unibyte (string-bytes (buffer-string))))
      (set-buffer-multibyte t)
      (let ((len-mb (buffer-size))
            (bytes-mb (string-bytes (buffer-string))))
        (prog1 (list len-unibyte bytes-unibyte len-mb bytes-mb
                     (mapcar #'char-charset (string-to-list (buffer-string))))
          (kill-buffer buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx65_compose_region_find_composition_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function find-comcomposition)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café 世界 hello")
  (compose-region 1 4 "")
  (compose-region 7 8 "")
  (list (find-composition 1)
        (find-composition 2)
        (find-composition 5)
        (find-comcomposition 7)
        (get-text-property 1 'composition)
        (get-text-property 7 'composition)))
"##,
        expect,
    );
}

#[test]
fn div_cx65_compose_string_find_composition_after_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"café\" 1 4 (composition ((3 . \"\")))) 4 (composition ((3 . \"\"))) ((3 . \"\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (compose-string "café" 1 4 ""))
       (props (text-properties-at 1 s))
       (comp (get-text-property 1 'composition s)))
  (list s (length s) props comp))
"##,
        expect,
    );
}

#[test]
fn div_cx65_current_bidi_paragraph_direction_per_script() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (left-to-right right-to-left right-to-left left-to-right left-to-right left-to-right right-to-left)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer (insert "Hello world") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "مرحبا بالعالم") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "שלום עולם") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "12345") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "你好世界") (current-bidi-paragraph-direction))
 (let ((buf (get-buffer-create " *neo-cx65-bidi-explicit*")))
   (with-current-buffer buf
     (erase-buffer)
     (insert "Hello world")
     (setq bidi-paragraph-direction 'right-to-left)
     (prog1 (current-bidi-paragraph-direction)
       (kill-buffer buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx65_encode_utf8_with_signature_bom_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11 14 239 187 191 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((plain "café世界")
       (no-sig (encode-coding-string plain 'utf-8))
       (with-sig (encode-coding-string plain 'utf-8-with-signature)))
  (list (string-bytes no-sig)
        (string-bytes with-sig)
        (aref with-sig 0)
        (aref with-sig 1)
        (aref with-sig 2)
        (string= (substring with-sig 3) no-sig)))
"##,
        expect,
    );
}

#[test]
fn div_cx65_decode_coding_region_in_buffer_with_invalid_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-number-of-arguments decode-coding-region 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string #x68 #x65 #x6c #x6c #x6f #xff #xfe))
  (set-buffer-multibyte t)
  (decode-coding-region (point-min) (point-max) 'utf-8-unix (current-buffer) t)
  (list (buffer-string) (buffer-size) (string-bytes (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx65_charset_plist_builtins_complete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((ascii ascii 1 [0 127 0 0 0 0 0 0]) (unicode unicode 3 [0 255 0 255 0 16 0 0]) (eight-bit-control eight-bit-control 1 [128 159]) (eight-bit-graphic eight-bit-graphic 1 [160 255]) (iso-8859-1 iso-8859-1 1 [0 255 0 0 0 0 0 0]) (latin-iso8859-1 latin-iso8859-1 1 [32 127]) (mule-unicode-0100-24ff mule-unicode-0100-24ff 2 [32 127 32 127]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (let ((p (charset-plist cs)))
            (list cs (plist-get p :name)
                  (plist-get p :dimension)
                  (plist-get p :code-space))))
        '(ascii unicode eight-bit-control eight-bit-graphic
          iso-8859-1 latin-iso8859-1 mule-unicode-0100-24ff))
"##,
        expect,
    );
}

#[test]
fn div_cx65_charset_chars_per_charset_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((ascii 128 1) (unicode 256 3) (eight-bit 128 1) (iso-8859-1 256 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (mapcar (lambda (cs) (list cs (charset-chars cs) (charset-dimension cs)))
            '(ascii unicode eight-bit iso-8859-1))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx65_format_S_of_recovered_eightbit_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\"世\\\\377\\0\\\\376\\\"\" \"\\\"\\\\377\\\\376\\\"\" nil nil \"cb71e3dc5c1e08ac536757e5f9f6a17d\" \"f3b25701fe362ec84616a93a45ce9998\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((raw (unibyte-string #xe4 #xb8 #x96 #xff #x00 #xfe))
       (decoded (decode-coding-string raw 'utf-8-unix t))
       (made (string-make-multibyte (unibyte-string #xff #xfe))))
  (list (format "%S" decoded)
        (format "%S" made)
        (string= decoded made)
        (equal decoded made)
        (md5 decoded)
        (md5 made)))
"##,
        expect,
    );
}

#[test]
fn div_cx65_string_bytes_vs_length_multibyte_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument stringp (decode-coding-string (unibyte-string 255) 'utf-8-unix t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (list (length s) (string-bytes s)
                (seq-map #'char-charset (string-to-list s))))
        '("ascii"
          "café"
          "世界"
          "😀"
          (decode-coding-string (unibyte-string #xff) 'utf-8-unix t)))
"##,
        expect,
    );
}

#[test]
fn div_cx65_detect_coding_string_with_bom_at_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 iso-latin-1 emacs-mule in-is13194-devanagari utf-8-auto utf-8-with-signature japanese-shift-jis chinese-big5 iso-2022-8bit-ss2) (iso-latin-1 in-is13194-devanagari chinese-iso-8bit chinese-big5 iso-2022-8bit-ss2) (iso-latin-1 in-is13194-devanagari chinese-iso-8bit chinese-big5 iso-2022-8bit-ss2) (undecided))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((bom-utf8 (unibyte-string #xef #xbb #xbf))
       (bom-utf16le (unibyte-string #xff #xfe))
       (bom-utf16be (unibyte-string #xfe #xff)))
  (list (detect-coding-string (concat bom-utf8 "hello"))
        (detect-coding-string (concat bom-utf16le "hello"))
        (detect-coding-string (concat bom-utf16be "hello"))
        (detect-coding-string "plain ascii")))
"##,
        expect,
    );
}

#[test]
fn div_cx65_set_buffer_multibyte_overlay_marker_textprop_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx65-mega*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (set-buffer-multibyte t)
    (insert "pre café 世界 0123456789 post")
    (put-text-property 1 5 'face 'bold)
    (put-text-property 7 11 'display "XX")
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 3 25)
      (undo-boundary)
      (set-buffer-multibyte nil)
      (let ((len-unibyte (buffer-size))
            (marker-pos (marker-position m))
            (overlay-state (list (overlayp ov) (overlay-start ov) (overlay-end ov))))
        (set-buffer-multibyte t)
        (let ((len-mb (buffer-size))
              (marker-pos-2 (marker-position m))
              (overlay-state-2 (list (overlayp ov) (overlay-start ov) (overlay-end ov))))
          (undo) (undo)
          (widen)
          (prog1 (list len-unibyte marker-pos overlay-state
                       len-mb marker-pos-2 overlay-state-2
                       (buffer-string) (buffer-size)
                       (text-properties-at 1))
            (kill-buffer buf))))))
"##,
        expect,
    );
}
