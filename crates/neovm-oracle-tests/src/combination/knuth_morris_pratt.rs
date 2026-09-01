//! Oracle parity tests for a Knuth-Morris-Pratt string matching algorithm
//! implemented in Elisp: failure function computation, pattern search
//! returning all match positions, overlapping matches, single character
//! patterns, empty pattern handling, and performance on repeated patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// KMP failure function computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kmp_failure_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute the KMP failure (partial match) table and verify for various patterns
    let form = r#"(progn
  ;; Build the failure function table for a pattern string.
  ;; Returns a vector where fail[i] = length of longest proper prefix of
  ;; pattern[0..i] that is also a suffix.
  (fset 'neovm--kmp-build-fail
    (lambda (pattern)
      (let* ((m (length pattern))
             (fail (make-vector m 0))
             (k 0)
             (i 1))
        (while (< i m)
          (while (and (> k 0)
                      (not (= (aref pattern i) (aref pattern k))))
            (setq k (aref fail (1- k))))
          (when (= (aref pattern i) (aref pattern k))
            (setq k (1+ k)))
          (aset fail i k)
          (setq i (1+ i)))
        fail)))

  (unwind-protect
      (list
       ;; Simple pattern: "ABAB"
       ;; fail = [0, 0, 1, 2]
       (funcall 'neovm--kmp-build-fail "ABAB")

       ;; Pattern with no repeats: "ABCDE"
       ;; fail = [0, 0, 0, 0, 0]
       (funcall 'neovm--kmp-build-fail "ABCDE")

       ;; All same characters: "AAAA"
       ;; fail = [0, 1, 2, 3]
       (funcall 'neovm--kmp-build-fail "AAAA")

       ;; Classic example: "ABCABD"
       ;; fail = [0, 0, 0, 1, 2, 0]
       (funcall 'neovm--kmp-build-fail "ABCABD")

       ;; Complex pattern: "AABAABAAA"
       (funcall 'neovm--kmp-build-fail "AABAABAAA")

       ;; Single character
       (funcall 'neovm--kmp-build-fail "X")

       ;; Two characters, same
       (funcall 'neovm--kmp-build-fail "AA")

       ;; Two characters, different
       (funcall 'neovm--kmp-build-fail "AB"))
    (fmakunbound 'neovm--kmp-build-fail)))"#;
    let expect = expect_test::expect![[
        r#""OK ([0 0 1 2] [0 0 0 0 0] [0 1 2 3] [0 0 0 1 2 0] [0 1 0 1 2 3 4 5 2] [0] [0 1] [0 0])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// KMP pattern search returning all occurrences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kmp_search_all_occurrences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full KMP search: find all positions where pattern occurs in text
    let form = r#"(progn
  (fset 'neovm--kmp2-build-fail
    (lambda (pattern)
      (let* ((m (length pattern))
             (fail (make-vector m 0))
             (k 0) (i 1))
        (while (< i m)
          (while (and (> k 0)
                      (not (= (aref pattern i) (aref pattern k))))
            (setq k (aref fail (1- k))))
          (when (= (aref pattern i) (aref pattern k))
            (setq k (1+ k)))
          (aset fail i k)
          (setq i (1+ i)))
        fail)))

  ;; KMP search: returns list of starting positions (0-indexed)
  (fset 'neovm--kmp2-search
    (lambda (text pattern)
      (let* ((n (length text))
             (m (length pattern))
             (fail (funcall 'neovm--kmp2-build-fail pattern))
             (matches nil)
             (k 0)
             (i 0))
        (if (= m 0)
            ;; Empty pattern matches at every position
            (let ((pos 0))
              (while (<= pos n)
                (setq matches (cons pos matches))
                (setq pos (1+ pos)))
              (nreverse matches))
          (while (< i n)
            (while (and (> k 0)
                        (not (= (aref text i) (aref pattern k))))
              (setq k (aref fail (1- k))))
            (when (= (aref text i) (aref pattern k))
              (setq k (1+ k)))
            (when (= k m)
              (setq matches (cons (- i m -1) matches))
              (setq k (aref fail (1- k))))
            (setq i (1+ i)))
          (nreverse matches)))))

  (unwind-protect
      (list
       ;; Basic search
       (funcall 'neovm--kmp2-search "ABCABCABC" "ABC")
       ;; => (0 3 6)

       ;; No match
       (funcall 'neovm--kmp2-search "ABCDEF" "XYZ")
       ;; => nil

       ;; Single match at beginning
       (funcall 'neovm--kmp2-search "HELLO WORLD" "HELLO")
       ;; => (0)

       ;; Single match at end
       (funcall 'neovm--kmp2-search "HELLO WORLD" "WORLD")
       ;; => (6)

       ;; Pattern equals text
       (funcall 'neovm--kmp2-search "EXACT" "EXACT")
       ;; => (0)

       ;; Pattern longer than text
       (funcall 'neovm--kmp2-search "AB" "ABCDEF")
       ;; => nil

       ;; Multiple non-overlapping
       (funcall 'neovm--kmp2-search "the cat sat on the mat" "the")
       ;; => (0 19) -- but KMP finds all, let's check

       ;; Search in string with repeated characters
       (funcall 'neovm--kmp2-search "AAAAAA" "AA"))
    (fmakunbound 'neovm--kmp2-build-fail)
    (fmakunbound 'neovm--kmp2-search)))"#;
    let expect = expect_test::expect![[r#""OK ((0 3 6) nil (0) (6) (0) nil (0 15) (0 1 2 3 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Overlapping matches
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kmp_overlapping_matches() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // KMP should find overlapping matches because failure function backtracks
    let form = r#"(progn
  (fset 'neovm--kmp3-build-fail
    (lambda (pattern)
      (let* ((m (length pattern))
             (fail (make-vector m 0))
             (k 0) (i 1))
        (while (< i m)
          (while (and (> k 0)
                      (not (= (aref pattern i) (aref pattern k))))
            (setq k (aref fail (1- k))))
          (when (= (aref pattern i) (aref pattern k))
            (setq k (1+ k)))
          (aset fail i k)
          (setq i (1+ i)))
        fail)))

  (fset 'neovm--kmp3-search
    (lambda (text pattern)
      (let* ((n (length text))
             (m (length pattern))
             (fail (funcall 'neovm--kmp3-build-fail pattern))
             (matches nil)
             (k 0) (i 0))
        (while (< i n)
          (while (and (> k 0)
                      (not (= (aref text i) (aref pattern k))))
            (setq k (aref fail (1- k))))
          (when (= (aref text i) (aref pattern k))
            (setq k (1+ k)))
          (when (= k m)
            (setq matches (cons (- i m -1) matches))
            (setq k (aref fail (1- k))))
          (setq i (1+ i)))
        (nreverse matches))))

  (unwind-protect
      (list
       ;; "AA" in "AAAA" -> overlapping: (0 1 2)
       (funcall 'neovm--kmp3-search "AAAA" "AA")

       ;; "ABA" in "ABABA" -> overlapping: (0 2)
       (funcall 'neovm--kmp3-search "ABABA" "ABA")

       ;; "ABAB" in "ABABABABAB" -> overlapping: (0 2 4 6)
       (funcall 'neovm--kmp3-search "ABABABABAB" "ABAB")

       ;; "AA" in "AAAAAA" -> (0 1 2 3 4)
       (funcall 'neovm--kmp3-search "AAAAAA" "AA")

       ;; "ABCABC" in "ABCABCABCABC" -> (0 3 6)
       (funcall 'neovm--kmp3-search "ABCABCABCABC" "ABCABC")

       ;; Non-overlapping case for comparison
       (funcall 'neovm--kmp3-search "XYZXYZ" "XYZ")

       ;; "ANA" in "BANANA" -> (1 3)
       (funcall 'neovm--kmp3-search "BANANA" "ANA"))
    (fmakunbound 'neovm--kmp3-build-fail)
    (fmakunbound 'neovm--kmp3-search)))"#;
    let expect =
        expect_test::expect![[r#""OK ((0 1 2) (0 2) (0 2 4 6) (0 1 2 3 4) (0 3 6) (0 3) (1 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Single character patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kmp_single_char_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Single character patterns should work correctly (degenerate case)
    let form = r#"(progn
  (fset 'neovm--kmp4-build-fail
    (lambda (pattern)
      (let* ((m (length pattern))
             (fail (make-vector m 0))
             (k 0) (i 1))
        (while (< i m)
          (while (and (> k 0)
                      (not (= (aref pattern i) (aref pattern k))))
            (setq k (aref fail (1- k))))
          (when (= (aref pattern i) (aref pattern k))
            (setq k (1+ k)))
          (aset fail i k)
          (setq i (1+ i)))
        fail)))

  (fset 'neovm--kmp4-search
    (lambda (text pattern)
      (let* ((n (length text))
             (m (length pattern))
             (fail (funcall 'neovm--kmp4-build-fail pattern))
             (matches nil)
             (k 0) (i 0))
        (while (< i n)
          (while (and (> k 0)
                      (not (= (aref text i) (aref pattern k))))
            (setq k (aref fail (1- k))))
          (when (= (aref text i) (aref pattern k))
            (setq k (1+ k)))
          (when (= k m)
            (setq matches (cons (- i m -1) matches))
            (setq k (aref fail (1- k))))
          (setq i (1+ i)))
        (nreverse matches))))

  (unwind-protect
      (list
       ;; Find all 'A' in "ABCABC"
       (funcall 'neovm--kmp4-search "ABCABC" "A")

       ;; Find all 'B' in "ABBBBA"
       (funcall 'neovm--kmp4-search "ABBBBA" "B")

       ;; Find char not present
       (funcall 'neovm--kmp4-search "ABCDEF" "Z")

       ;; Find all occurrences of single char in all-same string
       (funcall 'neovm--kmp4-search "XXXXX" "X")

       ;; Single char text, single char pattern - match
       (funcall 'neovm--kmp4-search "A" "A")

       ;; Single char text, single char pattern - no match
       (funcall 'neovm--kmp4-search "A" "B")

       ;; Find spaces
       (funcall 'neovm--kmp4-search "a b c d" " "))
    (fmakunbound 'neovm--kmp4-build-fail)
    (fmakunbound 'neovm--kmp4-search)))"#;
    let expect =
        expect_test::expect![[r#""OK ((0 3) (1 2 3 4) nil (0 1 2 3 4) (0) nil (1 3 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Empty pattern handling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kmp_empty_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Empty pattern: by convention matches at every position (0..n)
    let form = r#"(progn
  (fset 'neovm--kmp5-search
    (lambda (text pattern)
      (let* ((n (length text))
             (m (length pattern)))
        (if (= m 0)
            ;; Empty pattern: return positions 0..n
            (let ((matches nil) (i 0))
              (while (<= i n)
                (setq matches (cons i matches))
                (setq i (1+ i)))
              (nreverse matches))
          ;; Non-empty pattern: normal KMP
          (let ((fail (make-vector m 0))
                (k 0))
            ;; Build failure function
            (let ((i 1))
              (while (< i m)
                (while (and (> k 0)
                            (not (= (aref pattern i) (aref pattern k))))
                  (setq k (aref fail (1- k))))
                (when (= (aref pattern i) (aref pattern k))
                  (setq k (1+ k)))
                (aset fail i k)
                (setq i (1+ i))))
            ;; Search
            (setq k 0)
            (let ((matches nil) (i 0))
              (while (< i n)
                (while (and (> k 0)
                            (not (= (aref text i) (aref pattern k))))
                  (setq k (aref fail (1- k))))
                (when (= (aref text i) (aref pattern k))
                  (setq k (1+ k)))
                (when (= k m)
                  (setq matches (cons (- i m -1) matches))
                  (setq k (aref fail (1- k))))
                (setq i (1+ i)))
              (nreverse matches)))))))

  (unwind-protect
      (list
       ;; Empty pattern on "ABC" -> (0 1 2 3)
       (funcall 'neovm--kmp5-search "ABC" "")

       ;; Empty pattern on "" -> (0)
       (funcall 'neovm--kmp5-search "" "")

       ;; Empty pattern on single char -> (0 1)
       (funcall 'neovm--kmp5-search "X" "")

       ;; Verify count: empty pattern on n-char string gives n+1 matches
       (length (funcall 'neovm--kmp5-search "HELLO" ""))

       ;; Non-empty on empty text -> nil
       (funcall 'neovm--kmp5-search "" "ABC")

       ;; Verify both empty pattern and normal search work in same function
       (list (funcall 'neovm--kmp5-search "ABAB" "")
             (funcall 'neovm--kmp5-search "ABAB" "AB")
             (funcall 'neovm--kmp5-search "ABAB" "BA")))
    (fmakunbound 'neovm--kmp5-search)))"#;
    let expect =
        expect_test::expect![[r#""OK ((0 1 2 3) (0) (0 1) 6 nil ((0 1 2 3 4) (0 2) (1)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Performance on repeated patterns (worst-case behavior)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kmp_repeated_patterns_performance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // KMP handles worst-case inputs (like "AAAA...A" with pattern "AAB")
    // efficiently due to failure function. Verify correctness on such inputs.
    let form = r#"(progn
  (fset 'neovm--kmp6-build-fail
    (lambda (pattern)
      (let* ((m (length pattern))
             (fail (make-vector m 0))
             (k 0) (i 1))
        (while (< i m)
          (while (and (> k 0)
                      (not (= (aref pattern i) (aref pattern k))))
            (setq k (aref fail (1- k))))
          (when (= (aref pattern i) (aref pattern k))
            (setq k (1+ k)))
          (aset fail i k)
          (setq i (1+ i)))
        fail)))

  (fset 'neovm--kmp6-search
    (lambda (text pattern)
      (let* ((n (length text))
             (m (length pattern))
             (fail (funcall 'neovm--kmp6-build-fail pattern))
             (matches nil)
             (k 0) (i 0))
        (while (< i n)
          (while (and (> k 0)
                      (not (= (aref text i) (aref pattern k))))
            (setq k (aref fail (1- k))))
          (when (= (aref text i) (aref pattern k))
            (setq k (1+ k)))
          (when (= k m)
            (setq matches (cons (- i m -1) matches))
            (setq k (aref fail (1- k))))
          (setq i (1+ i)))
        (nreverse matches))))

  ;; Build repeated strings
  (fset 'neovm--kmp6-repeat
    (lambda (s n)
      (let ((result ""))
        (dotimes (_ n)
          (setq result (concat result s)))
        result)))

  (unwind-protect
      (let ((long-a (funcall 'neovm--kmp6-repeat "A" 50)))
        (list
         ;; "AAB" never found in "AAA...A" (50 A's)
         (funcall 'neovm--kmp6-search long-a "AAB")

         ;; "AAB" found once in "AAA...AAB"
         (funcall 'neovm--kmp6-search (concat long-a "B") "AAB")

         ;; Repeated pattern in repeated text
         (let ((text (funcall 'neovm--kmp6-repeat "AB" 20))   ;; 40 chars
               (pat "ABAB"))
           (length (funcall 'neovm--kmp6-search text pat)))

         ;; All same character: "AAA" in "AAAAAAAAAA" (10 A's)
         (funcall 'neovm--kmp6-search (funcall 'neovm--kmp6-repeat "A" 10) "AAA")

         ;; Periodic pattern
         (let ((text (funcall 'neovm--kmp6-repeat "ABCABC" 5))
               (pat "ABCABCABC"))
           (funcall 'neovm--kmp6-search text pat))

         ;; Verify count of "AB" in "ABABABABABABABABABABAB" (10 repetitions = 20 chars)
         (length (funcall 'neovm--kmp6-search
                          (funcall 'neovm--kmp6-repeat "AB" 10)
                          "AB"))

         ;; Worst case for naive search, but KMP handles: "AAAAAB" in long "AAA...AAAAAB"
         (let ((text (concat (funcall 'neovm--kmp6-repeat "A" 30) "B")))
           (funcall 'neovm--kmp6-search text "AAAAAB"))))
    (fmakunbound 'neovm--kmp6-build-fail)
    (fmakunbound 'neovm--kmp6-search)
    (fmakunbound 'neovm--kmp6-repeat)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (48) 19 (0 1 2 3 4 5 6 7) (0 3 6 9 12 15 18 21) 10 (25))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// KMP with case-insensitive matching via pre-processing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_kmp_case_insensitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement case-insensitive KMP by downcasing both text and pattern
    let form = r#"(progn
  (fset 'neovm--kmp7-build-fail
    (lambda (pattern)
      (let* ((m (length pattern))
             (fail (make-vector m 0))
             (k 0) (i 1))
        (while (< i m)
          (while (and (> k 0)
                      (not (= (aref pattern i) (aref pattern k))))
            (setq k (aref fail (1- k))))
          (when (= (aref pattern i) (aref pattern k))
            (setq k (1+ k)))
          (aset fail i k)
          (setq i (1+ i)))
        fail)))

  (fset 'neovm--kmp7-search
    (lambda (text pattern)
      (let* ((n (length text))
             (m (length pattern))
             (fail (funcall 'neovm--kmp7-build-fail pattern))
             (matches nil)
             (k 0) (i 0))
        (while (< i n)
          (while (and (> k 0)
                      (not (= (aref text i) (aref pattern k))))
            (setq k (aref fail (1- k))))
          (when (= (aref text i) (aref pattern k))
            (setq k (1+ k)))
          (when (= k m)
            (setq matches (cons (- i m -1) matches))
            (setq k (aref fail (1- k))))
          (setq i (1+ i)))
        (nreverse matches))))

  ;; Case-insensitive wrapper
  (fset 'neovm--kmp7-search-ci
    (lambda (text pattern)
      (funcall 'neovm--kmp7-search
               (downcase text) (downcase pattern))))

  (unwind-protect
      (list
       ;; Case-insensitive: "hello" matches "Hello", "HELLO", "hElLo"
       (funcall 'neovm--kmp7-search-ci "Hello World HELLO world" "hello")

       ;; Mixed case pattern
       (funcall 'neovm--kmp7-search-ci "The Quick Brown Fox" "THE")

       ;; All caps text, lowercase pattern
       (funcall 'neovm--kmp7-search-ci "ABCABCABC" "abc")

       ;; Verify case-sensitive vs case-insensitive
       (list
        (funcall 'neovm--kmp7-search "Hello HELLO" "hello")    ;; case-sensitive: nil
        (funcall 'neovm--kmp7-search-ci "Hello HELLO" "hello")) ;; case-insensitive: (0 6)

       ;; Empty results when truly no match
       (funcall 'neovm--kmp7-search-ci "ABCDEF" "XYZ"))
    (fmakunbound 'neovm--kmp7-build-fail)
    (fmakunbound 'neovm--kmp7-search)
    (fmakunbound 'neovm--kmp7-search-ci)))"#;
    let expect = expect_test::expect![[r#""OK ((0 12) (0) (0 3 6) (nil (0 6)) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
