//! Advanced oracle parity tests for `string-to-char`.
//!
//! Tests: ASCII chars, multibyte chars, empty string behavior, single vs
//! multi-char strings, roundtrip with `char-to-string`, character
//! classification pipelines, and edge cases.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic ASCII character extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_char_ascii_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Lowercase letters
  (string-to-char "a")
  (string-to-char "m")
  (string-to-char "z")
  ;; Uppercase letters
  (string-to-char "A")
  (string-to-char "M")
  (string-to-char "Z")
  ;; Digits
  (string-to-char "0")
  (string-to-char "5")
  (string-to-char "9")
  ;; Punctuation and special ASCII
  (string-to-char "!")
  (string-to-char "@")
  (string-to-char "#")
  (string-to-char "$")
  (string-to-char "%")
  (string-to-char "^")
  (string-to-char "&")
  (string-to-char "*")
  (string-to-char "(")
  (string-to-char ")")
  (string-to-char " ")
  (string-to-char "~")
  (string-to-char "`")
  ;; Control characters as strings via format
  (string-to-char "\t")
  (string-to-char "\n")
  ;; Verify values match char literals
  (= (string-to-char "A") ?A)
  (= (string-to-char "z") ?z)
  (= (string-to-char "0") ?0)
  (= (string-to-char " ") ?\s)
  (= (string-to-char "\n") ?\n)
  (= (string-to-char "\t") ?\t))"####;
    let expect = expect_test::expect![[
        r#""OK (97 109 122 65 77 90 48 53 57 33 64 35 36 37 94 38 42 40 41 32 126 96 9 10 t t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multibyte and Unicode character extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_char_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Latin extended
  (string-to-char "\u00e9")
  (string-to-char "\u00fc")
  (string-to-char "\u00f1")
  ;; Greek
  (string-to-char "\u03b1")
  (string-to-char "\u03c9")
  ;; Cyrillic
  (string-to-char "\u0414")
  (string-to-char "\u042f")
  ;; CJK
  (string-to-char "\u4e16")
  (string-to-char "\u754c")
  ;; Verify multibyte chars are > 127
  (> (string-to-char "\u00e9") 127)
  (> (string-to-char "\u03b1") 127)
  (> (string-to-char "\u4e16") 127)
  ;; Roundtrip: char-to-string of extracted char should give back original first char
  (string= (char-to-string (string-to-char "\u00e9")) "\u00e9")
  (string= (char-to-string (string-to-char "\u03b1")) "\u03b1")
  (string= (char-to-string (string-to-char "\u4e16")) "\u4e16")
  ;; Type check: always returns integer
  (integerp (string-to-char "\u00e9"))
  (integerp (string-to-char "\u4e16")))"####;
    let expect = expect_test::expect![[
        r#""OK (233 252 241 945 969 1044 1071 19990 30028 t t t t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-char strings: always returns first char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_char_multi_char_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Multi-char ASCII strings -> first char
  (string-to-char "hello")
  (string-to-char "world")
  (string-to-char "ABC")
  (string-to-char "123")
  ;; Verify it's the first character
  (= (string-to-char "hello") ?h)
  (= (string-to-char "world") ?w)
  (= (string-to-char "ABC") ?A)
  (= (string-to-char "123") ?1)
  ;; Multi-char with multibyte first char
  (= (string-to-char "\u00e9cole") (string-to-char "\u00e9"))
  ;; Multi-char with ASCII first char followed by multibyte
  (= (string-to-char "a\u00e9") ?a)
  ;; Longer strings
  (string-to-char "The quick brown fox")
  (= (string-to-char "The quick brown fox") ?T)
  ;; String with leading space
  (= (string-to-char " hello") ?\s)
  ;; String with leading newline
  (= (string-to-char "\nhello") ?\n)
  ;; String with leading tab
  (= (string-to-char "\thello") ?\t))"####;
    let expect = expect_test::expect![[r#""OK (104 119 65 49 t t t t t t 84 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Empty string and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_char_empty_and_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Empty string returns 0
  (string-to-char "")
  (= (string-to-char "") 0)
  (zerop (string-to-char ""))
  ;; Single character strings
  (string-to-char "x")
  (string-to-char " ")
  (string-to-char ".")
  (string-to-char "/")
  (string-to-char "\\")
  (string-to-char "\"")
  ;; Single char vs same char in longer string
  (= (string-to-char "x") (string-to-char "xyz"))
  (= (string-to-char "A") (string-to-char "ABC"))
  ;; Constructed strings via make-string
  (string-to-char (make-string 1 ?Z))
  (= (string-to-char (make-string 1 ?Z)) ?Z)
  (string-to-char (make-string 5 ?Q))
  (= (string-to-char (make-string 5 ?Q)) ?Q)
  ;; Constructed via concat
  (string-to-char (concat "H" "ello"))
  (= (string-to-char (concat "H" "ello")) ?H)
  ;; Substring result
  (string-to-char (substring "Hello" 1 2))
  (= (string-to-char (substring "Hello" 1 2)) ?e))"####;
    let expect =
        expect_test::expect![[r#""OK (0 t t 120 32 46 47 92 34 t t 90 t 81 t 72 t 101 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Roundtrip with char-to-string: comprehensive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_char_roundtrip_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; char -> string -> char roundtrip for ASCII
  (= (string-to-char (char-to-string ?A)) ?A)
  (= (string-to-char (char-to-string ?z)) ?z)
  (= (string-to-char (char-to-string ?0)) ?0)
  (= (string-to-char (char-to-string ?\s)) ?\s)
  (= (string-to-char (char-to-string ?\n)) ?\n)
  ;; string -> char -> string roundtrip for single-char strings
  (string= (char-to-string (string-to-char "A")) "A")
  (string= (char-to-string (string-to-char "z")) "z")
  (string= (char-to-string (string-to-char "0")) "0")
  (string= (char-to-string (string-to-char " ")) " ")
  ;; Roundtrip batch: printable ASCII range
  (let ((results nil))
    (let ((i 32))
      (while (<= i 126)
        (let* ((ch i)
               (s (char-to-string ch))
               (ch2 (string-to-char s))
               (s2 (char-to-string ch2)))
          (setq results (cons (and (= ch ch2) (string= s s2)) results)))
        (setq i (1+ i))))
    ;; All should be t
    (not (memq nil results)))
  ;; Double roundtrip
  (let ((ch ?X))
    (= ch (string-to-char (char-to-string (string-to-char (char-to-string ch))))))
  ;; Multibyte roundtrips
  (= (string-to-char (char-to-string 233)) 233)
  (string= (char-to-string (string-to-char "\u00e9")) "\u00e9"))"####;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Character classification pipelines using string-to-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_char_classification_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; Helper: classify a string's first character
  (fset 'neovm--test-classify-first-char
    (lambda (s)
      (let ((ch (string-to-char s)))
        (cond
         ((= ch 0) 'empty)
         ((and (>= ch ?a) (<= ch ?z)) 'lowercase)
         ((and (>= ch ?A) (<= ch ?Z)) 'uppercase)
         ((and (>= ch ?0) (<= ch ?9)) 'digit)
         ((= ch ?\s) 'space)
         ((= ch ?\n) 'newline)
         ((= ch ?\t) 'tab)
         ((and (>= ch 33) (<= ch 47)) 'punctuation)
         ((> ch 127) 'multibyte)
         (t 'other)))))

  (unwind-protect
      (list
       ;; Classify various strings
       (funcall 'neovm--test-classify-first-char "hello")
       (funcall 'neovm--test-classify-first-char "Hello")
       (funcall 'neovm--test-classify-first-char "42")
       (funcall 'neovm--test-classify-first-char " space")
       (funcall 'neovm--test-classify-first-char "\nnewline")
       (funcall 'neovm--test-classify-first-char "\ttab")
       (funcall 'neovm--test-classify-first-char "")
       (funcall 'neovm--test-classify-first-char "!bang")
       (funcall 'neovm--test-classify-first-char "\u00e9cole")
       ;; Pipeline: classify then collect
       (let ((strings '("Apple" "banana" "42" " " "" "\u00e9" "!" "\n")))
         (mapcar 'neovm--test-classify-first-char strings))
       ;; Pipeline: extract first chars, sort numerically
       (let ((strings '("cherry" "apple" "banana")))
         (sort (mapcar 'string-to-char strings) '<))
       ;; Build string from first chars of multiple strings
       (concat (mapcar (lambda (s) (string-to-char s))
                       '("H" "e" "l" "l" "o"))))
    (fmakunbound 'neovm--test-classify-first-char)))"####;
    let expect = expect_test::expect![[
        r#""OK (lowercase uppercase digit space newline tab empty punctuation multibyte (uppercase lowercase digit space empty multibyte punctuation newline) (97 98 99) \"Hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-to-char with dynamically constructed strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_char_dynamic_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; From format
  (string-to-char (format "%d" 42))
  (= (string-to-char (format "%d" 42)) ?4)
  (string-to-char (format "%c" 65))
  (= (string-to-char (format "%c" 65)) ?A)
  ;; From number-to-string
  (string-to-char (number-to-string 0))
  (= (string-to-char (number-to-string 0)) ?0)
  (string-to-char (number-to-string 99))
  (= (string-to-char (number-to-string 99)) ?9)
  (string-to-char (number-to-string -5))
  (= (string-to-char (number-to-string -5)) ?-)
  ;; From upcase/downcase
  (string-to-char (upcase "hello"))
  (= (string-to-char (upcase "hello")) ?H)
  (string-to-char (downcase "HELLO"))
  (= (string-to-char (downcase "HELLO")) ?h)
  ;; From symbol-name
  (string-to-char (symbol-name 'hello))
  (= (string-to-char (symbol-name 'hello)) ?h)
  (string-to-char (symbol-name 'nil))
  (= (string-to-char (symbol-name 'nil)) ?n)
  ;; From concat of empty + something
  (string-to-char (concat "" "test"))
  (= (string-to-char (concat "" "test")) ?t)
  ;; String with interior multibyte but ASCII first
  (= (string-to-char (concat "a" "\u03b1")) ?a))"####;
    let expect =
        expect_test::expect![[r#""OK (52 t 65 t 48 t 57 t 45 t 72 t 104 t 104 t 110 t 116 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
