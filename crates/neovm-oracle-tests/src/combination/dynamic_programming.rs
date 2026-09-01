//! Oracle parity tests for dynamic programming algorithm patterns:
//! Fibonacci (top-down and bottom-up), edit distance, LCS with backtracking,
//! 0/1 knapsack, coin change, and Kadane's maximum subarray.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Fibonacci: top-down memoization vs bottom-up tabulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_fibonacci_memoized_and_tabulated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Top-down with memoization via hash table
  (fset 'neovm--test-fib-memo
    (lambda (n memo)
      (or (gethash n memo)
          (let ((val (if (<= n 1)
                         n
                       (+ (funcall 'neovm--test-fib-memo (- n 1) memo)
                          (funcall 'neovm--test-fib-memo (- n 2) memo)))))
            (puthash n val memo)
            val))))

  ;; Bottom-up tabulation
  (fset 'neovm--test-fib-tab
    (lambda (n)
      (if (<= n 1) n
        (let ((table (make-vector (1+ n) 0)))
          (aset table 0 0)
          (aset table 1 1)
          (let ((i 2))
            (while (<= i n)
              (aset table i (+ (aref table (- i 1)) (aref table (- i 2))))
              (setq i (1+ i))))
          (aref table n)))))

  (unwind-protect
      (let ((memo (make-hash-table :test 'eql))
            (test-values '(0 1 2 5 10 15 20 25 30)))
        ;; Compute both ways and verify they match
        (let ((results
               (mapcar (lambda (n)
                         (let ((top-down (funcall 'neovm--test-fib-memo n memo))
                               (bottom-up (funcall 'neovm--test-fib-tab n)))
                           (list n top-down bottom-up (= top-down bottom-up))))
                       test-values)))
          (list results
                ;; Verify all matched
                (let ((all-match t))
                  (dolist (r results)
                    (unless (nth 3 r) (setq all-match nil)))
                  all-match)
                ;; Verify memoization table was populated
                (hash-table-count memo)
                ;; Verify known values
                (funcall 'neovm--test-fib-tab 10)   ;; 55
                (funcall 'neovm--test-fib-tab 20)))) ;; 6765
    (fmakunbound 'neovm--test-fib-memo)
    (fmakunbound 'neovm--test-fib-tab)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 0 0 t) (1 1 1 t) (2 1 1 t) (5 5 5 t) (10 55 55 t) (15 610 610 t) (20 6765 6765 t) (25 75025 75025 t) (30 832040 832040 t)) t 31 55 6765)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edit distance (Levenshtein) via DP table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_edit_distance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-edit-distance
    (lambda (s1 s2)
      "Compute Levenshtein edit distance between strings S1 and S2 using DP table."
      (let* ((m (length s1))
             (n (length s2))
             ;; Create (m+1) x (n+1) table as vector of vectors
             (dp (make-vector (1+ m) nil)))
        ;; Initialize rows
        (dotimes (i (1+ m))
          (aset dp i (make-vector (1+ n) 0)))
        ;; Base cases: dp[i][0] = i, dp[0][j] = j
        (dotimes (i (1+ m))
          (aset (aref dp i) 0 i))
        (dotimes (j (1+ n))
          (aset (aref dp 0) j j))
        ;; Fill table
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (let ((cost (if (= (aref s1 (1- i)) (aref s2 (1- j))) 0 1)))
                  (aset (aref dp i) j
                        (min (1+ (aref (aref dp (1- i)) j))        ;; deletion
                             (min (1+ (aref (aref dp i) (1- j)))    ;; insertion
                                  (+ (aref (aref dp (1- i)) (1- j)) ;; substitution
                                     cost)))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        ;; Return distance and the last row for debugging
        (list (aref (aref dp m) n)
              (let ((row nil) (j n))
                (while (>= j 0)
                  (setq row (cons (aref (aref dp m) j) row)
                        j (1- j)))
                row)))))

  (unwind-protect
      (let ((test-cases '(("kitten" "sitting")
                           ("" "abc")
                           ("abc" "")
                           ("abc" "abc")
                           ("saturday" "sunday")
                           ("intention" "execution")
                           ("abcdef" "azced"))))
        (mapcar (lambda (tc)
                  (let ((s1 (car tc)) (s2 (cadr tc)))
                    (list s1 s2 (car (funcall 'neovm--test-edit-distance s1 s2)))))
                test-cases))
    (fmakunbound 'neovm--test-edit-distance)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"kitten\" \"sitting\" 3) (\"\" \"abc\" 3) (\"abc\" \"\" 3) (\"abc\" \"abc\" 0) (\"saturday\" \"sunday\" 3) (\"intention\" \"execution\" 5) (\"abcdef\" \"azced\" 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest Common Subsequence (LCS) with backtracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_lcs_with_backtrack() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-lcs
    (lambda (s1 s2)
      "Compute LCS length and the actual subsequence string via backtracking."
      (let* ((m (length s1))
             (n (length s2))
             (dp (make-vector (1+ m) nil)))
        ;; Initialize DP table
        (dotimes (i (1+ m))
          (aset dp i (make-vector (1+ n) 0)))
        ;; Fill DP table
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (if (= (aref s1 (1- i)) (aref s2 (1- j)))
                    (aset (aref dp i) j
                          (1+ (aref (aref dp (1- i)) (1- j))))
                  (aset (aref dp i) j
                        (max (aref (aref dp (1- i)) j)
                             (aref (aref dp i) (1- j)))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        ;; Backtrack to find the actual LCS string
        (let ((i m) (j n) (result nil))
          (while (and (> i 0) (> j 0))
            (cond
             ((= (aref s1 (1- i)) (aref s2 (1- j)))
              (setq result (cons (aref s1 (1- i)) result)
                    i (1- i)
                    j (1- j)))
             ((> (aref (aref dp (1- i)) j)
                 (aref (aref dp i) (1- j)))
              (setq i (1- i)))
             (t (setq j (1- j)))))
          (list (aref (aref dp m) n)
                (concat result))))))

  (unwind-protect
      (let ((test-cases '(("ABCBDAB" "BDCAB")
                           ("AGGTAB" "GXTXAYB")
                           ("" "anything")
                           ("abc" "def")
                           ("abcde" "ace")
                           ("ABCDGH" "AEDFHR"))))
        (mapcar (lambda (tc)
                  (let ((s1 (car tc)) (s2 (cadr tc)))
                    (let ((result (funcall 'neovm--test-lcs s1 s2)))
                      (list s1 s2 (car result) (cadr result)))))
                test-cases))
    (fmakunbound 'neovm--test-lcs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"ABCBDAB\" \"BDCAB\" 4 \"BDAB\") (\"AGGTAB\" \"GXTXAYB\" 4 \"GTAB\") (\"\" \"anything\" 0 \"\") (\"abc\" \"def\" 0 \"\") (\"abcde\" \"ace\" 3 \"ace\") (\"ABCDGH\" \"AEDFHR\" 3 \"ADH\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 0/1 Knapsack problem
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_knapsack_01() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-knapsack
    (lambda (capacity weights values)
      "Solve 0/1 knapsack: returns (max-value . selected-items).
       WEIGHTS and VALUES are vectors, CAPACITY is integer."
      (let* ((n (length weights))
             ;; dp[i][w] = max value using first i items with capacity w
             (dp (make-vector (1+ n) nil)))
        ;; Initialize
        (dotimes (i (1+ n))
          (aset dp i (make-vector (1+ capacity) 0)))
        ;; Fill table
        (let ((i 1))
          (while (<= i n)
            (let ((w 0))
              (while (<= w capacity)
                (let ((wi (aref weights (1- i)))
                      (vi (aref values (1- i))))
                  (if (> wi w)
                      ;; Item too heavy, skip it
                      (aset (aref dp i) w (aref (aref dp (1- i)) w))
                    ;; Max of skip or take
                    (aset (aref dp i) w
                          (max (aref (aref dp (1- i)) w)
                               (+ vi (aref (aref dp (1- i)) (- w wi)))))))
                (setq w (1+ w))))
            (setq i (1+ i))))
        ;; Backtrack to find selected items
        (let ((selected nil)
              (w capacity)
              (i n))
          (while (> i 0)
            (when (not (= (aref (aref dp i) w)
                          (aref (aref dp (1- i)) w)))
              (setq selected (cons (1- i) selected)  ;; 0-indexed item
                    w (- w (aref weights (1- i)))))
            (setq i (1- i)))
          (list (aref (aref dp n) capacity)
                selected
                ;; Verify: sum of selected values = max value
                (let ((sum 0))
                  (dolist (idx selected)
                    (setq sum (+ sum (aref values idx))))
                  sum)
                ;; Verify: sum of selected weights <= capacity
                (let ((sum 0))
                  (dolist (idx selected)
                    (setq sum (+ sum (aref weights idx))))
                  (<= sum capacity)))))))

  (unwind-protect
      (list
       ;; Classic example
       (funcall 'neovm--test-knapsack 50
                [10 20 30]
                [60 100 120])
       ;; Larger instance
       (funcall 'neovm--test-knapsack 15
                [1 2 3 5 7 4]
                [1 6 10 15 16 8])
       ;; Edge: zero capacity
       (funcall 'neovm--test-knapsack 0
                [5 10 15]
                [10 20 30])
       ;; Edge: single item fits
       (funcall 'neovm--test-knapsack 10
                [10]
                [42]))
    (fmakunbound 'neovm--test-knapsack)))"#;
    let expect = expect_test::expect![[
        r#""OK ((220 (1 2) 220 t) (41 (2 3 4) 41 t) (0 nil 0 t) (42 (0) 42 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Coin change problem (minimum coins)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_coin_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-coin-change
    (lambda (coins amount)
      "Find minimum number of coins to make AMOUNT, and which coins.
       COINS is a list of denominations. Returns (min-coins . coin-list) or nil if impossible."
      (let* ((inf (1+ amount))  ;; sentinel for impossible
             ;; dp[i] = minimum coins to make amount i
             (dp (make-vector (1+ amount) inf))
             ;; parent[i] = which coin was used to reach amount i
             (parent (make-vector (1+ amount) -1)))
        (aset dp 0 0)
        ;; Fill DP table
        (let ((i 1))
          (while (<= i amount)
            (dolist (coin coins)
              (when (and (<= coin i)
                         (< (1+ (aref dp (- i coin))) (aref dp i)))
                (aset dp i (1+ (aref dp (- i coin))))
                (aset parent i coin)))
            (setq i (1+ i))))
        ;; Check if solution exists
        (if (= (aref dp amount) inf)
            nil  ;; impossible
          ;; Backtrack to find which coins
          (let ((result nil)
                (remaining amount))
            (while (> remaining 0)
              (let ((coin (aref parent remaining)))
                (setq result (cons coin result)
                      remaining (- remaining coin))))
            (list (aref dp amount)
                  (sort result #'<)
                  ;; Verify sum
                  (= amount (apply #'+ result))))))))

  (unwind-protect
      (list
       ;; Standard case
       (funcall 'neovm--test-coin-change '(1 5 10 25) 63)
       ;; Exact coins
       (funcall 'neovm--test-coin-change '(1 5 10 25) 25)
       ;; Impossible case (no 1-cent coin)
       (funcall 'neovm--test-coin-change '(3 7) 5)
       ;; Large denominations
       (funcall 'neovm--test-coin-change '(1 7 10) 14)
       ;; Edge: zero amount
       (funcall 'neovm--test-coin-change '(1 5 10) 0)
       ;; Single denomination
       (funcall 'neovm--test-coin-change '(3) 9))
    (fmakunbound 'neovm--test-coin-change)))"#;
    let expect = expect_test::expect![[
        r#""OK ((6 (1 1 1 10 25 25) t) (1 (25) t) nil (2 (7 7) t) (0 nil t) (3 (3 3 3) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Maximum subarray sum (Kadane's algorithm)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_kadanes_max_subarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-kadane
    (lambda (arr)
      "Find maximum subarray sum, with start and end indices.
       ARR is a vector of integers. Returns (max-sum start-idx end-idx subarray)."
      (let* ((n (length arr))
             (max-sum (aref arr 0))
             (current-sum (aref arr 0))
             (start 0) (end 0) (temp-start 0))
        (let ((i 1))
          (while (< i n)
            (let ((val (aref arr i)))
              ;; Either extend current subarray or start new one
              (if (> (+ current-sum val) val)
                  (setq current-sum (+ current-sum val))
                (setq current-sum val
                      temp-start i))
              ;; Update global max
              (when (> current-sum max-sum)
                (setq max-sum current-sum
                      start temp-start
                      end i)))
            (setq i (1+ i))))
        ;; Extract the actual subarray
        (let ((sub nil) (j end))
          (while (>= j start)
            (setq sub (cons (aref arr j) sub)
                  j (1- j)))
          (list max-sum start end sub
                ;; Verify sum matches
                (= max-sum (apply #'+ sub)))))))

  (unwind-protect
      (list
       ;; Classic example
       (funcall 'neovm--test-kadane [-2 1 -3 4 -1 2 1 -5 4])
       ;; All positive
       (funcall 'neovm--test-kadane [1 2 3 4 5])
       ;; All negative (should pick least negative)
       (funcall 'neovm--test-kadane [-8 -3 -6 -2 -5 -4])
       ;; Single element
       (funcall 'neovm--test-kadane [42])
       ;; Mixed with large valley
       (funcall 'neovm--test-kadane [5 4 -1 7 8 -100 3 4 5 6]))
    (fmakunbound 'neovm--test-kadane)))"#;
    let expect = expect_test::expect![[
        r#""OK ((6 3 6 (4 -1 2 1) t) (15 0 4 (1 2 3 4 5) t) (-2 3 3 (-2) t) (42 0 0 (42) t) (23 0 4 (5 4 -1 7 8) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix chain multiplication (optimal parenthesization)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_matrix_chain_multiplication() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--test-mcm
    (lambda (dims)
      "Compute minimum scalar multiplications for matrix chain.
       DIMS is a vector of dimensions: matrix i has dims[i] x dims[i+1].
       Returns (min-cost parenthesization-string)."
      (let* ((n (1- (length dims)))  ;; number of matrices
             (inf 999999999)
             ;; m[i][j] = min cost of multiplying matrices i..j
             (m (make-vector n nil))
             ;; s[i][j] = split point for optimal parenthesization
             (s (make-vector n nil)))
        ;; Initialize tables
        (dotimes (i n)
          (aset m i (make-vector n 0))
          (aset s i (make-vector n 0)))
        ;; Chain length from 2 to n
        (let ((len 2))
          (while (<= len n)
            (let ((i 0))
              (while (<= i (- n len))
                (let ((j (+ i len -1)))
                  (aset (aref m i) j inf)
                  ;; Try all split points
                  (let ((k i))
                    (while (< k j)
                      (let ((cost (+ (aref (aref m i) k)
                                     (aref (aref m (1+ k)) j)
                                     (* (aref dims i)
                                        (aref dims (1+ k))
                                        (aref dims (1+ j))))))
                        (when (< cost (aref (aref m i) j))
                          (aset (aref m i) j cost)
                          (aset (aref s i) j k)))
                      (setq k (1+ k)))))
                (setq i (1+ i))))
            (setq len (1+ len))))
        ;; Build parenthesization string
        (fset 'neovm--test-mcm-paren
          (lambda (s-table i j)
            (if (= i j)
                (format "M%d" (1+ i))
              (format "(%s x %s)"
                      (funcall 'neovm--test-mcm-paren s-table i (aref (aref s-table i) j))
                      (funcall 'neovm--test-mcm-paren s-table (1+ (aref (aref s-table i) j)) j)))))
        (list (aref (aref m 0) (1- n))
              (funcall 'neovm--test-mcm-paren s 0 (1- n))))))

  (unwind-protect
      (list
       ;; Classic example: 4 matrices
       ;; A1(10x30) A2(30x5) A3(5x60) A4(60x10)
       (funcall 'neovm--test-mcm [10 30 5 60 10])
       ;; 3 matrices: (40x20)(20x30)(30x10)
       (funcall 'neovm--test-mcm [40 20 30 10])
       ;; 2 matrices (trivial)
       (funcall 'neovm--test-mcm [10 20 30]))
    (fmakunbound 'neovm--test-mcm)
    (fmakunbound 'neovm--test-mcm-paren)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5000 \"((M1 x M2) x (M3 x M4))\") (14000 \"(M1 x (M2 x M3))\") (6000 \"(M1 x M2)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
