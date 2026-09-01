//! Oracle parity tests for advanced string algorithms: KMP pattern matching,
//! Boyer-Moore-like search, Rabin-Karp rolling hash search, string rotation
//! check, longest palindromic substring, and string interleaving check.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// KMP (Knuth-Morris-Pratt) pattern matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_algo_kmp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Build KMP failure function (partial match table)
  (fset 'neovm--test-kmp-failure
    (lambda (pattern)
      (let* ((m (length pattern))
             (fail (make-vector m 0))
             (k 0)
             (i 1))
        (when (> m 0) (aset fail 0 0))
        (while (< i m)
          (while (and (> k 0)
                      (/= (aref pattern i) (aref pattern k)))
            (setq k (aref fail (1- k))))
          (when (= (aref pattern i) (aref pattern k))
            (setq k (1+ k)))
          (aset fail i k)
          (setq i (1+ i)))
        fail)))

  ;; KMP search: returns list of all match start positions
  (fset 'neovm--test-kmp-search
    (lambda (text pattern)
      (let* ((n (length text))
             (m (length pattern))
             (fail (funcall 'neovm--test-kmp-failure pattern))
             (matches nil)
             (q 0)
             (i 0))
        (if (= m 0)
            nil
          (while (< i n)
            (while (and (> q 0)
                        (/= (aref pattern q) (aref text i)))
              (setq q (aref fail (1- q))))
            (when (= (aref pattern q) (aref text i))
              (setq q (1+ q)))
            (when (= q m)
              (setq matches (cons (- i m -1) matches))
              (setq q (aref fail (1- q))))
            (setq i (1+ i)))
          (nreverse matches)))))

  (unwind-protect
      (let* (;; Test failure function
             (f1 (append (funcall 'neovm--test-kmp-failure "ABCABD") nil))
             (f2 (append (funcall 'neovm--test-kmp-failure "AABAAC") nil))
             (f3 (append (funcall 'neovm--test-kmp-failure "AAAAA") nil))
             ;; Test search
             (s1 (funcall 'neovm--test-kmp-search "ABCABCABDABCABD" "ABCABD"))
             (s2 (funcall 'neovm--test-kmp-search "AAAAAA" "AA"))
             (s3 (funcall 'neovm--test-kmp-search "ABABABABAB" "ABAB"))
             (s4 (funcall 'neovm--test-kmp-search "HELLO WORLD" "XYZ"))
             (s5 (funcall 'neovm--test-kmp-search "" "ABC"))
             (s6 (funcall 'neovm--test-kmp-search "ABCDEF" "DEF"))
             (s7 (funcall 'neovm--test-kmp-search "THE CAT SAT ON THE MAT" "THE")))
        (list f1 f2 f3 s1 s2 s3 s4 s5 s6 s7))
    (fmakunbound 'neovm--test-kmp-failure)
    (fmakunbound 'neovm--test-kmp-search)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 0 0 1 2 0) (0 1 0 1 2 0) (0 1 2 3 4) (3 9) (0 1 2 3 4) (0 2 4 6) nil nil (3) (0 15))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Boyer-Moore-like search (bad character heuristic only)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_algo_boyer_moore_like() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Build bad-character table: for each char in pattern, store its last position.
  ;; Use a char-table for O(1) lookup by character code.
  (fset 'neovm--test-bm-bad-char-table
    (lambda (pattern)
      (let ((tbl (make-char-table 'generic -1))
            (m (length pattern))
            (i 0))
        (while (< i m)
          (set-char-table-range tbl (aref pattern i) i)
          (setq i (1+ i)))
        tbl)))

  ;; Boyer-Moore-like search using bad character heuristic.
  ;; Returns list of all match start positions.
  (fset 'neovm--test-bm-search
    (lambda (text pattern)
      (let* ((n (length text))
             (m (length pattern))
             (bad-char (funcall 'neovm--test-bm-bad-char-table pattern))
             (matches nil)
             (s 0))
        (if (= m 0)
            nil
          (while (<= s (- n m))
            (let ((j (1- m))
                  (matched t))
              ;; Compare pattern from right to left
              (while (and (>= j 0) matched)
                (if (= (aref pattern j) (aref text (+ s j)))
                    (setq j (1- j))
                  (setq matched nil)))
              (if matched
                  (progn
                    (setq matches (cons s matches))
                    ;; Shift by 1 after a match (simplified)
                    (setq s (1+ s)))
                ;; Shift using bad character heuristic
                (let* ((bad-pos (aref bad-char (aref text (+ s (1- m)))))
                       (shift (max 1 (- (1- m) bad-pos))))
                  (setq s (+ s shift))))))
          (nreverse matches)))))

  (unwind-protect
      (let* ((r1 (funcall 'neovm--test-bm-search "ABAAABCDABC" "ABC"))
             (r2 (funcall 'neovm--test-bm-search "AAAAAA" "AAA"))
             (r3 (funcall 'neovm--test-bm-search "HELLO WORLD HELLO" "HELLO"))
             (r4 (funcall 'neovm--test-bm-search "ABCDEF" "XYZ"))
             (r5 (funcall 'neovm--test-bm-search "ABABCABABCABABC" "ABABC"))
             ;; Verify against built-in string-search
             (verify (let ((tests '(("banana" "ana")
                                    ("mississippi" "issi")
                                    ("abcabcabc" "abc"))))
                       (mapcar (lambda (pair)
                                 (let* ((text (car pair))
                                        (pat (cadr pair))
                                        (bm-hits (funcall 'neovm--test-bm-search text pat))
                                        ;; First match from BM should agree with string-search
                                        (builtin (string-search pat text)))
                                   (list pat text (car bm-hits) builtin
                                         (= (or (car bm-hits) -1) (or builtin -1)))))
                               tests))))
        (list r1 r2 r3 r4 r5 verify))
    (fmakunbound 'neovm--test-bm-bad-char-table)
    (fmakunbound 'neovm--test-bm-search)))"#;
    let expect = expect_test::expect![[
        r#""OK ((4 8) (0 1 2 3) (0 12) nil (0 5 10) ((\"ana\" \"banana\" 1 1 t) (\"issi\" \"mississippi\" 1 1 t) (\"abc\" \"abcabcabc\" 0 0 t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rabin-Karp rolling hash string search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_algo_rabin_karp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Rabin-Karp: rolling hash with base 256 and prime modulus.
  ;; Uses modular arithmetic to avoid overflow on large strings.
  (defvar neovm--test-rk-base 256)
  (defvar neovm--test-rk-prime 101)

  (fset 'neovm--test-rk-hash
    (lambda (str start len)
      "Compute hash of substring str[start..start+len-1]."
      (let ((h 0) (i 0))
        (while (< i len)
          (setq h (% (+ (* h neovm--test-rk-base) (aref str (+ start i)))
                     neovm--test-rk-prime))
          (setq i (1+ i)))
        h)))

  (fset 'neovm--test-rk-pow-mod
    (lambda (base exp modulus)
      "Compute (base^exp) mod modulus."
      (let ((result 1) (b (% base modulus)) (e exp))
        (while (> e 0)
          (when (= (% e 2) 1)
            (setq result (% (* result b) modulus)))
          (setq e (/ e 2))
          (setq b (% (* b b) modulus)))
        result)))

  (fset 'neovm--test-rk-search
    (lambda (text pattern)
      "Return list of all positions where pattern occurs in text."
      (let* ((n (length text))
             (m (length pattern))
             (matches nil))
        (if (or (= m 0) (> m n))
            nil
          (let* ((pat-hash (funcall 'neovm--test-rk-hash pattern 0 m))
                 (txt-hash (funcall 'neovm--test-rk-hash text 0 m))
                 ;; h = base^(m-1) mod prime, for removing leading digit
                 (h (funcall 'neovm--test-rk-pow-mod
                             neovm--test-rk-base (1- m) neovm--test-rk-prime))
                 (i 0))
            ;; Check first window
            (when (= pat-hash txt-hash)
              (when (string= (substring text 0 m) pattern)
                (setq matches (cons 0 matches))))
            ;; Slide window
            (setq i 1)
            (while (<= i (- n m))
              ;; Remove leading char, add trailing char
              (setq txt-hash
                    (% (+ (* (- txt-hash (* (aref text (1- i)) h))
                             neovm--test-rk-base)
                          (aref text (+ i m -1)))
                       neovm--test-rk-prime))
              ;; Handle negative modulo
              (when (< txt-hash 0)
                (setq txt-hash (+ txt-hash neovm--test-rk-prime)))
              ;; Hash match => verify
              (when (= pat-hash txt-hash)
                (when (string= (substring text i (+ i m)) pattern)
                  (setq matches (cons i matches))))
              (setq i (1+ i)))
            (nreverse matches))))))

  (unwind-protect
      (let* ((r1 (funcall 'neovm--test-rk-search "AABAACAADAABAABA" "AABA"))
             (r2 (funcall 'neovm--test-rk-search "ABCDEFG" "CDE"))
             (r3 (funcall 'neovm--test-rk-search "AAAAAAA" "AAA"))
             (r4 (funcall 'neovm--test-rk-search "HELLO" "WORLD"))
             (r5 (funcall 'neovm--test-rk-search "ABCABCABC" "ABC"))
             ;; Cross-check with KMP-equivalent: first match should agree with string-search
             (checks (mapcar (lambda (pair)
                               (let* ((text (car pair))
                                      (pat (cadr pair))
                                      (rk (funcall 'neovm--test-rk-search text pat))
                                      (ss (string-search pat text)))
                                 (list (car rk) ss (= (or (car rk) -1) (or ss -1)))))
                             '(("the quick brown fox" "quick")
                               ("abababab" "bab")
                               ("xyz" "xyz")
                               ("a" "b")))))
        (list r1 r2 r3 r4 r5 checks))
    (fmakunbound 'neovm--test-rk-hash)
    (fmakunbound 'neovm--test-rk-pow-mod)
    (fmakunbound 'neovm--test-rk-search)
    (makunbound 'neovm--test-rk-base)
    (makunbound 'neovm--test-rk-prime)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 9 12) (2) (0 1 2 3 4) nil (0 3 6) ((4 4 t) (1 1 t) (0 0 t) (nil nil t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String rotation check: is B a rotation of A?
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_algo_rotation_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Method 1: concatenation — B is rotation of A iff B appears in A+A
  (fset 'neovm--test-rot-check-concat
    (lambda (a b)
      (and (= (length a) (length b))
           (> (length a) 0)
           (not (null (string-search b (concat a a)))))))

  ;; Method 2: find rotation offset — try each offset, check equality
  (fset 'neovm--test-rot-check-brute
    (lambda (a b)
      (if (or (/= (length a) (length b)) (= (length a) 0))
          nil
        (let ((n (length a))
              (found nil)
              (offset 0))
          (while (and (< offset n) (not found))
            (let ((match t) (i 0))
              (while (and (< i n) match)
                (unless (= (aref a (% (+ i offset) n)) (aref b i))
                  (setq match nil))
                (setq i (1+ i)))
              (when match (setq found offset)))
            (setq offset (1+ offset)))
          found))))

  ;; Method 3: find the rotation amount
  (fset 'neovm--test-rot-amount
    (lambda (a b)
      "Return rotation amount k such that rotating A left by k gives B, or nil."
      (funcall 'neovm--test-rot-check-brute a b)))

  (unwind-protect
      (let* ((pairs '(("abcde" "cdeab")      ; rotation by 2
                       ("abcde" "abcde")      ; rotation by 0
                       ("abcde" "eabcd")      ; rotation by 4
                       ("abc" "cab")           ; rotation by 2
                       ("abc" "bac")           ; not a rotation
                       ("a" "a")              ; trivial rotation
                       ("ab" "ba")            ; rotation by 1
                       ("hello" "llohe")      ; rotation by 2
                       ("hello" "world")      ; different chars
                       ("" "")               ; empty — not rotation (by convention)
                       ("abc" "abcd")))       ; different lengths
             (results
              (mapcar (lambda (pair)
                        (let* ((a (car pair))
                               (b (cadr pair))
                               (c1 (funcall 'neovm--test-rot-check-concat a b))
                               (c2 (funcall 'neovm--test-rot-check-brute a b))
                               ;; Both methods should agree on yes/no
                               (agree (eq (not (null c1)) (not (null c2)))))
                          (list a b (and c1 t) c2 agree)))
                      pairs)))
        results)
    (fmakunbound 'neovm--test-rot-check-concat)
    (fmakunbound 'neovm--test-rot-check-brute)
    (fmakunbound 'neovm--test-rot-amount)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"abcde\" \"cdeab\" t 2 t) (\"abcde\" \"abcde\" t 0 t) (\"abcde\" \"eabcd\" t 4 t) (\"abc\" \"cab\" t 2 t) (\"abc\" \"bac\" nil nil t) (\"a\" \"a\" t 0 t) (\"ab\" \"ba\" t 1 t) (\"hello\" \"llohe\" t 2 t) (\"hello\" \"world\" nil nil t) (\"\" \"\" nil nil t) (\"abc\" \"abcd\" nil nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest palindromic substring (expand around center)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_algo_longest_palindrome() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Expand around center to find palindrome length
  (fset 'neovm--test-pal-expand
    (lambda (s left right)
      "Expand around center [left,right] and return (start . length) of palindrome."
      (let ((n (length s))
            (l left)
            (r right))
        (while (and (>= l 0) (< r n)
                    (= (aref s l) (aref s r)))
          (setq l (1- l))
          (setq r (1+ r)))
        ;; l+1..r-1 is the palindrome
        (cons (1+ l) (- r l 1)))))

  ;; Find longest palindromic substring
  (fset 'neovm--test-pal-longest
    (lambda (s)
      (let ((n (length s))
            (best-start 0)
            (best-len 1)
            (i 0))
        (if (= n 0)
            ""
          (while (< i n)
            ;; Odd-length palindromes (center at i)
            (let* ((odd (funcall 'neovm--test-pal-expand s i i))
                   (odd-start (car odd))
                   (odd-len (cdr odd)))
              (when (> odd-len best-len)
                (setq best-start odd-start)
                (setq best-len odd-len)))
            ;; Even-length palindromes (center between i and i+1)
            (when (< (1+ i) n)
              (let* ((even (funcall 'neovm--test-pal-expand s i (1+ i)))
                     (even-start (car even))
                     (even-len (cdr even)))
                (when (> even-len best-len)
                  (setq best-start even-start)
                  (setq best-len even-len))))
            (setq i (1+ i)))
          (substring s best-start (+ best-start best-len))))))

  ;; Also check if a string is a palindrome
  (fset 'neovm--test-pal-is-palindrome
    (lambda (s)
      (let ((n (length s))
            (result t)
            (i 0))
        (while (and (< i (/ n 2)) result)
          (unless (= (aref s i) (aref s (- n 1 i)))
            (setq result nil))
          (setq i (1+ i)))
        result)))

  (unwind-protect
      (let* ((cases '("babad" "cbbd" "racecar" "a" "ac" "aacabdkacaa"
                       "forgeeksskeegfor" "abcba" "abcdef" "aaaa" ""))
             (results
              (mapcar (lambda (s)
                        (let* ((pal (funcall 'neovm--test-pal-longest s))
                               (is-pal (funcall 'neovm--test-pal-is-palindrome pal))
                               (len (length pal)))
                          (list s pal len is-pal)))
                      cases))
             ;; Additional: verify the found palindrome IS actually a palindrome
             ;; and IS actually a substring of the original
             (verify
              (mapcar (lambda (r)
                        (let ((orig (nth 0 r))
                              (pal (nth 1 r)))
                          (list (funcall 'neovm--test-pal-is-palindrome pal)
                                (if (> (length pal) 0)
                                    (not (null (string-search pal orig)))
                                  t))))
                      results)))
        (list results verify))
    (fmakunbound 'neovm--test-pal-expand)
    (fmakunbound 'neovm--test-pal-longest)
    (fmakunbound 'neovm--test-pal-is-palindrome)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"babad\" \"bab\" 3 t) (\"cbbd\" \"bb\" 2 t) (\"racecar\" \"racecar\" 7 t) (\"a\" \"a\" 1 t) (\"ac\" \"a\" 1 t) (\"aacabdkacaa\" \"aca\" 3 t) (\"forgeeksskeegfor\" \"geeksskeeg\" 10 t) (\"abcba\" \"abcba\" 5 t) (\"abcdef\" \"a\" 1 t) (\"aaaa\" \"aaaa\" 4 t) (\"\" \"\" 0 t)) ((t t) (t t) (t t) (t t) (t t) (t t) (t t) (t t) (t t) (t t) (t t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String interleaving check: is C an interleaving of A and B?
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_algo_interleaving() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Dynamic programming: is C an interleaving of A and B?
  ;; dp[i][j] = true if C[0..i+j-1] is an interleaving of A[0..i-1] and B[0..j-1]
  ;; We use a 2D vector (vector of vectors) for the DP table.
  (fset 'neovm--test-interleave-check
    (lambda (a b c)
      (let ((la (length a))
            (lb (length b))
            (lc (length c)))
        (if (/= lc (+ la lb))
            nil
          (let ((dp (let ((tbl (make-vector (1+ la) nil))
                          (i 0))
                      (while (<= i la)
                        (aset tbl i (make-vector (1+ lb) nil))
                        (setq i (1+ i)))
                      tbl)))
            ;; Base case
            (aset (aref dp 0) 0 t)
            ;; Fill first row: using only B
            (let ((j 1))
              (while (<= j lb)
                (aset (aref dp 0) j
                      (and (aref (aref dp 0) (1- j))
                           (= (aref b (1- j)) (aref c (1- j)))))
                (setq j (1+ j))))
            ;; Fill first column: using only A
            (let ((i 1))
              (while (<= i la)
                (aset (aref dp i) 0
                      (and (aref (aref dp (1- i)) 0)
                           (= (aref a (1- i)) (aref c (1- i)))))
                (setq i (1+ i))))
            ;; Fill rest of table
            (let ((i 1))
              (while (<= i la)
                (let ((j 1))
                  (while (<= j lb)
                    (let ((from-a (and (aref (aref dp (1- i)) j)
                                       (= (aref a (1- i))
                                          (aref c (+ i j -1)))))
                          (from-b (and (aref (aref dp i) (1- j))
                                       (= (aref b (1- j))
                                          (aref c (+ i j -1))))))
                      (aset (aref dp i) j (or from-a from-b)))
                    (setq j (1+ j))))
                (setq i (1+ i))))
            (and (aref (aref dp la) lb) t))))))

  ;; Also: construct an interleaving of A and B (alternating characters)
  (fset 'neovm--test-interleave-construct
    (lambda (a b)
      "Construct an interleaving by alternating chars from A and B."
      (let ((result nil)
            (ia 0) (ib 0)
            (la (length a)) (lb (length b))
            (toggle t))
        (while (or (< ia la) (< ib lb))
          (cond
            ((and toggle (< ia la))
             (setq result (cons (aref a ia) result))
             (setq ia (1+ ia))
             (setq toggle nil))
            ((< ib lb)
             (setq result (cons (aref b ib) result))
             (setq ib (1+ ib))
             (setq toggle t))
            ((< ia la)
             (setq result (cons (aref a ia) result))
             (setq ia (1+ ia)))
            (t nil)))
        (concat (nreverse result)))))

  (unwind-protect
      (let* (;; Positive cases
             (r1 (funcall 'neovm--test-interleave-check "aab" "axy" "aaxaby"))
             (r2 (funcall 'neovm--test-interleave-check "abc" "def" "adbecf"))
             (r3 (funcall 'neovm--test-interleave-check "" "abc" "abc"))
             (r4 (funcall 'neovm--test-interleave-check "abc" "" "abc"))
             (r5 (funcall 'neovm--test-interleave-check "" "" ""))
             ;; Negative cases
             (r6 (funcall 'neovm--test-interleave-check "abc" "def" "abcdefx"))
             (r7 (funcall 'neovm--test-interleave-check "abc" "def" "abdcfe"))
             (r8 (funcall 'neovm--test-interleave-check "abc" "def" "abcde"))
             ;; Construct and verify round-trip
             (constructed (funcall 'neovm--test-interleave-construct "abc" "xyz"))
             (r9 (funcall 'neovm--test-interleave-check "abc" "xyz" constructed))
             ;; More complex
             (r10 (funcall 'neovm--test-interleave-check "aabcc" "dbbca"
                           "aadbbcbcac")))
        (list r1 r2 r3 r4 r5 r6 r7 r8 constructed r9 r10))
    (fmakunbound 'neovm--test-interleave-check)
    (fmakunbound 'neovm--test-interleave-construct)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t nil nil nil \"axbycz\" t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
