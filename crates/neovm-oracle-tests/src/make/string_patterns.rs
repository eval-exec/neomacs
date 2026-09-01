//! Oracle parity tests for `make-string` — comprehensive coverage of all
//! parameters (LENGTH, INIT, MULTIBYTE), edge cases, Unicode codepoints,
//! interaction with string operations, and modification via `aset`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Zero-length strings and boundary conditions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_zero_length_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Zero length with various init chars
  (make-string 0 ?a)
  (make-string 0 ?Z)
  (make-string 0 0)
  (make-string 0 ?\u4e16)
  ;; Properties of zero-length strings
  (length (make-string 0 ?x))
  (string-bytes (make-string 0 ?x))
  (multibyte-string-p (make-string 0 ?x))
  (stringp (make-string 0 ?x))
  ;; Zero-length with explicit multibyte flag
  (multibyte-string-p (make-string 0 ?x nil))
  (multibyte-string-p (make-string 0 ?x t))
  ;; Zero-length string equality
  (string= (make-string 0 ?a) (make-string 0 ?b))
  (string= (make-string 0 ?a) "")
  (equal (make-string 0 ?x) "")
  ;; Zero-length concat behavior
  (concat (make-string 0 ?a) "hello" (make-string 0 ?z))
  ;; Zero-length is not eq to literal "" but is equal
  (equal (make-string 0 ?a) (make-string 0 ?b)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\" 0 0 nil t nil t t t t \"hello\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Large strings — verify length, bytes, first/last chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_large_and_multibyte_sizes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Large ASCII string
  (let ((s (make-string 5000 ?x)))
    (list (length s) (string-bytes s)
          (aref s 0) (aref s 4999)
          (multibyte-string-p s)))
  ;; Large multibyte string (each char > 1 byte in UTF-8)
  (let ((s (make-string 2000 ?\u03b1)))  ; Greek alpha
    (list (length s) (> (string-bytes s) (length s))
          (aref s 0) (aref s 1999)
          (multibyte-string-p s)))
  ;; Large unibyte string
  (let ((s (make-string 3000 200 nil)))
    (list (length s) (string-bytes s)
          (aref s 0) (aref s 2999)
          (multibyte-string-p s)))
  ;; String-bytes relationship: ASCII multibyte has bytes == length
  (let ((s (make-string 100 ?A t)))
    (list (= (length s) (string-bytes s))
          (multibyte-string-p s)))
  ;; String-bytes for 3-byte UTF-8 char
  (let ((s (make-string 10 ?\u4e16)))
    (list (length s) (string-bytes s)
          ;; each char is 3 bytes in UTF-8
          (= (string-bytes s) (* 3 (length s))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((5000 5000 120 120 nil) (2000 t 945 945 t) (3000 6000 200 200 t) (t t) (10 30 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multibyte flag — nil vs t vs omitted, interaction with char value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_multibyte_flag_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; ASCII char, no flag (default): multibyte
  (multibyte-string-p (make-string 3 ?a))
  ;; ASCII char, nil flag: unibyte
  (multibyte-string-p (make-string 3 ?a nil))
  ;; ASCII char, t flag: multibyte
  (multibyte-string-p (make-string 3 ?a t))
  ;; Non-nil non-t value as flag (any truthy value means multibyte)
  (multibyte-string-p (make-string 3 ?a 42))
  (multibyte-string-p (make-string 3 ?a 'yes))
  ;; Byte value (128-255) with nil flag: unibyte
  (let ((s (make-string 3 200 nil)))
    (list (multibyte-string-p s) (aref s 0)))
  ;; Byte value with t flag: multibyte (raw byte mapped to 3FxxHH)
  (let ((s (make-string 2 #xa0 t)))
    (list (multibyte-string-p s) (length s)))
  ;; Unicode char always multibyte regardless of flag
  (multibyte-string-p (make-string 2 ?\u00e9))
  (multibyte-string-p (make-string 2 ?\u00e9 nil))
  (multibyte-string-p (make-string 2 ?\u00e9 t))
  ;; Compare string-bytes between unibyte and multibyte versions
  (let ((uni (make-string 5 ?z nil))
        (multi (make-string 5 ?z t)))
    (list (string-bytes uni) (string-bytes multi)
          (= (string-bytes uni) (string-bytes multi))
          (string= uni multi)))
  ;; Char 0 with various flags
  (let ((s1 (make-string 3 0))
        (s2 (make-string 3 0 nil))
        (s3 (make-string 3 0 t)))
    (list (length s1) (length s2) (length s3)
          (multibyte-string-p s1)
          (multibyte-string-p s2)
          (multibyte-string-p s3))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil t t t (t 200) (t 2) t t t (5 5 t t) (3 3 3 nil nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unicode codepoints — various planes and combining characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_unicode_codepoints() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Latin-1 supplement: e-acute
  (let ((s (make-string 4 ?\u00e9)))
    (list (length s) (aref s 0) (aref s 3)
          (string= s (concat (make-string 2 ?\u00e9) (make-string 2 ?\u00e9)))))
  ;; CJK Unified Ideographs
  (let ((s (make-string 3 ?\u4e16)))
    (list (length s) (aref s 0) (string-bytes s)))
  ;; Cyrillic
  (let ((s (make-string 5 ?\u0414)))  ; De
    (list (length s) (aref s 0) (multibyte-string-p s)))
  ;; Arabic
  (let ((s (make-string 3 ?\u0639)))  ; Ain
    (list (length s) (aref s 0)))
  ;; Mathematical symbols
  (let ((s (make-string 4 ?\u2200)))  ; for-all
    (list (length s) (aref s 0)))
  ;; Box drawing
  (let ((s (make-string 10 ?\u2500)))
    (list (length s) (aref s 0) (aref s 9)))
  ;; Comparison across scripts
  (let ((latin (make-string 3 ?\u00e9))
        (greek (make-string 3 ?\u03b1))
        (cjk (make-string 3 ?\u4e16)))
    (list (equal latin greek)
          (equal latin cjk)
          (< (string-bytes latin) (string-bytes cjk))
          (= (length latin) (length greek) (length cjk)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((4 233 233 t) (3 19990 9) (5 1044 t) (3 1593) (4 8704) (10 9472 9472) (nil nil t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with string operations: concat, substring, string-match
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_with_string_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; concat with make-string pieces
  (concat (make-string 3 ?<) "content" (make-string 3 ?>))
  ;; substring of make-string result
  (substring (make-string 10 ?x) 3 7)
  (substring (make-string 10 ?x) 0 0)
  (substring (make-string 10 ?x) 5)
  ;; string-match on make-string result
  (let ((s (make-string 5 ?a)))
    (list (string-match "aaa" s)
          (string-match "b" s)
          (string-match "^a+$" s)))
  ;; replace-regexp-in-string on make-string
  (replace-regexp-in-string "a" "b" (make-string 5 ?a))
  ;; upcase/downcase
  (upcase (make-string 4 ?a))
  (downcase (make-string 4 ?A))
  ;; string-to-list
  (string-to-list (make-string 4 ?z))
  ;; number-to-string vs make-string with digit chars
  (string= (make-string 3 ?0) "000")
  ;; Split and rejoin
  (let ((s (concat (make-string 3 ?a) "-" (make-string 3 ?b) "-" (make-string 3 ?c))))
    (split-string s "-"))
  ;; Comparison operations
  (string< (make-string 3 ?a) (make-string 3 ?b))
  (string< (make-string 3 ?a) (make-string 4 ?a))
  (string= (make-string 3 ?a) (make-string 3 ?a))
  (string= (make-string 3 ?a) (make-string 4 ?a)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"<<<content>>>\" \"xxxx\" \"\" \"xxxxx\" (0 nil 0) \"bbbbb\" \"AAAA\" \"aaaa\" (122 122 122 122) t (\"aaa\" \"bbb\" \"ccc\") t t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with length, string-bytes, multibyte-string-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_measurement_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; length vs string-bytes for various char sizes
  (let ((ascii (make-string 10 ?a))
        (latin1 (make-string 10 ?\u00e9))   ; 2 bytes per char
        (cjk (make-string 10 ?\u4e16))      ; 3 bytes per char
        (unibyte (make-string 10 200 nil)))
    (list
     ;; ASCII: length == bytes
     (length ascii) (string-bytes ascii) (= (length ascii) (string-bytes ascii))
     ;; Latin-1: length < bytes
     (length latin1) (string-bytes latin1) (< (length latin1) (string-bytes latin1))
     ;; CJK: length << bytes
     (length cjk) (string-bytes cjk)
     ;; Unibyte: length == bytes
     (length unibyte) (string-bytes unibyte) (= (length unibyte) (string-bytes unibyte))
     ;; multibyte-string-p
     (multibyte-string-p ascii)
     (multibyte-string-p latin1)
     (multibyte-string-p cjk)
     (multibyte-string-p unibyte)))
  ;; string-width for different char types
  (let ((ascii (make-string 5 ?a))
        (cjk (make-string 5 ?\u4e16)))  ; CJK chars are typically width 2
    (list (string-width ascii)
          (string-width cjk)))
  ;; seq-length on make-string result
  (seq-length (make-string 7 ?q)))"#;
    let expect =
        expect_test::expect![[r#""OK ((10 10 t 10 20 t 10 30 10 20 nil nil t t t) (5 10) 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building strings then modifying with aset — complex mutation patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_aset_complex_mutations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Build a string then overwrite every other char
  (let ((s (make-string 8 ?-)))
    (dotimes (i 4) (aset s (* i 2) ?*))
    s)
  ;; Build a string, fill with digits
  (let ((s (make-string 10 ?0)))
    (dotimes (i 10) (aset s i (+ ?0 (% i 10))))
    s)
  ;; XOR cipher: create key string, XOR message chars against it
  (let ((msg (make-string 6 ?a))
        (key (make-string 6 ?a)))
    ;; Set msg to "secret" (sort of)
    (aset msg 0 ?h) (aset msg 1 ?e) (aset msg 2 ?l)
    (aset msg 3 ?l) (aset msg 4 ?o) (aset msg 5 ?!)
    ;; Set key to repeating pattern
    (dotimes (i 6) (aset key i (+ ?A (% i 3))))
    ;; XOR encrypt
    (let ((enc (make-string 6 0)))
      (dotimes (i 6) (aset enc i (logxor (aref msg i) (aref key i))))
      ;; XOR decrypt
      (let ((dec (make-string 6 0)))
        (dotimes (i 6) (aset dec i (logxor (aref enc i) (aref key i))))
        ;; decrypted should equal original
        (list (string= dec msg) dec msg))))
  ;; Aset with multibyte char into ASCII string (promotes to multibyte)
  (let ((s (make-string 5 ?a)))
    (aset s 2 ?\u00e9)
    (list s (length s) (multibyte-string-p s)
          (aref s 0) (aref s 2) (aref s 4)))
  ;; Build a palindrome via aset
  (let* ((half "abcde")
         (len (* 2 (length half)))
         (s (make-string len ?x)))
    (dotimes (i (length half))
      (aset s i (aref half i))
      (aset s (- len 1 i) (aref half i)))
    (list s (string= s (concat (string ?a ?b ?c ?d ?e ?e ?d ?c ?b ?a))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"*-*-*-*-\" \"0123456789\" (t \"hello!\" \"hello!\") (\"aa�aa\" 5 nil 97 233 97) (\"abcdeedcba\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-string as building block for string algorithms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_string_algorithm_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Repeat a multi-char pattern using make-string + aset
  (let ((pattern "abc")
        (repeat-count 4))
    (let* ((plen (length pattern))
           (total (* plen repeat-count))
           (result (make-string total ?x)))
      (dotimes (i total)
        (aset result i (aref pattern (% i plen))))
      result))
  ;; Build a string histogram (bar chart)
  (let ((values '(3 7 2 5 9 1 4)))
    (mapcar (lambda (v) (make-string v ?#)) values))
  ;; Levenshtein-style: create a row of a DP table as string of digits
  (let ((row (make-string 6 ?0)))
    (dotimes (i 6) (aset row i (+ ?0 i)))
    row)
  ;; Interleave two make-string results
  (let ((a (make-string 5 ?A))
        (b (make-string 5 ?B))
        (result (make-string 10 ?x)))
    (dotimes (i 5)
      (aset result (* i 2) (aref a i))
      (aset result (1+ (* i 2)) (aref b i)))
    result)
  ;; make-string as separator in mapconcat
  (mapconcat #'identity '("one" "two" "three") (make-string 3 ?-))
  ;; Complex: build formatted number with leading zeros
  (let ((nums '(1 23 456 7 89)))
    (mapcar (lambda (n)
              (let* ((s (number-to-string n))
                     (pad-len (max 0 (- 5 (length s)))))
                (concat (make-string pad-len ?0) s)))
            nums)))"#;
    let expect = expect_test::expect![[
        r##########""OK (\"abcabcabcabc\" (\"###\" \"#######\" \"##\" \"#####\" \"#########\" \"#\" \"####\") \"012345\" \"ABABABABAB\" \"one---two---three\" (\"00001\" \"00023\" \"00456\" \"00007\" \"00089\"))""##########
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
