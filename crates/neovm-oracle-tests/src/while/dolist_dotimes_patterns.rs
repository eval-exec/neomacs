//! Oracle parity tests for complex `while`, `dolist`, `dotimes` patterns:
//! multiple termination conditions, result forms, destructuring inside loops,
//! for-each-with-index via dotimes, nested cartesian products, and
//! reduce/fold implementations using while.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// while with multiple termination conditions (compound guard)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_multiple_termination_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Process a stream of values until:
    //   1) the list is exhausted, OR
    //   2) we've accumulated a sum >= 100, OR
    //   3) we encounter a negative value, OR
    //   4) we've processed more than 8 items
    // Track which condition actually fired.
    let form = r#"(let ((streams '((10 20 30 40 50 60)
                       (5 10 15 20 25 30 35 40 45 50)
                       (10 20 -5 30 40)
                       (1 2 3 4 5 6 7 8 9 10 11 12)
                       ()
                       (200)))
          (results nil))
  (dolist (stream streams (nreverse results))
    (let ((remaining stream)
          (sum 0)
          (count 0)
          (reason nil))
      (while (and remaining
                  (< sum 100)
                  (>= (car remaining) 0)
                  (< count 8))
        (setq sum (+ sum (car remaining)))
        (setq count (1+ count))
        (setq remaining (cdr remaining)))
      ;; Determine which condition stopped the loop
      (setq reason
            (cond
             ((null remaining) 'exhausted)
             ((>= sum 100) 'sum-limit)
             ((and remaining (< (car remaining) 0)) 'negative)
             ((>= count 8) 'count-limit)
             (t 'unknown)))
      (setq results (cons (list 'sum sum
                                'count count
                                'reason reason
                                'remaining-len (length remaining))
                          results)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((sum 100 count 4 reason sum-limit remaining-len 2) (sum 105 count 6 reason sum-limit remaining-len 4) (sum 30 count 2 reason negative remaining-len 3) (sum 36 count 8 reason count-limit remaining-len 4) (sum 0 count 0 reason exhausted remaining-len 0) (sum 200 count 1 reason exhausted remaining-len 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// dolist with result form: complex result referencing multiple vars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_result_form_complex_aggregation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use dolist result form to compute statistics after iteration.
    // The result form accesses the accumulated state to produce
    // min, max, mean, and a histogram.
    let form = r#"(let ((values '(3 7 1 9 4 2 8 5 6 10 3 7 2 8))
          (sum 0)
          (count 0)
          (min-val nil)
          (max-val nil)
          (histogram (make-vector 11 0)))
  (dolist (v values
           (list 'count count
                 'sum sum
                 'min min-val
                 'max max-val
                 'mean (/ (float sum) count)
                 'var-after-loop v
                 'histogram (let ((h nil))
                              (dotimes (i 11 (nreverse h))
                                (when (> (aref histogram i) 0)
                                  (setq h (cons (cons i (aref histogram i)) h)))))))
    (setq sum (+ sum v))
    (setq count (1+ count))
    (when (or (null min-val) (< v min-val))
      (setq min-val v))
    (when (or (null max-val) (> v max-val))
      (setq max-val v))
    (aset histogram v (1+ (aref histogram v)))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable v)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// dotimes with result form: build a sieve of Eratosthenes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dotimes_result_form_sieve() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use dotimes to implement sieve of Eratosthenes,
    // returning collected primes via the result form.
    let form = r#"(let* ((limit 50)
          (sieve (make-vector (1+ limit) t))
          (primes nil))
  ;; Mark 0 and 1 as not prime
  (aset sieve 0 nil)
  (aset sieve 1 nil)
  ;; Sieve: for each i from 2 to sqrt(limit), mark multiples
  (dotimes (raw-i (1+ limit)
            ;; Result form: collect primes
            (let ((result nil))
              (dotimes (j (1+ limit) (nreverse result))
                (when (aref sieve j)
                  (setq result (cons j result))))))
    (let ((i raw-i))
      (when (and (>= i 2) (aref sieve i))
        ;; Mark all multiples of i starting from i*i
        (let ((j (* i i)))
          (while (<= j limit)
            (aset sieve j nil)
            (setq j (+ j i))))))))"#;
    let expect = expect_test::expect![[r#""OK (2 3 5 7 11 13 17 19 23 29 31 37 41 43 47)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// dolist destructuring patterns (using let inside the loop body)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dolist_destructuring_via_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Iterate over a list of records (represented as lists), destructure
    // each record using let, compute derived values, and accumulate results.
    let form = r#"(let ((students '(("Alice" 90 85 92)
                       ("Bob" 78 82 88)
                       ("Carol" 95 91 97)
                       ("Dave" 60 65 70)
                       ("Eve" 88 92 90)))
          (results nil)
          (total-avg 0)
          (top-students nil)
          (bottom-students nil))
  (dolist (student students)
    (let ((name (nth 0 student))
          (math (nth 1 student))
          (science (nth 2 student))
          (english (nth 3 student)))
      (let ((avg (/ (+ math science english) 3.0)))
        (setq total-avg (+ total-avg avg))
        (setq results (cons (list name
                                  'avg avg
                                  'best (cond
                                         ((and (>= math science) (>= math english)) 'math)
                                         ((>= science english) 'science)
                                         (t 'english))
                                  'pass (>= avg 70.0))
                            results))
        (when (>= avg 90.0)
          (setq top-students (cons name top-students)))
        (when (< avg 75.0)
          (setq bottom-students (cons name bottom-students))))))
  (list 'results (nreverse results)
        'class-avg (/ total-avg (length students))
        'top (nreverse top-students)
        'bottom (nreverse bottom-students)))"#;
    let expect = expect_test::expect![[
        r#""OK (results ((\"Alice\" avg 89.0 best english pass t) (\"Bob\" avg 82.66666666666667 best english pass t) (\"Carol\" avg 94.33333333333333 best english pass t) (\"Dave\" avg 65.0 best english pass nil) (\"Eve\" avg 90.0 best science pass t)) class-avg 84.2 top (\"Carol\" \"Eve\") bottom (\"Dave\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// for-each-with-index using dotimes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dotimes_foreach_with_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use dotimes to iterate a list by index, simulating enumerate().
    // Process pairs of adjacent elements and track even/odd index items.
    let form = r#"(let* ((items '(alpha beta gamma delta epsilon zeta eta theta))
          (len (length items))
          (pairs nil)
          (even-items nil)
          (odd-items nil)
          (indexed nil))
  ;; Enumerate with index
  (dotimes (i len)
    (let ((item (nth i items)))
      (setq indexed (cons (list i item) indexed))
      (if (= (% i 2) 0)
          (setq even-items (cons item even-items))
        (setq odd-items (cons item odd-items)))
      ;; Build adjacent pairs (i, i+1)
      (when (< (1+ i) len)
        (setq pairs (cons (list item (nth (1+ i) items)) pairs)))))
  (list 'indexed (nreverse indexed)
        'even (nreverse even-items)
        'odd (nreverse odd-items)
        'pairs (nreverse pairs)
        'total len))"#;
    let expect = expect_test::expect![[
        r#""OK (indexed ((0 alpha) (1 beta) (2 gamma) (3 delta) (4 epsilon) (5 zeta) (6 eta) (7 theta)) even (alpha gamma epsilon eta) odd (beta delta zeta theta) pairs ((alpha beta) (beta gamma) (gamma delta) (delta epsilon) (epsilon zeta) (zeta eta) (eta theta)) total 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested dolist/dotimes for cartesian product with accumulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_dolist_dotimes_cartesian() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute a multiplication table using dotimes x dotimes,
    // then use dolist x dolist for set intersection/union on lists.
    let form = r#"(list
  ;; Part 1: Multiplication table via nested dotimes
  (let ((table nil))
    (dotimes (i 5)
      (let ((row nil))
        (dotimes (j 5)
          (setq row (cons (* (1+ i) (1+ j)) row)))
        (setq table (cons (nreverse row) table))))
    (nreverse table))

  ;; Part 2: Set operations via nested dolist
  (let ((set-a '(1 3 5 7 9 11 13))
        (set-b '(2 3 5 7 11 13 17))
        (intersection nil)
        (union nil)
        (a-minus-b nil))
    ;; Intersection: elements in both
    (dolist (a set-a)
      (dolist (b set-b)
        (when (= a b)
          (setq intersection (cons a intersection)))))
    ;; Union: all of A + elements of B not in A
    (setq union (copy-sequence set-a))
    (dolist (b set-b)
      (let ((found nil))
        (dolist (a set-a)
          (when (= a b) (setq found t)))
        (unless found
          (setq union (cons b union)))))
    ;; A - B: elements in A not in B
    (dolist (a set-a)
      (let ((in-b nil))
        (dolist (b set-b)
          (when (= a b) (setq in-b t)))
        (unless in-b
          (setq a-minus-b (cons a a-minus-b)))))
    (list 'intersection (sort (nreverse intersection) #'<)
          'union (sort union #'<)
          'a-minus-b (sort (nreverse a-minus-b) #'<)))

  ;; Part 3: Mixed dolist/dotimes for string building
  (let ((words '("hello" "world" "foo"))
        (result nil))
    (dolist (word words)
      (let ((chars nil))
        (dotimes (i (length word))
          (setq chars (cons (upcase (aref word i)) chars)))
        (setq result (cons (concat (mapcar #'char-to-string (nreverse chars)))
                           result))))
    (nreverse result)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp \"H\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Implementing reduce/fold using while
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_reduce_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement foldl and foldr using while loops, then test them
    // with various operations: sum, product, list reversal,
    // max, string concatenation.
    let form = r#"(progn
  (fset 'neovm--foldl
    (lambda (fn init lst)
      "Left fold: (fn (fn (fn init e1) e2) e3)"
      (let ((acc init)
            (remaining lst))
        (while remaining
          (setq acc (funcall fn acc (car remaining)))
          (setq remaining (cdr remaining)))
        acc)))

  (fset 'neovm--foldr
    (lambda (fn init lst)
      "Right fold: (fn e1 (fn e2 (fn e3 init)))"
      (let ((reversed nil)
            (remaining lst))
        ;; First reverse the list
        (while remaining
          (setq reversed (cons (car remaining) reversed))
          (setq remaining (cdr remaining)))
        ;; Then fold from the right
        (let ((acc init))
          (while reversed
            (setq acc (funcall fn (car reversed) acc))
            (setq reversed (cdr reversed)))
          acc))))

  (unwind-protect
      (let ((nums '(1 2 3 4 5 6 7 8 9 10)))
        (list
         ;; Sum via foldl
         (funcall 'neovm--foldl #'+ 0 nums)
         ;; Product via foldl
         (funcall 'neovm--foldl #'* 1 nums)
         ;; Max via foldl
         (funcall 'neovm--foldl #'max 0 nums)
         ;; List reversal via foldl
         (funcall 'neovm--foldl (lambda (acc x) (cons x acc)) nil '(a b c d e))
         ;; String concatenation via foldl
         (funcall 'neovm--foldl (lambda (acc s) (concat acc "-" s))
                  "start" '("a" "b" "c"))
         ;; Right fold: build list (identity via foldr)
         (funcall 'neovm--foldr #'cons nil '(a b c d e))
         ;; Right fold: right-associative subtraction
         ;; 1 - (2 - (3 - (4 - (5 - 0)))) = 1 - (2 - (3 - (4 - 5))) = ...
         (funcall 'neovm--foldr #'- 0 '(1 2 3 4 5))
         ;; Foldl on empty list returns init
         (funcall 'neovm--foldl #'+ 42 nil)
         ;; Foldr on singleton
         (funcall 'neovm--foldr #'cons nil '(only))))
    (fmakunbound 'neovm--foldl)
    (fmakunbound 'neovm--foldr)))"#;
    let expect = expect_test::expect![[
        r#""OK (55 3628800 10 (e d c b a) \"start-a-b-c\" (a b c d e) 3 42 (only))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// while simulating a stack-based calculator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_stack_calculator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Process a sequence of tokens (numbers and operators) using
    // a while loop with a stack, implementing RPN evaluation.
    let form = r#"(progn
  (fset 'neovm--rpn-eval
    (lambda (tokens)
      "Evaluate RPN expression given as list of tokens."
      (let ((stack nil)
            (remaining tokens))
        (while remaining
          (let ((tok (car remaining)))
            (cond
             ((numberp tok)
              (setq stack (cons tok stack)))
             ((eq tok '+)
              (let ((b (car stack))
                    (a (cadr stack)))
                (setq stack (cons (+ a b) (cddr stack)))))
             ((eq tok '-)
              (let ((b (car stack))
                    (a (cadr stack)))
                (setq stack (cons (- a b) (cddr stack)))))
             ((eq tok '*)
              (let ((b (car stack))
                    (a (cadr stack)))
                (setq stack (cons (* a b) (cddr stack)))))
             ((eq tok 'dup)
              (setq stack (cons (car stack) stack)))
             ((eq tok 'swap)
              (let ((top (car stack))
                    (second (cadr stack)))
                (setq stack (cons second (cons top (cddr stack))))))
             ((eq tok 'over)
              (setq stack (cons (cadr stack) stack)))))
          (setq remaining (cdr remaining)))
        stack)))

  (unwind-protect
      (list
       ;; 3 4 + => 7
       (funcall 'neovm--rpn-eval '(3 4 +))
       ;; 5 3 - 2 * => (5-3)*2 = 4
       (funcall 'neovm--rpn-eval '(5 3 - 2 *))
       ;; 10 2 3 + * => 10*(2+3) = 50
       (funcall 'neovm--rpn-eval '(10 2 3 + *))
       ;; dup: 5 dup * => 25
       (funcall 'neovm--rpn-eval '(5 dup *))
       ;; swap: 3 7 swap - => 7-3 = 4
       (funcall 'neovm--rpn-eval '(3 7 swap -))
       ;; over: 3 5 over => stack is (3 5 3), then + => (8 3)
       (funcall 'neovm--rpn-eval '(3 5 over +))
       ;; Complex: (2+3)*(4+5) = 45
       (funcall 'neovm--rpn-eval '(2 3 + 4 5 + *))
       ;; Empty
       (funcall 'neovm--rpn-eval nil))
    (fmakunbound 'neovm--rpn-eval)))"#;
    let expect = expect_test::expect![[r#""OK ((7) (4) (50) (25) (4) (8 3) (45) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// dotimes with zero and one iteration edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dotimes_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test dotimes with boundary counts: 0, 1, and with result forms
    // that reference the loop variable at its final value.
    let form = r#"(list
  ;; dotimes with 0 iterations: body never runs, result form evaluated
  (let ((ran nil))
    (dotimes (i 0 (list 'ran ran 'i i))
      (setq ran t)))

  ;; dotimes with 1 iteration
  (let ((values nil))
    (dotimes (i 1 (list 'values (nreverse values) 'final-i i))
      (setq values (cons (list 'iter i) values))))

  ;; dotimes result form returns loop variable (should be = count)
  (dotimes (i 5 i))

  ;; dotimes result form is nil when omitted — returns nil
  (dotimes (i 3))

  ;; dolist with empty list: result form still runs
  (let ((count 0))
    (dolist (x nil (list 'count count 'x x))
      (setq count (1+ count))))

  ;; dolist result form default is nil
  (dolist (x '(1 2 3)))

  ;; Nested dotimes with result forms
  (let ((matrix nil))
    (dotimes (i 3
              (nreverse matrix))
      (let ((row nil))
        (dotimes (j 3
                  (setq matrix (cons (nreverse row) matrix)))
          (setq row (cons (+ (* i 10) j) row)))))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
