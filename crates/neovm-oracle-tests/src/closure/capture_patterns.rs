//! Oracle parity tests for closure capture semantics with complex patterns:
//! lexical variable capture, shared mutable state between closures, nested
//! closures returning closures, rest arguments in closures, iterator
//! implementations, memoization, and event handler registration.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Closures capturing lexical variables with deep nesting and shadowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_capture_lexical_deep_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three levels of let-binding with shadowing; closures capture from
    // different levels and we verify each sees the correct binding.
    let form = r#"(let ((a 1) (b 2) (c 3))
      (let ((f1 (lambda () (list a b c))))
        (let ((a 10) (d 40))
          (let ((f2 (lambda () (list a b c d))))
            (let ((a 100) (b 200) (c 300) (d 400))
              (let ((f3 (lambda () (list a b c d))))
                (list
                  (funcall f1)   ;; sees a=1 b=2 c=3
                  (funcall f2)   ;; sees a=10 b=2 c=3 d=40
                  (funcall f3)   ;; sees a=100 b=200 c=300 d=400
                  ;; Verify independence
                  (funcall f1)
                  (funcall f2))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3) (10 2 3 40) (100 200 300 400) (1 2 3) (10 2 3 40))""#
    ]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        "((1 2 3) (10 2 3 40) (100 200 300 400) (1 2 3) (10 2 3 40))",
        &o,
        &n,
    );
}

