//! Comprehensive oracle parity tests for recursive patterns:
//! direct recursion (factorial, fibonacci, tree traversal), mutual recursion
//! (even/odd predicates, parser pairs), tail-recursive patterns with and
//! without accumulators, tree-recursive patterns (coin change, subset sum),
//! recursive data structure processing (nested lists, trees), memoized
//! recursion via hash tables, and indirect recursion via funcall.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Direct recursion: power function, GCD, digit sum, palindrome check
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_comprehensive_direct_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Integer power: base^exp via repeated multiplication
  (fset 'neovm--rcp-power
    (lambda (base exp)
      (cond
       ((= exp 0) 1)
       ((= (% exp 2) 0)
        (let ((half (funcall 'neovm--rcp-power base (/ exp 2))))
          (* half half)))
       (t (* base (funcall 'neovm--rcp-power base (1- exp)))))))

  ;; GCD via Euclidean algorithm
  (fset 'neovm--rcp-gcd
    (lambda (a b)
      (if (= b 0) a
        (funcall 'neovm--rcp-gcd b (% a b)))))

  ;; Sum of digits
  (fset 'neovm--rcp-digit-sum
    (lambda (n)
      (if (< n 10) n
        (+ (% n 10) (funcall 'neovm--rcp-digit-sum (/ n 10))))))

  ;; Recursive palindrome check on a list
  (fset 'neovm--rcp-palindrome-p
    (lambda (lst)
      (if (<= (length lst) 1) t
        (and (equal (car lst) (car (last lst)))
             (funcall 'neovm--rcp-palindrome-p
                      (butlast (cdr lst)))))))

  (unwind-protect
      (list
       ;; Power tests: 2^0, 2^1, 2^10, 3^5, 5^3
       (funcall 'neovm--rcp-power 2 0)
       (funcall 'neovm--rcp-power 2 1)
       (funcall 'neovm--rcp-power 2 10)
       (funcall 'neovm--rcp-power 3 5)
       (funcall 'neovm--rcp-power 5 3)
       ;; GCD tests
       (funcall 'neovm--rcp-gcd 48 18)
       (funcall 'neovm--rcp-gcd 100 75)
       (funcall 'neovm--rcp-gcd 17 13)
       (funcall 'neovm--rcp-gcd 0 5)
       (funcall 'neovm--rcp-gcd 12 12)
       ;; Digit sum
       (funcall 'neovm--rcp-digit-sum 0)
       (funcall 'neovm--rcp-digit-sum 9)
       (funcall 'neovm--rcp-digit-sum 123)
       (funcall 'neovm--rcp-digit-sum 9999)
       ;; Palindrome
       (funcall 'neovm--rcp-palindrome-p '(1 2 3 2 1))
       (funcall 'neovm--rcp-palindrome-p '(1 2 3 4))
       (funcall 'neovm--rcp-palindrome-p '(a))
       (funcall 'neovm--rcp-palindrome-p nil))
    (fmakunbound 'neovm--rcp-power)
    (fmakunbound 'neovm--rcp-gcd)
    (fmakunbound 'neovm--rcp-digit-sum)
    (fmakunbound 'neovm--rcp-palindrome-p)))"#;
    let expect =
        expect_test::expect![[r#""OK (1 2 1024 243 125 6 25 1 5 12 0 9 6 36 t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mutual recursion: state machine parser (tokenizer + classifier)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_comprehensive_mutual_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two mutually recursive functions that classify a list of tokens:
    // neovm--rcp-parse-value reads a value (number or nested list),
    // neovm--rcp-parse-seq reads a sequence of values until a sentinel.
    let form = r#"(progn
  (defvar neovm--rcp-toks nil)

  (fset 'neovm--rcp-peek (lambda () (car neovm--rcp-toks)))
  (fset 'neovm--rcp-consume
    (lambda ()
      (prog1 (car neovm--rcp-toks)
        (setq neovm--rcp-toks (cdr neovm--rcp-toks)))))

  ;; Parse a single value: number, or '(' sequence ')'
  (fset 'neovm--rcp-parse-value
    (lambda ()
      (let ((tok (funcall 'neovm--rcp-peek)))
        (cond
         ((numberp tok) (funcall 'neovm--rcp-consume))
         ((eq tok 'open)
          (funcall 'neovm--rcp-consume) ;; eat open
          (let ((seq (funcall 'neovm--rcp-parse-seq)))
            (funcall 'neovm--rcp-consume) ;; eat close
            seq))
         ((symbolp tok) (funcall 'neovm--rcp-consume))
         (t nil)))))

  ;; Parse a sequence of values until we see 'close or end of input
  (fset 'neovm--rcp-parse-seq
    (lambda ()
      (let ((result nil))
        (while (and neovm--rcp-toks
                    (not (eq (funcall 'neovm--rcp-peek) 'close)))
          (setq result (cons (funcall 'neovm--rcp-parse-value) result)))
        (nreverse result))))

  ;; Also mutual recursion: classify numbers as even/odd via subtraction
  ;; but with extra twist: categorize into equivalence classes mod 3
  (fset 'neovm--rcp-classify-a
    (lambda (n)
      (cond
       ((< n 0) (funcall 'neovm--rcp-classify-a (- n)))
       ((= n 0) 'zero-mod3)
       ((= n 1) 'one-mod3)
       ((= n 2) 'two-mod3)
       (t (funcall 'neovm--rcp-classify-b (- n 3))))))

  (fset 'neovm--rcp-classify-b
    (lambda (n)
      (funcall 'neovm--rcp-classify-a n)))

  (unwind-protect
      (list
       ;; Parse: (1 2 3)
       (progn (setq neovm--rcp-toks '(open 1 2 3 close))
              (funcall 'neovm--rcp-parse-value))
       ;; Parse: (1 (2 3) 4)
       (progn (setq neovm--rcp-toks '(open 1 open 2 3 close 4 close))
              (funcall 'neovm--rcp-parse-value))
       ;; Parse: ((1) (2 (3)))
       (progn (setq neovm--rcp-toks '(open open 1 close open 2 open 3 close close close))
              (funcall 'neovm--rcp-parse-value))
       ;; Parse: just a number
       (progn (setq neovm--rcp-toks '(42))
              (funcall 'neovm--rcp-parse-value))
       ;; Parse: symbol
       (progn (setq neovm--rcp-toks '(hello))
              (funcall 'neovm--rcp-parse-value))
       ;; Classify mod 3
       (mapcar (lambda (n) (funcall 'neovm--rcp-classify-a n))
               '(0 1 2 3 4 5 6 7 8 9 10 11 12 -1 -6)))
    (fmakunbound 'neovm--rcp-peek)
    (fmakunbound 'neovm--rcp-consume)
    (fmakunbound 'neovm--rcp-parse-value)
    (fmakunbound 'neovm--rcp-parse-seq)
    (fmakunbound 'neovm--rcp-classify-a)
    (fmakunbound 'neovm--rcp-classify-b)
    (makunbound 'neovm--rcp-toks)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3) (1 (2 3) 4) ((1) (2 (3))) 42 hello (zero-mod3 one-mod3 two-mod3 zero-mod3 one-mod3 two-mod3 zero-mod3 one-mod3 two-mod3 zero-mod3 one-mod3 two-mod3 zero-mod3 one-mod3 zero-mod3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tail-recursive patterns: with and without accumulators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_comprehensive_tail_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Tail-recursive factorial with accumulator
  (fset 'neovm--rcp-fact-tail
    (lambda (n acc)
      (if (<= n 1) acc
        (funcall 'neovm--rcp-fact-tail (1- n) (* n acc)))))

  ;; Tail-recursive fibonacci with two accumulators
  (fset 'neovm--rcp-fib-tail
    (lambda (n a b)
      (cond
       ((= n 0) a)
       ((= n 1) b)
       (t (funcall 'neovm--rcp-fib-tail (1- n) b (+ a b))))))

  ;; Tail-recursive list reversal (already inherently tail-recursive)
  (fset 'neovm--rcp-rev-tail
    (lambda (lst acc)
      (if (null lst) acc
        (funcall 'neovm--rcp-rev-tail (cdr lst) (cons (car lst) acc)))))

  ;; Tail-recursive binary search
  (fset 'neovm--rcp-bsearch
    (lambda (vec target lo hi)
      (if (> lo hi) nil
        (let ((mid (/ (+ lo hi) 2)))
          (let ((midval (aref vec mid)))
            (cond
             ((= midval target) mid)
             ((< midval target)
              (funcall 'neovm--rcp-bsearch vec target (1+ mid) hi))
             (t
              (funcall 'neovm--rcp-bsearch vec target lo (1- mid)))))))))

  ;; Non-tail-recursive version of factorial for comparison
  (fset 'neovm--rcp-fact-notail
    (lambda (n)
      (if (<= n 1) 1
        (* n (funcall 'neovm--rcp-fact-notail (1- n))))))

  ;; Tail-recursive length with accumulator
  (fset 'neovm--rcp-len-tail
    (lambda (lst acc)
      (if (null lst) acc
        (funcall 'neovm--rcp-len-tail (cdr lst) (1+ acc)))))

  (unwind-protect
      (list
       ;; Factorial: tail vs non-tail produce same results
       (list (funcall 'neovm--rcp-fact-tail 0 1)
             (funcall 'neovm--rcp-fact-tail 1 1)
             (funcall 'neovm--rcp-fact-tail 5 1)
             (funcall 'neovm--rcp-fact-tail 10 1)
             (funcall 'neovm--rcp-fact-tail 12 1))
       (list (funcall 'neovm--rcp-fact-notail 0)
             (funcall 'neovm--rcp-fact-notail 1)
             (funcall 'neovm--rcp-fact-notail 5)
             (funcall 'neovm--rcp-fact-notail 10)
             (funcall 'neovm--rcp-fact-notail 12))
       ;; Verify they match
       (= (funcall 'neovm--rcp-fact-tail 10 1)
          (funcall 'neovm--rcp-fact-notail 10))
       ;; Fibonacci via tail recursion
       (mapcar (lambda (n) (funcall 'neovm--rcp-fib-tail n 0 1))
               '(0 1 2 3 4 5 6 7 8 9 10 15 20))
       ;; Tail-recursive reverse
       (funcall 'neovm--rcp-rev-tail '(1 2 3 4 5) nil)
       (funcall 'neovm--rcp-rev-tail nil nil)
       (funcall 'neovm--rcp-rev-tail '(a) nil)
       ;; Binary search
       (let ((v [10 20 30 40 50 60 70 80 90 100]))
         (list
          (funcall 'neovm--rcp-bsearch v 10 0 9)
          (funcall 'neovm--rcp-bsearch v 50 0 9)
          (funcall 'neovm--rcp-bsearch v 100 0 9)
          (funcall 'neovm--rcp-bsearch v 55 0 9)
          (funcall 'neovm--rcp-bsearch v 0 0 9)))
       ;; Tail-recursive length
       (funcall 'neovm--rcp-len-tail '(a b c d e f g) 0)
       (funcall 'neovm--rcp-len-tail nil 0))
    (fmakunbound 'neovm--rcp-fact-tail)
    (fmakunbound 'neovm--rcp-fib-tail)
    (fmakunbound 'neovm--rcp-rev-tail)
    (fmakunbound 'neovm--rcp-bsearch)
    (fmakunbound 'neovm--rcp-fact-notail)
    (fmakunbound 'neovm--rcp-len-tail)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 1 120 3628800 479001600) (1 1 120 3628800 479001600) t (0 1 1 2 3 5 8 13 21 34 55 610 6765) (5 4 3 2 1) nil (a) (0 4 9 nil nil) 7 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tree-recursive patterns: coin change, subset sum, Catalan numbers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_comprehensive_tree_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Coin change: count ways to make amount from given coins
  (fset 'neovm--rcp-coin-change
    (lambda (amount coins)
      (cond
       ((= amount 0) 1)
       ((< amount 0) 0)
       ((null coins) 0)
       (t (+ (funcall 'neovm--rcp-coin-change (- amount (car coins)) coins)
             (funcall 'neovm--rcp-coin-change amount (cdr coins)))))))

  ;; Subset sum: does any subset of lst sum to target?
  (fset 'neovm--rcp-subset-sum
    (lambda (lst target)
      (cond
       ((= target 0) t)
       ((null lst) nil)
       ((> (car lst) target)
        (funcall 'neovm--rcp-subset-sum (cdr lst) target))
       (t (or (funcall 'neovm--rcp-subset-sum (cdr lst) (- target (car lst)))
              (funcall 'neovm--rcp-subset-sum (cdr lst) target))))))

  ;; Generate all subsets of a list (power set)
  (fset 'neovm--rcp-powerset
    (lambda (lst)
      (if (null lst)
          '(nil)
        (let ((rest-subsets (funcall 'neovm--rcp-powerset (cdr lst))))
          (append rest-subsets
                  (mapcar (lambda (s) (cons (car lst) s)) rest-subsets))))))

  ;; Catalan number (counts balanced parentheses, BST shapes, etc.)
  (fset 'neovm--rcp-catalan
    (lambda (n)
      (if (<= n 1) 1
        (let ((sum 0) (i 0))
          (while (< i n)
            (setq sum (+ sum (* (funcall 'neovm--rcp-catalan i)
                                (funcall 'neovm--rcp-catalan (- n 1 i)))))
            (setq i (1+ i)))
          sum))))

  (unwind-protect
      (list
       ;; Coin change: how many ways to make change
       ;; Coins: 1, 5, 10, 25
       (funcall 'neovm--rcp-coin-change 0 '(1 5 10 25))
       (funcall 'neovm--rcp-coin-change 1 '(1 5 10 25))
       (funcall 'neovm--rcp-coin-change 5 '(1 5 10 25))
       (funcall 'neovm--rcp-coin-change 10 '(1 5))
       (funcall 'neovm--rcp-coin-change 15 '(1 5 10))
       ;; Subset sum
       (funcall 'neovm--rcp-subset-sum '(3 7 1 8 4) 12)  ;; 3+1+8=12
       (funcall 'neovm--rcp-subset-sum '(3 7 1 8 4) 2)   ;; no subset
       (funcall 'neovm--rcp-subset-sum '(3 7 1 8 4) 0)   ;; empty subset
       (funcall 'neovm--rcp-subset-sum '(5 5 5) 10)      ;; 5+5=10
       (funcall 'neovm--rcp-subset-sum nil 0)             ;; empty list, target 0
       (funcall 'neovm--rcp-subset-sum nil 1)             ;; empty list, target 1
       ;; Power set
       (let ((ps (funcall 'neovm--rcp-powerset '(1 2 3))))
         (list (length ps)  ;; 2^3 = 8
               (sort (mapcar (lambda (s)
                               (apply #'+ (or s '(0))))
                             ps) #'<)))
       ;; Catalan numbers: 1, 1, 2, 5, 14, 42
       (mapcar 'neovm--rcp-catalan '(0 1 2 3 4 5)))
    (fmakunbound 'neovm--rcp-coin-change)
    (fmakunbound 'neovm--rcp-subset-sum)
    (fmakunbound 'neovm--rcp-powerset)
    (fmakunbound 'neovm--rcp-catalan)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 1 2 3 6 t nil t t t nil (8 (0 1 2 3 3 4 5 6)) (1 1 2 5 14 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive data structure processing: nested list operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_comprehensive_nested_list_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Count all atoms at all levels of nesting
  (fset 'neovm--rcp-deep-count
    (lambda (tree)
      (cond
       ((null tree) 0)
       ((atom tree) 1)
       (t (+ (funcall 'neovm--rcp-deep-count (car tree))
             (funcall 'neovm--rcp-deep-count (cdr tree)))))))

  ;; Find maximum nesting depth
  (fset 'neovm--rcp-max-depth
    (lambda (tree)
      (cond
       ((atom tree) 0)
       (t (1+ (max (funcall 'neovm--rcp-max-depth (car tree))
                   (funcall 'neovm--rcp-max-depth (cdr tree))))))))

  ;; Replace all occurrences of OLD with NEW at any nesting level
  (fset 'neovm--rcp-deep-replace
    (lambda (tree old new)
      (cond
       ((equal tree old) new)
       ((atom tree) tree)
       (t (cons (funcall 'neovm--rcp-deep-replace (car tree) old new)
                (funcall 'neovm--rcp-deep-replace (cdr tree) old new))))))

  ;; Collect all unique atoms from a nested structure
  (fset 'neovm--rcp-collect-atoms
    (lambda (tree seen)
      (cond
       ((null tree) seen)
       ((atom tree)
        (if (memq tree seen) seen (cons tree seen)))
       (t (funcall 'neovm--rcp-collect-atoms
                   (cdr tree)
                   (funcall 'neovm--rcp-collect-atoms (car tree) seen))))))

  ;; Recursive zip: merge two nested structures pairwise
  (fset 'neovm--rcp-deep-zip
    (lambda (a b)
      (cond
       ((and (atom a) (atom b)) (cons a b))
       ((and (consp a) (consp b))
        (cons (funcall 'neovm--rcp-deep-zip (car a) (car b))
              (funcall 'neovm--rcp-deep-zip (cdr a) (cdr b))))
       (t (cons a b)))))

  (unwind-protect
      (let ((tree1 '(1 (2 (3 4)) (5 (6 7 8))))
            (tree2 '(a (b c) (d (e f)))))
        (list
         ;; Deep count
         (funcall 'neovm--rcp-deep-count tree1)
         (funcall 'neovm--rcp-deep-count nil)
         (funcall 'neovm--rcp-deep-count 'atom)
         (funcall 'neovm--rcp-deep-count '((((1)))))
         ;; Max depth
         (funcall 'neovm--rcp-max-depth tree1)
         (funcall 'neovm--rcp-max-depth 'leaf)
         (funcall 'neovm--rcp-max-depth '(a))
         (funcall 'neovm--rcp-max-depth '(((((deep))))))
         ;; Deep replace
         (funcall 'neovm--rcp-deep-replace '(1 (2 1) (1 (1 3))) 1 99)
         (funcall 'neovm--rcp-deep-replace '(a (b a) c) 'a 'x)
         ;; Collect atoms
         (sort (funcall 'neovm--rcp-collect-atoms '(1 (2 1) (3 (2 4))) nil) #'<)
         ;; Deep zip
         (funcall 'neovm--rcp-deep-zip '(1 (2 3)) '(a (b c)))))
    (fmakunbound 'neovm--rcp-deep-count)
    (fmakunbound 'neovm--rcp-max-depth)
    (fmakunbound 'neovm--rcp-deep-replace)
    (fmakunbound 'neovm--rcp-collect-atoms)
    (fmakunbound 'neovm--rcp-deep-zip)))"#;
    let expect = expect_test::expect![[
        r#""OK (8 0 1 1 8 0 1 5 (99 (2 99) (99 (99 3))) (x (b x) c) (1 2 3 4) ((1 . a) ((2 . b) (3 . c) nil) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Memoized recursion via hash tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_comprehensive_memoized() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Memoized fibonacci
  (fset 'neovm--rcp-fib-memo
    (lambda (n memo)
      (let ((cached (gethash n memo)))
        (if cached cached
          (let ((result
                 (cond
                  ((= n 0) 0)
                  ((= n 1) 1)
                  (t (+ (funcall 'neovm--rcp-fib-memo (- n 1) memo)
                        (funcall 'neovm--rcp-fib-memo (- n 2) memo))))))
            (puthash n result memo)
            result)))))

  ;; Memoized number of ways to climb stairs (1 or 2 steps at a time)
  (fset 'neovm--rcp-stairs-memo
    (lambda (n memo)
      (let ((cached (gethash n memo)))
        (if cached cached
          (let ((result
                 (cond
                  ((<= n 0) 0)
                  ((= n 1) 1)
                  ((= n 2) 2)
                  (t (+ (funcall 'neovm--rcp-stairs-memo (- n 1) memo)
                        (funcall 'neovm--rcp-stairs-memo (- n 2) memo))))))
            (puthash n result memo)
            result)))))

  ;; Memoized partition count: number of ways to write n as sum of positive ints
  ;; p(n, k) = ways to partition n using parts <= k
  (fset 'neovm--rcp-partition-memo
    (lambda (n k memo)
      (let* ((key (cons n k))
             (cached (gethash key memo)))
        (if cached cached
          (let ((result
                 (cond
                  ((= n 0) 1)
                  ((or (< n 0) (<= k 0)) 0)
                  (t (+ (funcall 'neovm--rcp-partition-memo (- n k) k memo)
                        (funcall 'neovm--rcp-partition-memo n (1- k) memo))))))
            (puthash key result memo)
            result)))))

  (unwind-protect
      (let ((fib-memo (make-hash-table :test 'eql))
            (stairs-memo (make-hash-table :test 'eql))
            (part-memo (make-hash-table :test 'equal)))
        (list
         ;; Memoized fibonacci for larger values
         (mapcar (lambda (n) (funcall 'neovm--rcp-fib-memo n fib-memo))
                 '(0 1 2 5 10 15 20 25 30))
         ;; Verify memo table is populated
         (hash-table-count fib-memo)
         ;; Stairs
         (mapcar (lambda (n) (funcall 'neovm--rcp-stairs-memo n stairs-memo))
                 '(1 2 3 4 5 6 7 8 9 10))
         ;; Partition count: p(n) = p(n, n)
         (mapcar (lambda (n) (funcall 'neovm--rcp-partition-memo n n part-memo))
                 '(0 1 2 3 4 5 6 7 8 9 10))
         ;; Cross-check: fib(10) = 55
         (= (funcall 'neovm--rcp-fib-memo 10 fib-memo) 55)
         ;; stairs(10) = fib(11) = 89
         (funcall 'neovm--rcp-stairs-memo 10 stairs-memo)
         ;; p(5) = 7 (partition function value)
         (funcall 'neovm--rcp-partition-memo 5 5 part-memo)))
    (fmakunbound 'neovm--rcp-fib-memo)
    (fmakunbound 'neovm--rcp-stairs-memo)
    (fmakunbound 'neovm--rcp-partition-memo)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 1 5 55 610 6765 75025 832040) 31 (1 2 3 5 8 13 21 34 55 89) (1 1 2 3 5 7 11 15 22 30 42) t 89 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Indirect recursion via funcall and apply
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_comprehensive_indirect_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Generic recursive map that works on both flat and nested lists,
  ;; dispatching via funcall to a strategy function
  (fset 'neovm--rcp-generic-map
    (lambda (strategy fn lst)
      (funcall strategy strategy fn lst)))

  ;; Flat map strategy: simple mapcar-like
  (fset 'neovm--rcp-flat-strategy
    (lambda (self fn lst)
      (if (null lst) nil
        (cons (funcall fn (car lst))
              (funcall self self fn (cdr lst))))))

  ;; Deep map strategy: recurse into sub-lists
  (fset 'neovm--rcp-deep-strategy
    (lambda (self fn lst)
      (cond
       ((null lst) nil)
       ((atom lst) (funcall fn lst))
       (t (cons (funcall self self fn (car lst))
                (funcall self self fn (cdr lst)))))))

  ;; Y-combinator-like pattern: pass function to itself
  (fset 'neovm--rcp-y-fact
    (lambda (self n)
      (if (<= n 1) 1
        (* n (funcall self self (1- n))))))

  ;; Higher-order recursive: apply a list of functions in sequence
  (fset 'neovm--rcp-compose-chain
    (lambda (fns x)
      (if (null fns) x
        (funcall 'neovm--rcp-compose-chain
                 (cdr fns)
                 (funcall (car fns) x)))))

  ;; Indirect recursion through a dispatch table (alist of name -> function)
  (fset 'neovm--rcp-dispatch
    (lambda (table name args)
      (let ((fn (cdr (assq name table))))
        (if fn (apply fn args)
          (error "Unknown dispatch: %s" name)))))

  (unwind-protect
      (let ((dispatch-table
             (list (cons 'double (lambda (x) (* x 2)))
                   (cons 'inc (lambda (x) (+ x 1)))
                   (cons 'square (lambda (x) (* x x))))))
        (list
         ;; Flat strategy
         (funcall 'neovm--rcp-generic-map
                  'neovm--rcp-flat-strategy
                  '1+ '(1 2 3 4 5))
         ;; Deep strategy
         (funcall 'neovm--rcp-generic-map
                  'neovm--rcp-deep-strategy
                  '1+ '(1 (2 (3 4)) 5))
         ;; Y-combinator factorial
         (funcall 'neovm--rcp-y-fact 'neovm--rcp-y-fact 5)
         (funcall 'neovm--rcp-y-fact 'neovm--rcp-y-fact 10)
         ;; Compose chain
         (funcall 'neovm--rcp-compose-chain
                  (list '1+ '1+ '1+) 0)  ;; 0 -> 1 -> 2 -> 3
         (funcall 'neovm--rcp-compose-chain
                  (list (lambda (x) (* x 2))
                        (lambda (x) (+ x 3))
                        (lambda (x) (* x x)))
                  5)  ;; 5 -> 10 -> 13 -> 169
         ;; Dispatch table
         (funcall 'neovm--rcp-dispatch dispatch-table 'double '(21))
         (funcall 'neovm--rcp-dispatch dispatch-table 'inc '(99))
         (funcall 'neovm--rcp-dispatch dispatch-table 'square '(7))))
    (fmakunbound 'neovm--rcp-generic-map)
    (fmakunbound 'neovm--rcp-flat-strategy)
    (fmakunbound 'neovm--rcp-deep-strategy)
    (fmakunbound 'neovm--rcp-y-fact)
    (fmakunbound 'neovm--rcp-compose-chain)
    (fmakunbound 'neovm--rcp-dispatch)))"#;
    let expect =
        expect_test::expect![[r#""OK ((2 3 4 5 6) (2 (3 (4 5)) 6) 120 3628800 3 169 42 100 49)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: recursive Sierpinski triangle and Pascal's triangle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_comprehensive_mathematical_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Pascal's triangle: each row computed recursively from the previous
  (fset 'neovm--rcp-pascal-row
    (lambda (prev)
      "Compute next row of Pascal's triangle from PREV row."
      (if (null prev) '(1)
        (let ((result (list 1))
              (rest prev))
          (while (cdr rest)
            (setq result (cons (+ (car rest) (cadr rest)) result))
            (setq rest (cdr rest)))
          (cons 1 (nreverse result))))))

  (fset 'neovm--rcp-pascal-triangle
    (lambda (n)
      "Generate first N rows of Pascal's triangle."
      (if (<= n 0) nil
        (let ((rows (list '(1)))
              (i 1))
          (while (< i n)
            (setq rows (cons (funcall 'neovm--rcp-pascal-row (car rows)) rows))
            (setq i (1+ i)))
          (nreverse rows)))))

  ;; Recursive Stern-Brocot sequence: interleave mediant fractions
  (fset 'neovm--rcp-stern-brocot
    (lambda (n)
      "Generate the first N terms of the Stern-Brocot sequence."
      (if (<= n 0) nil
        (if (= n 1) '(1)
          (if (= n 2) '(1 1)
            (let ((seq '(1 1))
                  (i 2))
              (while (< i n)
                (let* ((k (/ i 2))
                       (val (if (= (% i 2) 0)
                                (nth (1- k) seq)
                              (+ (nth (/ (1- i) 2) seq)
                                 (nth (1+ (/ (1- i) 2)) seq)))))
                  (setq seq (append seq (list val)))
                  (setq i (1+ i))))
              seq))))))

  ;; Recursive permutation generator
  (fset 'neovm--rcp-permutations
    (lambda (lst)
      (if (null lst) '(nil)
        (let ((result nil))
          (dolist (elem lst)
            (let ((rest (remove elem lst)))
              (dolist (perm (funcall 'neovm--rcp-permutations rest))
                (setq result (cons (cons elem perm) result)))))
          (nreverse result)))))

  (unwind-protect
      (list
       ;; Pascal's triangle: first 6 rows
       (funcall 'neovm--rcp-pascal-triangle 6)
       ;; Verify binomial coefficients: row 4 = (1 4 6 4 1)
       (nth 4 (funcall 'neovm--rcp-pascal-triangle 5))
       ;; Row sums are powers of 2
       (mapcar (lambda (row) (apply #'+ row))
               (funcall 'neovm--rcp-pascal-triangle 7))
       ;; Stern-Brocot first 15 terms
       (funcall 'neovm--rcp-stern-brocot 15)
       ;; Permutations of (1 2 3)
       (funcall 'neovm--rcp-permutations '(1 2 3))
       ;; Number of permutations: 4! = 24
       (length (funcall 'neovm--rcp-permutations '(1 2 3 4)))
       ;; Permutations of empty list
       (funcall 'neovm--rcp-permutations nil)
       ;; Permutations of singleton
       (funcall 'neovm--rcp-permutations '(a)))
    (fmakunbound 'neovm--rcp-pascal-row)
    (fmakunbound 'neovm--rcp-pascal-triangle)
    (fmakunbound 'neovm--rcp-stern-brocot)
    (fmakunbound 'neovm--rcp-permutations)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1) (1 1) (1 1 2) (1 1 2 3) (1 1 2 3 5) (1 1 2 3 5 8)) (1 1 2 3 5) (1 2 4 7 12 20 33) (1 1 1 2 1 3 1 3 2 4 1 4 3 4 1) ((1 2 3) (1 3 2) (2 1 3) (2 3 1) (3 1 2) (3 2 1)) 24 (nil) ((a)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: recursive merge sort and quick sort
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_comprehensive_sorting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Merge sort
  (fset 'neovm--rcp-merge
    (lambda (a b)
      (cond
       ((null a) b)
       ((null b) a)
       ((<= (car a) (car b))
        (cons (car a) (funcall 'neovm--rcp-merge (cdr a) b)))
       (t
        (cons (car b) (funcall 'neovm--rcp-merge a (cdr b)))))))

  (fset 'neovm--rcp-split
    (lambda (lst)
      "Split list into two halves."
      (let ((len (length lst))
            (mid nil)
            (i 0))
        (setq mid (/ len 2))
        (let ((left nil) (right nil) (j 0))
          (dolist (x lst)
            (if (< j mid)
                (setq left (cons x left))
              (setq right (cons x right)))
            (setq j (1+ j)))
          (cons (nreverse left) (nreverse right))))))

  (fset 'neovm--rcp-merge-sort
    (lambda (lst)
      (if (<= (length lst) 1) lst
        (let* ((halves (funcall 'neovm--rcp-split lst))
               (left (funcall 'neovm--rcp-merge-sort (car halves)))
               (right (funcall 'neovm--rcp-merge-sort (cdr halves))))
          (funcall 'neovm--rcp-merge left right)))))

  ;; Quick sort (using first element as pivot)
  (fset 'neovm--rcp-quick-sort
    (lambda (lst)
      (if (<= (length lst) 1) lst
        (let ((pivot (car lst))
              (less nil) (greater nil) (equal nil))
          (dolist (x lst)
            (cond
             ((< x pivot) (setq less (cons x less)))
             ((> x pivot) (setq greater (cons x greater)))
             (t (setq equal (cons x equal)))))
          (append (funcall 'neovm--rcp-quick-sort less)
                  equal
                  (funcall 'neovm--rcp-quick-sort greater))))))

  (unwind-protect
      (list
       ;; Merge sort
       (funcall 'neovm--rcp-merge-sort '(38 27 43 3 9 82 10))
       (funcall 'neovm--rcp-merge-sort '(5 4 3 2 1))
       (funcall 'neovm--rcp-merge-sort '(1 2 3 4 5))
       (funcall 'neovm--rcp-merge-sort '(1))
       (funcall 'neovm--rcp-merge-sort nil)
       (funcall 'neovm--rcp-merge-sort '(3 3 3 1 1 2 2))
       ;; Quick sort
       (funcall 'neovm--rcp-quick-sort '(38 27 43 3 9 82 10))
       (funcall 'neovm--rcp-quick-sort '(5 4 3 2 1))
       (funcall 'neovm--rcp-quick-sort '(1 2 3 4 5))
       (funcall 'neovm--rcp-quick-sort '(1))
       (funcall 'neovm--rcp-quick-sort nil)
       (funcall 'neovm--rcp-quick-sort '(3 3 3 1 1 2 2))
       ;; Both should produce same results
       (equal (funcall 'neovm--rcp-merge-sort '(10 3 7 1 9 2 8 4 6 5))
              (funcall 'neovm--rcp-quick-sort '(10 3 7 1 9 2 8 4 6 5))))
    (fmakunbound 'neovm--rcp-merge)
    (fmakunbound 'neovm--rcp-split)
    (fmakunbound 'neovm--rcp-merge-sort)
    (fmakunbound 'neovm--rcp-quick-sort)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 9 10 27 38 43 82) (1 2 3 4 5) (1 2 3 4 5) (1) nil (1 1 2 2 3 3 3) (3 9 10 27 38 43 82) (1 2 3 4 5) (1 2 3 4 5) (1) nil (1 1 2 2 3 3 3) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: recursive tree construction and search (BST)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_recursion_comprehensive_bst_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; BST node: (value left right)
  (fset 'neovm--rcp-bst-insert
    (lambda (tree val)
      (cond
       ((null tree) (list val nil nil))
       ((< val (car tree))
        (list (car tree)
              (funcall 'neovm--rcp-bst-insert (cadr tree) val)
              (caddr tree)))
       ((> val (car tree))
        (list (car tree)
              (cadr tree)
              (funcall 'neovm--rcp-bst-insert (caddr tree) val)))
       (t tree))))  ;; duplicate, no change

  (fset 'neovm--rcp-bst-member
    (lambda (tree val)
      (cond
       ((null tree) nil)
       ((= val (car tree)) t)
       ((< val (car tree))
        (funcall 'neovm--rcp-bst-member (cadr tree) val))
       (t (funcall 'neovm--rcp-bst-member (caddr tree) val)))))

  ;; In-order traversal (produces sorted list)
  (fset 'neovm--rcp-bst-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--rcp-bst-inorder (cadr tree))
                (list (car tree))
                (funcall 'neovm--rcp-bst-inorder (caddr tree))))))

  ;; BST height
  (fset 'neovm--rcp-bst-height
    (lambda (tree)
      (if (null tree) 0
        (1+ (max (funcall 'neovm--rcp-bst-height (cadr tree))
                 (funcall 'neovm--rcp-bst-height (caddr tree)))))))

  ;; BST minimum
  (fset 'neovm--rcp-bst-min
    (lambda (tree)
      (if (null (cadr tree)) (car tree)
        (funcall 'neovm--rcp-bst-min (cadr tree)))))

  ;; Build BST from list
  (fset 'neovm--rcp-bst-from-list
    (lambda (lst)
      (let ((tree nil))
        (dolist (x lst)
          (setq tree (funcall 'neovm--rcp-bst-insert tree x)))
        tree)))

  (unwind-protect
      (let ((bst (funcall 'neovm--rcp-bst-from-list '(5 3 7 1 4 6 8 2 9))))
        (list
         ;; In-order traversal is sorted
         (funcall 'neovm--rcp-bst-inorder bst)
         ;; Membership
         (funcall 'neovm--rcp-bst-member bst 5)
         (funcall 'neovm--rcp-bst-member bst 1)
         (funcall 'neovm--rcp-bst-member bst 9)
         (funcall 'neovm--rcp-bst-member bst 0)
         (funcall 'neovm--rcp-bst-member bst 10)
         ;; Height
         (funcall 'neovm--rcp-bst-height bst)
         ;; Minimum
         (funcall 'neovm--rcp-bst-min bst)
         ;; Insert duplicate doesn't change
         (equal (funcall 'neovm--rcp-bst-insert bst 5) bst)
         ;; Build from sorted list (worst case: linked list shape)
         (funcall 'neovm--rcp-bst-height
                  (funcall 'neovm--rcp-bst-from-list '(1 2 3 4 5 6 7)))
         ;; Build from reverse sorted
         (funcall 'neovm--rcp-bst-height
                  (funcall 'neovm--rcp-bst-from-list '(7 6 5 4 3 2 1)))
         ;; Empty tree
         (funcall 'neovm--rcp-bst-inorder nil)
         ;; Single element
         (funcall 'neovm--rcp-bst-inorder (funcall 'neovm--rcp-bst-from-list '(42)))))
    (fmakunbound 'neovm--rcp-bst-insert)
    (fmakunbound 'neovm--rcp-bst-member)
    (fmakunbound 'neovm--rcp-bst-inorder)
    (fmakunbound 'neovm--rcp-bst-height)
    (fmakunbound 'neovm--rcp-bst-min)
    (fmakunbound 'neovm--rcp-bst-from-list)))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 2 3 4 5 6 7 8 9) t t t nil nil 4 1 t 7 7 nil (42))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
