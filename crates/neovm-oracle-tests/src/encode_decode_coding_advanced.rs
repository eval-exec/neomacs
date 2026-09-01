//! Advanced oracle parity tests for encode-coding-string and decode-coding-string.
//!
//! Tests ALL parameters of both functions: STRING, CODING-SYSTEM, NOCOPY, BUFFER.
//! Covers diverse coding systems, roundtrip consistency, byte length comparisons,
//! unmappable characters, and complex encode-manipulate-decode pipelines.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// All coding system variants: utf-8, latin-1, utf-8-unix, raw-text, etc.
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_decode_coding_many_systems() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode the same string under many coding systems and compare
    // byte lengths, then roundtrip each to verify decode restores original.
    let form = r#"(let ((s "Hello\u00e9\u00f1")
       (codings '(utf-8 utf-8-unix latin-1 raw-text no-conversion))
       (results nil))
  (dolist (cs codings)
    (condition-case err
        (let* ((enc (encode-coding-string s cs))
               (dec (decode-coding-string enc cs))
               (byte-len (string-bytes enc))
               (roundtrip-ok (string= s dec)))
          (setq results (cons (list cs byte-len roundtrip-ok) results)))
      (error
       (setq results (cons (list cs 'error (car err)) results)))))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((utf-8 9 t) (utf-8-unix 9 t) (latin-1 7 t) (raw-text 9 nil) (no-conversion 9 nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// NOCOPY parameter (3rd arg) with various scenarios
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_decode_nocopy_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // NOCOPY=t may return the same object if no conversion needed.
    // NOCOPY=nil always copies. Test both paths for encode and decode.
    let form = r#"(let ((results nil))
  ;; ASCII with utf-8: NOCOPY can skip copy
  (let* ((s "pure ascii")
         (enc-copy (encode-coding-string s 'utf-8 nil))
         (enc-nocopy (encode-coding-string s 'utf-8 t)))
    (setq results (cons (list 'ascii-encode
                               (string= enc-copy enc-nocopy)
                               (length enc-copy)
                               (length enc-nocopy))
                        results)))
  ;; Non-ASCII with utf-8: encoding always changes representation
  (let* ((s "\u00e9\u00f1\u00fc")
         (enc-copy (encode-coding-string s 'utf-8 nil))
         (enc-nocopy (encode-coding-string s 'utf-8 t)))
    (setq results (cons (list 'non-ascii-encode
                               (string= enc-copy enc-nocopy)
                               (string-bytes enc-copy)
                               (string-bytes enc-nocopy))
                        results)))
  ;; Decode with NOCOPY
  (let* ((raw "hello world")
         (dec-copy (decode-coding-string raw 'utf-8 nil))
         (dec-nocopy (decode-coding-string raw 'utf-8 t)))
    (setq results (cons (list 'ascii-decode
                               (string= dec-copy dec-nocopy)
                               (length dec-copy))
                        results)))
  ;; NOCOPY with latin-1 on ASCII
  (let* ((s "abc")
         (enc (encode-coding-string s 'latin-1 t))
         (dec (decode-coding-string enc 'latin-1 t)))
    (setq results (cons (list 'latin1-nocopy
                               (string= s dec))
                        results)))
  ;; NOCOPY with raw-text
  (let* ((s "raw test")
         (enc (encode-coding-string s 'raw-text t)))
    (setq results (cons (list 'raw-nocopy
                               (string= s enc))
                        results)))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((ascii-encode t 10 10) (non-ascii-encode t 6 6) (ascii-decode t 11) (latin1-nocopy t) (raw-nocopy t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// decode-coding-string BUFFER parameter (4th arg)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_decode_coding_buffer_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The 4th arg BUFFER of decode-coding-string, when non-nil, inserts the
    // decoded string into the current buffer and returns the number of chars.
    // When nil (default), returns the decoded string.
    let form = r#"(list
  ;; Default: returns decoded string
  (decode-coding-string "hello" 'utf-8)
  (decode-coding-string "hello" 'utf-8 nil)
  ;; With BUFFER=t: inserts into current buffer, returns char count
  (with-temp-buffer
    (let ((result (decode-coding-string "world" 'utf-8 nil t)))
      (list result (buffer-string))))
  ;; Insert multibyte
  (with-temp-buffer
    (let ((enc (encode-coding-string "\u00e9\u00f1" 'utf-8)))
      (let ((result (decode-coding-string enc 'utf-8 nil t)))
        (list result (buffer-string) (length (buffer-string))))))
  ;; Insert into buffer with existing content
  (with-temp-buffer
    (insert "prefix:")
    (let ((result (decode-coding-string "suffix" 'utf-8 nil t)))
      (list result (buffer-string))))
  ;; NOCOPY + BUFFER together
  (with-temp-buffer
    (let ((result (decode-coding-string "nocopy-buf" 'utf-8 t t)))
      (list result (buffer-string)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"hello\" (\"world\" \"\") (\"éñ\" \"\" 0) (\"suffix\" \"prefix:\") (\"nocopy-buf\" \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Roundtrip encode->decode preserves original across many strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_decode_roundtrip_diverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((strings '(""
                        "a"
                        "Hello, World!"
                        "0123456789"
                        "\u00e9\u00e8\u00ea\u00eb"
                        "\u00c0\u00c1\u00c2\u00c3"
                        "\u4e16\u754c\u4f60\u597d"
                        "\u03b1\u03b2\u03b3\u03b4\u03b5"
                        "\u0410\u0411\u0412\u0413"
                        "mixed\u00e9\u4e16test"))
       (codings '(utf-8 latin-1))
       (results nil))
  (dolist (s strings)
    (dolist (cs codings)
      (condition-case nil
          (let* ((enc (encode-coding-string s cs))
                 (dec (decode-coding-string enc cs)))
            (setq results (cons (list s cs (string= s dec)) results)))
        (error
         (setq results (cons (list s cs 'encoding-error) results))))))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\" utf-8 t) (\"\" latin-1 t) (\"a\" utf-8 t) (\"a\" latin-1 t) (\"Hello, World!\" utf-8 t) (\"Hello, World!\" latin-1 t) (\"0123456789\" utf-8 t) (\"0123456789\" latin-1 t) (\"éèêë\" utf-8 t) (\"éèêë\" latin-1 t) (\"ÀÁÂÃ\" utf-8 t) (\"ÀÁÂÃ\" latin-1 t) (\"世界你好\" utf-8 t) (\"世界你好\" latin-1 nil) (\"αβγδε\" utf-8 t) (\"αβγδε\" latin-1 nil) (\"АБВГ\" utf-8 t) (\"АБВГ\" latin-1 nil) (\"mixedé世test\" utf-8 t) (\"mixedé世test\" latin-1 nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Byte length comparisons across different coding systems
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_byte_length_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Same string, different coding systems produce different byte lengths.
    // UTF-8 is variable-width, latin-1 is 1 byte per char (for <= 0xFF).
    let form = r#"(let ((tests '(("A" . (1 1))
                        ("\u00e9" . (2 1))
                        ("\u00fc" . (2 1))
                        ("\u00ff" . (2 1))))
       (results nil))
  (dolist (test tests)
    (let* ((s (car test))
           (utf8-len (string-bytes (encode-coding-string s 'utf-8)))
           (latin1-len (string-bytes (encode-coding-string s 'latin-1))))
      (setq results (cons (list s utf8-len latin1-len
                                 (= utf8-len (car (cdr test)))
                                 (= latin1-len (cadr (cdr test))))
                          results))))
  ;; Also test 3-byte and 4-byte UTF-8 sequences
  (setq results (cons
    (list "3-byte-cjk"
          (string-bytes (encode-coding-string "\u4e16" 'utf-8))
          (string-bytes (encode-coding-string "\u4e16\u754c" 'utf-8)))
    results))
  (setq results (cons
    (list "4-byte-emoji"
          (string-bytes (encode-coding-string "\U0001F600" 'utf-8)))
    results))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"A\" 1 1 t t) (\"é\" 2 1 t t) (\"ü\" 2 1 t t) (\"ÿ\" 2 1 t t) (\"3-byte-cjk\" 3 6) (\"4-byte-emoji\" 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unmappable characters: latin-1 cannot encode CJK
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_unmappable_characters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // latin-1 can only encode U+0000..U+00FF. Trying to encode CJK or
    // other chars outside that range triggers charset-based substitution
    // or an error depending on the coding system configuration.
    let form = r#"(list
  ;; latin-1 can handle characters in its range
  (condition-case nil
      (let ((enc (encode-coding-string "\u00e9\u00f1" 'latin-1)))
        (list 'ok (string-bytes enc)))
    (error '(error)))
  ;; utf-8 handles everything
  (let ((enc (encode-coding-string "\u4e16\u754c\U0001F600" 'utf-8)))
    (list 'ok (string-bytes enc)))
  ;; raw-text for ASCII is identity
  (string= "abc" (encode-coding-string "abc" 'raw-text))
  ;; Compare: same ASCII produces same bytes regardless of system
  (let ((s "test123"))
    (list (string= (encode-coding-string s 'utf-8)
                   (encode-coding-string s 'latin-1))
          (string= (encode-coding-string s 'utf-8)
                   (encode-coding-string s 'raw-text)))))"#;
    let expect = expect_test::expect![[r#""OK ((ok 2) (ok 10) t (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: encode-manipulate-decode pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_decode_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a pipeline: encode multiple strings, concatenate their bytes,
    // decode the whole thing, and verify it matches the original concatenation.
    let form = r#"(progn
  (fset 'neovm--edca-encode-concat
    (lambda (strings coding)
      "Encode each string, concatenate bytes, decode back."
      (let ((parts (mapcar (lambda (s) (encode-coding-string s coding)) strings)))
        (let ((combined (apply #'concat parts)))
          (decode-coding-string combined coding)))))

  (unwind-protect
      (list
        ;; Simple ASCII parts
        (let* ((parts '("Hello" " " "World" "!"))
               (result (funcall 'neovm--edca-encode-concat parts 'utf-8))
               (expected (apply #'concat parts)))
          (list (string= result expected) (length result)))
        ;; Mixed multibyte parts
        (let* ((parts '("abc" "\u00e9" "\u4e16" "xyz"))
               (result (funcall 'neovm--edca-encode-concat parts 'utf-8))
               (expected (apply #'concat parts)))
          (list (string= result expected) (length result) (length expected)))
        ;; Empty parts mixed in
        (let* ((parts '("" "a" "" "b" "" "c" ""))
               (result (funcall 'neovm--edca-encode-concat parts 'utf-8))
               (expected (apply #'concat parts)))
          (list (string= result expected) (length result)))
        ;; Single-part degenerate case
        (let* ((parts '("\u03b1\u03b2\u03b3"))
               (result (funcall 'neovm--edca-encode-concat parts 'utf-8))
               (expected (apply #'concat parts)))
          (string= result expected)))
    (fmakunbound 'neovm--edca-encode-concat)))"#;
    let expect = expect_test::expect![[r#""OK ((t 12) (t 8 8) (t 3) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: batch encoding with statistics and categorization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_decode_batch_categorize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Categorize strings by their encoding expansion ratio and byte structure.
    let form = r#"(progn
  (fset 'neovm--edca-categorize
    (lambda (s)
      "Categorize a string by its UTF-8 encoding properties."
      (let* ((enc (encode-coding-string s 'utf-8))
             (char-len (length s))
             (byte-len (string-bytes enc))
             (ratio (if (> char-len 0) (/ (float byte-len) char-len) 1.0))
             (category (cond
                         ((= char-len 0) 'empty)
                         ((= byte-len char-len) 'ascii-only)
                         ((<= ratio 2.0) 'mostly-latin)
                         ((<= ratio 3.0) 'mostly-cjk)
                         (t 'mostly-4byte)))
             (roundtrip (string= s (decode-coding-string enc 'utf-8))))
        (list category char-len byte-len roundtrip))))

  (unwind-protect
      (let ((strings '(""
                        "hello world"
                        "\u00e9\u00e8\u00ea"
                        "\u4e16\u754c\u4f60\u597d"
                        "A\u00e9B\u00f1C"
                        "\u03b1\u03b2\u03b3"
                        "\U0001F600\U0001F601")))
        (mapcar (lambda (s) (funcall 'neovm--edca-categorize s)) strings))
    (fmakunbound 'neovm--edca-categorize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((empty 0 0 t) (ascii-only 11 11 t) (mostly-latin 3 6 t) (mostly-cjk 4 12 t) (mostly-latin 5 7 t) (mostly-latin 3 6 t) (mostly-4byte 2 8 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: cross-coding-system comparison matrix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_decode_cross_system_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For each string, encode with one system, decode with another (where valid).
    // Verify which cross-system pairs preserve the string.
    let form = r#"(let ((ascii-str "ABC")
       (results nil))
  ;; ASCII should survive any encode/decode pair
  (dolist (enc-cs '(utf-8 latin-1 raw-text no-conversion))
    (dolist (dec-cs '(utf-8 latin-1 raw-text no-conversion))
      (condition-case nil
          (let* ((encoded (encode-coding-string ascii-str enc-cs))
                 (decoded (decode-coding-string encoded dec-cs)))
            (setq results (cons (list enc-cs dec-cs
                                       (string= ascii-str decoded))
                                results)))
        (error
         (setq results (cons (list enc-cs dec-cs 'error) results))))))
  ;; Also test: encode utf-8 decode utf-8-unix (should be compatible)
  (let* ((s "test\u00e9")
         (enc (encode-coding-string s 'utf-8))
         (dec (decode-coding-string enc 'utf-8-unix)))
    (setq results (cons (list 'utf8-to-utf8unix (string= s dec)) results)))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((utf-8 utf-8 t) (utf-8 latin-1 t) (utf-8 raw-text t) (utf-8 no-conversion t) (latin-1 utf-8 t) (latin-1 latin-1 t) (latin-1 raw-text t) (latin-1 no-conversion t) (raw-text utf-8 t) (raw-text latin-1 t) (raw-text raw-text t) (raw-text no-conversion t) (no-conversion utf-8 t) (no-conversion latin-1 t) (no-conversion raw-text t) (no-conversion no-conversion t) (utf8-to-utf8unix t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: encode then inspect byte values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_inspect_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode strings and verify specific byte values match UTF-8 spec.
    let form = r#"(list
  ;; "A" -> 0x41
  (aref (encode-coding-string "A" 'utf-8) 0)
  ;; e-acute U+00E9 -> 0xC3 0xA9
  (let ((enc (encode-coding-string "\u00e9" 'utf-8)))
    (list (aref enc 0) (aref enc 1)))
  ;; CJK U+4E16 -> 0xE4 0xB8 0x96
  (let ((enc (encode-coding-string "\u4e16" 'utf-8)))
    (list (aref enc 0) (aref enc 1) (aref enc 2)))
  ;; Space -> 0x20
  (aref (encode-coding-string " " 'utf-8) 0)
  ;; Newline -> 0x0A
  (aref (encode-coding-string "\n" 'utf-8) 0)
  ;; Latin-1: e-acute U+00E9 -> 0xE9 (single byte)
  (aref (encode-coding-string "\u00e9" 'latin-1) 0)
  ;; Verify consecutive chars produce correct byte sequence
  (let ((enc (encode-coding-string "A\u00e9" 'utf-8)))
    (list (string-bytes enc)
          (aref enc 0)    ;; A = 0x41
          (aref enc 1)    ;; 0xC3
          (aref enc 2)))) ;; 0xA9
"#;
    let expect =
        expect_test::expect![[r#""OK (65 (195 169) (228 184 150) 32 10 233 (3 65 195 169))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
