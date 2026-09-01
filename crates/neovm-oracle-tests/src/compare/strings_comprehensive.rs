//! Comprehensive oracle parity tests for `compare-strings` with ALL parameters:
//! STRING1, START1, END1, STRING2, START2, END2, IGNORE-CASE.
//! Tests partial comparisons, return value semantics, Unicode, edge cases,
//! and algorithmic usage patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Full parameter exploration: all 7 args with systematic combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_comprehensive_all_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; All nils (full string comparison)
      (push (compare-strings "abc" nil nil "abc" nil nil) results)
      (push (compare-strings "abc" nil nil "abc" nil nil nil) results)

      ;; START1 only
      (push (compare-strings "xxabc" 2 nil "abc" nil nil) results)
      (push (compare-strings "xxabc" 2 nil "abd" nil nil) results)

      ;; END1 only
      (push (compare-strings "abcxx" nil 3 "abc" nil nil) results)
      (push (compare-strings "abcxx" nil 3 "abd" nil nil) results)

      ;; START1 + END1
      (push (compare-strings "xxabcyy" 2 5 "abc" nil nil) results)
      (push (compare-strings "xxabcyy" 2 5 "abd" nil nil) results)

      ;; START2 only
      (push (compare-strings "abc" nil nil "xxabc" 2 nil) results)

      ;; END2 only
      (push (compare-strings "abc" nil nil "abcxx" nil 3) results)

      ;; START2 + END2
      (push (compare-strings "abc" nil nil "xxabcyy" 2 5) results)

      ;; All four: START1, END1, START2, END2
      (push (compare-strings "aaXYZbb" 2 5 "ccXYZdd" 2 5) results)
      (push (compare-strings "aaABCbb" 2 5 "ccXYZdd" 2 5) results)

      ;; All seven params including IGNORE-CASE
      (push (compare-strings "aaXYZbb" 2 5 "ccxyzdd" 2 5 t) results)
      (push (compare-strings "aaXYZbb" 2 5 "ccxyzdd" 2 5 nil) results)

      ;; START at 0 explicitly vs nil
      (push (compare-strings "abc" 0 nil "abc" 0 nil) results)
      (push (compare-strings "abc" 0 3 "abc" 0 3) results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t t t -3 t -3 t -3 t t t t -1 t -1 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Return value semantics: t, positive N, negative N
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_comprehensive_return_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Equal -> t
      (push (compare-strings "hello" nil nil "hello" nil nil) results)

      ;; STR1 < STR2 at position 1 -> -1
      (push (compare-strings "a" nil nil "b" nil nil) results)

      ;; STR1 > STR2 at position 1 -> 1
      (push (compare-strings "b" nil nil "a" nil nil) results)

      ;; Differ at position 4: "abcD" vs "abcZ"
      (push (compare-strings "abcD" nil nil "abcZ" nil nil) results)
      (push (compare-strings "abcZ" nil nil "abcD" nil nil) results)

      ;; STR1 is prefix of STR2: returns -(len+1)
      (push (compare-strings "ab" nil nil "abcd" nil nil) results)  ;; -3
      (push (compare-strings "a" nil nil "abcd" nil nil) results)   ;; -2
      (push (compare-strings "" nil nil "abc" nil nil) results)     ;; -1

      ;; STR2 is prefix of STR1: returns +(len+1)
      (push (compare-strings "abcd" nil nil "ab" nil nil) results)  ;; 3
      (push (compare-strings "abcd" nil nil "a" nil nil) results)   ;; 2
      (push (compare-strings "abc" nil nil "" nil nil) results)     ;; 1

      ;; Same length, differ at last char
      (push (compare-strings "helloa" nil nil "helloz" nil nil) results)

      ;; Subranges produce same position semantics
      (push (compare-strings "xxABC" 2 nil "xxABD" 2 nil) results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t -1 1 -4 4 -3 -2 -1 3 2 1 -6 -3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case sensitivity vs case insensitivity: thorough coverage
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_comprehensive_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Case-sensitive: uppercase < lowercase in ASCII
      (push (compare-strings "A" nil nil "a" nil nil nil) results)
      (push (compare-strings "a" nil nil "A" nil nil nil) results)
      (push (compare-strings "Hello" nil nil "hello" nil nil nil) results)
      (push (compare-strings "HELLO" nil nil "hello" nil nil nil) results)

      ;; Case-insensitive: should be equal
      (push (compare-strings "A" nil nil "a" nil nil t) results)
      (push (compare-strings "Hello" nil nil "hELLO" nil nil t) results)
      (push (compare-strings "HELLO" nil nil "hello" nil nil t) results)
      (push (compare-strings "HeLLo WoRLd" nil nil "hello world" nil nil t) results)

      ;; Case-insensitive with subranges
      (push (compare-strings "xxHELLOyy" 2 7 "zzHelloww" 2 7 t) results)
      (push (compare-strings "xxHELLOyy" 2 7 "zzHelloww" 2 7 nil) results)

      ;; Non-alphabetic chars unaffected by case flag
      (push (compare-strings "123!@#" nil nil "123!@#" nil nil t) results)
      (push (compare-strings "abc123" nil nil "ABC123" nil nil t) results)
      (push (compare-strings "abc-def" nil nil "ABC-DEF" nil nil t) results)

      ;; Non-nil, non-t value for IGNORE-CASE (any non-nil works)
      (push (compare-strings "ABC" nil nil "abc" nil nil 'yes) results)
      (push (compare-strings "ABC" nil nil "abc" nil nil 1) results)

      ;; Case-insensitive but content actually differs
      (push (compare-strings "APPLE" nil nil "BANANA" nil nil t) results)
      (push (compare-strings "apple" nil nil "banana" nil nil t) results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (-1 1 -1 -1 t t t t t -2 t t t t t -1 -1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Empty substrings, boundary conditions, zero-length ranges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_comprehensive_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Both empty strings
      (push (compare-strings "" nil nil "" nil nil) results)

      ;; Empty via zero-length range
      (push (compare-strings "abc" 0 0 "xyz" 0 0) results)
      (push (compare-strings "abc" 1 1 "xyz" 1 1) results)
      (push (compare-strings "abc" 3 3 "xyz" 3 3) results)

      ;; Empty range vs non-empty range
      (push (compare-strings "abc" 0 0 "abc" nil nil) results)
      (push (compare-strings "abc" nil nil "abc" 0 0) results)

      ;; Start equals string length (end of string)
      (push (compare-strings "abc" 3 nil "xyz" 3 nil) results)

      ;; Single char subranges
      (push (compare-strings "abc" 0 1 "axyz" 0 1) results)
      (push (compare-strings "abc" 1 2 "xbyz" 1 2) results)
      (push (compare-strings "abc" 2 3 "xyc" 2 3) results)

      ;; End beyond string length (nil = end of string)
      (push (compare-strings "abc" 0 nil "abc" 0 nil) results)
      (push (compare-strings "abc" 1 nil "xbc" 1 nil) results)

      ;; Adjacent subranges in same string
      (push (compare-strings "abcabc" 0 3 "abcabc" 3 6) results)

      ;; Whitespace-only strings
      (push (compare-strings "   " nil nil "   " nil nil) results)
      (push (compare-strings " " nil nil "  " nil nil) results)
      (push (compare-strings "\t" nil nil "\t" nil nil) results)
      (push (compare-strings "\n" nil nil "\n" nil nil) results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t t t t -1 1 t t t t t t t t -2 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unicode string comparisons
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_comprehensive_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Same Unicode strings
      (push (compare-strings "\u00e9" nil nil "\u00e9" nil nil) results)

      ;; Different Unicode chars
      (push (compare-strings "\u00e9" nil nil "\u00e8" nil nil) results)

      ;; Mixed ASCII and Unicode
      (push (compare-strings "caf\u00e9" nil nil "caf\u00e9" nil nil) results)
      (push (compare-strings "caf\u00e9" nil nil "cafe" nil nil) results)

      ;; Unicode subrange comparison
      (push (compare-strings "xx\u00e9yy" 2 3 "zz\u00e9ww" 2 3) results)

      ;; CJK characters
      (push (compare-strings "\u4e16\u754c" nil nil "\u4e16\u754c" nil nil) results)
      (push (compare-strings "\u4e16" nil nil "\u754c" nil nil) results)

      ;; Greek letters
      (push (compare-strings "\u03b1\u03b2\u03b3" nil nil "\u03b1\u03b2\u03b3" nil nil) results)
      (push (compare-strings "\u03b1" nil nil "\u03b2" nil nil) results)

      ;; Case-insensitive with accented chars
      ;; (Emacs compare-strings case folding may or may not handle accented chars)
      (push (compare-strings "ABC" nil nil "abc" nil nil t) results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t 1 t 4 t t -1 t -1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Algorithmic: lexicographic sort using compare-strings subranges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_comprehensive_sort_algorithm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a radix-like sort that compares character by character
    // using compare-strings with single-char subranges
    let form = r#"(progn
  ;; Compare two strings character by character using compare-strings
  ;; Return result of first differing position
  (fset 'neovm--cs-charwise-cmp
    (lambda (s1 s2 ignore-case)
      (let ((len1 (length s1))
            (len2 (length s2))
            (i 0)
            (result nil))
        (while (and (not result) (< i len1) (< i len2))
          (let ((r (compare-strings s1 i (1+ i) s2 i (1+ i) ignore-case)))
            (unless (eq r t)
              (setq result (list 'diff-at i r))))
          (setq i (1+ i)))
        (or result
            (cond
             ((= len1 len2) 'equal)
             ((< len1 len2) 'shorter)
             (t 'longer))))))

  ;; Insertion sort using compare-strings
  (fset 'neovm--cs-insertion-sort
    (lambda (strings ignore-case)
      (let ((sorted (list (car strings))))
        (dolist (s (cdr strings))
          (let ((inserted nil)
                (result nil)
                (rest sorted)
                (prev nil))
            (while (and rest (not inserted))
              (let ((r (compare-strings s nil nil (car rest) nil nil ignore-case)))
                (if (or (eq r t) (and (integerp r) (< r 0)))
                    (progn
                      (if prev
                          (setcdr prev (cons s rest))
                        (setq sorted (cons s rest)))
                      (setq inserted t))
                  (setq prev rest)
                  (setq rest (cdr rest)))))
            (unless inserted
              (if prev
                  (setcdr prev (list s))
                (setq sorted (list s))))))
        sorted)))

  (unwind-protect
      (list
       ;; Character-wise comparison results
       (funcall 'neovm--cs-charwise-cmp "abc" "abc" nil)
       (funcall 'neovm--cs-charwise-cmp "abc" "abd" nil)
       (funcall 'neovm--cs-charwise-cmp "abc" "ab" nil)
       (funcall 'neovm--cs-charwise-cmp "ab" "abc" nil)
       (funcall 'neovm--cs-charwise-cmp "ABC" "abc" t)
       ;; Insertion sort: case-sensitive
       (funcall 'neovm--cs-insertion-sort
                '("banana" "apple" "cherry" "date" "apricot") nil)
       ;; Insertion sort: case-insensitive
       (funcall 'neovm--cs-insertion-sort
                '("Banana" "apple" "Cherry" "DATE" "apricot") t)
       ;; Sort single-char strings
       (funcall 'neovm--cs-insertion-sort
                '("z" "a" "m" "b" "y") nil))
    (fmakunbound 'neovm--cs-charwise-cmp)
    (fmakunbound 'neovm--cs-insertion-sort)))"#;
    let expect = expect_test::expect![[
        r#""OK (equal (diff-at 2 -1) longer shorter equal (\"apple\" \"apricot\" \"banana\" \"cherry\" \"date\") (\"apple\" \"apricot\" \"Banana\" \"Cherry\" \"DATE\") (\"a\" \"b\" \"m\" \"y\" \"z\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sliding window substring comparison (pattern matching via compare-strings)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_comprehensive_pattern_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement substring search using compare-strings as the comparison primitive
    let form = r#"(progn
  ;; Find all positions where pattern appears in text using compare-strings
  (fset 'neovm--cs-find-all
    (lambda (text pattern ignore-case)
      (let ((tlen (length text))
            (plen (length pattern))
            (positions nil)
            (i 0))
        (while (<= (+ i plen) tlen)
          (when (eq t (compare-strings text i (+ i plen) pattern nil nil ignore-case))
            (push i positions))
          (setq i (1+ i)))
        (nreverse positions))))

  ;; Count occurrences using compare-strings subranges
  (fset 'neovm--cs-count
    (lambda (text pattern ignore-case)
      (length (funcall 'neovm--cs-find-all text pattern ignore-case))))

  (unwind-protect
      (list
       ;; Find "ab" in "aababcab"
       (funcall 'neovm--cs-find-all "aababcab" "ab" nil)
       ;; Case-insensitive search
       (funcall 'neovm--cs-find-all "Hello hello HELLO" "hello" t)
       ;; No matches
       (funcall 'neovm--cs-find-all "abcdef" "xyz" nil)
       ;; Pattern equals text
       (funcall 'neovm--cs-find-all "abc" "abc" nil)
       ;; Pattern longer than text
       (funcall 'neovm--cs-find-all "ab" "abcd" nil)
       ;; Single char pattern
       (funcall 'neovm--cs-find-all "abacaba" "a" nil)
       ;; Count occurrences
       (funcall 'neovm--cs-count "mississippi" "ss" nil)
       (funcall 'neovm--cs-count "Mississippi" "ss" t)
       ;; Overlapping occurrences
       (funcall 'neovm--cs-find-all "aaa" "aa" nil))
    (fmakunbound 'neovm--cs-find-all)
    (fmakunbound 'neovm--cs-count)))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 3 6) (0 6 12) nil (0) nil (0 2 4 6) 2 2 (0 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
