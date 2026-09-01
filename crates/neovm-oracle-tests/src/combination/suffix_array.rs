//! Oracle parity tests for suffix array operations implemented in Elisp:
//! building sorted suffix arrays, binary search for patterns, longest
//! common prefix (LCP) computation, pattern occurrence counting,
//! longest repeated substring, and multi-pattern search.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Build sorted array of all suffixes with original indices
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_array_build() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build suffix array: for a string of length N, create all N suffixes,
    // pair them with their starting index, and sort lexicographically.
    let form = r#"(progn
  (fset 'neovm--sa-build
    (lambda (text)
      "Build suffix array for TEXT. Returns sorted list of (index . suffix)."
      (let ((suffixes nil)
            (len (length text))
            (i 0))
        (while (< i len)
          (setq suffixes (cons (cons i (substring text i)) suffixes))
          (setq i (1+ i)))
        ;; Sort by suffix string
        (sort (nreverse suffixes)
              (lambda (a b) (string< (cdr a) (cdr b)))))))

  (unwind-protect
      (list
       ;; Simple string
       (funcall 'neovm--sa-build "banana")
       ;; Single character
       (funcall 'neovm--sa-build "a")
       ;; All same characters
       (funcall 'neovm--sa-build "aaaa")
       ;; Already sorted
       (funcall 'neovm--sa-build "abcd")
       ;; Reverse sorted
       (funcall 'neovm--sa-build "dcba")
       ;; With repetition
       (funcall 'neovm--sa-build "abcabc"))
    (fmakunbound 'neovm--sa-build)))"#;
    let expect = expect_test::expect![[
        r#""OK (((5 . \"a\") (3 . \"ana\") (1 . \"anana\") (0 . \"banana\") (4 . \"na\") (2 . \"nana\")) ((0 . \"a\")) ((3 . \"a\") (2 . \"aa\") (1 . \"aaa\") (0 . \"aaaa\")) ((0 . \"abcd\") (1 . \"bcd\") (2 . \"cd\") (3 . \"d\")) ((3 . \"a\") (2 . \"ba\") (1 . \"cba\") (0 . \"dcba\")) ((3 . \"abc\") (0 . \"abcabc\") (4 . \"bc\") (1 . \"bcabc\") (5 . \"c\") (2 . \"cabc\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Binary search for pattern in suffix array
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_array_binary_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Given a suffix array (as a sorted vector of (index . suffix) pairs),
    // use binary search to find if a pattern exists as a prefix of any suffix.
    let form = r#"(progn
  (fset 'neovm--sa-build-vec
    (lambda (text)
      "Build suffix array as a vector for binary search."
      (let* ((suffixes nil)
             (len (length text))
             (i 0))
        (while (< i len)
          (setq suffixes (cons (cons i (substring text i)) suffixes))
          (setq i (1+ i)))
        (setq suffixes (sort (nreverse suffixes)
                             (lambda (a b) (string< (cdr a) (cdr b)))))
        (vconcat suffixes))))

  (fset 'neovm--sa-prefix-p
    (lambda (pattern suffix)
      "Check if PATTERN is a prefix of SUFFIX."
      (let ((plen (length pattern))
            (slen (length suffix)))
        (and (<= plen slen)
             (string= pattern (substring suffix 0 plen))))))

  (fset 'neovm--sa-compare
    (lambda (pattern suffix)
      "Compare PATTERN with SUFFIX for binary search.
       Returns negative if pattern < suffix prefix, 0 if match, positive if pattern > suffix prefix."
      (let* ((plen (length pattern))
             (slen (length suffix))
             (cmp-len (min plen slen))
             (prefix (substring suffix 0 cmp-len)))
        (cond
         ((string= pattern prefix)
          (if (<= plen slen) 0 1))
         ((string< pattern prefix) -1)
         (t 1)))))

  (fset 'neovm--sa-search
    (lambda (sa pattern)
      "Binary search for PATTERN in suffix array SA.
       Returns (index . suffix) or nil."
      (let ((lo 0)
            (hi (1- (length sa)))
            (result nil))
        (while (and (<= lo hi) (null result))
          (let* ((mid (/ (+ lo hi) 2))
                 (entry (aref sa mid))
                 (suffix (cdr entry))
                 (cmp (funcall 'neovm--sa-compare pattern suffix)))
            (cond
             ((= cmp 0) (setq result entry))
             ((< cmp 0) (setq hi (1- mid)))
             (t (setq lo (1+ mid))))))
        result)))

  (unwind-protect
      (let ((sa (funcall 'neovm--sa-build-vec "mississippi")))
        (list
         ;; Find existing patterns
         (funcall 'neovm--sa-search sa "issi")
         (funcall 'neovm--sa-search sa "miss")
         (funcall 'neovm--sa-search sa "sipp")
         (funcall 'neovm--sa-search sa "pi")
         (funcall 'neovm--sa-search sa "i")
         ;; Pattern not found
         (funcall 'neovm--sa-search sa "xyz")
         (funcall 'neovm--sa-search sa "mist")
         ;; Full string
         (funcall 'neovm--sa-search sa "mississippi")
         ;; Single char at end
         (funcall 'neovm--sa-search sa "p")))
    (fmakunbound 'neovm--sa-build-vec)
    (fmakunbound 'neovm--sa-prefix-p)
    (fmakunbound 'neovm--sa-compare)
    (fmakunbound 'neovm--sa-search)))"#;
    let expect = expect_test::expect![[
        r#""OK ((4 . \"issippi\") (0 . \"mississippi\") (6 . \"sippi\") (9 . \"pi\") (4 . \"issippi\") nil nil (0 . \"mississippi\") (9 . \"pi\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest common prefix (LCP) array computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_array_lcp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute the LCP array from a suffix array. LCP[i] = length of the
    // longest common prefix between suffix[i] and suffix[i-1] in the
    // sorted suffix array.
    let form = r#"(progn
  (fset 'neovm--sa-lcp-pair
    (lambda (s1 s2)
      "Compute length of longest common prefix between S1 and S2."
      (let ((len (min (length s1) (length s2)))
            (i 0))
        (while (and (< i len) (= (aref s1 i) (aref s2 i)))
          (setq i (1+ i)))
        i)))

  (fset 'neovm--sa-build-sorted
    (lambda (text)
      "Build suffix array as sorted list of suffixes."
      (let ((suffixes nil)
            (len (length text))
            (i 0))
        (while (< i len)
          (setq suffixes (cons (substring text i) suffixes))
          (setq i (1+ i)))
        (sort (nreverse suffixes) #'string<))))

  (fset 'neovm--sa-compute-lcp
    (lambda (sorted-suffixes)
      "Compute LCP array from sorted suffixes.
       Returns list of LCP values (length = n-1)."
      (let ((lcps nil)
            (remaining sorted-suffixes))
        (when (cdr remaining)
          (while (cdr remaining)
            (setq lcps (cons (funcall 'neovm--sa-lcp-pair
                                      (car remaining)
                                      (cadr remaining))
                             lcps))
            (setq remaining (cdr remaining))))
        (nreverse lcps))))

  (unwind-protect
      (list
       ;; "banana": suffixes sorted: a, ana, anana, banana, na, nana
       ;; LCPs: (a,ana)=1, (ana,anana)=3, (anana,banana)=0, (banana,na)=0, (na,nana)=2
       (let ((sorted (funcall 'neovm--sa-build-sorted "banana")))
         (list 'suffixes sorted
               'lcp (funcall 'neovm--sa-compute-lcp sorted)))

       ;; "abcabc": repeated pattern
       (let ((sorted (funcall 'neovm--sa-build-sorted "abcabc")))
         (list 'suffixes sorted
               'lcp (funcall 'neovm--sa-compute-lcp sorted)))

       ;; "aaaa": all same
       (let ((sorted (funcall 'neovm--sa-build-sorted "aaaa")))
         (list 'suffixes sorted
               'lcp (funcall 'neovm--sa-compute-lcp sorted)))

       ;; "abcd": no common prefixes
       (let ((sorted (funcall 'neovm--sa-build-sorted "abcd")))
         (list 'suffixes sorted
               'lcp (funcall 'neovm--sa-compute-lcp sorted))))
    (fmakunbound 'neovm--sa-lcp-pair)
    (fmakunbound 'neovm--sa-build-sorted)
    (fmakunbound 'neovm--sa-compute-lcp)))"#;
    let expect = expect_test::expect![[
        r#""OK ((suffixes (\"a\" \"ana\" \"anana\" \"banana\" \"na\" \"nana\") lcp (1 3 0 0 2)) (suffixes (\"abc\" \"abcabc\" \"bc\" \"bcabc\" \"c\" \"cabc\") lcp (3 0 2 0 1)) (suffixes (\"a\" \"aa\" \"aaa\" \"aaaa\") lcp (1 2 3)) (suffixes (\"abcd\" \"bcd\" \"cd\" \"d\") lcp (0 0 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Count occurrences of a pattern using suffix array
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_array_count_occurrences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count how many times a pattern appears as a substring by
    // finding all suffixes that start with the pattern.
    let form = r#"(progn
  (fset 'neovm--sa-count-build
    (lambda (text)
      (let ((suffixes nil)
            (len (length text))
            (i 0))
        (while (< i len)
          (setq suffixes (cons (cons i (substring text i)) suffixes))
          (setq i (1+ i)))
        (sort (nreverse suffixes)
              (lambda (a b) (string< (cdr a) (cdr b)))))))

  (fset 'neovm--sa-count-pattern
    (lambda (sa pattern)
      "Count occurrences of PATTERN in the text using suffix array SA.
       Returns (count positions)."
      (let ((count 0)
            (positions nil)
            (plen (length pattern))
            (remaining sa))
        (while remaining
          (let* ((entry (car remaining))
                 (suffix (cdr entry))
                 (idx (car entry)))
            (when (and (>= (length suffix) plen)
                       (string= pattern (substring suffix 0 plen)))
              (setq count (1+ count))
              (setq positions (cons idx positions))))
          (setq remaining (cdr remaining)))
        (list count (sort positions #'<)))))

  (unwind-protect
      (let ((sa (funcall 'neovm--sa-count-build "abracadabra")))
        (list
         ;; "abra" appears twice
         (funcall 'neovm--sa-count-pattern sa "abra")
         ;; "a" appears 5 times
         (funcall 'neovm--sa-count-pattern sa "a")
         ;; "bra" appears twice
         (funcall 'neovm--sa-count-pattern sa "bra")
         ;; "c" appears once
         (funcall 'neovm--sa-count-pattern sa "c")
         ;; "z" appears zero times
         (funcall 'neovm--sa-count-pattern sa "z")
         ;; Full string
         (funcall 'neovm--sa-count-pattern sa "abracadabra")
         ;; "abracadabra" appears once
         (car (funcall 'neovm--sa-count-pattern sa "abracadabra"))))
    (fmakunbound 'neovm--sa-count-build)
    (fmakunbound 'neovm--sa-count-pattern)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 (0 7)) (5 (0 3 5 7 10)) (2 (1 8)) (1 (4)) (0 nil) (1 (0)) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Find longest repeated substring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_array_longest_repeated() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The longest repeated substring can be found by building the suffix
    // array, computing the LCP array, and finding the maximum LCP value.
    let form = r#"(progn
  (fset 'neovm--sa-lr-lcp
    (lambda (s1 s2)
      (let ((len (min (length s1) (length s2)))
            (i 0))
        (while (and (< i len) (= (aref s1 i) (aref s2 i)))
          (setq i (1+ i)))
        i)))

  (fset 'neovm--sa-lr-build
    (lambda (text)
      (let ((suffixes nil)
            (len (length text))
            (i 0))
        (while (< i len)
          (setq suffixes (cons (substring text i) suffixes))
          (setq i (1+ i)))
        (sort (nreverse suffixes) #'string<))))

  (fset 'neovm--sa-longest-repeated
    (lambda (text)
      "Find the longest repeated substring in TEXT."
      (let* ((sorted (funcall 'neovm--sa-lr-build text))
             (best-len 0)
             (best-str "")
             (remaining sorted))
        (when (cdr remaining)
          (while (cdr remaining)
            (let ((lcp-len (funcall 'neovm--sa-lr-lcp
                                    (car remaining)
                                    (cadr remaining))))
              (when (> lcp-len best-len)
                (setq best-len lcp-len)
                (setq best-str (substring (car remaining) 0 lcp-len))))
            (setq remaining (cdr remaining))))
        (cons best-len best-str))))

  (unwind-protect
      (list
       ;; "banana" -> "ana" (length 3)
       (funcall 'neovm--sa-longest-repeated "banana")
       ;; "abcabc" -> "abc" (length 3)
       (funcall 'neovm--sa-longest-repeated "abcabc")
       ;; "aaaa" -> "aaa" (length 3)
       (funcall 'neovm--sa-longest-repeated "aaaa")
       ;; "abcd" -> "" (no repeat)
       (funcall 'neovm--sa-longest-repeated "abcd")
       ;; "mississippi" -> "issi" (length 4)
       (funcall 'neovm--sa-longest-repeated "mississippi")
       ;; "abcdefabc" -> "abc" (length 3)
       (funcall 'neovm--sa-longest-repeated "abcdefabc")
       ;; Single char -> "" (no repeat possible)
       (funcall 'neovm--sa-longest-repeated "x"))
    (fmakunbound 'neovm--sa-lr-lcp)
    (fmakunbound 'neovm--sa-lr-build)
    (fmakunbound 'neovm--sa-longest-repeated)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 . \"ana\") (3 . \"abc\") (3 . \"aaa\") (0 . \"\") (4 . \"issi\") (3 . \"abc\") (0 . \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Find all occurrences of multiple patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_array_multi_pattern_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Given a text and multiple patterns, use the suffix array to
    // find all occurrences of each pattern efficiently.
    let form = r#"(progn
  (fset 'neovm--sa-mp-build
    (lambda (text)
      (let ((suffixes nil)
            (len (length text))
            (i 0))
        (while (< i len)
          (setq suffixes (cons (cons i (substring text i)) suffixes))
          (setq i (1+ i)))
        (sort (nreverse suffixes)
              (lambda (a b) (string< (cdr a) (cdr b)))))))

  (fset 'neovm--sa-mp-find-all
    (lambda (sa pattern)
      "Find all positions where PATTERN occurs."
      (let ((positions nil)
            (plen (length pattern))
            (remaining sa))
        (while remaining
          (let* ((entry (car remaining))
                 (suffix (cdr entry)))
            (when (and (>= (length suffix) plen)
                       (string= pattern (substring suffix 0 plen)))
              (setq positions (cons (car entry) positions))))
          (setq remaining (cdr remaining)))
        (sort positions #'<))))

  (fset 'neovm--sa-mp-search-all
    (lambda (text patterns)
      "Search for all PATTERNS in TEXT. Returns alist of (pattern . positions)."
      (let ((sa (funcall 'neovm--sa-mp-build text))
            (results nil))
        (dolist (pat patterns)
          (let ((positions (funcall 'neovm--sa-mp-find-all sa pat)))
            (setq results (cons (cons pat positions) results))))
        (nreverse results))))

  (unwind-protect
      (list
       ;; Multiple patterns in "the quick brown fox jumps over the lazy dog"
       (funcall 'neovm--sa-mp-search-all
                "the quick brown fox jumps over the lazy dog"
                '("the" "o" "quick" "xyz" "he" " "))

       ;; Overlapping patterns in "aaabaaab"
       (funcall 'neovm--sa-mp-search-all
                "aaabaaab"
                '("a" "aa" "aab" "b" "aaab"))

       ;; Single character text
       (funcall 'neovm--sa-mp-search-all "x" '("x" "y" "xx"))

       ;; Repeated text
       (funcall 'neovm--sa-mp-search-all
                "abababab"
                '("ab" "ba" "aba" "bab")))
    (fmakunbound 'neovm--sa-mp-build)
    (fmakunbound 'neovm--sa-mp-find-all)
    (fmakunbound 'neovm--sa-mp-search-all)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"the\" 0 31) (\"o\" 12 17 26 41) (\"quick\" 4) (\"xyz\") (\"he\" 1 32) (\" \" 3 9 15 19 25 30 34 39)) ((\"a\" 0 1 2 4 5 6) (\"aa\" 0 1 4 5) (\"aab\" 1 5) (\"b\" 3 7) (\"aaab\" 0 4)) ((\"x\" 0) (\"y\") (\"xx\")) ((\"ab\" 0 2 4 6) (\"ba\" 1 3 5) (\"aba\" 0 2 4) (\"bab\" 1 3 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
