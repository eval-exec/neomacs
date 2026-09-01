//! Oracle parity tests for comprehensive string encoding operations:
//! `encode-coding-string`/`decode-coding-string` with utf-8, latin-1, etc.,
//! `string-bytes` vs `length` for multibyte, `string-to-multibyte`/
//! `string-to-unibyte`, `multibyte-string-p`/`unibyte-string-p`, byte-level
//! access via `aref` on unibyte strings, `concat` mixing unibyte/multibyte,
//! `string-as-multibyte`/`string-as-unibyte`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// encode-coding-string / decode-coding-string with multiple coding systems
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_decode_multiple_systems() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; UTF-8 encoding of ASCII
  (encode-coding-string "hello" 'utf-8)
  (length (encode-coding-string "hello" 'utf-8))

  ;; UTF-8 encoding of Latin chars
  (length (encode-coding-string "\u00e9\u00f1\u00fc" 'utf-8))

  ;; UTF-8 encoding of CJK
  (length (encode-coding-string "\u4e16\u754c" 'utf-8))

  ;; Latin-1 encoding of Latin chars (1 byte each)
  (length (encode-coding-string "\u00e9\u00f1\u00fc" 'latin-1))

  ;; raw-text encoding
  (encode-coding-string "abc" 'raw-text)

  ;; Decode roundtrips
  (decode-coding-string (encode-coding-string "hello" 'utf-8) 'utf-8)
  (decode-coding-string (encode-coding-string "\u00e9clair" 'utf-8) 'utf-8)
  (decode-coding-string (encode-coding-string "\u4e16\u754c" 'utf-8) 'utf-8)
  (decode-coding-string (encode-coding-string "\u00e9" 'latin-1) 'latin-1)

  ;; Empty string
  (encode-coding-string "" 'utf-8)
  (decode-coding-string "" 'utf-8)
  (length (encode-coding-string "" 'utf-8)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" 5 6 6 3 \"abc\" \"hello\" \"éclair\" \"世界\" #(\"é\" 0 1 (charset iso-8859-1)) \"\" \"\" 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-bytes vs length for multibyte strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_bytes_vs_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; ASCII: bytes = chars
  (let ((s "hello"))
    (list (length s) (string-bytes s) (= (length s) (string-bytes s))))

  ;; Latin-1 range: 2 bytes per char in UTF-8 internal
  (let ((s "\u00e9\u00f1"))
    (list (length s) (string-bytes s)))

  ;; CJK: 3 bytes per char in UTF-8 internal
  (let ((s "\u4e16\u754c"))
    (list (length s) (string-bytes s)))

  ;; Mixed: ASCII + Latin + CJK
  (let ((s "A\u00e9\u4e16"))
    (list (length s) (string-bytes s)))

  ;; Empty
  (list (length "") (string-bytes ""))

  ;; Single char strings
  (list (string-bytes "a") (string-bytes "\u00e9") (string-bytes "\u4e16"))

  ;; Unibyte string: bytes = chars
  (let ((s (encode-coding-string "hello" 'utf-8)))
    (list (length s) (string-bytes s) (= (length s) (string-bytes s)))))"#;
    let expect =
        expect_test::expect![[r#""OK ((5 5 t) (2 4) (2 6) (3 6) (0 0) (1 2 3) (5 5 t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// multibyte-string-p / unibyte-string-p predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_unibyte_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Regular string literals are multibyte
  (multibyte-string-p "hello")
  (unibyte-string-p "hello")

  ;; Multibyte chars definitely multibyte
  (multibyte-string-p "\u00e9")
  (unibyte-string-p "\u00e9")
  (multibyte-string-p "\u4e16")

  ;; Encoded strings are unibyte
  (let ((enc (encode-coding-string "hello" 'utf-8)))
    (list (multibyte-string-p enc) (unibyte-string-p enc)))

  ;; make-string with multibyte arg
  (multibyte-string-p (make-string 5 ?a))

  ;; Empty strings
  (multibyte-string-p "")
  (unibyte-string-p "")

  ;; Not a string -> nil
  (multibyte-string-p 42)
  (unibyte-string-p 42)
  (multibyte-string-p nil)
  (unibyte-string-p nil))"#;
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-to-multibyte / string-to-unibyte conversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_multibyte_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Convert unibyte to multibyte (ASCII stays same)
  (let* ((uni (encode-coding-string "hello" 'raw-text))
         (multi (string-to-multibyte uni)))
    (list (unibyte-string-p uni)
          (multibyte-string-p multi)
          (string-equal uni multi)
          (length multi)))

  ;; Convert multibyte ASCII to unibyte
  (let* ((multi "hello")
         (uni (string-to-unibyte multi)))
    (list (multibyte-string-p multi)
          (unibyte-string-p uni)
          (string-equal multi uni)))

  ;; string-to-multibyte on already-multibyte (no-op)
  (let ((s "hello"))
    (eq s (string-to-multibyte s)))

  ;; string-to-unibyte on already-unibyte
  (let ((s (encode-coding-string "hello" 'utf-8)))
    (eq s (string-to-unibyte s)))

  ;; Empty string conversions
  (string-to-multibyte "")
  (string-to-unibyte "")
  (multibyte-string-p (string-to-multibyte "")))"#;
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// aref on unibyte strings: byte-level access
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aref_unibyte_string_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; aref on ASCII string returns character code
  (aref "hello" 0)
  (aref "hello" 4)
  (aref "ABC" 1)

  ;; aref on unibyte (encoded) string: access individual bytes
  (let ((enc (encode-coding-string "\u00e9" 'utf-8)))
    (list (length enc)
          (aref enc 0)
          (aref enc 1)))

  ;; aref on unibyte CJK bytes
  (let ((enc (encode-coding-string "\u4e16" 'utf-8)))
    (list (length enc)
          (aref enc 0)
          (aref enc 1)
          (aref enc 2)))

  ;; aref on multibyte string returns char code (not byte)
  (aref "\u00e9" 0)
  (aref "\u4e16" 0)

  ;; Compare: multibyte aref vs unibyte aref
  (let* ((s "\u00e9clair")
         (enc (encode-coding-string s 'utf-8)))
    (list (aref s 0)        ;; char code of e-acute
          (aref enc 0)      ;; first byte of UTF-8 encoding
          (aref s 1)        ;; char code of 'c'
          (length s)
          (length enc))))"#;
    let expect = expect_test::expect![[
        r#""OK (104 111 66 (2 195 169) (3 228 184 150) 233 19990 (233 195 99 6 7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// concat mixing unibyte and multibyte strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concat_mixing_unibyte_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; concat two multibyte strings
  (let ((r (concat "hello" " world")))
    (list r (multibyte-string-p r)))

  ;; concat multibyte + unibyte: result is multibyte
  (let* ((multi "hello")
         (uni (encode-coding-string " world" 'raw-text))
         (result (concat multi uni)))
    (list (multibyte-string-p result)
          result
          (length result)))

  ;; concat two unibyte: result is unibyte
  (let* ((a (encode-coding-string "hello" 'raw-text))
         (b (encode-coding-string " world" 'raw-text))
         (result (concat a b)))
    (list (unibyte-string-p result)
          (length result)))

  ;; concat multibyte with non-ASCII
  (let ((result (concat "hello " "\u4e16\u754c")))
    (list (length result) (string-bytes result) (multibyte-string-p result)))

  ;; concat empty strings
  (concat "" "")
  (concat "" "hello")
  (concat "hello" "")

  ;; concat nil and strings
  (concat)
  (concat nil)
  (concat "a" nil "b"))"#;
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-as-multibyte / string-as-unibyte (reinterpret, no conversion)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_as_multibyte_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; string-as-multibyte on ASCII unibyte: reinterprets bytes as multibyte
  (let* ((uni (encode-coding-string "hello" 'raw-text))
         (multi (string-as-multibyte uni)))
    (list (unibyte-string-p uni)
          (multibyte-string-p multi)
          (string-equal uni multi)
          (length uni)
          (length multi)))

  ;; string-as-unibyte on multibyte ASCII: reinterpret as unibyte
  (let* ((multi "hello")
         (uni (string-as-unibyte multi)))
    (list (multibyte-string-p multi)
          (unibyte-string-p uni)
          (length multi)
          (length uni)))

  ;; Already multibyte -> string-as-multibyte is identity-ish
  (let ((s "hello"))
    (string-equal s (string-as-multibyte s)))

  ;; Already unibyte -> string-as-unibyte is identity-ish
  (let ((s (encode-coding-string "abc" 'raw-text)))
    (string-equal s (string-as-unibyte s)))

  ;; string-as-unibyte on multibyte string containing non-ASCII
  ;; returns the raw bytes
  (let* ((s "\u00e9")
         (raw (string-as-unibyte s)))
    (list (unibyte-string-p raw)
          (length raw))))"#;
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Encoding with NOCOPY and BUFFER params
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_decode_nocopy_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; NOCOPY=nil: always copies
  (let* ((s "hello")
         (enc (encode-coding-string s 'utf-8 nil)))
    (list (string-equal s enc) (length enc)))

  ;; NOCOPY=t: may return same object for ASCII
  (let* ((s "hello")
         (enc (encode-coding-string s 'utf-8 t)))
    (list (string-equal s enc) (length enc)))

  ;; NOCOPY with multibyte: must convert regardless
  (let* ((s "\u00e9clair")
         (enc-copy (encode-coding-string s 'utf-8 nil))
         (enc-nocopy (encode-coding-string s 'utf-8 t)))
    (list (string-equal enc-copy enc-nocopy)
          (length enc-copy)
          (unibyte-string-p enc-copy)
          (unibyte-string-p enc-nocopy)))

  ;; decode with NOCOPY
  (let* ((enc (encode-coding-string "hello" 'utf-8))
         (dec (decode-coding-string enc 'utf-8 nil)))
    (list (string-equal "hello" dec)
          (multibyte-string-p dec))))"#;
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: encoding pipeline with statistics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encoding_pipeline_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((test-strings '("ASCII only"
                                         "\u00e9\u00e8\u00ea\u00eb"
                                         "\u4e16\u754c\u4f60\u597d"
                                         "mixed:\u00e9+\u4e16"
                                         ""
                                         "a")))
                    (let ((results nil))
                      (dolist (s test-strings)
                        (let* ((utf8 (encode-coding-string s 'utf-8))
                               (char-len (length s))
                               (byte-len (length utf8))
                               (roundtrip (decode-coding-string utf8 'utf-8))
                               (is-ascii (= char-len byte-len)))
                          (setq results
                                (cons (list char-len
                                            byte-len
                                            (string-equal s roundtrip)
                                            is-ascii
                                            (multibyte-string-p s)
                                            (unibyte-string-p utf8))
                                      results))))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cross-coding-system roundtrip comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cross_coding_system_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; ASCII: all coding systems produce same result
  (let ((s "hello world"))
    (list
     (string-equal (encode-coding-string s 'utf-8)
                   (encode-coding-string s 'latin-1))
     (string-equal (encode-coding-string s 'utf-8)
                   (encode-coding-string s 'raw-text))
     (= (length (encode-coding-string s 'utf-8))
        (length (encode-coding-string s 'latin-1)))))

  ;; Latin char: UTF-8 is longer than Latin-1
  (let ((s "\u00e9"))
    (list
     (length (encode-coding-string s 'utf-8))
     (length (encode-coding-string s 'latin-1))
     (> (length (encode-coding-string s 'utf-8))
        (length (encode-coding-string s 'latin-1)))))

  ;; Verify: encode-latin1 then decode-latin1 roundtrips
  (let ((s "\u00e9\u00f1\u00fc"))
    (string-equal s (decode-coding-string
                      (encode-coding-string s 'latin-1)
                      'latin-1)))

  ;; utf-8-unix vs utf-8 (line ending variants)
  (let ((s "line1\nline2\n"))
    (string-equal (encode-coding-string s 'utf-8)
                  (encode-coding-string s 'utf-8-unix))))"#;
    let expect = expect_test::expect![[r#""OK ((t t t) (2 1 t) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-bytes on various string types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_bytes_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Pure ASCII
  (string-bytes "")
  (string-bytes "a")
  (string-bytes "abcdefghij")

  ;; Latin-1 range chars (2 bytes each in internal representation)
  (string-bytes "\u00e9")
  (string-bytes "\u00e9\u00f1\u00fc")

  ;; CJK (3 bytes each)
  (string-bytes "\u4e16")
  (string-bytes "\u4e16\u754c\u4f60\u597d")

  ;; Mixed
  (string-bytes "hello\u00e9world\u4e16")

  ;; Unibyte strings: bytes = length
  (let ((s (encode-coding-string "test" 'utf-8)))
    (= (string-bytes s) (length s)))

  ;; Make-string
  (string-bytes (make-string 10 ?a))
  (string-bytes (make-string 0 ?a))

  ;; Substring preserves byte accounting
  (let ((s "hello\u4e16world"))
    (list (string-bytes s)
          (string-bytes (substring s 0 5))
          (string-bytes (substring s 5 6))
          (string-bytes (substring s 6)))))"#;
    let expect = expect_test::expect![[r#""OK (0 1 10 2 6 3 12 15 t 10 0 (13 5 3 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Encoding with concat of encoded segments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encode_concat_decode_segments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((s1 "Hello")
                          (s2 " \u4e16\u754c")
                          (s3 "!")
                          ;; Encode each segment separately
                          (e1 (encode-coding-string s1 'utf-8))
                          (e2 (encode-coding-string s2 'utf-8))
                          (e3 (encode-coding-string s3 'utf-8))
                          ;; Concatenate encoded bytes
                          (combined (concat e1 e2 e3))
                          ;; Decode combined
                          (decoded (decode-coding-string combined 'utf-8))
                          ;; Compare with direct encoding of full string
                          (full (concat s1 s2 s3))
                          (full-enc (encode-coding-string full 'utf-8)))
                    (list
                     (string-equal decoded full)
                     (string-equal combined full-enc)
                     (length e1) (length e2) (length e3)
                     (length combined)
                     (length full)
                     (unibyte-string-p combined)
                     (multibyte-string-p decoded)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-string with unibyte flag
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_multibyte_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Default make-string: multibyte for ASCII range
  (multibyte-string-p (make-string 5 ?a))
  (length (make-string 5 ?a))
  (string-bytes (make-string 5 ?a))

  ;; make-string with multibyte char
  (let ((s (make-string 3 ?\u00e9)))
    (list (length s) (string-bytes s) (multibyte-string-p s)))

  ;; make-string length 0
  (make-string 0 ?x)
  (length (make-string 0 ?x))

  ;; make-string length 1
  (let ((s (make-string 1 ?\u4e16)))
    (list (length s) (string-bytes s) (aref s 0))))"#;
    let expect = expect_test::expect![[r#""OK (nil 5 5 (3 6 t) \"\" 0 (1 3 19990))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build encoding lookup table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encoding_lookup_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((chars '(?a ?\u00e9 ?\u4e16 ?z ?\u00f1))
                        (systems '(utf-8 latin-1)))
                    ;; Build table: for each char, for each system, record byte length
                    (let ((table nil))
                      (dolist (ch chars)
                        (let ((row (list ch)))
                          (dolist (sys systems)
                            (condition-case nil
                                (let ((enc (encode-coding-string (string ch) sys)))
                                  (setq row (append row (list (cons sys (length enc))))))
                              (error
                               (setq row (append row (list (cons sys :error)))))))
                          (setq table (cons row table))))
                      (nreverse table)))"#;
    let expect = expect_test::expect![[
        r#""OK ((97 (utf-8 . 1) (latin-1 . 1)) (233 (utf-8 . 2) (latin-1 . 1)) (19990 (utf-8 . 3) (latin-1 . 1)) (122 (utf-8 . 1) (latin-1 . 1)) (241 (utf-8 . 2) (latin-1 . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// substring on multibyte and unibyte: char vs byte semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_multibyte_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; substring on multibyte: char-based indexing
  (substring "hello\u4e16world" 5 6)
  (substring "hello\u4e16world" 0 5)
  (substring "hello\u4e16world" 6)

  ;; substring on unibyte: byte-based indexing (same as char for unibyte)
  (let ((enc (encode-coding-string "hello" 'utf-8)))
    (list (substring enc 0 3) (length (substring enc 0 3))))

  ;; Verify: char at index of multibyte
  (let ((s "\u00e9\u00f1\u00fc"))
    (list (aref s 0) (aref s 1) (aref s 2)
          (substring s 0 1) (substring s 1 2) (substring s 2 3)))

  ;; string-bytes of substrings
  (let ((s "A\u00e9\u4e16B"))
    (list
     (string-bytes (substring s 0 1))   ;; "A" -> 1 byte
     (string-bytes (substring s 1 2))   ;; e-acute -> 2 bytes
     (string-bytes (substring s 2 3))   ;; CJK -> 3 bytes
     (string-bytes (substring s 3 4)))) ;; "B" -> 1 byte
)"#;
    let expect = expect_test::expect![[
        r#""OK (\"世\" \"hello\" \"world\" (\"hel\" 3) (233 241 252 \"é\" \"ñ\" \"ü\") (1 2 3 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
