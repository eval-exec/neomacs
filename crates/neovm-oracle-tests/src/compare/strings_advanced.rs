//! Advanced oracle parity tests for `compare-strings`.
//!
//! Tests all 7 parameters (STR1, START1, END1, STR2, START2, END2, IGNORE-CASE),
//! partial substring comparisons, return value semantics (t for equal,
//! negative/positive for mismatch position), nil boundaries, empty substrings,
//! case-insensitive matching, and multi-step algorithmic usage.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Return value semantics: exact position of first difference
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_return_position_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // compare-strings returns:
    //   t           if the substrings are equal
    //   -N          if STR1 < STR2, where N = 1-based position of first difference
    //   +N          if STR1 > STR2, where N = 1-based position of first difference
    let form = r#"(let ((results nil))
      ;; Equal strings -> t
      (push (compare-strings "hello" nil nil "hello" nil nil) results)

      ;; First char differs: "a" < "z" -> -1
      (push (compare-strings "abc" nil nil "zbc" nil nil) results)

      ;; First char differs: "z" > "a" -> 1
      (push (compare-strings "zbc" nil nil "abc" nil nil) results)

      ;; Differ at position 3: "abcX" vs "abcY" -> -4 or +4
      (push (compare-strings "abcx" nil nil "abcy" nil nil) results)
      (push (compare-strings "abcy" nil nil "abcx" nil nil) results)

      ;; STR1 shorter prefix: "ab" vs "abcd" -> -(length("ab")+1) = -3
      (push (compare-strings "ab" nil nil "abcd" nil nil) results)

      ;; STR2 shorter prefix: "abcd" vs "ab" -> +3
      (push (compare-strings "abcd" nil nil "ab" nil nil) results)

      ;; Differ at position 5: "helloA" vs "helloZ"
      (push (compare-strings "helloA" nil nil "helloZ" nil nil) results)

      ;; Single character strings
      (push (compare-strings "a" nil nil "b" nil nil) results)
      (push (compare-strings "b" nil nil "a" nil nil) results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t -1 1 -4 4 -3 3 -6 -1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Substring comparisons with all START/END combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_substring_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Both use subranges: "xxABCyy"[2..5] vs "zzABCww"[2..5] -> "ABC" vs "ABC" -> t
      (push (compare-strings "xxABCyy" 2 5 "zzABCww" 2 5) results)

      ;; Different subranges same string: "abcdefgh"[0..3] vs "abcdefgh"[3..6]
      ;; "abc" vs "def" -> -1 (a < d)
      (push (compare-strings "abcdefgh" 0 3 "abcdefgh" 3 6) results)

      ;; nil means 0 for start, length for end
      (push (compare-strings "hello" nil nil "hello" nil nil) results)
      (push (compare-strings "hello" nil 3 "hel" nil nil) results)
      (push (compare-strings "hello" 2 nil "llo" nil nil) results)

      ;; START1 only, END1 nil (rest of string)
      (push (compare-strings "prefix-MATCH" 7 nil "MATCH" nil nil) results)

      ;; START2 only, END2 nil
      (push (compare-strings "MATCH" nil nil "prefix-MATCH" 7 nil) results)

      ;; Both starts offset, both ends nil
      (push (compare-strings "xxHELLO" 2 nil "yyHELLO" 2 nil) results)

      ;; Zero-length substrings (start = end)
      (push (compare-strings "abc" 1 1 "xyz" 2 2) results)

      ;; Asymmetric: one substring longer than the other
      ;; "abcde"[1..4] = "bcd" vs "Xbcde"[1..3] = "bc"
      (push (compare-strings "abcde" 1 4 "Xbcde" 1 3) results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t -1 t t t t t t t 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case-insensitive comparisons (7th parameter)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_case_insensitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Basic case-insensitive equal
      (push (compare-strings "Hello" nil nil "HELLO" nil nil t) results)
      (push (compare-strings "WORLD" nil nil "world" nil nil t) results)

      ;; Mixed case with subranges
      (push (compare-strings "xxAbCyy" 2 5 "zzaBcww" 2 5 t) results)

      ;; Case-sensitive same strings differ
      (push (compare-strings "Hello" nil nil "HELLO" nil nil nil) results)

      ;; Case-insensitive but content differs
      (push (compare-strings "APPLE" nil nil "application" nil nil t) results)

      ;; Case-insensitive prefix comparison
      (push (compare-strings "ABC" nil nil "abcdef" nil nil t) results)

      ;; Non-nil value (not just t) for ignore-case
      (push (compare-strings "FOO" nil nil "foo" nil nil 'yes) results)

      ;; Case-insensitive with numeric chars (should be unaffected)
      (push (compare-strings "abc123" nil nil "ABC123" nil nil t) results)

      ;; Case-insensitive with special chars
      (push (compare-strings "Hello, World!" nil nil "hello, world!" nil nil t) results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t t t 2 -5 -4 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases: empty substrings, single chars, boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
      ;; Both empty strings
      (push (compare-strings "" nil nil "" nil nil) results)

      ;; One empty, one non-empty
      (push (compare-strings "" nil nil "abc" nil nil) results)
      (push (compare-strings "abc" nil nil "" nil nil) results)

      ;; Empty via subrange: start=end
      (push (compare-strings "abc" 0 0 "xyz" 0 0) results)
      (push (compare-strings "abc" 2 2 "" nil nil) results)

      ;; Single character comparisons
      (push (compare-strings "a" nil nil "a" nil nil) results)
      (push (compare-strings "a" nil nil "b" nil nil) results)

      ;; Very long strings - same prefix, differ at end
      (push (compare-strings
             "aaaaaaaaaaaaaaaaaaaax" nil nil
             "aaaaaaaaaaaaaaaaaaaay" nil nil) results)

      ;; Start at end of string
      (push (compare-strings "abc" 3 nil "xyz" 3 nil) results)

      ;; Whitespace comparisons
      (push (compare-strings "  " nil nil "  " nil nil) results)
      (push (compare-strings " a" nil nil "a " nil nil) results)

      (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t -1 1 t t t -1 -21 t t -1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Algorithmic usage: binary search for common prefix length
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_common_prefix_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use compare-strings to find the longest common prefix of two strings
    // via binary search on substring length
    let form = r#"(progn
  (fset 'neovm--test-common-prefix-len
    (lambda (s1 s2)
      "Find length of longest common prefix using compare-strings."
      (let ((lo 0)
            (hi (min (length s1) (length s2))))
        (while (< lo hi)
          (let ((mid (/ (+ lo hi 1) 2)))
            (if (eq t (compare-strings s1 0 mid s2 0 mid))
                (setq lo mid)
              (setq hi (1- mid)))))
        lo)))

  (unwind-protect
      (let ((tests '(("abcdef" "abcxyz")
                     ("hello" "hello world")
                     ("" "anything")
                     ("completely" "different")
                     ("same" "same")
                     ("aab" "aac")
                     ("prefix123" "prefix456"))))
        (mapcar (lambda (pair)
                  (let ((s1 (car pair))
                        (s2 (cadr pair)))
                    (let ((len (funcall 'neovm--test-common-prefix-len s1 s2)))
                      (list s1 s2 len
                            (substring s1 0 len)))))
                tests))
    (fmakunbound 'neovm--test-common-prefix-len)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"abcdef\" \"abcxyz\" 3 \"abc\") (\"hello\" \"hello world\" 5 \"hello\") (\"\" \"anything\" 0 \"\") (\"completely\" \"different\" 0 \"\") (\"same\" \"same\" 4 \"same\") (\"aab\" \"aac\" 2 \"aa\") (\"prefix123\" \"prefix456\" 6 \"prefix\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sorting strings by subrange comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_sort_by_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sort a list of strings by their middle portion using compare-strings
    let form = r#"(progn
  (fset 'neovm--test-substr-less-p
    (lambda (a b start end)
      "Compare substrings of A and B from START to END."
      (let ((r (compare-strings a start end b start end)))
        (and (integerp r) (< r 0)))))

  (unwind-protect
      (let ((words '("xxBxx" "xxAxx" "xxDxx" "xxCxx" "xxExx")))
        ;; Sort by middle character (position 2 to 3)
        (let ((sorted-mid
               (sort (copy-sequence words)
                     (lambda (a b) (funcall 'neovm--test-substr-less-p a b 2 3)))))
          ;; Sort by first two chars
          (let ((words2 '("baXX" "abXX" "aaXX" "bbXX" "azXX")))
            (let ((sorted-prefix
                   (sort (copy-sequence words2)
                         (lambda (a b) (funcall 'neovm--test-substr-less-p a b 0 2)))))
              ;; Case-insensitive sort using compare-strings
              (let ((words3 '("Banana" "apple" "Cherry" "date" "APRICOT")))
                (let ((sorted-ci
                       (sort (copy-sequence words3)
                             (lambda (a b)
                               (let ((r (compare-strings a nil nil b nil nil t)))
                                 (and (integerp r) (< r 0)))))))
                  (list sorted-mid sorted-prefix sorted-ci)))))))
    (fmakunbound 'neovm--test-substr-less-p)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"xxAxx\" \"xxBxx\" \"xxCxx\" \"xxDxx\" \"xxExx\") (\"aaXX\" \"abXX\" \"azXX\" \"baXX\" \"bbXX\") (\"apple\" \"APRICOT\" \"Banana\" \"Cherry\" \"date\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-string pairwise comparison matrix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compare_strings_pairwise_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a comparison matrix between all pairs, then verify properties:
    // reflexivity (diagonal = t), antisymmetry, transitivity
    let form = r#"(let ((strings '("alpha" "beta" "gamma" "alpha" "delta")))
      (let ((n (length strings))
            (matrix nil))
        ;; Build NxN comparison matrix
        (let ((i 0))
          (while (< i n)
            (let ((row nil) (j 0))
              (while (< j n)
                (push (compare-strings (nth i strings) nil nil
                                        (nth j strings) nil nil)
                      row)
                (setq j (1+ j)))
              (push (nreverse row) matrix))
            (setq i (1+ i))))
        (setq matrix (nreverse matrix))
        ;; Check reflexivity: diagonal should be t
        (let ((diagonal nil) (i 0))
          (while (< i n)
            (push (nth i (nth i matrix)) diagonal)
            (setq i (1+ i)))
          ;; Check antisymmetry: if M[i][j] = -k, M[j][i] should be +k (for same pos)
          (let ((antisym-ok t) (i 0))
            (while (< i n)
              (let ((j (1+ i)))
                (while (< j n)
                  (let ((a (nth j (nth i matrix)))
                        (b (nth i (nth j matrix))))
                    (when (and (integerp a) (integerp b))
                      (unless (and (= (abs a) (abs b))
                                   (not (= (signum a) (signum b))))
                        (setq antisym-ok nil))))
                  (setq j (1+ j))))
              (setq i (1+ i)))
            (list :matrix matrix
                  :diagonal (nreverse diagonal)
                  :antisymmetric antisym-ok)))))"#;
    let expect = expect_test::expect![[r#""ERR (void-function signum)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
