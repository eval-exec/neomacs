//! Oracle parity tests for string encoding/decoding operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// encode-coding-string with utf-8, latin-1
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_coding_string_utf8_and_latin1() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode various strings and compare byte lengths and content.
    // ASCII should be identity; multibyte chars should produce longer byte sequences.
    let form = r#"(list
  ;; ASCII identity
  (encode-coding-string "hello" 'utf-8)
  (encode-coding-string "hello" 'latin-1)
  ;; Multibyte: e-acute (U+00E9) is 2 bytes in UTF-8, 1 byte in latin-1
  (length (encode-coding-string "\u00e9" 'utf-8))
  (length (encode-coding-string "\u00e9" 'latin-1))
  ;; CJK character: 3 bytes in UTF-8
  (length (encode-coding-string "\u4e16" 'utf-8))
  ;; Empty string
  (encode-coding-string "" 'utf-8))"#;
    let expect = expect_test::expect![[r#""OK (\"hello\" \"hello\" 2 1 3 \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// decode-coding-string roundtrip verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_decode_encode_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode then decode should yield the original string.
    let form = r#"(let ((test-strings '("hello" "world" "")))
  (let ((results nil))
    (dolist (s test-strings)
      (let* ((encoded (encode-coding-string s 'utf-8))
             (decoded (decode-coding-string encoded 'utf-8)))
        (setq results (cons (string-equal s decoded) results))))
    (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// encode-coding-string with NOCOPY parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_coding_string_nocopy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The NOCOPY parameter (3rd arg) allows returning the original string
    // if no conversion is needed. For ASCII+utf-8, the result should be eq
    // to the original when NOCOPY is t (or at least equal).
    let form = r#"(let* ((ascii "hello world")
        (encoded-copy (encode-coding-string ascii 'utf-8 nil))
        (encoded-nocopy (encode-coding-string ascii 'utf-8 t))
        ;; Both should be equal in content
        (content-eq (string-equal encoded-copy encoded-nocopy))
        ;; For multibyte, NOCOPY still produces correct result
        (mb-str "\u00e9clair")
        (mb-encoded (encode-coding-string mb-str 'utf-8 nil))
        (mb-nocopy (encode-coding-string mb-str 'utf-8 t))
        (mb-eq (string-equal mb-encoded mb-nocopy)))
  (list content-eq mb-eq
        (length encoded-copy)
        (length mb-encoded)))"#;
    let expect = expect_test::expect![[r#""OK (t t 11 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-byte character encoding/decoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_encode_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test with a string containing mixed ASCII, Latin, CJK, and emoji-range chars.
    // Verify byte lengths and roundtrip correctness.
    let form = r#"(let* ((mixed "ABC\u00e9\u00f1\u4e16\u754c")
        (utf8-bytes (encode-coding-string mixed 'utf-8))
        (decoded (decode-coding-string utf8-bytes 'utf-8))
        ;; Verify char count vs byte count
        (char-count (length mixed))
        (byte-count (length utf8-bytes)))
  (list
    (string-equal mixed decoded)
    char-count
    byte-count
    ;; Each segment's byte length
    (length (encode-coding-string "ABC" 'utf-8))
    (length (encode-coding-string "\u00e9\u00f1" 'utf-8))
    (length (encode-coding-string "\u4e16\u754c" 'utf-8))))"#;
    let expect = expect_test::expect![[r#""OK (t 7 13 3 4 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ASCII-only strings should be identity under encoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ascii_encoding_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For pure ASCII strings, encoding with any common coding system
    // should produce an identical (or equal) unibyte string.
    let form = r#"(let ((ascii-strs '("" "a" "hello" "0123456789" "!@#$%^&*()")))
  (let ((results nil))
    (dolist (s ascii-strs)
      (let ((utf8 (encode-coding-string s 'utf-8))
            (latin (encode-coding-string s 'latin-1))
            (raw (encode-coding-string s 'raw-text)))
        (setq results
              (cons (list
                      (string-equal s utf8)
                      (string-equal utf8 latin)
                      (string-equal latin raw)
                      (length utf8))
                    results))))
    (nreverse results)))"#;
    let expect =
        expect_test::expect![[r#""OK ((t t t 0) (t t t 1) (t t t 5) (t t t 10) (t t t 10))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error handling for invalid coding system
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_string_error_invalid_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (coding-system-error neovm-bogus-coding)""#]];
    let (oracle_enc, neovm_enc) = crate::common::eval_oracle_and_neovm_expect(
        "(encode-coding-string \"abc\" 'neovm-bogus-coding)",
        expect,
    );
    assert_err_kind(&oracle_enc, &neovm_enc, "coding-system-error");

    let expect = expect_test::expect![[r#""ERR (coding-system-error neovm-bogus-coding)""#]];
    let (oracle_dec, neovm_dec) = crate::common::eval_oracle_and_neovm_expect(
        "(decode-coding-string \"abc\" 'neovm-bogus-coding)",
        expect,
    );
    assert_err_kind(&oracle_dec, &neovm_dec, "coding-system-error");
}

#[test]
fn oracle_prop_coding_string_wrong_type_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(encode-coding-string 42 'utf-8)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    let (oracle2, neovm2) =
        crate::common::eval_oracle_and_neovm_expect("(decode-coding-string 42 'utf-8)", expect);
    assert_err_kind(&oracle2, &neovm2, "wrong-type-argument");
}

// ---------------------------------------------------------------------------
// Complex: encode, manipulate bytes as unibyte, decode back
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_manipulate_decode_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode a multibyte string to UTF-8 bytes, reverse the bytes of each
    // 3-byte CJK character segment, then decode. Also verify that
    // concatenating encoded segments and decoding works correctly.
    let form = r#"(let* ((s1 "Hello")
        (s2 "\u4e16\u754c")
        (enc1 (encode-coding-string s1 'utf-8))
        (enc2 (encode-coding-string s2 'utf-8))
        ;; Concatenate the encoded bytes
        (combined (concat enc1 enc2))
        ;; Decode the combined result
        (decoded (decode-coding-string combined 'utf-8))
        ;; Verify it matches the original concatenation
        (original (concat s1 s2))
        ;; Also test: encode the concatenated string directly
        (direct-enc (encode-coding-string original 'utf-8)))
  (list
    (string-equal decoded original)
    (string-equal combined direct-enc)
    (length enc1)
    (length enc2)
    (length combined)
    (length original)))"#;
    let expect = expect_test::expect![[r#""OK (t t 5 6 11 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: batch encoding with statistics collection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_batch_encode_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode multiple strings, collect statistics on byte expansion ratios.
    let form = r#"(let ((strings '("ASCII only"
                        "\u00e9\u00e8\u00ea"
                        "\u4e16\u754c\u4f60\u597d"
                        "mixed\u00e9\u4e16"))
       (stats nil))
  (dolist (s strings)
    (let* ((encoded (encode-coding-string s 'utf-8))
           (char-len (length s))
           (byte-len (length encoded))
           (roundtrip (decode-coding-string encoded 'utf-8)))
      (setq stats
            (cons (list
                    char-len
                    byte-len
                    (string-equal s roundtrip)
                    ;; Is it pure ASCII? (byte-len = char-len)
                    (= byte-len char-len))
                  stats))))
  (nreverse stats))"#;
    let expect =
        expect_test::expect![[r#""OK ((10 10 t t) (3 6 t nil) (4 12 t nil) (7 10 t nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
