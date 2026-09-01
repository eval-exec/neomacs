//! Oracle parity tests for multibyte string operations in Elisp.
//!
//! Covers: `string-to-multibyte`/`string-to-unibyte`,
//! `encode-coding-string`/`decode-coding-string`, `char-to-string` for Unicode,
//! `string-width` for CJK characters, `truncate-string-to-width`, `char-width`,
//! `string-to-list`, `string-to-vector`, multibyte regex matching,
//! `string-as-unibyte`/`string-as-multibyte`, and composite characters.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string-to-multibyte / string-to-unibyte
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_string_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; ASCII string is already multibyte in modern Emacs
  (multibyte-string-p "hello")
  ;; string-to-multibyte on ASCII
  (let ((s (string-to-multibyte "hello")))
    (list (multibyte-string-p s)
          (length s)
          (string-bytes s)))
  ;; Unibyte string via make-string
  (let ((u (make-string 5 ?a)))
    (list (length u) (string-bytes u)))
  ;; string-to-multibyte preserves content
  (string= (string-to-multibyte "test") "test")
  ;; string-to-unibyte on pure ASCII
  (let ((u (string-to-unibyte "abc")))
    (list (length u) (string-bytes u)))
  ;; Round-trip: multibyte -> unibyte -> multibyte for ASCII
  (let* ((orig "Hello123")
         (uni (string-to-unibyte orig))
         (multi (string-to-multibyte uni)))
    (string= orig multi))
  ;; string-bytes vs length for multibyte
  (let ((s "\303\251"))  ;; UTF-8 for e-acute
    (list (length s) (string-bytes s)))
  ;; Multibyte string with various codepoints
  (let ((s (concat (char-to-string 955) (char-to-string 960) (char-to-string 963))))
    ;; Greek lambda, pi, sigma
    (list (length s)
          (> (string-bytes s) (length s))
          (multibyte-string-p s))))
