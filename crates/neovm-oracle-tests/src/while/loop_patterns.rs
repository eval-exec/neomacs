//! Advanced oracle parity tests for `while` loop patterns:
//! accumulator patterns, early exit with `catch`/`throw`, nested while loops,
//! while with complex conditions, do-while simulation, for-each simulation,
//! infinite loop with catch-based exit, and while-let equivalent patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Accumulator patterns: multiple simultaneous accumulators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_loop_multi_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Process a list of transactions, maintaining running balance, max, min,
    // count of deposits vs withdrawals, and a filtered list
    let form = r#"(let ((transactions '((deposit . 500) (withdraw . 200) (deposit . 300)
                          (withdraw . 50) (deposit . 1000) (withdraw . 750)
                          (deposit . 100) (withdraw . 400)))
          (remaining nil)
          (balance 0)
          (max-balance 0)
          (min-balance 0)
          (deposit-count 0)
          (withdraw-count 0)
          (large-txns nil))
  (setq remaining transactions)
  (while remaining
    (let* ((txn (car remaining))
           (kind (car txn))
           (amount (cdr txn)))
      (cond
       ((eq kind 'deposit)
        (setq balance (+ balance amount))
        (setq deposit-count (1+ deposit-count)))
       ((eq kind 'withdraw)
        (setq balance (- balance amount))
        (setq withdraw-count (1+ withdraw-count))))
      (when (> balance max-balance) (setq max-balance balance))
      (when (< balance min-balance) (setq min-balance balance))
      (when (> amount 300)
        (setq large-txns (cons (list kind amount balance) large-txns))))
    (setq remaining (cdr remaining)))
  (list 'balance balance
        'max max-balance
        'min min-balance
        'deposits deposit-count
        'withdrawals withdraw-count
        'large-txns (nreverse large-txns)))"#;
    let expect = expect_test::expect![[
        r#""OK (balance 500 max 1550 min 0 deposits 4 withdrawals 4 large-txns ((deposit 500 500) (deposit 1000 1550) (withdraw 750 800) (withdraw 400 500)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Early exit with catch/throw from while loop
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_loop_catch_throw_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Search a 2D grid (list of lists) for a target value,
    // throw immediately on finding it with coordinates
    let form = r#"(let ((grid '((1 2 3 4 5)
                  (6 7 8 9 10)
                  (11 12 13 14 15)
                  (16 17 18 19 20)
                  (21 22 23 24 25))))
  (list
   ;; Search for 14 (should find at row 2, col 3)
   (catch 'found
     (let ((row 0) (rows grid))
       (while rows
         (let ((col 0) (cells (car rows)))
           (while cells
             (when (= (car cells) 14)
               (throw 'found (list 'at row col)))
             (setq col (1+ col))
             (setq cells (cdr cells))))
         (setq row (1+ row))
         (setq rows (cdr rows)))
       'not-found))
   ;; Search for 99 (should not be found)
   (catch 'found
     (let ((row 0) (rows grid))
       (while rows
         (let ((col 0) (cells (car rows)))
           (while cells
             (when (= (car cells) 99)
               (throw 'found (list 'at row col)))
             (setq col (1+ col))
             (setq cells (cdr cells))))
         (setq row (1+ row))
         (setq rows (cdr rows)))
       'not-found))
   ;; Search for first value > 20
   (catch 'found
     (let ((rows grid))
       (while rows
         (let ((cells (car rows)))
           (while cells
             (when (> (car cells) 20)
               (throw 'found (car cells)))
             (setq cells (cdr cells))))
         (setq rows (cdr rows)))
       'not-found))))"#;
    let expect = expect_test::expect![[r#""OK ((at 2 3) not-found 21)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested while loops: generate combinations and permutations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_loop_nested_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate all 3-element combinations from a list
    // using triple-nested while loops
    let form = r#"(let ((items '(a b c d e))
          (combos nil))
  ;; Triple-nested while for combinations C(5,3)
  (let ((i-list items))
    (while (cddr i-list)  ;; need at least 3 elements remaining
      (let ((j-list (cdr i-list)))
        (while (cdr j-list)
          (let ((k-list (cdr j-list)))
            (while k-list
              (setq combos
                    (cons (list (car i-list) (car j-list) (car k-list))
                          combos))
              (setq k-list (cdr k-list))))
          (setq j-list (cdr j-list))))
      (setq i-list (cdr i-list))))
  (list 'count (length combos)
        'combos (nreverse combos)))"#;
    let expect = expect_test::expect![[
        r#""OK (count 10 combos ((a b c) (a b d) (a b e) (a c d) (a c e) (a d e) (b c d) (b c e) (b d e) (c d e)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// While with complex conditions: short-circuit with side effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_loop_complex_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complex while conditions using and/or with side effects in the condition
    // to demonstrate short-circuit evaluation behavior
    let form = r#"(let ((data '(2 4 6 8 10 3 12 14))
          (remaining nil)
          (sum 0)
          (steps 0)
          (condition-evals nil))
  ;; While: element is even AND sum < 25 AND we haven't processed 10 items
  ;; Track which conditions were evaluated at each step
  (setq remaining data)
  (while (and remaining
              (progn
                (setq condition-evals (cons (list 'check-even (car remaining)) condition-evals))
                (= 0 (% (car remaining) 2)))
              (progn
                (setq condition-evals (cons (list 'check-sum sum) condition-evals))
                (< sum 25)))
    (setq sum (+ sum (car remaining)))
    (setq steps (1+ steps))
    (setq remaining (cdr remaining)))
  (list 'sum sum
        'steps steps
        'remaining-head (if remaining (car remaining) 'exhausted)
        'evals-count (length condition-evals)
        ;; Show the evaluation trace (reversed since cons'd)
        'last-evals (let ((n (min 6 (length condition-evals))))
                      (let ((result nil) (lst condition-evals) (i 0))
                        (while (< i n)
                          (setq result (cons (car lst) result))
                          (setq lst (cdr lst))
                          (setq i (1+ i)))
                        result))))"#;
    let expect = expect_test::expect![[
        r#""OK (sum 30 steps 5 remaining-head 3 evals-count 11 last-evals ((check-sum 6) (check-even 8) (check-sum 12) (check-even 10) (check-sum 20) (check-even 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Do-while simulation: body executes at least once
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_loop_do_while_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate do-while using (let ((continue t)) (while continue ...))
    // and also using the (prog1 body (while cond body)) pattern
    let form = r#"(list
  ;; Pattern 1: explicit flag for do-while
  ;; Find next power of 2 >= n
  (let ((results nil))
    (dolist (n '(1 5 7 8 15 16 17 100 1024))
      (let ((power 1)
            (first-time t))
        (while (or first-time (< power n))
          (setq first-time nil)
          (when (< power n)
            (setq power (* power 2))))
        (setq results (cons (cons n power) results))))
    (nreverse results))

  ;; Pattern 2: do-while for digit extraction (works even for n=0)
  (let ((results nil))
    (dolist (n '(0 5 42 100 9999 12345))
      (let ((digits nil)
            (num n)
            (first-time t))
        (while (or first-time (> num 0))
          (setq first-time nil)
          (setq digits (cons (% num 10) digits))
          (setq num (/ num 10)))
        (setq results (cons (cons n digits) results))))
    (nreverse results))

  ;; Pattern 3: do-while for GCD (Euclidean algorithm)
  (let ((results nil))
    (dolist (pair '((48 18) (100 75) (17 13) (0 5) (12 12) (1071 462)))
      (let ((a (car pair))
            (b (cadr pair))
            (steps 0))
        (if (= b 0)
            (setq results (cons (list pair a steps) results))
          (let ((continue t))
            (while continue
              (let ((temp (% a b)))
                (setq a b b temp steps (1+ steps))
                (when (= b 0) (setq continue nil))))
            (setq results (cons (list pair a steps) results))))))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 1) (5 . 8) (7 . 8) (8 . 8) (15 . 16) (16 . 16) (17 . 32) (100 . 128) (1024 . 1024)) ((0 0) (5 5) (42 4 2) (100 1 0 0) (9999 9 9 9 9) (12345 1 2 3 4 5)) (((48 18) 6 3) ((100 75) 25 2) ((17 13) 1 3) ((0 5) 5 1) ((12 12) 12 1) ((1071 462) 21 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// For-each simulation with index tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_loop_foreach_with_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate enumerate/zip patterns using while loops with index counters
    let form = r#"(list
  ;; Enumerate: produce (index . element) pairs
  (let ((items '(apple banana cherry date elderberry))
        (remaining nil)
        (idx 0)
        (result nil))
    (setq remaining items)
    (while remaining
      (setq result (cons (cons idx (car remaining)) result))
      (setq idx (1+ idx))
      (setq remaining (cdr remaining)))
    (nreverse result))

  ;; Zip: merge two lists element-by-element (stop at shorter)
  (let ((keys '(name age city country))
        (vals '("Alice" 30 "Paris" "France" "extra"))
        (k-rem nil) (v-rem nil)
        (result nil))
    (setq k-rem keys v-rem vals)
    (while (and k-rem v-rem)
      (setq result (cons (cons (car k-rem) (car v-rem)) result))
      (setq k-rem (cdr k-rem))
      (setq v-rem (cdr v-rem)))
    (nreverse result))

  ;; Sliding window: compute moving average of window size 3
  (let ((data '(10 20 30 40 50 60 70 80))
        (window-size 3)
        (idx 0)
        (len 8)
        (averages nil))
    (while (<= (+ idx window-size) len)
      (let ((sum 0) (j 0)
            (sublist (nthcdr idx data)))
        (while (< j window-size)
          (setq sum (+ sum (car sublist)))
          (setq sublist (cdr sublist))
          (setq j (1+ j)))
        (setq averages (cons (/ (float sum) window-size) averages)))
      (setq idx (1+ idx)))
    (nreverse averages)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . apple) (1 . banana) (2 . cherry) (3 . date) (4 . elderberry)) ((name . \"Alice\") (age . 30) (city . \"Paris\") (country . \"France\")) (20.0 30.0 40.0 50.0 60.0 70.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Infinite loop with catch-based exit (event loop simulation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_loop_infinite_with_catch_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate an event loop that processes a queue of events
    // until it receives a 'quit event, using catch/throw for clean exit
    let form = r#"(progn
  (fset 'neovm--wlp-process-events
    (lambda (event-queue)
      (let ((log nil)
            (state (make-hash-table))
            (events event-queue)
            (tick 0))
        (puthash 'counter 0 state)
        (puthash 'status 'running state)
        (catch 'quit-loop
          (while t  ;; infinite loop
            (if (null events)
                (throw 'quit-loop 'queue-exhausted)
              (let ((event (car events)))
                (setq events (cdr events))
                (setq tick (1+ tick))
                (cond
                 ((eq event 'increment)
                  (puthash 'counter (1+ (gethash 'counter state)) state)
                  (setq log (cons (list tick 'inc (gethash 'counter state)) log)))
                 ((eq event 'decrement)
                  (puthash 'counter (1- (gethash 'counter state)) state)
                  (setq log (cons (list tick 'dec (gethash 'counter state)) log)))
                 ((eq event 'reset)
                  (puthash 'counter 0 state)
                  (setq log (cons (list tick 'reset 0) log)))
                 ((eq event 'quit)
                  (puthash 'status 'stopped state)
                  (setq log (cons (list tick 'quit) log))
                  (throw 'quit-loop 'clean-exit))
                 (t
                  (setq log (cons (list tick 'unknown event) log))))))))
        (list 'counter (gethash 'counter state)
              'status (gethash 'status state)
              'ticks tick
              'log (nreverse log)))))
  (unwind-protect
      (list
       ;; Normal operation with quit
       (funcall 'neovm--wlp-process-events
                '(increment increment increment decrement quit increment))
       ;; Queue exhaustion (no quit event)
       (funcall 'neovm--wlp-process-events
                '(increment increment reset increment increment))
       ;; Immediate quit
       (funcall 'neovm--wlp-process-events '(quit))
       ;; Empty queue
       (funcall 'neovm--wlp-process-events nil)
       ;; Unknown events mixed in
       (funcall 'neovm--wlp-process-events
                '(increment bogus decrement fizz quit)))
    (fmakunbound 'neovm--wlp-process-events)))"#;
    let expect = expect_test::expect![[
        r#""OK ((counter 2 status stopped ticks 5 log ((1 inc 1) (2 inc 2) (3 inc 3) (4 dec 2) (5 quit))) (counter 2 status running ticks 5 log ((1 inc 1) (2 inc 2) (3 reset 0) (4 inc 1) (5 inc 2))) (counter 0 status stopped ticks 1 log ((1 quit))) (counter 0 status running ticks 0 log nil) (counter 0 status stopped ticks 5 log ((1 inc 1) (2 unknown bogus) (3 dec 0) (4 unknown fizz) (5 quit))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// While with hash-table iteration and convergence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_while_loop_pagerank_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple PageRank-like iterative computation using while loop
    // until convergence or max iterations
    let form = r#"(let* ((damping 0.85)
          (pages '(a b c))
          (links (make-hash-table))  ;; page -> list of outgoing links
          (ranks (make-hash-table))  ;; page -> current rank
          (new-ranks (make-hash-table))
          (converged nil)
          (iterations 0)
          (max-iter 20)
          (epsilon 0.001))
  ;; Setup link graph: A->B, A->C, B->C, C->A
  (puthash 'a '(b c) links)
  (puthash 'b '(c) links)
  (puthash 'c '(a) links)
  ;; Initialize ranks to 1/N
  (let ((n (length pages)))
    (dolist (p pages)
      (puthash p (/ 1.0 n) ranks)))
  ;; Iterate until convergence
  (while (and (not converged) (< iterations max-iter))
    (setq iterations (1+ iterations))
    ;; Compute new ranks
    (dolist (p pages)
      (puthash p (/ (- 1.0 damping) (length pages)) new-ranks))
    ;; Distribute rank through links
    (dolist (p pages)
      (let* ((out-links (gethash p links))
             (share (/ (* damping (gethash p ranks)) (length out-links))))
        (dolist (target out-links)
          (puthash target (+ (gethash target new-ranks) share) new-ranks))))
    ;; Check convergence
    (let ((max-diff 0.0))
      (dolist (p pages)
        (let ((diff (abs (- (gethash p new-ranks) (gethash p ranks)))))
          (when (> diff max-diff) (setq max-diff diff))))
      (when (< max-diff epsilon)
        (setq converged t)))
    ;; Copy new-ranks to ranks
    (dolist (p pages)
      (puthash p (gethash p new-ranks) ranks)))
  ;; Collect results sorted by rank (descending)
  (let ((result nil))
    (dolist (p pages)
      (setq result (cons (list p (gethash p ranks)) result)))
    (setq result (sort result (lambda (a b) (> (cadr a) (cadr b)))))
    (list 'converged converged
          'iterations iterations
          'rankings result)))"#;
    let expect = expect_test::expect![[
        r#""OK (converged t iterations 12 rankings ((c 0.39754204999713316) (a 0.3879107424975632) (b 0.21454720750530354)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
