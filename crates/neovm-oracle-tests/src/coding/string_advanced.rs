//! Advanced oracle parity tests for encode/decode coding string primitives.
//!
//! Tests encode-coding-string and decode-coding-string with various
//! coding systems (utf-8, latin-1, raw-text, no-conversion), empty
//! strings, multibyte characters, multi-encoding comparisons, and
//! encoding detection heuristics.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// encode-coding-string with utf-8, latin-1, raw-text
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_string_advanced_encode_various_codings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; UTF-8 encoding of ASCII produces same bytes
  (string= (encode-coding-string "hello" 'utf-8) "hello")
  ;; Length of UTF-8 encoded ASCII
  (length (encode-coding-string "hello" 'utf-8))
  ;; latin-1 encoding of ASCII
  (string= (encode-coding-string "abc" 'latin-1) "abc")
  ;; raw-text encoding of ASCII
  (string= (encode-coding-string "test" 'raw-text) "test")
  ;; UTF-8 byte count for 2-byte char (e-acute)
  (string-bytes (encode-coding-string "\u00E9" 'utf-8))
  ;; UTF-8 byte count for 3-byte char (CJK)
  (string-bytes (encode-coding-string "\u4E2D" 'utf-8))
  ;; Latin-1 can encode latin-1 chars in single byte
  (string-bytes (encode-coding-string "\u00E9" 'latin-1))
  ;; Compare byte lengths across encodings for the same string
  (let ((s "\u00E9"))
    (list (string-bytes (encode-coding-string s 'utf-8))
          (string-bytes (encode-coding-string s 'latin-1)))))"#;
    let expect = expect_test::expect![[r#""OK (t 5 t t 2 3 1 (2 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// decode-coding-string roundtrip verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_string_advanced_decode_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Roundtrip: encode then decode should be identity
  (let* ((orig "Hello, World!")
         (enc (encode-coding-string orig 'utf-8))
         (dec (decode-coding-string enc 'utf-8)))
    (string= orig dec))
  ;; Roundtrip with non-ASCII
  (let* ((orig "\u00E9\u00E8\u00EA")
         (enc (encode-coding-string orig 'utf-8))
         (dec (decode-coding-string enc 'utf-8)))
    (string= orig dec))
  ;; Roundtrip with latin-1
  (let* ((orig "\u00C0\u00C1\u00C2")
         (enc (encode-coding-string orig 'latin-1))
         (dec (decode-coding-string enc 'latin-1)))
    (string= orig dec))
  ;; Roundtrip: CJK with utf-8
  (let* ((orig "\u4E2D\u6587\u6D4B\u8BD5")
         (enc (encode-coding-string orig 'utf-8))
         (dec (decode-coding-string enc 'utf-8)))
    (string= orig dec))
  ;; Roundtrip preserves string length
  (let* ((orig "abc\u00E9\u4E2D")
         (enc (encode-coding-string orig 'utf-8))
         (dec (decode-coding-string enc 'utf-8)))
    (= (length orig) (length dec)))
  ;; Decode unibyte string
  (multibyte-string-p (decode-coding-string "abc" 'utf-8)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// encode-coding-string on empty string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_string_advanced_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Empty string with utf-8
  (encode-coding-string "" 'utf-8)
  (decode-coding-string "" 'utf-8)
  ;; Empty string with latin-1
  (encode-coding-string "" 'latin-1)
  (decode-coding-string "" 'latin-1)
  ;; Empty string with raw-text
  (encode-coding-string "" 'raw-text)
  (decode-coding-string "" 'raw-text)
  ;; Empty string with no-conversion
  (encode-coding-string "" 'no-conversion)
  (decode-coding-string "" 'no-conversion)
  ;; Properties of empty encoded strings
  (length (encode-coding-string "" 'utf-8))
  (string-bytes (encode-coding-string "" 'utf-8))
  (string= (encode-coding-string "" 'utf-8) "")
  (string= (decode-coding-string "" 'utf-8) ""))"#;
    let expect =
        expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" 0 0 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// encode-coding-string with multibyte characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_string_advanced_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Multibyte chars: compare byte counts across UTF-8 encoding
  ;; 1-byte (ASCII): A = 0x41
  (string-bytes (encode-coding-string "A" 'utf-8))
  ;; 2-byte: e-acute = 0xC3 0xA9
  (string-bytes (encode-coding-string "\u00E9" 'utf-8))
  ;; 3-byte: CJK ideograph = 0xE4 0xB8 0xAD
  (string-bytes (encode-coding-string "\u4E2D" 'utf-8))
  ;; 4-byte: emoji = 0xF0 0x9F 0x98 0x80
  (string-bytes (encode-coding-string "\U0001F600" 'utf-8))
  ;; Mixed multibyte string
  (let* ((s "A\u00E9\u4E2D\U0001F600")
         (encoded (encode-coding-string s 'utf-8)))
    (list (length s)                     ;; 4 chars
          (string-bytes encoded)          ;; 1+2+3+4 = 10 bytes
          (string= s (decode-coding-string encoded 'utf-8))))
  ;; Multiple CJK characters
  (let ((s "\u5F00\u53D1\u8005"))
    (list (length s)
          (string-bytes (encode-coding-string s 'utf-8))))
  ;; Greek text
  (let ((s "\u03B1\u03B2\u03B3\u03B4"))
    (list (length s)
          (string-bytes (encode-coding-string s 'utf-8)))))"#;
    let expect = expect_test::expect![[r#""OK (1 2 3 4 (4 10 t) (3 9) (4 8))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// encode/decode with no-conversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_string_advanced_no_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; no-conversion treats input as raw bytes
  (encode-coding-string "abc" 'no-conversion)
  (decode-coding-string "abc" 'no-conversion)
  ;; Roundtrip with no-conversion
  (let* ((orig "test\n123")
         (enc (encode-coding-string orig 'no-conversion))
         (dec (decode-coding-string enc 'no-conversion)))
    (string= orig dec))
  ;; no-conversion preserves ASCII byte values
  (let* ((s "Hello")
         (enc (encode-coding-string s 'no-conversion)))
    (list (length enc) (string-bytes enc)))
  ;; Compare no-conversion vs raw-text for ASCII
  (let ((s "abc"))
    (string= (encode-coding-string s 'no-conversion)
             (encode-coding-string s 'raw-text)))
  ;; No-conversion on string with newlines and tabs
  (let* ((s "line1\nline2\ttab")
         (enc (encode-coding-string s 'no-conversion))
         (dec (decode-coding-string enc 'no-conversion)))
    (string= s dec)))"#;
    let expect = expect_test::expect![[r#""OK (\"abc\" \"abc\" t (5 5) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-encoding comparison (same string, different codings)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_string_advanced_multi_encoding_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare how the same string looks when encoded with different systems
    let form = r#"(progn
  (fset 'neovm--csa-compare-encodings
    (lambda (s codings)
      "Encode S with each coding in CODINGS, return list of (coding byte-count roundtrip-ok)."
      (mapcar (lambda (coding)
                (condition-case nil
                    (let* ((encoded (encode-coding-string s coding))
                           (decoded (decode-coding-string encoded coding))
                           (ok (string= s decoded)))
                      (list coding (string-bytes encoded) ok))
                  (error (list coding 'error nil))))
              codings)))

  (unwind-protect
      (let ((codings '(utf-8 utf-8-unix no-conversion raw-text)))
        (list
          ;; Pure ASCII: all encodings produce same byte count
          (funcall 'neovm--csa-compare-encodings "hello" codings)
          ;; Latin-1 char: utf-8 uses 2 bytes, others may differ
          (funcall 'neovm--csa-compare-encodings "\u00E9" codings)
          ;; CJK: utf-8 uses 3 bytes per char
          (funcall 'neovm--csa-compare-encodings "\u4E2D\u6587" codings)
          ;; Empty string: all should agree
          (funcall 'neovm--csa-compare-encodings "" codings)
          ;; String with mixed char widths
          (let ((s "A\u00E9\u4E2D"))
            (funcall 'neovm--csa-compare-encodings s codings))))
    (fmakunbound 'neovm--csa-compare-encodings)))"#;
    let expect = expect_test::expect![[
        r#""OK (((utf-8 5 t) (utf-8-unix 5 t) (no-conversion 5 t) (raw-text 5 t)) ((utf-8 2 t) (utf-8-unix 2 t) (no-conversion 2 nil) (raw-text 2 nil)) ((utf-8 6 t) (utf-8-unix 6 t) (no-conversion 6 nil) (raw-text 6 nil)) ((utf-8 0 t) (utf-8-unix 0 t) (no-conversion 0 t) (raw-text 0 t)) ((utf-8 6 t) (utf-8-unix 6 t) (no-conversion 6 nil) (raw-text 6 nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: encoding detection heuristic (analyze byte patterns)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_string_advanced_byte_pattern_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Analyze encoded byte patterns to detect encoding characteristics
    let form = r#"(progn
  (fset 'neovm--csa-byte-stats
    (lambda (s)
      "Analyze the byte distribution of a UTF-8 encoded string."
      (let* ((encoded (encode-coding-string s 'utf-8))
             (nbytes (string-bytes encoded))
             (ascii-count 0)
             (high-count 0)
             (leading-2 0)   ;; 110xxxxx (2-byte lead)
             (leading-3 0)   ;; 1110xxxx (3-byte lead)
             (leading-4 0)   ;; 11110xxx (4-byte lead)
             (continuation 0)) ;; 10xxxxxx
        (dotimes (i nbytes)
          (let ((byte (aref encoded i)))
            (cond
              ((< byte #x80) (setq ascii-count (1+ ascii-count)))
              ((< byte #xC0) (setq continuation (1+ continuation)))
              ((< byte #xE0) (setq leading-2 (1+ leading-2)))
              ((< byte #xF0) (setq leading-3 (1+ leading-3)))
              (t (setq leading-4 (1+ leading-4))))
            (when (>= byte #x80) (setq high-count (1+ high-count)))))
        (list nbytes ascii-count high-count
              leading-2 leading-3 leading-4 continuation
              ;; Verify: lead bytes + ascii = char count
              (= (+ ascii-count leading-2 leading-3 leading-4) (length s))))))

  (unwind-protect
      (list
        ;; Pure ASCII: all bytes < 0x80
        (funcall 'neovm--csa-byte-stats "hello")
        ;; 2-byte chars: latin accented
        (funcall 'neovm--csa-byte-stats "\u00E9\u00E8\u00EA")
        ;; 3-byte chars: CJK
        (funcall 'neovm--csa-byte-stats "\u4E2D\u6587")
        ;; 4-byte chars: emoji
        (funcall 'neovm--csa-byte-stats "\U0001F600\U0001F601")
        ;; Mixed: 1+2+3+4 byte chars
        (funcall 'neovm--csa-byte-stats "A\u00E9\u4E2D\U0001F600")
        ;; Empty
        (funcall 'neovm--csa-byte-stats ""))
    (fmakunbound 'neovm--csa-byte-stats)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 5 0 0 0 0 0 t) (6 0 6 3 0 0 3 t) (6 0 6 0 2 0 4 t) (8 0 8 0 0 2 6 t) (10 1 9 1 1 1 6 t) (0 0 0 0 0 0 0 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: encode-coding-string with nocopy and buffer flags
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_string_advanced_nocopy_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The NOCOPY flag: if the result would be identical, may return the same string
    let form = r#"(list
  ;; With NOCOPY=t, encoding ASCII with utf-8 may skip copy
  (let* ((s "hello")
         (enc (encode-coding-string s 'utf-8 t)))
    (list (string= s enc) (length enc)))
  ;; With NOCOPY=nil, always returns (possibly new) string
  (let* ((s "hello")
         (enc (encode-coding-string s 'utf-8 nil)))
    (list (string= s enc) (length enc)))
  ;; decode-coding-string with NOCOPY
  (let* ((s "hello")
         (dec (decode-coding-string s 'utf-8 t)))
    (list (string= s dec) (length dec)))
  ;; NOCOPY for non-ASCII: encoding always produces new bytes
  (let* ((s "\u00E9")
         (enc (encode-coding-string s 'utf-8 t))
         (dec (decode-coding-string enc 'utf-8 t)))
    (string= s dec))
  ;; Verify NOCOPY flag doesn't affect correctness
  (let ((test-strings '("" "a" "abc" "\u00E9" "\u4E2D\u6587")))
    (mapcar (lambda (s)
              (string= (encode-coding-string s 'utf-8 nil)
                       (encode-coding-string s 'utf-8 t)))
            test-strings)))"#;
    let expect = expect_test::expect![[r#""OK ((t 5) (t 5) (t 5) t (t t t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
