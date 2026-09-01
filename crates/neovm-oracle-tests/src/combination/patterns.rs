//! Oracle parity tests for mixed Elisp design patterns:
//! observer, strategy, builder, memoization, decorator, pipeline.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 1. Observer pattern: register callbacks, notify on change
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_observer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An observable value with a list of watchers. Setting the value
    // notifies all watchers, which accumulate a log.
    let form = r#"(let ((watchers nil)
       (log nil)
       (current-value 0))
  (let ((add-watcher
         (lambda (name fn)
           (setq watchers (cons (cons name fn) watchers))))
        (set-value
         (lambda (new-val)
           (let ((old current-value))
             (setq current-value new-val)
             (dolist (w watchers)
               (funcall (cdr w) (car w) old new-val)))))
        (remove-watcher
         (lambda (name)
           (setq watchers
                 (let ((result nil))
                   (dolist (w watchers)
                     (unless (eq (car w) name)
                       (setq result (cons w result))))
                   (nreverse result))))))
    ;; Register watchers
    (funcall add-watcher 'logger
             (lambda (name old new)
               (setq log (cons (list name old '-> new) log))))
    (funcall add-watcher 'validator
             (lambda (name old new)
               (when (< new 0)
                 (setq log (cons (list name 'negative-warning new) log)))))
    (funcall add-watcher 'counter
             (lambda (name old new)
               (setq log (cons (list name 'change-count
                                      (length log)) log))))
    ;; Trigger changes
    (funcall set-value 10)
    (funcall set-value 20)
    ;; Remove validator, then set negative
    (funcall remove-watcher 'validator)
    (funcall set-value -5)
    (list current-value
          (length watchers)
          (nreverse log))))"#;
    let expect = expect_test::expect![[
        r#""OK (-5 2 ((counter change-count 0) (logger 0 -> 10) (counter change-count 2) (logger 10 -> 20) (counter change-count 4) (logger 20 -> -5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2. Strategy pattern: pass comparison function to sort/filter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_strategy_sort_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use different comparison strategies to sort and filter data.
    let form = r#"(let ((data '(5 3 8 1 9 2 7 4 6 10)))
  ;; Strategy 1: sort ascending, filter evens
  (let* ((asc-sorted (sort (copy-sequence data) #'<))
         (evens (let ((result nil))
                  (dolist (x asc-sorted)
                    (when (= 0 (% x 2))
                      (setq result (cons x result))))
                  (nreverse result))))
    ;; Strategy 2: sort descending, filter > 5
    (let* ((desc-sorted (sort (copy-sequence data) #'>))
           (big (let ((result nil))
                  (dolist (x desc-sorted)
                    (when (> x 5)
                      (setq result (cons x result))))
                  (nreverse result))))
      ;; Strategy 3: sort by distance from 5
      (let* ((dist-sorted (sort (copy-sequence data)
                                (lambda (a b)
                                  (< (abs (- a 5)) (abs (- b 5)))))))
        (list evens big dist-sorted)))))"#;
    let expect =
        expect_test::expect![[r#""OK ((2 4 6 8 10) (10 9 8 7 6) (5 4 6 3 7 8 2 1 9 10))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 3. Builder pattern: chain of setcdr to build list incrementally
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_builder_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a list incrementally using a head sentinel and a tail pointer.
    // Mimics a "builder" pattern for efficient append.
    let form = r#"(let* ((sentinel (list 'head))
        (tail sentinel))
  ;; Append elements one by one
  (dolist (item '(alpha beta gamma delta epsilon))
    (let ((new-cell (list item)))
      (setcdr tail new-cell)
      (setq tail new-cell)))
  ;; Conditionally append based on a predicate
  (let ((numbers '(1 2 3 4 5 6 7 8 9 10)))
    (dolist (n numbers)
      (when (= 0 (% n 3))
        (let ((new-cell (list (list 'div3 n))))
          (setcdr tail new-cell)
          (setq tail new-cell)))))
  ;; Build nested structure
  (let ((sub-sentinel (list 'sub-head))
        (sub-tail nil))
    (setq sub-tail sub-sentinel)
    (dotimes (i 4)
      (let ((new-cell (list (* i i))))
        (setcdr sub-tail new-cell)
        (setq sub-tail new-cell)))
    ;; Append the sub-list as a single nested element
    (setcdr tail (list (cdr sub-sentinel))))
  ;; Return everything after the sentinel
  (cdr sentinel))"#;
    let expect = expect_test::expect![[
        r#""OK (alpha beta gamma delta epsilon (div3 3) (div3 6) (div3 9) (0 1 4 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 4. Memoization: hash-table cache wrapping a recursive function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_memoized_fibonacci() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Memoized Fibonacci with a closure over the cache.
    // Also counts cache hits vs misses.
    let form = r#"(progn
  (defvar neovm--test-fib-cache (make-hash-table))
  (defvar neovm--test-fib-hits 0)
  (defvar neovm--test-fib-misses 0)
  (fset 'neovm--test-memo-fib
    (lambda (n)
      (let ((cached (gethash n neovm--test-fib-cache)))
        (if cached
            (progn
              (setq neovm--test-fib-hits (1+ neovm--test-fib-hits))
              cached)
          (setq neovm--test-fib-misses (1+ neovm--test-fib-misses))
          (let ((result
                 (cond
                   ((= n 0) 0)
                   ((= n 1) 1)
                   (t (+ (funcall 'neovm--test-memo-fib (- n 1))
                         (funcall 'neovm--test-memo-fib (- n 2)))))))
            (puthash n result neovm--test-fib-cache)
            result)))))
  (unwind-protect
      (let ((fibs nil))
        (dolist (n '(0 1 2 5 10 15 20))
          (setq fibs (cons (funcall 'neovm--test-memo-fib n) fibs)))
        (list (nreverse fibs)
              (list 'hits neovm--test-fib-hits
                    'misses neovm--test-fib-misses)
              (hash-table-count neovm--test-fib-cache)))
    (fmakunbound 'neovm--test-memo-fib)
    (makunbound 'neovm--test-fib-cache)
    (makunbound 'neovm--test-fib-hits)
    (makunbound 'neovm--test-fib-misses)))"#;
    let expect = expect_test::expect![[r#""OK ((0 1 1 5 55 610 6765) (hits 24 misses 21) 21)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 5. Decorator pattern: wrapping function behavior with logging/timing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_decorator_wrapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build decorators as higher-order functions that wrap a base function.
    // Stack multiple decorators: logging, argument validation, result caching.
    let form = r#"(let ((call-log nil))
  ;; Logging decorator
  (let ((make-logged
         (lambda (name fn)
           (lambda (&rest args)
             (setq call-log (cons (list 'call name args) call-log))
             (let ((result (apply fn args)))
               (setq call-log (cons (list 'return name result) call-log))
               result))))
        ;; Validation decorator: ensure all args are numbers
        (make-validated
         (lambda (fn)
           (lambda (&rest args)
             (dolist (a args)
               (unless (numberp a)
                 (error "Non-numeric argument: %S" a)))
             (apply fn args))))
        ;; Caching decorator
        (make-cached
         (lambda (fn)
           (let ((cache (make-hash-table :test 'equal)))
             (lambda (&rest args)
               (let ((cached (gethash args cache)))
                 (or cached
                     (let ((result (apply fn args)))
                       (puthash args result cache)
                       result))))))))
    ;; Base function: sum of squares
    (let* ((sum-sq (lambda (a b) (+ (* a a) (* b b))))
           ;; Stack decorators: validate -> log -> cache -> base
           (decorated (funcall make-logged 'sum-sq
                        (funcall make-cached
                          (funcall make-validated sum-sq)))))
      (list
        (funcall decorated 3 4)
        (funcall decorated 5 12)
        ;; This should hit the cache (no extra log entries for inner call)
        (funcall decorated 3 4)
        (length (nreverse call-log))))))"#;
    let expect = expect_test::expect![[r#""OK (25 169 25 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 6. Pipeline: compose multiple transformation functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_pipeline_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a data processing pipeline as a list of functions.
    // Apply them in sequence, collecting intermediate results.
    let form = r#"(let* ((pipeline
          (list
            ;; Stage 1: generate range 1..10
            (lambda (_) (let ((r nil)) (dotimes (i 10) (setq r (cons (1+ i) r))) (nreverse r)))
            ;; Stage 2: square each element
            (lambda (lst) (mapcar (lambda (x) (* x x)) lst))
            ;; Stage 3: filter to keep only those > 20
            (lambda (lst)
              (let ((result nil))
                (dolist (x lst)
                  (when (> x 20) (setq result (cons x result))))
                (nreverse result)))
            ;; Stage 4: compute running sum
            (lambda (lst)
              (let ((sum 0) (result nil))
                (dolist (x lst)
                  (setq sum (+ sum x))
                  (setq result (cons sum result)))
                (nreverse result)))
            ;; Stage 5: pair each with its index
            (lambda (lst)
              (let ((result nil) (i 0))
                (dolist (x lst)
                  (setq result (cons (cons i x) result))
                  (setq i (1+ i)))
                (nreverse result)))))
        ;; Execute pipeline, collecting snapshots at each stage
        (snapshots nil)
        (current nil))
  (dolist (stage pipeline)
    (setq current (funcall stage current))
    (setq snapshots (cons current snapshots)))
  (nreverse snapshots))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9 10) (1 4 9 16 25 36 49 64 81 100) (25 36 49 64 81 100) (25 61 110 174 255 355) ((0 . 25) (1 . 61) (2 . 110) (3 . 174) (4 . 255) (5 . 355)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 7. Topological sort via DFS on a DAG
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_topological_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
  (fset 'neovm--test-topo-visit
    (lambda (node graph visited result)
      (unless (gethash node visited)
        (puthash node t visited)
        (dolist (dep (cdr (assq node graph)))
          (funcall 'neovm--test-topo-visit
                   dep graph visited result))
        (setcar result (cons node (car result))))))
  (unwind-protect
      (let ((graph '((a . (b c)) (b . (d)) (c . (d)) (d . ())))
            (visited (make-hash-table))
            (result (list nil)))
        (dolist (node '(a b c d))
          (funcall 'neovm--test-topo-visit
                   node graph visited result))
        (car result))
    (fmakunbound 'neovm--test-topo-visit)))";
    let expect = expect_test::expect![[r#""OK (a c b d)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 8. Lazy sequence via closures (infinite generator)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_lazy_filtered_generator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a lazy sequence that generates Fizzbuzz values, take first N.
    let form = r#"(let* ((make-counter
          (lambda (start)
            (let ((n start))
              (lambda ()
                (prog1 n (setq n (1+ n)))))))
        ;; Fizzbuzz mapper
        (fizzbuzz
         (lambda (gen)
           (lambda ()
             (let ((n (funcall gen)))
               (cond
                 ((= 0 (% n 15)) (list n 'fizzbuzz))
                 ((= 0 (% n 3))  (list n 'fizz))
                 ((= 0 (% n 5))  (list n 'buzz))
                 (t               (list n n)))))))
        ;; Filter: keep only fizz/buzz/fizzbuzz entries
        (make-filter
         (lambda (pred gen)
           (lambda ()
             (let ((val nil) (found nil))
               (while (not found)
                 (setq val (funcall gen))
                 (when (funcall pred val)
                   (setq found t)))
               val))))
        ;; Take N from generator
        (take-n
         (lambda (gen n)
           (let ((result nil))
             (dotimes (_ n)
               (setq result (cons (funcall gen) result)))
             (nreverse result)))))
  ;; Pipeline: counter -> fizzbuzz -> filter non-numbers -> take 8
  (let* ((counter (funcall make-counter 1))
         (fb (funcall fizzbuzz counter))
         (filtered (funcall make-filter
                     (lambda (entry) (symbolp (cadr entry)))
                     fb)))
    (funcall take-n filtered 8)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 fizz) (5 buzz) (6 fizz) (9 fizz) (10 buzz) (12 fizz) (15 fizzbuzz) (18 fizz))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 9. Command pattern with undo
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pattern_command_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((state 0)
                      (undo-stack nil))
                  (let ((execute
                         (lambda (cmd)
                           (let ((old state))
                             (cond
                               ((eq (car cmd) 'add)
                                (setq state (+ state (cadr cmd))))
                               ((eq (car cmd) 'mul)
                                (setq state (* state (cadr cmd)))))
                             (setq undo-stack
                                   (cons old undo-stack)))))
                        (undo
                         (lambda ()
                           (when undo-stack
                             (setq state (car undo-stack)
                                   undo-stack (cdr undo-stack))))))
                    (funcall execute '(add 5))
                    (funcall execute '(mul 3))
                    (funcall execute '(add 2))
                    (let ((before-undo state))
                      (funcall undo)
                      (funcall undo)
                      (list before-undo state undo-stack))))";
    let expect = expect_test::expect![[r#""OK (17 5 (0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