// ---------------------------------------------------------------------------
// Multiple closures sharing and mutating the same captured variable
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_shared_mutable_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A log system: multiple closures (add, get, clear, search) share
    // the same mutable log list. Mutations from one closure are visible
    // to all others.
    let form = r#"(let ((log nil))
      (let ((add-entry
              (lambda (level msg)
                (setq log (cons (list level msg) log))
                (length log)))
            (get-log (lambda () (reverse log)))
            (clear-log (lambda () (setq log nil) t))
            (search-log
              (lambda (level)
                (let ((result nil))
                  (dolist (entry (reverse log))
                    (when (eq (car entry) level)
                      (setq result (cons (cadr entry) result))))
                  (nreverse result)))))
        (funcall add-entry 'info "started")
        (funcall add-entry 'warn "low memory")
        (funcall add-entry 'info "processing")
        (funcall add-entry 'error "disk full")
        (funcall add-entry 'info "retrying")
        (let ((all (funcall get-log))
              (infos (funcall search-log 'info))
              (warns (funcall search-log 'warn))
              (errors (funcall search-log 'error)))
          (funcall clear-log)
          (list
            (length all)
            infos
            warns
            errors
            (funcall get-log)   ;; empty after clear
            (funcall add-entry 'info "restarted")
            (funcall get-log)))))"#;
    let expect = expect_test::expect![[
        r#""OK (5 (\"started\" \"processing\" \"retrying\") (\"low memory\") (\"disk full\") nil 1 ((info \"restarted\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure mutation of captured variable via setq
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_mutation_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An accumulator closure that tracks running sum, count, min, and max.
    let form = r#"(let ((total 0) (count 0) (lo nil) (hi nil))
      (let ((add (lambda (x)
                   (setq total (+ total x))
                   (setq count (1+ count))
                   (when (or (null lo) (< x lo)) (setq lo x))
                   (when (or (null hi) (> x hi)) (setq hi x))
                   nil))
            (stats (lambda ()
                     (list total count lo hi
                           (if (> count 0)
                               (/ (float total) count)
                             0.0)))))
        (dolist (v '(10 3 7 -2 15 8 1))
          (funcall add v))
        (let ((s1 (funcall stats)))
          (funcall add 100)
          (funcall add -50)
          (let ((s2 (funcall stats)))
            (list s1 s2)))))"#;
    let expect =
        expect_test::expect![r#""OK ((42 7 -2 15 6.0) (92 9 -50 100 10.222222222222221))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested closures: closure returning closure (currying, composition)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_nested_composition_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a pipeline of transformations using closures that return closures.
    // compose returns a closure that applies g then f.
    // pipe chains multiple transformations.
    let form = r#"(let ((compose
                     (lambda (f g)
                       (lambda (x) (funcall f (funcall g x)))))
                    (make-adder
                     (lambda (n) (lambda (x) (+ x n))))
                    (make-multiplier
                     (lambda (n) (lambda (x) (* x n)))))
      ;; Build composed functions
      (let ((add5-then-double
              (funcall compose
                       (funcall make-multiplier 2)
                       (funcall make-adder 5)))
            (double-then-add5
              (funcall compose
                       (funcall make-adder 5)
                       (funcall make-multiplier 2)))
            ;; Triple composition: add 1, multiply by 3, add 10
            (triple-comp
              (funcall compose
                       (funcall make-adder 10)
                       (funcall compose
                                (funcall make-multiplier 3)
                                (funcall make-adder 1)))))
        (list
          ;; (3 + 5) * 2 = 16
          (funcall add5-then-double 3)
          ;; (3 * 2) + 5 = 11
          (funcall double-then-add5 3)
          ;; ((2 + 1) * 3) + 10 = 19
          (funcall triple-comp 2)
          ;; Map composed function over a list
          (mapcar add5-then-double '(0 1 2 3 4))
          ;; Verify independence of closures
          (funcall (funcall make-adder 100) 1)
          (funcall (funcall make-multiplier 100) 2))))"#;
    let expect = expect_test::expect![[r#""OK (16 11 19 (10 12 14 16 18) 101 200)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(16 11 19 (10 12 14 16 18) 101 200)", &o, &n);
}

// ---------------------------------------------------------------------------
// Closures with rest arguments (&rest) and optional (&optional)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_rest_and_optional_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test closures with various argument patterns including &rest
    // and &optional, combined with captured variables.
    let form = r#"(let ((prefix ">>")
                    (suffix "<<"))
      (let ((wrap
              (lambda (s &optional left right)
                (concat (or left prefix) s (or right suffix))))
            (join-all
              (lambda (sep &rest parts)
                (mapconcat #'identity parts sep)))
            (format-entry
              (lambda (key &rest values)
                (concat (symbol-name key) "="
                        (mapconcat (lambda (v)
                                    (cond ((stringp v) v)
                                          ((numberp v) (number-to-string v))
                                          ((symbolp v) (symbol-name v))
                                          (t "?")))
                                  values ","))))
            (variadic-math
              (lambda (op &rest nums)
                (cond
                  ((eq op 'sum) (apply #'+ nums))
                  ((eq op 'product) (apply #'* nums))
                  ((eq op 'max-val)
                   (let ((m (car nums)))
                     (dolist (n (cdr nums)) (when (> n m) (setq m n)))
                     m))
                  ((eq op 'min-val)
                   (let ((m (car nums)))
                     (dolist (n (cdr nums)) (when (< n m) (setq m n)))
                     m))))))
        (list
          ;; wrap with defaults
          (funcall wrap "hello")
          ;; wrap with overrides
          (funcall wrap "hello" "[" "]")
          ;; wrap with only left override
          (funcall wrap "hello" "(" nil)
          ;; join-all
          (funcall join-all ", " "a" "b" "c")
          (funcall join-all "-" "x" "y")
          ;; format-entry
          (funcall format-entry 'name "Alice" "Bob")
          (funcall format-entry 'score 100 200 300)
          ;; variadic-math
          (funcall variadic-math 'sum 1 2 3 4 5)
          (funcall variadic-math 'product 2 3 4)
          (funcall variadic-math 'max-val 3 7 2 9 1)
          (funcall variadic-math 'min-val 3 7 2 9 1))))"#;
    let expect = expect_test::expect![[
        r#""OK (\">>hello<<\" \"[hello]\" \"(hello<<\" \"a, b, c\" \"x-y\" \"name=Alice,Bob\" \"score=100,200,300\" 15 24 9 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: implementing iterators with closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_iterator_implementation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement stateful iterators using closures. Each call to the
    // iterator returns the next value or 'done.
    let form = r#"(progn
  ;; List iterator
  (fset 'neovm--make-list-iter
    (lambda (lst)
      (let ((remaining lst))
        (lambda ()
          (if remaining
              (let ((val (car remaining)))
                (setq remaining (cdr remaining))
                val)
            'done)))))

  ;; Range iterator: from START below END with step STEP
  (fset 'neovm--make-range-iter
    (lambda (start end &optional step)
      (let ((current start)
            (s (or step 1)))
        (lambda ()
          (if (< current end)
              (let ((val current))
                (setq current (+ current s))
                val)
            'done)))))

  ;; Map iterator: applies FN to each value from ITER
  (fset 'neovm--make-map-iter
    (lambda (fn iter)
      (lambda ()
        (let ((val (funcall iter)))
          (if (eq val 'done) 'done
            (funcall fn val))))))

  ;; Filter iterator: keeps only values where PRED returns non-nil
  (fset 'neovm--make-filter-iter
    (lambda (pred iter)
      (lambda ()
        (let ((result 'done) (searching t))
          (while searching
            (let ((val (funcall iter)))
              (cond
                ((eq val 'done)
                 (setq searching nil))
                ((funcall pred val)
                 (setq result val)
                 (setq searching nil)))))
          result))))

  ;; Collect all values from iterator into a list
  (fset 'neovm--iter-collect
    (lambda (iter)
      (let ((result nil) (val nil))
        (setq val (funcall iter))
        (while (not (eq val 'done))
          (setq result (cons val result))
          (setq val (funcall iter)))
        (nreverse result))))

  (unwind-protect
      (list
        ;; Basic list iterator
        (funcall 'neovm--iter-collect
                 (funcall 'neovm--make-list-iter '(a b c d e)))
        ;; Range iterator
        (funcall 'neovm--iter-collect
                 (funcall 'neovm--make-range-iter 0 5))
        ;; Range with step
        (funcall 'neovm--iter-collect
                 (funcall 'neovm--make-range-iter 0 10 3))
        ;; Map over range: square each number 0..4
        (funcall 'neovm--iter-collect
                 (funcall 'neovm--make-map-iter
                          (lambda (x) (* x x))
                          (funcall 'neovm--make-range-iter 0 5)))
        ;; Filter: even numbers from 0..9
        (funcall 'neovm--iter-collect
                 (funcall 'neovm--make-filter-iter
                          (lambda (x) (= (% x 2) 0))
                          (funcall 'neovm--make-range-iter 0 10)))
        ;; Chained: range 1..10, filter odd, map to square
        (funcall 'neovm--iter-collect
                 (funcall 'neovm--make-map-iter
                          (lambda (x) (* x x))
                          (funcall 'neovm--make-filter-iter
                                   (lambda (x) (= (% x 2) 1))
                                   (funcall 'neovm--make-range-iter 1 10))))
        ;; Empty iterator
        (funcall 'neovm--iter-collect
                 (funcall 'neovm--make-list-iter nil)))
    (fmakunbound 'neovm--make-list-iter)
    (fmakunbound 'neovm--make-range-iter)
    (fmakunbound 'neovm--make-map-iter)
    (fmakunbound 'neovm--make-filter-iter)
    (fmakunbound 'neovm--iter-collect)))"#;
    let expect = expect_test::expect![
        r#""OK ((a b c d e) (0 1 2 3 4) (0 3 6 9) (0 1 4 9 16) (0 2 4 6 8) (1 9 25 49 81) nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: memoization using closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_memoization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement memoization: a closure wraps a function, caching results
    // in a hash table captured in its environment.
    let form = r#"(progn
  ;; Memoize a single-argument function using a hash table
  (fset 'neovm--memoize
    (lambda (fn)
      (let ((cache (make-hash-table :test 'equal))
            (call-count 0)
            (hit-count 0))
        (list
          ;; The memoized function
          (lambda (arg)
            (setq call-count (1+ call-count))
            (let ((cached (gethash arg cache 'neovm--miss)))
              (if (not (eq cached 'neovm--miss))
                  (progn (setq hit-count (1+ hit-count)) cached)
                (let ((result (funcall fn arg)))
                  (puthash arg result cache)
                  result))))
          ;; Stats accessor
          (lambda () (list call-count hit-count (hash-table-count cache)))))))

  (unwind-protect
      (let* ((fib-calls 0)
             ;; Naive recursive fib (wrapped for counting)
             (pair (funcall 'neovm--memoize
                            (lambda (n)
                              (setq fib-calls (1+ fib-calls))
                              (if (<= n 1) n
                                (+ (funcall memo-fib (- n 1))
                                   (funcall memo-fib (- n 2)))))))
             (memo-fib (car pair))
             (fib-stats (cadr pair)))
        ;; Compute fibonacci with memoization
        (let ((fib-10 (funcall memo-fib 10))
              (stats-after-10 (funcall fib-stats)))
          ;; Calling again should be all cache hits
          (funcall memo-fib 10)
          (funcall memo-fib 5)
          (funcall memo-fib 8)
          (let ((stats-after-repeats (funcall fib-stats)))
            (list fib-10
                  stats-after-10
                  stats-after-repeats
                  ;; Verify correctness
                  (mapcar memo-fib '(0 1 2 3 4 5 6 7 8 9 10))))))
    (fmakunbound 'neovm--memoize)))"#;
    let expect = expect_test::expect![r#""ERR (void-variable memo-fib)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: event handler registration with closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_event_handler_registration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An event bus: register handlers (closures) for named events,
    // then dispatch events. Each handler captures its own context.
    let form = r#"(let ((handlers nil)
                    (event-log nil))
      (let ((on (lambda (event-name handler)
                  (let ((existing (assq event-name handlers)))
                    (if existing
                        (setcdr existing (cons handler (cdr existing)))
                      (setq handlers (cons (list event-name handler) handlers))))))
            (emit (lambda (event-name &rest data)
                    (setq event-log (cons (list event-name data) event-log))
                    (let ((entry (assq event-name handlers))
                          (results nil))
                      (when entry
                        (dolist (handler (cdr entry))
                          (setq results (cons (apply handler data) results))))
                      (nreverse results))))
            (get-log (lambda () (reverse event-log))))

        ;; Register handlers that capture their own state
        (let ((click-count 0)
              (total-keys 0))
          ;; Two click handlers with shared click-count
          (funcall on 'click
                   (lambda (x y)
                     (setq click-count (1+ click-count))
                     (list 'click-at x y click-count)))
          (funcall on 'click
                   (lambda (x y)
                     (list 'distance (+ (abs x) (abs y)))))

          ;; Key handler
          (funcall on 'key
                   (lambda (ch)
                     (setq total-keys (1+ total-keys))
                     (list 'key ch total-keys)))

          ;; Emit events
          (let ((r1 (funcall emit 'click 10 20))
                (r2 (funcall emit 'click 5 -3))
                (r3 (funcall emit 'key ?a))
                (r4 (funcall emit 'key ?b))
                (r5 (funcall emit 'click 0 0))
                (r6 (funcall emit 'unknown)))
            (list r1 r2 r3 r4 r5 r6
                  click-count total-keys
                  (length (funcall get-log)))))))"#;
    let expect = expect_test::expect![
        r#""OK (((distance 30) (click-at 10 20 1)) ((distance 8) (click-at 5 -3 2)) ((key 97 1)) ((key 98 2)) ((distance 0) (click-at 0 0 3)) nil 3 2 6)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closures capturing loop variables (let-over-lambda per iteration)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_capture_loop_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A common gotcha: closures created in a loop should each capture
    // their own copy of the loop variable (with let binding each iteration).
    let form = r#"(let ((closures nil))
      ;; Build closures that each capture a distinct value of i
      (let ((i 0))
        (while (< i 5)
          (let ((captured i))
            (setq closures (cons (lambda () captured) closures)))
          (setq i (1+ i))))
      ;; Call them: should get (4 3 2 1 0) because cons reverses order
      (let ((results (mapcar #'funcall closures)))
        ;; Also test with dotimes
        (let ((closures2 nil))
          (dotimes (j 5)
            (let ((captured j))
              (setq closures2 (cons (lambda () (* captured captured)) closures2))))
          (list results
                (mapcar #'funcall closures2)
                ;; Test: closures that return closures, each capturing iteration var
                (let ((makers nil))
                  (dotimes (k 3)
                    (let ((captured k))
                      (setq makers (cons (lambda (x) (+ x captured)) makers))))
                  (mapcar (lambda (f) (funcall f 100)) makers))))))"#;
    let expect = expect_test::expect![r#""OK ((4 3 2 1 0) (16 9 4 1 0) (102 101 100))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