"####;
    let expect = expect_test::expect![[r#""OK (nil (t 5 5) (5 5) t (3 3) t (2 2) (3 t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// encode-coding-string / decode-coding-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_encode_decode_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Encode ASCII with utf-8
  (let ((encoded (encode-coding-string "hello" 'utf-8)))
    (list (length encoded) (string-bytes encoded)))
  ;; Encode and decode round-trip with utf-8
  (let* ((orig "Hello, World!")
         (encoded (encode-coding-string orig 'utf-8))
         (decoded (decode-coding-string encoded 'utf-8)))
    (string= orig decoded))
  ;; Encode multibyte string
  (let* ((s (concat (char-to-string #x00e9)))  ;; e-acute
         (enc (encode-coding-string s 'utf-8)))
    (list (length s) (string-bytes enc)))
  ;; Latin-1 encoding
  (let* ((s (char-to-string #x00e9))
         (enc (encode-coding-string s 'iso-latin-1))
         (dec (decode-coding-string enc 'iso-latin-1)))
    (string= s dec))
  ;; Encode empty string
  (let ((enc (encode-coding-string "" 'utf-8)))
    (list (length enc) (string= enc "")))
  ;; Encode with 'raw-text
  (let* ((s "plain ascii")
         (enc (encode-coding-string s 'raw-text)))
    (string= s enc))
  ;; decode-coding-string with nocopy parameter (3rd arg t)
  (let ((dec (decode-coding-string "abc" 'utf-8)))
    (string= dec "abc"))
  ;; Multiple encodings produce same result for ASCII
  (let ((s "ascii only"))
    (string= (encode-coding-string s 'utf-8)
             (encode-coding-string s 'iso-latin-1))))
"####;
    let expect = expect_test::expect![[r#""OK ((5 5) t (1 2) t (0 t) t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-to-string for Unicode codepoints
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_char_to_string_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Basic ASCII
  (char-to-string ?A)
  (char-to-string ?z)
  (char-to-string ?0)
  (char-to-string ? )
  ;; Latin extended
  (char-to-string #x00e9)   ;; e-acute
  (char-to-string #x00f1)   ;; n-tilde
  (char-to-string #x00fc)   ;; u-diaeresis
  ;; Greek
  (length (char-to-string #x03b1))  ;; alpha
  (length (char-to-string #x03c9))  ;; omega
  ;; CJK
  (length (char-to-string #x4e2d))  ;; Chinese "middle"
  (length (char-to-string #x65e5))  ;; Japanese "day"
  ;; Emoji/symbols
  (length (char-to-string #x2603))  ;; snowman
  (length (char-to-string #x2764))  ;; heart
  ;; Round-trip: char-to-string then string-to-char
  (= (string-to-char (char-to-string #x03bb)) #x03bb)
  (= (string-to-char (char-to-string #x4e2d)) #x4e2d)
  ;; Concatenating char-to-string results
  (let ((s (concat (char-to-string ?H)
                   (char-to-string ?i)
                   (char-to-string ?!)
                   (char-to-string #x263a))))
    (list (length s) (substring s 0 2))))
"####;
    let expect = expect_test::expect![[
        r#""OK (\"A\" \"z\" \"0\" \" \" \"é\" \"ñ\" \"ü\" 1 1 1 1 1 1 t t (4 \"Hi\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-width for CJK and mixed-width characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_string_width_cjk() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; ASCII: each char has width 1
  (string-width "hello")
  (string-width "")
  (string-width "a")
  ;; CJK characters are double-width
  (string-width (char-to-string #x4e2d))   ;; 2
  (string-width (concat (char-to-string #x4e2d)
                        (char-to-string #x6587)))  ;; 4
  ;; Mixed ASCII and CJK
  (string-width (concat "ab" (char-to-string #x4e2d) "cd"))  ;; 2+2+2 = 6
  ;; Latin accented characters: width 1
  (string-width (char-to-string #x00e9))
  ;; Greek letters: width 1
  (string-width (char-to-string #x03b1))
  ;; Tab character
  (string-width "\t")
  ;; Newline
  (string-width "\n")
  ;; Control characters
  (string-width (char-to-string 1))
  ;; Multiple CJK
  (string-width (concat (char-to-string #x4e00)
                        (char-to-string #x4e8c)
                        (char-to-string #x4e09))))
"####;
    let expect = expect_test::expect![[r#""OK (5 0 1 2 4 6 1 1 8 0 2 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-width
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_char_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; ASCII
  (char-width ?a)
  (char-width ?Z)
  (char-width ?0)
  (char-width ? )
  ;; CJK: double width
  (char-width #x4e2d)
  (char-width #x65e5)
  ;; Latin extended: single width
  (char-width #x00e9)
  (char-width #x00f1)
  ;; Greek
  (char-width #x03b1)
  (char-width #x03c9)
  ;; Tab
  (char-width ?\t)
  ;; Newline
  (char-width ?\n)
  ;; Fullwidth Latin
  (char-width #xff21)  ;; fullwidth A
  ;; Halfwidth Katakana
  (char-width #xff71))  ;; halfwidth A (katakana)
"####;
    let expect = expect_test::expect![[r#""OK (1 1 1 1 2 2 1 1 1 1 8 0 2 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// truncate-string-to-width
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_truncate_string_to_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Basic ASCII truncation
  (truncate-string-to-width "hello world" 5)
  (truncate-string-to-width "hello world" 11)
  (truncate-string-to-width "hello world" 20)
  (truncate-string-to-width "hello" 0)
  ;; With CJK (each char width 2)
  (let ((cjk (concat (char-to-string #x4e2d)
                      (char-to-string #x6587)
                      (char-to-string #x5b57))))
    (list
     ;; Full string width = 6
     (string-width cjk)
     ;; Truncate to width 4 -> first 2 CJK chars
     (truncate-string-to-width cjk 4)
     ;; Truncate to width 3 -> first CJK char + padding? depends on impl
     (truncate-string-to-width cjk 2)
     ;; Truncate to width 0
     (truncate-string-to-width cjk 0)))
  ;; With start-column parameter
  (truncate-string-to-width "abcdefghij" 5 2)
  ;; With ellipsis
  (truncate-string-to-width "hello world" 8 nil "...")
  ;; Mixed width with truncation
  (let ((mixed (concat "ab" (char-to-string #x4e2d) "cd")))
    (truncate-string-to-width mixed 4)))
"####;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"hello world\" \"hello world\" \"\" (6 \"中文\" \"中\" \"\") \"cde\" \"hello wo\" \"ab中\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-to-list and string-to-vector
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_string_to_list_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; string-to-list on ASCII
  (append (string-to-list "abc") nil)
  ;; string-to-list on empty string
  (string-to-list "")
  ;; string-to-list on multibyte
  (let ((chars (string-to-list (concat (char-to-string #x03b1)
                                       (char-to-string #x03b2)
                                       (char-to-string #x03b3)))))
    (list (length chars)
          (= (car chars) #x03b1)
          (= (nth 1 chars) #x03b2)
          (= (nth 2 chars) #x03b3)))
  ;; string-to-vector on ASCII
  (string-to-vector "hello")
  ;; string-to-vector on empty string
  (string-to-vector "")
  ;; string-to-vector on multibyte
  (let ((v (string-to-vector (concat (char-to-string #x4e2d)
                                      (char-to-string #x6587)))))
    (list (length v)
          (= (aref v 0) #x4e2d)
          (= (aref v 1) #x6587)))
  ;; Round-trip: string -> list -> concat back
  (let* ((orig "test")
         (chars (string-to-list orig))
         (rebuilt (apply 'string chars)))
    (string= orig rebuilt))
  ;; Round-trip with multibyte
  (let* ((orig (concat (char-to-string #x00e9) "abc"))
         (v (string-to-vector orig))
         (rebuilt (concat v)))
    (string= orig rebuilt)))
"####;
    let expect = expect_test::expect![[
        r#""OK ((97 98 99) nil (3 t t t) [104 101 108 108 111] [] (2 t t) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multibyte regex matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_regex_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Match ASCII in multibyte string
  (string-match "hello" "say hello world")
  ;; Match multibyte literal
  (let ((s (concat "abc" (char-to-string #x00e9) "def")))
    (string-match (char-to-string #x00e9) s))
  ;; Character class with multibyte
  (let ((s (concat "a" (char-to-string #x03b1) "b")))
    (string-match "[a-z]" s))
  ;; Dot matches multibyte character
  (let ((s (concat "x" (char-to-string #x4e2d) "y")))
    (list (string-match "x.y" s)
          (match-string 0 s)))
  ;; Regex on pure ASCII
  (string-match "[0-9]+" "abc123def")
  ;; Regex with groups
  (let ((s "foo123bar456"))
    (string-match "\\([a-z]+\\)\\([0-9]+\\)" s)
    (list (match-string 1 s)
          (match-string 2 s)))
  ;; Case-insensitive match in multibyte context
  (let ((case-fold-search t))
    (string-match "HELLO" "say hello there"))
  ;; string-match-p (no side effects on match data)
  (string-match-p "abc" "xabcx")
  ;; No match returns nil
  (string-match "xyz" "abcdef")
  ;; Greedy vs lazy matching
  (progn
    (string-match "a.*b" "aXXbYYb")
    (match-string 0 "aXXbYYb")))
"####;
    let expect = expect_test::expect![[
        r#""OK (4 3 0 (0 \"x中y\") 3 (\"foo\" \"123\") 4 1 nil \"aXXbYYb\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-as-unibyte / string-as-multibyte
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_string_as_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; string-as-unibyte on ASCII string
  (let ((s (string-as-unibyte "hello")))
    (list (length s) (string-bytes s)))
  ;; string-as-multibyte on ASCII
  (let ((s (string-as-multibyte "hello")))
    (list (length s) (multibyte-string-p s)))
  ;; Round-trip for ASCII
  (string= (string-as-multibyte (string-as-unibyte "test")) "test")
  ;; string-as-unibyte preserves byte count
  (let* ((orig "abc")
         (uni (string-as-unibyte orig)))
    (= (string-bytes orig) (string-bytes uni)))
  ;; On empty strings
  (string-as-unibyte "")
  (string-as-multibyte "")
  ;; Both have length 0 for empty
  (length (string-as-unibyte ""))
  (length (string-as-multibyte ""))
  ;; string-as-unibyte vs string-to-unibyte: different semantics
  ;; string-as-unibyte reinterprets bytes, string-to-unibyte converts
  (let ((ascii "abc"))
    (list (string= (string-as-unibyte ascii) (string-to-unibyte ascii))
          (length (string-as-unibyte ascii))
          (length (string-to-unibyte ascii)))))
"####;
    let expect = expect_test::expect![[r#""OK ((5 5) (5 t) t t \"\" \"\" 0 0 (t 3 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Comprehensive multibyte operations combining multiple functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multibyte_comprehensive_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Build a string from codepoints and verify properties
  (let* ((chars (list ?A #x00e9 #x4e2d #x03b1 ?Z))
         (s (apply 'string chars)))
    (list
     (length s)
     (string-width s)  ;; A=1 + e-acute=1 + CJK=2 + alpha=1 + Z=1 = 6
     (string-bytes s)
     (multibyte-string-p s)
     ;; Extract each character back
     (= (aref s 0) ?A)
     (= (aref s 1) #x00e9)
     (= (aref s 2) #x4e2d)
     (= (aref s 3) #x03b1)
     (= (aref s 4) ?Z)))
  ;; substring on multibyte
  (let ((s (concat (char-to-string #x03b1)
                   (char-to-string #x03b2)
                   (char-to-string #x03b3)
                   (char-to-string #x03b4)
                   (char-to-string #x03b5))))
    (list
     (length (substring s 1 3))
     (= (aref (substring s 0 1) 0) #x03b1)
     (= (aref (substring s 4) 0) #x03b5)))
  ;; upcase/downcase on multibyte
  (list
   (upcase "hello")
   (downcase "HELLO")
   (upcase (char-to-string #x00e9))  ;; e-acute -> E-acute
   (downcase (char-to-string #x00c9)))  ;; E-acute -> e-acute
  ;; concat multibyte strings
  (let ((a (char-to-string #x03b1))
        (b " + ")
        (c (char-to-string #x03b2))
        (d " = ")
        (e (char-to-string #x03b3)))
    (let ((result (concat a b c d e)))
      (list (length result)
            (string-width result)))))
"####;
    let expect = expect_test::expect![[
        r#""OK ((5 6 9 t t t t t t) (2 t t) (\"HELLO\" \"hello\" \"É\" \"é\") (9 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
