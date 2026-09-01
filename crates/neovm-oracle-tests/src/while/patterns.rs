//! Oracle parity tests for complex `while` loop patterns:
//! multiple exit conditions, collect-then-nreverse, catch/throw early exit,
//! nested 2D iteration, accumulator patterns, buffer scanning,
//! merge of sorted lists, and binary search.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// while with multiple exit conditions (and/or in test)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_pattern_multi_exit_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk a list, stop when we hit a negative OR accumulated sum exceeds 50
    // The and/or in the while test creates complex short-circuit evaluation
    let form = r#"(let ((data '(3 7 12 5 8 20 4 -1 9 6))
                        (remaining nil)
                        (sum 0)
                        (count 0)
                        (result nil))
                    (setq remaining data)
                    (while (and remaining
                                (>= (car remaining) 0)
                                (or (<= sum 50)
                                    (progn (setq result 'overflow) nil)))
                      (let ((x (car remaining)))
                        (setq sum (+ sum x)
                              count (1+ count)
                              remaining (cdr remaining))))
                    (list count sum
                          (if remaining (car remaining) 'exhausted)
                          result))"#;
    let expect = expect_test::expect![[r#""OK (6 55 4 overflow)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// while collecting results in reverse then nreverse
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_pattern_collect_nreverse_sieve() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sieve of Eratosthenes using while + nreverse for efficient collection
    let form = r#"(let* ((limit 50)
                         (sieve (make-vector (1+ limit) t))
                         (i 2)
                         (primes nil))
                    ;; Mark composites
                    (while (<= (* i i) limit)
                      (when (aref sieve i)
                        (let ((j (* i i)))
                          (while (<= j limit)
                            (aset sieve j nil)
                            (setq j (+ j i)))))
                      (setq i (1+ i)))
                    ;; Collect primes in reverse, then nreverse
                    (setq i 2)
                    (while (<= i limit)
                      (when (aref sieve i)
                        (setq primes (cons i primes)))
                      (setq i (1+ i)))
                    (nreverse primes))"#;
    let expect = expect_test::expect![[r#""OK (2 3 5 7 11 13 17 19 23 29 31 37 41 43 47)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// while with catch/throw for early exit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_pattern_catch_throw_early_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Search nested alist for a value, throw on first match
    // Also tests that cleanup/accumulation state is preserved up to the throw
    let form = r#"(let ((database '((users . ((alice . ((age . 30) (role . admin)))
                                              (bob . ((age . 25) (role . user)))
                                              (carol . ((age . 35) (role . admin)))))
                                    (groups . ((admins . (alice carol))
                                               (staff . (alice bob carol))))))
                        (visited nil))
                    (catch 'found
                      (let ((tables database))
                        (while tables
                          (let* ((table (car tables))
                                 (entries (cdr table)))
                            (setq visited (cons (car table) visited))
                            (while entries
                              (let ((entry (car entries)))
                                (when (and (consp (cdr entry))
                                           (assq 'role (cdr entry))
                                           (eq (cdr (assq 'role (cdr entry))) 'admin)
                                           (> (cdr (assq 'age (cdr entry))) 32))
                                  (throw 'found
                                         (list 'match (car entry)
                                               'visited (nreverse visited)))))
                              (setq entries (cdr entries))))
                          (setq tables (cdr tables))))
                      'not-found))"#;
    let expect = expect_test::expect![[r#""OK (match carol visited (users))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nested while (2D iteration) — matrix operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_pattern_nested_2d_matrix_multiply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // 2x3 * 3x2 matrix multiplication using nested while loops
    // Each matrix is a vector of vectors
    let form = r#"(let* ((a (vector (vector 1 2 3)
                                    (vector 4 5 6)))
                          (b (vector (vector 7 8)
                                    (vector 9 10)
                                    (vector 11 12)))
                          (rows-a (length a))
                          (cols-a (length (aref a 0)))
                          (cols-b (length (aref b 0)))
                          (result (make-vector rows-a nil))
                          (i 0))
                    ;; Initialize result rows
                    (while (< i rows-a)
                      (aset result i (make-vector cols-b 0))
                      (setq i (1+ i)))
                    ;; Triple nested while for multiplication
                    (setq i 0)
                    (while (< i rows-a)
                      (let ((j 0))
                        (while (< j cols-b)
                          (let ((k 0) (sum 0))
                            (while (< k cols-a)
                              (setq sum (+ sum (* (aref (aref a i) k)
                                                  (aref (aref b k) j))))
                              (setq k (1+ k)))
                            (aset (aref result i) j sum))
                          (setq j (1+ j))))
                      (setq i (1+ i)))
                    ;; Convert to lists for comparison
                    (list (append (aref result 0) nil)
                          (append (aref result 1) nil)))"#;
    let expect = expect_test::expect![[r#""OK ((58 64) (139 154))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// while as accumulator (running sum/product with complex reduction)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_pattern_accumulator_run_length_encode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Run-length encoding: '(a a a b b c a a) => '((3 . a) (2 . b) (1 . c) (2 . a))
    let form = r#"(let ((input '(a a a b b c a a d d d d))
                        (remaining nil)
                        (result nil)
                        (current nil)
                        (count 0))
                    (setq remaining input)
                    (when remaining
                      (setq current (car remaining)
                            count 1
                            remaining (cdr remaining)))
                    (while remaining
                      (if (eq (car remaining) current)
                          (setq count (1+ count))
                        (setq result (cons (cons count current) result)
                              current (car remaining)
                              count 1))
                      (setq remaining (cdr remaining)))
                    ;; Don't forget the last run
                    (when current
                      (setq result (cons (cons count current) result)))
                    (nreverse result))"#;
    let expect = expect_test::expect![[r#""OK ((3 . a) (2 . b) (1 . c) (2 . a) (4 . d))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// while with buffer scanning (forward-line + processing)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_pattern_buffer_csv_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse CSV-like lines from a buffer, computing per-column aggregates
    let form = r#"(with-temp-buffer
                    (insert "name,score,grade\nalice,95,A\nbob,82,B\ncarol,91,A\ndave,78,C\n")
                    (goto-char (point-min))
                    ;; Skip header
                    (forward-line 1)
                    (let ((names nil)
                          (total-score 0)
                          (count 0)
                          (grades (make-hash-table :test 'equal)))
                      (while (not (eobp))
                        (let* ((line-start (point))
                               (_ (end-of-line))
                               (line-end (point))
                               (line (buffer-substring-no-properties line-start line-end)))
                          (when (> (length line) 0)
                            (let* ((comma1 (string-match "," line))
                                   (comma2 (string-match "," line (1+ comma1)))
                                   (name (substring line 0 comma1))
                                   (score (string-to-number (substring line (1+ comma1) comma2)))
                                   (grade (substring line (1+ comma2))))
                              (setq names (cons name names)
                                    total-score (+ total-score score)
                                    count (1+ count))
                              (puthash grade
                                       (1+ (gethash grade grades 0))
                                       grades))))
                        (forward-line 1))
                      (list (nreverse names)
                            total-score
                            count
                            (gethash "A" grades 0)
                            (gethash "B" grades 0)
                            (gethash "C" grades 0))))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"alice\" \"bob\" \"carol\" \"dave\") 346 4 2 1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: while-based merge of two sorted lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_pattern_merge_sorted_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Merge two sorted lists, then use merge for merge-sort on a list
    let form = r#"(progn
  (fset 'neovm--test-merge
    (lambda (a b)
      (let ((result nil))
        (while (and a b)
          (if (<= (car a) (car b))
              (progn (setq result (cons (car a) result)
                           a (cdr a)))
            (setq result (cons (car b) result)
                  b (cdr b))))
        ;; Append remainder
        (while a
          (setq result (cons (car a) result)
                a (cdr a)))
        (while b
          (setq result (cons (car b) result)
                b (cdr b)))
        (nreverse result))))

  ;; Split list into two halves using slow/fast pointer
  (fset 'neovm--test-split
    (lambda (lst)
      (let ((slow lst) (fast (cdr lst)))
        (while (and fast (cdr fast))
          (setq slow (cdr slow)
                fast (cddr fast)))
        (let ((second (cdr slow)))
          (setcdr slow nil)
          (list lst second)))))

  ;; Merge sort
  (fset 'neovm--test-msort
    (lambda (lst)
      (if (or (null lst) (null (cdr lst)))
          lst
        (let* ((halves (funcall 'neovm--test-split lst))
               (left (funcall 'neovm--test-msort (car halves)))
               (right (funcall 'neovm--test-msort (cadr halves))))
          (funcall 'neovm--test-merge left right)))))

  (unwind-protect
      (list
        ;; Basic merge
        (funcall 'neovm--test-merge '(1 3 5 7) '(2 4 6 8))
        ;; Merge with duplicates
        (funcall 'neovm--test-merge '(1 2 2 5) '(2 3 4))
        ;; Full merge sort
        (funcall 'neovm--test-msort '(38 27 43 3 9 82 10))
        ;; Already sorted
        (funcall 'neovm--test-msort '(1 2 3 4 5))
        ;; Reverse sorted
        (funcall 'neovm--test-msort '(5 4 3 2 1)))
    (fmakunbound 'neovm--test-merge)
    (fmakunbound 'neovm--test-split)
    (fmakunbound 'neovm--test-msort)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8) (1 2 2 2 3 4 5) (3 9 10 27 38 43 82) (1 2 3 4 5) (1 2 3 4 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: while implementing binary search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_pattern_binary_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Binary search on sorted vector, returning index or nil
    // Also: binary search to find insertion point (like bisect_left)
    let form = r#"(progn
  (fset 'neovm--test-bsearch
    (lambda (vec target)
      (let ((lo 0)
            (hi (1- (length vec)))
            (found nil))
        (while (and (<= lo hi) (not found))
          (let* ((mid (/ (+ lo hi) 2))
                 (val (aref vec mid)))
            (cond
              ((= val target) (setq found mid))
              ((< val target) (setq lo (1+ mid)))
              (t (setq hi (1- mid))))))
        found)))

  ;; bisect-left: find leftmost insertion index
  (fset 'neovm--test-bisect-left
    (lambda (vec target)
      (let ((lo 0)
            (hi (length vec)))
        (while (< lo hi)
          (let ((mid (/ (+ lo hi) 2)))
            (if (< (aref vec mid) target)
                (setq lo (1+ mid))
              (setq hi mid))))
        lo)))

  (unwind-protect
      (let ((sorted [2 5 8 12 16 23 38 56 72 91]))
        (list
          ;; Found cases
          (funcall 'neovm--test-bsearch sorted 23)
          (funcall 'neovm--test-bsearch sorted 2)
          (funcall 'neovm--test-bsearch sorted 91)
          ;; Not found
          (funcall 'neovm--test-bsearch sorted 50)
          (funcall 'neovm--test-bsearch sorted 1)
          ;; Bisect-left: insertion points
          (funcall 'neovm--test-bisect-left sorted 23)
          (funcall 'neovm--test-bisect-left sorted 24)
          (funcall 'neovm--test-bisect-left sorted 1)
          (funcall 'neovm--test-bisect-left sorted 100)))
    (fmakunbound 'neovm--test-bsearch)
    (fmakunbound 'neovm--test-bisect-left)))"#;
    let expect = expect_test::expect![[r#""OK (5 0 9 nil nil 5 6 0 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
