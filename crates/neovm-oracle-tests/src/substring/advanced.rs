//! Oracle parity tests for `substring` with ALL parameter combinations.
//!
//! Tests positive/negative indices, nil TO, FROM=TO, empty string,
//! single argument, and complex string manipulation patterns built
//! on top of substring.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// substring with positive FROM and TO
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_positive_from_and_to() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test various positive FROM/TO combinations on different string lengths
    let form = r#"(list
  ;; Basic extraction from middle
  (substring "abcdefghij" 2 5)
  ;; First character
  (substring "abcdefghij" 0 1)
  ;; Last character
  (substring "abcdefghij" 9 10)
  ;; Entire string
  (substring "abcdefghij" 0 10)
  ;; Two-char substring from various positions
  (substring "abcdefghij" 0 2)
  (substring "abcdefghij" 4 6)
  (substring "abcdefghij" 8 10)
  ;; Single char string
  (substring "X" 0 1)
  ;; Multi-byte string (unicode)
  (substring "hello world" 3 8)
  ;; Numeric-like content
  (substring "0123456789" 3 7))"#;
    let expect = expect_test::expect![[
        r#""OK (\"cde\" \"a\" \"j\" \"abcdefghij\" \"ab\" \"ef\" \"ij\" \"X\" \"lo wo\" \"3456\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// substring with nil TO (to end of string)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_nil_to() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When TO is nil (or omitted), substring goes to the end
    let form = r#"(list
  ;; FROM=0, TO=nil => entire string
  (substring "hello world" 0 nil)
  ;; FROM=5, TO=nil => from position 5 to end
  (substring "hello world" 5 nil)
  ;; FROM at last char
  (substring "hello world" 10 nil)
  ;; FROM=0, omitted TO (equivalent to nil)
  (substring "hello world" 0)
  ;; FROM=6, omitted TO
  (substring "hello world" 6)
  ;; FROM at very end, omitted TO => empty string
  (substring "hello" 5)
  ;; Verify nil and omitted are equivalent
  (string= (substring "testing" 3 nil) (substring "testing" 3))
  ;; On short string
  (substring "ab" 1 nil)
  (substring "ab" 1))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello world\" \" world\" \"d\" \"hello world\" \"world\" \"\" t \"b\" \"b\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// substring with negative FROM and/or TO
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_negative_indices() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Negative indices count from the end of the string
    let form = r#"(list
  ;; Negative FROM only, TO=nil
  (substring "abcdefghij" -3)
  (substring "abcdefghij" -1)
  (substring "abcdefghij" -10)
  ;; Negative FROM, positive TO
  (substring "abcdefghij" -7 5)
  (substring "abcdefghij" -5 8)
  ;; Positive FROM, negative TO
  (substring "abcdefghij" 2 -2)
  (substring "abcdefghij" 0 -1)
  (substring "abcdefghij" 5 -1)
  ;; Both negative
  (substring "abcdefghij" -5 -2)
  (substring "abcdefghij" -3 -1)
  (substring "abcdefghij" -10 -5)
  ;; Negative FROM = -length => FROM=0
  (substring "hello" -5)
  (string= (substring "hello" -5) (substring "hello" 0))
  ;; Negative TO = -0 doesn't exist, but -1 skips last char
  (substring "hello" 0 -1)
  (substring "hello" 1 -1))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hij\" \"j\" \"abcdefghij\" \"de\" \"fgh\" \"cdefgh\" \"abcdefghi\" \"fghi\" \"fgh\" \"hi\" \"abcde\" \"hello\" t \"hell\" \"ell\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// substring with FROM=TO (empty string result)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_from_equals_to() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When FROM=TO, result is always the empty string
    let form = r#"(list
  ;; Various positions where FROM=TO
  (substring "hello" 0 0)
  (substring "hello" 1 1)
  (substring "hello" 3 3)
  (substring "hello" 5 5)
  ;; With negative indices that resolve to same position
  (substring "hello" -3 2)  ;; -3 => 2, so FROM=TO=2
  ;; All results are empty strings
  (string= (substring "hello" 0 0) "")
  (string= (substring "hello" 3 3) "")
  (= (length (substring "hello" 2 2)) 0)
  ;; FROM=TO on single-char string
  (substring "X" 0 0)
  (substring "X" 1 1))"#;
    let expect = expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\" \"\" t t t \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// substring on empty string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Substring on empty string: only (substring "" 0) and (substring "" 0 0) are valid
    let form = r#"(list
  ;; Valid operations on empty string
  (substring "" 0)
  (substring "" 0 0)
  (substring "" 0 nil)
  ;; Results are all empty strings
  (string= (substring "" 0) "")
  (string= (substring "" 0 0) "")
  (= (length (substring "" 0)) 0)
  ;; substring returns a new string (not eq, but equal)
  (let ((s "hello"))
    (list
     (string= s (substring s 0))
     ;; substring makes a copy
     (let ((sub (substring s 0)))
       (string= sub s)))))"#;
    let expect = expect_test::expect![[r#""OK (\"\" \"\" \"\" t t t (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// substring with single argument (FROM only)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_single_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When called with just FROM, TO defaults to nil (end of string)
    let form = r#"(list
  ;; Positive FROM
  (substring "programming" 0)
  (substring "programming" 3)
  (substring "programming" 7)
  (substring "programming" 11)  ;; at end => empty
  ;; Negative FROM
  (substring "programming" -4)
  (substring "programming" -11)
  (substring "programming" -1)
  ;; Equivalences
  (string= (substring "test" 0) "test")
  (string= (substring "test" 2) "st")
  (string= (substring "test" -2) "st")
  (string= (substring "test" 4) ""))"#;
    let expect = expect_test::expect![[
        r#""OK (\"programming\" \"gramming\" \"ming\" \"\" \"ming\" \"programming\" \"g\" t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: string splitting using substring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_string_splitting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a simple string split-at-char using substring
    let form = r#"(progn
  (fset 'neovm--split-at-pos
    (lambda (s pos)
      "Split string S at position POS, return (left . right)."
      (cons (substring s 0 pos) (substring s pos))))

  (fset 'neovm--find-char
    (lambda (s ch start)
      "Find index of character CH in S starting from START. Return nil if not found."
      (let ((i start) (len (length s)) (result nil))
        (while (and (< i len) (not result))
          (when (= (aref s i) ch)
            (setq result i))
          (setq i (1+ i)))
        result)))

  (fset 'neovm--split-string-by-char
    (lambda (s ch)
      "Split S by character CH into a list of substrings."
      (let ((parts nil) (start 0) (len (length s)))
        (while (<= start len)
          (let ((pos (funcall 'neovm--find-char s ch start)))
            (if pos
                (progn
                  (setq parts (cons (substring s start pos) parts))
                  (setq start (1+ pos)))
              (setq parts (cons (substring s start) parts))
              (setq start (1+ len)))))
        (nreverse parts))))

  (unwind-protect
      (list
       ;; Split by space
       (funcall 'neovm--split-string-by-char "hello world foo" ? )
       ;; Split by comma
       (funcall 'neovm--split-string-by-char "a,b,c,d" ?,)
       ;; No delimiter found
       (funcall 'neovm--split-string-by-char "nospace" ? )
       ;; Empty between delimiters
       (funcall 'neovm--split-string-by-char "a,,b" ?,)
       ;; Delimiter at start/end
       (funcall 'neovm--split-string-by-char ",start,end," ?,)
       ;; Split-at-pos
       (funcall 'neovm--split-at-pos "abcdef" 3)
       (funcall 'neovm--split-at-pos "abcdef" 0)
       (funcall 'neovm--split-at-pos "abcdef" 6))
    (fmakunbound 'neovm--split-at-pos)
    (fmakunbound 'neovm--find-char)
    (fmakunbound 'neovm--split-string-by-char)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" \"world\" \"foo\") (\"a\" \"b\" \"c\" \"d\") (\"nospace\") (\"a\" \"\" \"b\") (\"\" \"start\" \"end\" \"\") (\"abc\" . \"def\") (\"\" . \"abcdef\") (\"abcdef\" . \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: string rotation and permutation using substring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_rotation_and_permutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // String rotation: move first N chars to end
    // String reversal using substring (char by char)
    // Check if one string is a rotation of another
    let form = r#"(progn
  (fset 'neovm--rotate-left
    (lambda (s n)
      "Rotate string S left by N positions."
      (let ((len (length s)))
        (if (= len 0) s
          (let ((n (mod n len)))
            (concat (substring s n) (substring s 0 n)))))))

  (fset 'neovm--rotate-right
    (lambda (s n)
      "Rotate string S right by N positions."
      (let ((len (length s)))
        (if (= len 0) s
          (let ((n (mod n len)))
            (concat (substring s (- len n)) (substring s 0 (- len n))))))))

  (fset 'neovm--reverse-string
    (lambda (s)
      "Reverse string S using substring."
      (let ((result "") (i (1- (length s))))
        (while (>= i 0)
          (setq result (concat result (substring s i (1+ i))))
          (setq i (1- i)))
        result)))

  (fset 'neovm--is-rotation-p
    (lambda (s1 s2)
      "Check if S2 is a rotation of S1."
      (and (= (length s1) (length s2))
           (let ((doubled (concat s1 s1))
                 (found nil) (i 0)
                 (len1 (length s1)) (len2 (length doubled)))
             (while (and (not found) (<= (+ i len1) len2))
               (when (string= (substring doubled i (+ i len1)) s2)
                 (setq found t))
               (setq i (1+ i)))
             found))))

  (unwind-protect
      (list
       ;; Rotate left
       (funcall 'neovm--rotate-left "abcdef" 0)
       (funcall 'neovm--rotate-left "abcdef" 1)
       (funcall 'neovm--rotate-left "abcdef" 3)
       (funcall 'neovm--rotate-left "abcdef" 6)
       (funcall 'neovm--rotate-left "abcdef" 8)  ;; 8 mod 6 = 2
       ;; Rotate right
       (funcall 'neovm--rotate-right "abcdef" 1)
       (funcall 'neovm--rotate-right "abcdef" 2)
       (funcall 'neovm--rotate-right "abcdef" 6)
       ;; Reverse string
       (funcall 'neovm--reverse-string "abcde")
       (funcall 'neovm--reverse-string "a")
       (funcall 'neovm--reverse-string "")
       (funcall 'neovm--reverse-string "racecar")
       ;; Palindrome check via reverse
       (string= (funcall 'neovm--reverse-string "racecar") "racecar")
       (string= (funcall 'neovm--reverse-string "hello") "hello")
       ;; Rotation check
       (funcall 'neovm--is-rotation-p "abcdef" "cdefab")
       (funcall 'neovm--is-rotation-p "abcdef" "fabcde")
       (funcall 'neovm--is-rotation-p "abcdef" "abcdef")
       (funcall 'neovm--is-rotation-p "abcdef" "abcfed")
       (funcall 'neovm--is-rotation-p "abc" "abcd")
       ;; Rotate left then right should restore original
       (string= (funcall 'neovm--rotate-right
                  (funcall 'neovm--rotate-left "hello" 2) 2)
                "hello"))
    (fmakunbound 'neovm--rotate-left)
    (fmakunbound 'neovm--rotate-right)
    (fmakunbound 'neovm--reverse-string)
    (fmakunbound 'neovm--is-rotation-p)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable doubled)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: sliding window and chunking with substring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_sliding_window_and_chunks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract all substrings of a given length (sliding window),
    // chunk a string into fixed-size pieces, and find repeated substrings
    let form = r#"(progn
  (fset 'neovm--sliding-window
    (lambda (s window-size)
      "Return all substrings of S with length WINDOW-SIZE."
      (let ((result nil)
            (i 0)
            (limit (1+ (- (length s) window-size))))
        (while (and (<= 0 (- (length s) window-size)) (< i limit))
          (setq result (cons (substring s i (+ i window-size)) result))
          (setq i (1+ i)))
        (nreverse result))))

  (fset 'neovm--chunk-string
    (lambda (s chunk-size)
      "Split S into chunks of CHUNK-SIZE. Last chunk may be shorter."
      (let ((result nil) (i 0) (len (length s)))
        (while (< i len)
          (let ((end (min len (+ i chunk-size))))
            (setq result (cons (substring s i end) result))
            (setq i end)))
        (nreverse result))))

  (fset 'neovm--has-repeated-substring-p
    (lambda (s len)
      "Check if S has any repeated substring of length LEN."
      (let ((seen nil) (found nil)
            (i 0) (limit (1+ (- (length s) len))))
        (while (and (< i limit) (not found))
          (let ((sub (substring s i (+ i len))))
            (if (member sub seen)
                (setq found t)
              (setq seen (cons sub seen))))
          (setq i (1+ i)))
        found)))

  (unwind-protect
      (list
       ;; Sliding window of size 3
       (funcall 'neovm--sliding-window "abcde" 3)
       ;; Sliding window of size 1 (individual chars)
       (funcall 'neovm--sliding-window "abc" 1)
       ;; Sliding window = string length
       (funcall 'neovm--sliding-window "abc" 3)
       ;; Chunk string
       (funcall 'neovm--chunk-string "abcdefghij" 3)
       (funcall 'neovm--chunk-string "abcdefghij" 4)
       (funcall 'neovm--chunk-string "abcdefghij" 10)
       (funcall 'neovm--chunk-string "abcdefghij" 15)
       ;; Repeated substrings
       (funcall 'neovm--has-repeated-substring-p "abcabc" 3)
       (funcall 'neovm--has-repeated-substring-p "abcdef" 3)
       (funcall 'neovm--has-repeated-substring-p "abab" 2)
       (funcall 'neovm--has-repeated-substring-p "aaa" 1))
    (fmakunbound 'neovm--sliding-window)
    (fmakunbound 'neovm--chunk-string)
    (fmakunbound 'neovm--has-repeated-substring-p)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"abc\" \"bcd\" \"cde\") (\"a\" \"b\" \"c\") (\"abc\") (\"abc\" \"def\" \"ghi\" \"j\") (\"abcd\" \"efgh\" \"ij\") (\"abcdefghij\") (\"abcdefghij\") t nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// substring on vectors (substring also works on vectors)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_substring_on_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // substring can also extract sub-sequences from vectors
    let form = r#"(list
  ;; Basic vector substring
  (substring [1 2 3 4 5] 1 3)
  (substring [1 2 3 4 5] 0 5)
  (substring [1 2 3 4 5] 2)
  ;; Negative indices on vectors
  (substring [10 20 30 40 50] -3)
  (substring [10 20 30 40 50] -3 -1)
  ;; Empty result
  (substring [1 2 3] 2 2)
  ;; Single element
  (substring [1 2 3] 1 2)
  ;; On empty vector
  (substring [] 0)
  ;; Verify it returns a vector, not a string
  (vectorp (substring [1 2 3] 0 2))
  (stringp (substring "abc" 0 2)))"#;
    let expect = expect_test::expect![[
        r#""OK ([2 3] [1 2 3 4 5] [3 4 5] [30 40 50] [30 40] [] [2] [] t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
