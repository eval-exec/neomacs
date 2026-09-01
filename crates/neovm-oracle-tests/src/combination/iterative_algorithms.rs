//! Complex oracle tests for iterative algorithm implementations in Elisp.
//!
//! Tests a sieve of Eratosthenes (vector marking), binary search on
//! a sorted vector, stack-based RPN calculator, run-length encoding
//! with hash tables, topological sort (Kahn's algorithm), and matrix
//! operations (transpose, multiply) using nested vectors.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Sieve of Eratosthenes with segmented prime counting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iteralgo_sieve_with_prime_gaps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute primes up to 100, then find the largest prime gap
    // (difference between consecutive primes).
    let form = r#"(let ((limit 100))
  (let ((sieve (make-vector (1+ limit) t)))
    (aset sieve 0 nil)
    (aset sieve 1 nil)
    (let ((i 2))
      (while (<= (* i i) limit)
        (when (aref sieve i)
          (let ((j (* i i)))
            (while (<= j limit)
              (aset sieve j nil)
              (setq j (+ j i)))))
        (setq i (1+ i))))
    ;; Collect primes
    (let ((primes nil))
      (let ((k 2))
        (while (<= k limit)
          (when (aref sieve k)
            (setq primes (cons k primes)))
          (setq k (1+ k))))
      (setq primes (nreverse primes))
      ;; Find max gap and its bounding primes
      (let ((max-gap 0)
            (gap-start 0)
            (gap-end 0)
            (prev (car primes))
            (rest (cdr primes)))
        (while rest
          (let ((curr (car rest))
                (gap (- (car rest) prev)))
            (when (> gap max-gap)
              (setq max-gap gap
                    gap-start prev
                    gap-end curr))
            (setq prev curr
                  rest (cdr rest))))
        (list 'count (length primes)
              'max-gap max-gap
              'between gap-start gap-end
              'first-5 (take 5 primes)
              'last-5 (last primes 5))))))"#;
    let expect = expect_test::expect![[
        r#""OK (count 25 max-gap 8 between 89 97 first-5 (2 3 5 7 11) last-5 (73 79 83 89 97))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Binary search on sorted vector with insertion point
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iteralgo_binary_search_insertion_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Binary search that returns (found . index) or (nil . insertion-point).
    let form = r#"(let ((bsearch
         (lambda (vec target)
           (let ((lo 0)
                 (hi (1- (length vec)))
                 (result nil))
             (while (and (<= lo hi) (not result))
               (let ((mid (/ (+ lo hi) 2)))
                 (let ((val (aref vec mid)))
                   (cond
                    ((= val target) (setq result (cons t mid)))
                    ((< val target) (setq lo (1+ mid)))
                    (t (setq hi (1- mid)))))))
             (or result (cons nil lo))))))
  (let ((sorted [2 5 8 12 16 23 38 56 72 91]))
    (list
     ;; Found cases
     (funcall bsearch sorted 2)
     (funcall bsearch sorted 23)
     (funcall bsearch sorted 91)
     ;; Not found: insertion points
     (funcall bsearch sorted 1)
     (funcall bsearch sorted 10)
     (funcall bsearch sorted 50)
     (funcall bsearch sorted 100))))"#;
    let expect = expect_test::expect![[
        r#""OK ((t . 0) (t . 5) (t . 9) (nil . 0) (nil . 3) (nil . 7) (nil . 10))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stack-based RPN calculator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iteralgo_rpn_calculator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Evaluate RPN expressions using a list as a stack.
    // Supports +, -, *, / and dup.
    let form = r#"(progn
  (fset 'neovm--rpn-eval
    (lambda (tokens)
      (let ((stack nil))
        (dolist (tok tokens)
          (cond
           ((numberp tok)
            (setq stack (cons tok stack)))
           ((eq tok '+)
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (+ a b) (cddr stack)))))
           ((eq tok '-)
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (- a b) (cddr stack)))))
           ((eq tok '*)
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (* a b) (cddr stack)))))
           ((eq tok '/)
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (/ a b) (cddr stack)))))
           ((eq tok 'dup)
            (setq stack (cons (car stack) stack)))))
        stack)))
  (unwind-protect
      (list
       ;; 3 4 + => 7
       (funcall 'neovm--rpn-eval '(3 4 +))
       ;; 5 1 2 + 4 * + 3 - => 5 + (1+2)*4 - 3 = 14
       (funcall 'neovm--rpn-eval '(5 1 2 + 4 * + 3 -))
       ;; 10 dup * => 100
       (funcall 'neovm--rpn-eval '(10 dup *))
       ;; 15 7 1 1 + - / 3 * 2 1 1 + + - => complex
       (funcall 'neovm--rpn-eval '(15 7 1 1 + - / 3 * 2 1 1 + + -)))
    (fmakunbound 'neovm--rpn-eval)))"#;
    let expect = expect_test::expect![[r#""OK ((7) (14) (100) (5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Run-length encoding/decoding with hash-table frequency analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iteralgo_rle_with_frequency_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // RLE encode, then build a frequency table of run lengths using hash-table.
    let form = r#"(let ((rle-encode
         (lambda (lst)
           (when lst
             (let ((result nil)
                   (current (car lst))
                   (count 1))
               (dolist (x (cdr lst))
                 (if (equal x current)
                     (setq count (1+ count))
                   (setq result (cons (cons current count) result)
                         current x
                         count 1)))
               (setq result (cons (cons current count) result))
               (nreverse result))))))
  (let ((input '(a a a b b c c c c a a d d d d d d b)))
    (let ((encoded (funcall rle-encode input)))
      ;; Build frequency table of run lengths
      (let ((freq (make-hash-table :test 'equal)))
        (dolist (pair encoded)
          (let ((len (cdr pair)))
            (puthash len (1+ (or (gethash len freq) 0)) freq)))
        ;; Collect freq entries sorted by key
        (let ((freq-list nil))
          (maphash (lambda (k v) (setq freq-list (cons (cons k v) freq-list))) freq)
          (setq freq-list (sort freq-list (lambda (a b) (< (car a) (car b)))))
          ;; Also decode and verify roundtrip
          (let ((decoded nil))
            (dolist (pair encoded)
              (dotimes (_ (cdr pair))
                (setq decoded (cons (car pair) decoded))))
            (setq decoded (nreverse decoded))
            (list 'encoded encoded
                  'run-length-freq freq-list
                  'roundtrip-ok (equal input decoded))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (encoded ((a . 3) (b . 2) (c . 4) (a . 2) (d . 6) (b . 1)) run-length-freq ((1 . 1) (2 . 2) (3 . 1) (4 . 1) (6 . 1)) roundtrip-ok t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Topological sort (Kahn's algorithm)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iteralgo_topological_sort_kahn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Topological sort of a DAG using Kahn's algorithm with hash tables
    // for adjacency lists and in-degree counts.
    let form = r#"(let ((edges '((a b) (a c) (b d) (c d) (d e) (b e) (f c))))
  ;; Build adjacency list and in-degree table
  (let ((adj (make-hash-table :test 'eq))
        (in-deg (make-hash-table :test 'eq))
        (all-nodes nil))
    ;; Collect all nodes and initialize
    (dolist (e edges)
      (let ((from (car e)) (to (cadr e)))
        (unless (memq from all-nodes) (setq all-nodes (cons from all-nodes)))
        (unless (memq to all-nodes) (setq all-nodes (cons to all-nodes)))
        ;; Add edge to adjacency list
        (puthash from (cons to (or (gethash from adj) nil)) adj)
        ;; Increment in-degree
        (puthash to (1+ (or (gethash to in-deg) 0)) in-deg)))
    ;; Initialize in-degree for nodes with no incoming edges
    (dolist (n all-nodes)
      (unless (gethash n in-deg)
        (puthash n 0 in-deg)))
    ;; Kahn's algorithm: queue = nodes with in-degree 0
    (let ((queue nil)
          (result nil))
      (dolist (n all-nodes)
        (when (= (gethash n in-deg) 0)
          (setq queue (cons n queue))))
      ;; Sort queue for deterministic output
      (setq queue (sort queue (lambda (a b)
                                (string< (symbol-name a) (symbol-name b)))))
      (while queue
        (let ((node (car queue)))
          (setq queue (cdr queue))
          (setq result (cons node result))
          ;; Decrease in-degree for neighbors
          (dolist (neighbor (or (gethash node adj) nil))
            (puthash neighbor (1- (gethash neighbor in-deg)) in-deg)
            (when (= (gethash neighbor in-deg) 0)
              (setq queue (sort (cons neighbor queue)
                                (lambda (a b)
                                  (string< (symbol-name a) (symbol-name b)))))))))
      ;; Verify: result length should equal number of nodes (no cycle)
      (list 'order (nreverse result)
            'valid (= (length result) (length all-nodes))))))"#;
    let expect = expect_test::expect![[r#""OK (order (a b f c d e) valid nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix operations: transpose and multiply using nested vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_iteralgo_matrix_transpose_multiply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Matrix represented as vector of row-vectors.
    // Implement transpose and multiply, verify (A^T)^T = A and
    // A * I = A.
    let form = r#"(progn
  (fset 'neovm--mat-rows (lambda (m) (length m)))
  (fset 'neovm--mat-cols (lambda (m) (length (aref m 0))))
  (fset 'neovm--mat-ref (lambda (m r c) (aref (aref m r) c)))

  (fset 'neovm--mat-transpose
    (lambda (m)
      (let ((rows (funcall 'neovm--mat-rows m))
            (cols (funcall 'neovm--mat-cols m)))
        (let ((result (make-vector cols nil)))
          (dotimes (c cols)
            (let ((row (make-vector rows 0)))
              (dotimes (r rows)
                (aset row r (funcall 'neovm--mat-ref m r c)))
              (aset result c row)))
          result))))

  (fset 'neovm--mat-multiply
    (lambda (a b)
      (let ((ar (funcall 'neovm--mat-rows a))
            (ac (funcall 'neovm--mat-cols a))
            (br (funcall 'neovm--mat-rows b))
            (bc (funcall 'neovm--mat-cols b)))
        (let ((result (make-vector ar nil)))
          (dotimes (i ar)
            (let ((row (make-vector bc 0)))
              (dotimes (j bc)
                (let ((sum 0))
                  (dotimes (k ac)
                    (setq sum (+ sum (* (funcall 'neovm--mat-ref a i k)
                                        (funcall 'neovm--mat-ref b k j)))))
                  (aset row j sum)))
              (aset result i row)))
          result))))

  (unwind-protect
      (let ((a (vector (vector 1 2 3) (vector 4 5 6))))
        (let ((at (funcall 'neovm--mat-transpose a)))
          (let ((att (funcall 'neovm--mat-transpose at)))
            ;; Identity matrix 3x3
            (let ((eye (vector (vector 1 0 0) (vector 0 1 0) (vector 0 0 1))))
              (let ((a-times-eye (funcall 'neovm--mat-multiply a eye)))
                ;; 2x2 multiplication
                (let ((m1 (vector (vector 1 2) (vector 3 4)))
                      (m2 (vector (vector 5 6) (vector 7 8))))
                  (let ((prod (funcall 'neovm--mat-multiply m1 m2)))
                    (list 'transpose-of-a at
                          'double-transpose-eq (equal a att)
                          'a-times-identity (equal a a-times-eye)
                          'product prod))))))))
    (fmakunbound 'neovm--mat-rows)
    (fmakunbound 'neovm--mat-cols)
    (fmakunbound 'neovm--mat-ref)
    (fmakunbound 'neovm--mat-transpose)
    (fmakunbound 'neovm--mat-multiply)))"#;
    let expect = expect_test::expect![[
        r#""OK (transpose-of-a [[1 4] [2 5] [3 6]] double-transpose-eq t a-times-identity t product [[19 22] [43 50]])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
