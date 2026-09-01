//! Oracle parity tests for promise/future-like patterns in Elisp:
//! deferred computation (lazy evaluation), promise chains (then/catch),
//! promise.all (wait for all), promise.race (first to resolve),
//! promise combinators, and error propagation through promise chains.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Deferred computation: lazy evaluation with memoization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_promise_deferred_lazy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A "deferred" is a thunk that computes its value at most once.
    // Represents lazy evaluation with memoization. Subsequent forces
    // return the cached result without re-evaluating.
    let form = r#"
(progn
  ;; A deferred is a cons: (status . payload)
  ;; status = 'pending -> payload is thunk
  ;; status = 'resolved -> payload is cached value
  (fset 'neovm--prom-defer
    (lambda (thunk)
      (cons 'pending thunk)))

  (fset 'neovm--prom-force
    (lambda (d)
      (if (eq (car d) 'resolved)
          (cdr d)
        ;; Evaluate thunk and cache
        (let ((val (funcall (cdr d))))
          (setcar d 'resolved)
          (setcdr d val)
          val))))

  (fset 'neovm--prom-forced-p
    (lambda (d) (eq (car d) 'resolved)))

  (fset 'neovm--prom-defer-map
    (lambda (d f)
      (funcall 'neovm--prom-defer
               (lambda () (funcall f (funcall 'neovm--prom-force d))))))

  (unwind-protect
      (let* ((call-count 0)
             ;; Deferred that increments counter (to track evaluations)
             (d1 (funcall 'neovm--prom-defer
                          (lambda () (setq call-count (1+ call-count)) 42)))
             ;; Chain: d1 -> double -> add 1
             (d2 (funcall 'neovm--prom-defer-map d1
                          (lambda (x) (* x 2))))
             (d3 (funcall 'neovm--prom-defer-map d2
                          (lambda (x) (1+ x)))))
        (list
          ;; Before any force
          (list 'before
                (funcall 'neovm--prom-forced-p d1)
                (funcall 'neovm--prom-forced-p d2)
                (funcall 'neovm--prom-forced-p d3)
                'calls call-count)

          ;; Force d3 (should cascade through d2 -> d1)
          (let ((v3 (funcall 'neovm--prom-force d3)))
            (list 'after-force-d3
                  v3
                  (funcall 'neovm--prom-forced-p d1)
                  (funcall 'neovm--prom-forced-p d2)
                  (funcall 'neovm--prom-forced-p d3)
                  'calls call-count))

          ;; Force d1 again (should not increment counter)
          (let ((v1 (funcall 'neovm--prom-force d1)))
            (list 'second-force-d1
                  v1
                  'calls call-count))

          ;; Multiple deferred values with shared dependency
          (let* ((base (funcall 'neovm--prom-defer (lambda () 10)))
                 (branch-a (funcall 'neovm--prom-defer-map base
                                    (lambda (x) (* x x))))
                 (branch-b (funcall 'neovm--prom-defer-map base
                                    (lambda (x) (+ x 5)))))
            (list 'shared
                  (funcall 'neovm--prom-force branch-a)
                  (funcall 'neovm--prom-force branch-b)
                  (funcall 'neovm--prom-force base)))))
    (fmakunbound 'neovm--prom-defer)
    (fmakunbound 'neovm--prom-force)
    (fmakunbound 'neovm--prom-forced-p)
    (fmakunbound 'neovm--prom-defer-map)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((before nil nil nil calls 0) (after-force-d3 85 t t t calls 1) (second-force-d1 42 calls 1) (shared 100 15 10))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Promise chains: then/catch for sequential async-like patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_promise_then_catch_chains() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A promise is (resolved . value) or (rejected . reason).
    // `then` transforms resolved values; `catch` handles rejections.
    // Build chains that mix success and failure paths.
    let form = r#"
(progn
  (fset 'neovm--prom-resolve (lambda (val) (cons 'resolved val)))
  (fset 'neovm--prom-reject (lambda (reason) (cons 'rejected reason)))
  (fset 'neovm--prom-resolved-p (lambda (p) (eq (car p) 'resolved)))
  (fset 'neovm--prom-rejected-p (lambda (p) (eq (car p) 'rejected)))
  (fset 'neovm--prom-value (lambda (p) (cdr p)))

  ;; then: if resolved, apply f (which returns a new promise)
  (fset 'neovm--prom-then
    (lambda (promise f)
      (if (funcall 'neovm--prom-resolved-p promise)
          (funcall f (funcall 'neovm--prom-value promise))
        promise)))

  ;; catch: if rejected, apply handler (which returns a new promise)
  (fset 'neovm--prom-catch
    (lambda (promise handler)
      (if (funcall 'neovm--prom-rejected-p promise)
          (funcall handler (funcall 'neovm--prom-value promise))
        promise)))

  ;; map: like then but wraps result in resolve automatically
  (fset 'neovm--prom-map
    (lambda (promise f)
      (funcall 'neovm--prom-then promise
               (lambda (val) (funcall 'neovm--prom-resolve (funcall f val))))))

  ;; chain: apply a list of then-functions sequentially
  (fset 'neovm--prom-chain
    (lambda (promise fns)
      (let ((current promise))
        (dolist (f fns)
          (setq current (funcall 'neovm--prom-then current f)))
        current)))

  (unwind-protect
      (let ((safe-div (lambda (divisor)
              (lambda (n)
                (if (= divisor 0)
                    (funcall 'neovm--prom-reject
                             (format "division by zero: %d/0" n))
                  (funcall 'neovm--prom-resolve (/ n divisor))))))
            (must-positive (lambda (n)
              (if (> n 0)
                  (funcall 'neovm--prom-resolve n)
                (funcall 'neovm--prom-reject
                         (format "not positive: %d" n))))))
        (list
          ;; Successful chain: 100 -> /5 -> /4 -> result
          (funcall 'neovm--prom-chain
            (funcall 'neovm--prom-resolve 100)
            (list (funcall safe-div 5) (funcall safe-div 4)))

          ;; Chain with failure in middle: 100 -> /5 -> /0 -> /2
          (funcall 'neovm--prom-chain
            (funcall 'neovm--prom-resolve 100)
            (list (funcall safe-div 5) (funcall safe-div 0) (funcall safe-div 2)))

          ;; Catch and recover: 100 -> /0 -> catch(return 1) -> *10
          (funcall 'neovm--prom-map
            (funcall 'neovm--prom-catch
              (funcall 'neovm--prom-then
                (funcall 'neovm--prom-resolve 100)
                (funcall safe-div 0))
              (lambda (reason)
                (funcall 'neovm--prom-resolve 1)))
            (lambda (x) (* x 10)))

          ;; Map over resolved
          (funcall 'neovm--prom-map
            (funcall 'neovm--prom-resolve 7)
            (lambda (x) (* x x)))

          ;; Map over rejected (no change)
          (funcall 'neovm--prom-map
            (funcall 'neovm--prom-reject "already failed")
            (lambda (x) (* x x)))

          ;; Multi-step with positive check
          (funcall 'neovm--prom-chain
            (funcall 'neovm--prom-resolve 100)
            (list (funcall safe-div 5)
                  must-positive
                  (funcall safe-div 2)
                  must-positive))

          ;; Fails at positive check
          (funcall 'neovm--prom-chain
            (funcall 'neovm--prom-resolve 3)
            (list (funcall safe-div 5)   ;; 3/5 = 0 (integer div)
                  must-positive))))       ;; 0 not positive
    (fmakunbound 'neovm--prom-resolve)
    (fmakunbound 'neovm--prom-reject)
    (fmakunbound 'neovm--prom-resolved-p)
    (fmakunbound 'neovm--prom-rejected-p)
    (fmakunbound 'neovm--prom-value)
    (fmakunbound 'neovm--prom-then)
    (fmakunbound 'neovm--prom-catch)
    (fmakunbound 'neovm--prom-map)
    (fmakunbound 'neovm--prom-chain)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((resolved . 5) (rejected . \"division by zero: 20/0\") (resolved . 10) (resolved . 49) (rejected . \"already failed\") (resolved . 10) (rejected . \"not positive: 0\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Promise.all: wait for all promises, short-circuit on first rejection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_promise_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // promise-all takes a list of promises and returns:
    // - (resolved . (list of values)) if ALL resolved
    // - (rejected . first-rejection-reason) if ANY rejected
    let form = r#"
(progn
  (fset 'neovm--pa-resolve (lambda (val) (cons 'resolved val)))
  (fset 'neovm--pa-reject (lambda (reason) (cons 'rejected reason)))

  (fset 'neovm--pa-all
    (lambda (promises)
      (let ((values nil) (failed nil) (fail-reason nil))
        (dolist (p promises)
          (unless failed
            (if (eq (car p) 'resolved)
                (setq values (cons (cdr p) values))
              (setq failed t fail-reason (cdr p)))))
        (if failed
            (funcall 'neovm--pa-reject fail-reason)
          (funcall 'neovm--pa-resolve (nreverse values))))))

  ;; promise-all-settled: always collects all results (never short-circuits)
  (fset 'neovm--pa-all-settled
    (lambda (promises)
      (funcall 'neovm--pa-resolve
               (mapcar (lambda (p)
                         (if (eq (car p) 'resolved)
                             (list 'fulfilled (cdr p))
                           (list 'rejected (cdr p))))
                       promises))))

  (unwind-protect
      (list
        ;; All resolved
        (funcall 'neovm--pa-all
                 (list (funcall 'neovm--pa-resolve 1)
                       (funcall 'neovm--pa-resolve 2)
                       (funcall 'neovm--pa-resolve 3)))

        ;; One rejected (middle)
        (funcall 'neovm--pa-all
                 (list (funcall 'neovm--pa-resolve 1)
                       (funcall 'neovm--pa-reject "fail at 2")
                       (funcall 'neovm--pa-resolve 3)))

        ;; First rejected
        (funcall 'neovm--pa-all
                 (list (funcall 'neovm--pa-reject "fail at 1")
                       (funcall 'neovm--pa-resolve 2)
                       (funcall 'neovm--pa-resolve 3)))

        ;; Empty list
        (funcall 'neovm--pa-all nil)

        ;; Single resolved
        (funcall 'neovm--pa-all (list (funcall 'neovm--pa-resolve 42)))

        ;; Single rejected
        (funcall 'neovm--pa-all (list (funcall 'neovm--pa-reject "oops")))

        ;; Many resolved
        (funcall 'neovm--pa-all
                 (mapcar (lambda (n) (funcall 'neovm--pa-resolve (* n n)))
                         '(1 2 3 4 5 6 7 8 9 10)))

        ;; All-settled: mix of resolved and rejected
        (funcall 'neovm--pa-all-settled
                 (list (funcall 'neovm--pa-resolve 10)
                       (funcall 'neovm--pa-reject "error-a")
                       (funcall 'neovm--pa-resolve 30)
                       (funcall 'neovm--pa-reject "error-b")
                       (funcall 'neovm--pa-resolve 50)))

        ;; All-settled: all resolved
        (funcall 'neovm--pa-all-settled
                 (list (funcall 'neovm--pa-resolve "a")
                       (funcall 'neovm--pa-resolve "b")
                       (funcall 'neovm--pa-resolve "c")))

        ;; All-settled: all rejected
        (funcall 'neovm--pa-all-settled
                 (list (funcall 'neovm--pa-reject "e1")
                       (funcall 'neovm--pa-reject "e2"))))
    (fmakunbound 'neovm--pa-resolve)
    (fmakunbound 'neovm--pa-reject)
    (fmakunbound 'neovm--pa-all)
    (fmakunbound 'neovm--pa-all-settled)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((resolved 1 2 3) (rejected . \"fail at 2\") (rejected . \"fail at 1\") (resolved) (resolved 42) (rejected . \"oops\") (resolved 1 4 9 16 25 36 49 64 81 100) (resolved (fulfilled 10) (rejected \"error-a\") (fulfilled 30) (rejected \"error-b\") (fulfilled 50)) (resolved (fulfilled \"a\") (fulfilled \"b\") (fulfilled \"c\")) (resolved (rejected \"e1\") (rejected \"e2\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Promise.race: first to resolve/reject wins
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_promise_race() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // promise-race returns the first promise in the list (simulating which
    // resolves first). Also implement promise-any (first resolved, ignoring
    // rejections) and promise-none (all must reject).
    let form = r#"
(progn
  (fset 'neovm--pr-resolve (lambda (val) (cons 'resolved val)))
  (fset 'neovm--pr-reject (lambda (reason) (cons 'rejected reason)))

  ;; race: first promise wins (regardless of resolved/rejected)
  (fset 'neovm--pr-race
    (lambda (promises)
      (if promises (car promises) (funcall 'neovm--pr-reject "empty race"))))

  ;; any: first resolved wins, rejects only if all reject
  (fset 'neovm--pr-any
    (lambda (promises)
      (let ((first-resolved nil) (rejections nil))
        (dolist (p promises)
          (if (eq (car p) 'resolved)
              (unless first-resolved
                (setq first-resolved p))
            (setq rejections (cons (cdr p) rejections))))
        (if first-resolved
            first-resolved
          (funcall 'neovm--pr-reject (nreverse rejections))))))

  ;; Simulate priority-based resolution: promises have (priority . promise)
  ;; Race resolves the highest-priority resolved promise
  (fset 'neovm--pr-priority-race
    (lambda (priority-promises)
      (let ((sorted (sort (copy-sequence priority-promises)
                          (lambda (a b) (> (car a) (car b)))))
            (result nil))
        (dolist (pp sorted)
          (unless result
            (when (eq (car (cdr pp)) 'resolved)
              (setq result (cdr pp)))))
        (or result (funcall 'neovm--pr-reject "no resolved promises")))))

  (unwind-protect
      (list
        ;; Race: first is resolved
        (funcall 'neovm--pr-race
                 (list (funcall 'neovm--pr-resolve "fast")
                       (funcall 'neovm--pr-resolve "slow")
                       (funcall 'neovm--pr-reject "failed")))

        ;; Race: first is rejected
        (funcall 'neovm--pr-race
                 (list (funcall 'neovm--pr-reject "fail-fast")
                       (funcall 'neovm--pr-resolve "ok")))

        ;; Race: empty
        (funcall 'neovm--pr-race nil)

        ;; Any: skip rejections, find first resolved
        (funcall 'neovm--pr-any
                 (list (funcall 'neovm--pr-reject "e1")
                       (funcall 'neovm--pr-reject "e2")
                       (funcall 'neovm--pr-resolve "winner")
                       (funcall 'neovm--pr-resolve "also ok")))

        ;; Any: all rejected
        (funcall 'neovm--pr-any
                 (list (funcall 'neovm--pr-reject "e1")
                       (funcall 'neovm--pr-reject "e2")
                       (funcall 'neovm--pr-reject "e3")))

        ;; Any: first is resolved
        (funcall 'neovm--pr-any
                 (list (funcall 'neovm--pr-resolve "immediate")))

        ;; Priority race: highest priority resolved wins
        (funcall 'neovm--pr-priority-race
                 (list (cons 1 (funcall 'neovm--pr-resolve "low"))
                       (cons 3 (funcall 'neovm--pr-resolve "high"))
                       (cons 2 (funcall 'neovm--pr-resolve "medium"))))

        ;; Priority race: highest priority is rejected, next wins
        (funcall 'neovm--pr-priority-race
                 (list (cons 3 (funcall 'neovm--pr-reject "high-fail"))
                       (cons 2 (funcall 'neovm--pr-resolve "medium-ok"))
                       (cons 1 (funcall 'neovm--pr-resolve "low-ok"))))

        ;; Priority race: all rejected
        (funcall 'neovm--pr-priority-race
                 (list (cons 3 (funcall 'neovm--pr-reject "a"))
                       (cons 2 (funcall 'neovm--pr-reject "b")))))
    (fmakunbound 'neovm--pr-resolve)
    (fmakunbound 'neovm--pr-reject)
    (fmakunbound 'neovm--pr-race)
    (fmakunbound 'neovm--pr-any)
    (fmakunbound 'neovm--pr-priority-race)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((resolved . \"fast\") (rejected . \"fail-fast\") (rejected . \"empty race\") (resolved . \"winner\") (rejected \"e1\" \"e2\" \"e3\") (resolved . \"immediate\") (resolved . \"high\") (resolved . \"medium-ok\") (rejected . \"no resolved promises\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Promise combinators: composition, fanout, retry
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_promise_combinators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Higher-order promise combinators: fanout (run multiple transformations
    // on the same value), sequence (chain a list of promise-returning fns),
    // retry (attempt up to N times before giving up), and fallback (try
    // alternatives in order).
    let form = r#"
(progn
  (fset 'neovm--pc-resolve (lambda (val) (cons 'resolved val)))
  (fset 'neovm--pc-reject (lambda (reason) (cons 'rejected reason)))

  (fset 'neovm--pc-then
    (lambda (promise f)
      (if (eq (car promise) 'resolved)
          (funcall f (cdr promise))
        promise)))

  (fset 'neovm--pc-map
    (lambda (promise f)
      (if (eq (car promise) 'resolved)
          (funcall 'neovm--pc-resolve (funcall f (cdr promise)))
        promise)))

  ;; fanout: apply multiple fns to a resolved value, collect results
  (fset 'neovm--pc-fanout
    (lambda (promise fns)
      (if (eq (car promise) 'resolved)
          (let ((val (cdr promise))
                (results nil)
                (failed nil)
                (fail-reason nil))
            (dolist (f fns)
              (unless failed
                (let ((r (funcall f val)))
                  (if (eq (car r) 'resolved)
                      (setq results (cons (cdr r) results))
                    (setq failed t fail-reason (cdr r))))))
            (if failed
                (funcall 'neovm--pc-reject fail-reason)
              (funcall 'neovm--pc-resolve (nreverse results))))
        promise)))

  ;; sequence: chain a list of promise-returning fns
  (fset 'neovm--pc-sequence
    (lambda (initial fns)
      (let ((current initial))
        (dolist (f fns)
          (setq current (funcall 'neovm--pc-then current f)))
        current)))

  ;; retry: try a function up to N times, passing attempt number
  (fset 'neovm--pc-retry
    (lambda (f max-attempts)
      (let ((attempt 0) (result nil) (done nil))
        (while (and (not done) (< attempt max-attempts))
          (setq result (funcall f attempt))
          (setq attempt (1+ attempt))
          (when (eq (car result) 'resolved)
            (setq done t)))
        result)))

  ;; fallback: try alternatives in order, return first resolved
  (fset 'neovm--pc-fallback
    (lambda (promise alternatives)
      (let ((current promise))
        (while (and (eq (car current) 'rejected) alternatives)
          (setq current (funcall (car alternatives)))
          (setq alternatives (cdr alternatives)))
        current)))

  (unwind-protect
      (list
        ;; Fanout: compute multiple derived values
        (funcall 'neovm--pc-fanout
                 (funcall 'neovm--pc-resolve 10)
                 (list (lambda (x) (funcall 'neovm--pc-resolve (* x x)))
                       (lambda (x) (funcall 'neovm--pc-resolve (* x 2)))
                       (lambda (x) (funcall 'neovm--pc-resolve (+ x 1)))))

        ;; Fanout with failure
        (funcall 'neovm--pc-fanout
                 (funcall 'neovm--pc-resolve 10)
                 (list (lambda (x) (funcall 'neovm--pc-resolve (* x x)))
                       (lambda (x) (funcall 'neovm--pc-reject "compute failed"))
                       (lambda (x) (funcall 'neovm--pc-resolve (+ x 1)))))

        ;; Fanout on rejected promise
        (funcall 'neovm--pc-fanout
                 (funcall 'neovm--pc-reject "no input")
                 (list (lambda (x) (funcall 'neovm--pc-resolve x))))

        ;; Sequence of transformations
        (funcall 'neovm--pc-sequence
                 (funcall 'neovm--pc-resolve 2)
                 (list (lambda (x) (funcall 'neovm--pc-resolve (* x 3)))     ;; 6
                       (lambda (x) (funcall 'neovm--pc-resolve (+ x 4)))     ;; 10
                       (lambda (x) (funcall 'neovm--pc-resolve (* x x)))))   ;; 100

        ;; Sequence with mid-chain failure
        (funcall 'neovm--pc-sequence
                 (funcall 'neovm--pc-resolve 5)
                 (list (lambda (x) (funcall 'neovm--pc-resolve (* x 2)))
                       (lambda (x) (funcall 'neovm--pc-reject "step 2 failed"))
                       (lambda (x) (funcall 'neovm--pc-resolve (+ x 100)))))

        ;; Retry: succeeds on 3rd attempt (attempt >= 2)
        (funcall 'neovm--pc-retry
                 (lambda (attempt)
                   (if (>= attempt 2)
                       (funcall 'neovm--pc-resolve (format "success on attempt %d" attempt))
                     (funcall 'neovm--pc-reject (format "failed attempt %d" attempt))))
                 5)

        ;; Retry: never succeeds
        (funcall 'neovm--pc-retry
                 (lambda (attempt)
                   (funcall 'neovm--pc-reject (format "failed attempt %d" attempt)))
                 3)

        ;; Fallback: first fails, second succeeds
        (funcall 'neovm--pc-fallback
                 (funcall 'neovm--pc-reject "primary failed")
                 (list (lambda () (funcall 'neovm--pc-reject "backup-1 failed"))
                       (lambda () (funcall 'neovm--pc-resolve "backup-2 ok"))
                       (lambda () (funcall 'neovm--pc-resolve "backup-3 ok"))))

        ;; Fallback: first succeeds (no alternatives tried)
        (funcall 'neovm--pc-fallback
                 (funcall 'neovm--pc-resolve "primary ok")
                 (list (lambda () (funcall 'neovm--pc-resolve "should not reach"))))

        ;; Fallback: all fail
        (funcall 'neovm--pc-fallback
                 (funcall 'neovm--pc-reject "primary failed")
                 (list (lambda () (funcall 'neovm--pc-reject "alt-1 failed"))
                       (lambda () (funcall 'neovm--pc-reject "alt-2 failed")))))
    (fmakunbound 'neovm--pc-resolve)
    (fmakunbound 'neovm--pc-reject)
    (fmakunbound 'neovm--pc-then)
    (fmakunbound 'neovm--pc-map)
    (fmakunbound 'neovm--pc-fanout)
    (fmakunbound 'neovm--pc-sequence)
    (fmakunbound 'neovm--pc-retry)
    (fmakunbound 'neovm--pc-fallback)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((resolved 100 20 11) (rejected . \"compute failed\") (rejected . \"no input\") (resolved . 100) (rejected . \"step 2 failed\") (resolved . \"success on attempt 2\") (rejected . \"failed attempt 2\") (resolved . \"backup-2 ok\") (resolved . \"primary ok\") (rejected . \"alt-2 failed\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error propagation through complex promise chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_promise_error_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a multi-layer processing pipeline with error context accumulation:
    // each step wraps errors with additional context, and catch handlers can
    // attempt recovery or re-raise with enriched information.
    let form = r#"
(progn
  (fset 'neovm--pe-resolve (lambda (val) (cons 'resolved val)))
  (fset 'neovm--pe-reject (lambda (reason) (cons 'rejected reason)))

  (fset 'neovm--pe-then
    (lambda (p f)
      (if (eq (car p) 'resolved) (funcall f (cdr p)) p)))

  (fset 'neovm--pe-catch
    (lambda (p handler)
      (if (eq (car p) 'rejected) (funcall handler (cdr p)) p)))

  ;; wrap-error: if rejected, prepend context to reason
  (fset 'neovm--pe-wrap-error
    (lambda (p context)
      (if (eq (car p) 'rejected)
          (funcall 'neovm--pe-reject
                   (format "%s: %s" context (cdr p)))
        p)))

  ;; try-recover: attempt recovery, if that fails too, combine errors
  (fset 'neovm--pe-try-recover
    (lambda (p recovery-fn)
      (if (eq (car p) 'rejected)
          (let ((original-error (cdr p))
                (recovery (funcall recovery-fn (cdr p))))
            (if (eq (car recovery) 'resolved)
                recovery
              (funcall 'neovm--pe-reject
                       (format "recovery failed [original: %s] [recovery: %s]"
                               original-error (cdr recovery)))))
        p)))

  ;; Build a data processing pipeline
  (fset 'neovm--pe-validate-input
    (lambda (data)
      (if (and (listp data) (assq 'name data) (assq 'value data))
          (funcall 'neovm--pe-resolve data)
        (funcall 'neovm--pe-reject "invalid input structure"))))

  (fset 'neovm--pe-parse-value
    (lambda (data)
      (let ((v (cdr (assq 'value data))))
        (cond
          ((numberp v)
           (funcall 'neovm--pe-resolve (cons '(parsed . t) data)))
          ((stringp v)
           (let ((n (string-to-number v)))
             (if (and (= n 0) (not (string= v "0")))
                 (funcall 'neovm--pe-reject (format "unparseable value: %S" v))
               (funcall 'neovm--pe-resolve
                        (cons (cons 'value n)
                              (assq-delete-all 'value data))))))
          (t (funcall 'neovm--pe-reject (format "unexpected value type: %S" v)))))))

  (fset 'neovm--pe-check-range
    (lambda (data)
      (let ((v (cdr (assq 'value data))))
        (if (and (>= v 0) (<= v 100))
            (funcall 'neovm--pe-resolve data)
          (funcall 'neovm--pe-reject
                   (format "value %d out of range [0,100]" v))))))

  (fset 'neovm--pe-process-record
    (lambda (data)
      (funcall 'neovm--pe-wrap-error
        (funcall 'neovm--pe-wrap-error
          (funcall 'neovm--pe-wrap-error
            (funcall 'neovm--pe-then
              (funcall 'neovm--pe-then
                (funcall 'neovm--pe-validate-input data)
                'neovm--pe-parse-value)
              'neovm--pe-check-range)
            "range-check")
          "parsing")
        (format "record(%S)" (cdr (assq 'name data))))))

  (unwind-protect
      (let ((records
             (list
              ;; Valid record
              '((name . "alice") (value . 42))
              ;; Valid with string value
              '((name . "bob") (value . "75"))
              ;; Invalid structure (missing value)
              '((name . "charlie"))
              ;; Unparseable string value
              '((name . "dave") (value . "abc"))
              ;; Out of range
              '((name . "eve") (value . 150))
              ;; Zero (edge case)
              '((name . "frank") (value . 0))
              ;; String "0" (edge case)
              '((name . "grace") (value . "0"))
              ;; Boundary value
              '((name . "heidi") (value . 100)))))
        (list
          ;; Process each record
          (mapcar 'neovm--pe-process-record records)

          ;; Try recovery on failed records
          (mapcar (lambda (data)
                    (funcall 'neovm--pe-try-recover
                      (funcall 'neovm--pe-process-record data)
                      (lambda (err)
                        ;; Recovery: clamp out-of-range values
                        (if (string-match-p "out of range" err)
                            (let ((v (cdr (assq 'value data))))
                              (funcall 'neovm--pe-resolve
                                       (cons (cons 'value (max 0 (min 100 v)))
                                             (assq-delete-all 'value data))))
                          (funcall 'neovm--pe-reject
                                   (format "unrecoverable: %s" err))))))
                  records)))
    (fmakunbound 'neovm--pe-resolve)
    (fmakunbound 'neovm--pe-reject)
    (fmakunbound 'neovm--pe-then)
    (fmakunbound 'neovm--pe-catch)
    (fmakunbound 'neovm--pe-wrap-error)
    (fmakunbound 'neovm--pe-try-recover)
    (fmakunbound 'neovm--pe-validate-input)
    (fmakunbound 'neovm--pe-parse-value)
    (fmakunbound 'neovm--pe-check-range)
    (fmakunbound 'neovm--pe-process-record)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((resolved (parsed . t) (name . \"alice\") (value . 42)) (resolved (value . 75) (name . \"bob\")) (rejected . \"record(\\\"charlie\\\"): parsing: range-check: invalid input structure\") (rejected . \"record(\\\"dave\\\"): parsing: range-check: unparseable value: \\\"abc\\\"\") (rejected . \"record(\\\"eve\\\"): parsing: range-check: value 150 out of range [0,100]\") (resolved (parsed . t) (name . \"frank\") (value . 0)) (resolved (value . 0) (name . \"grace\")) (resolved (parsed . t) (name . \"heidi\") (value . 100))) ((resolved (parsed . t) (name . \"alice\") (value . 42)) (rejected . \"recovery failed [original: record(\\\"bob\\\"): parsing: range-check: invalid input structure] [recovery: unrecoverable: record(\\\"bob\\\"): parsing: range-check: invalid input structure]\") (rejected . \"recovery failed [original: record(\\\"charlie\\\"): parsing: range-check: invalid input structure] [recovery: unrecoverable: record(\\\"charlie\\\"): parsing: range-check: invalid input structure]\") (rejected . \"recovery failed [original: record(\\\"dave\\\"): parsing: range-check: unparseable value: \\\"abc\\\"] [recovery: unrecoverable: record(\\\"dave\\\"): parsing: range-check: unparseable value: \\\"abc\\\"]\") (resolved (value . 100) (name . \"eve\")) (resolved (parsed . t) (name . \"frank\") (value . 0)) (rejected . \"recovery failed [original: record(\\\"grace\\\"): parsing: range-check: invalid input structure] [recovery: unrecoverable: record(\\\"grace\\\"): parsing: range-check: invalid input structure]\") (resolved (parsed . t) (name . \"heidi\") (value . 100))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Promise-based task scheduler with dependencies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_promise_task_scheduler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a task scheduler where tasks have dependencies.
    // A task resolves only if all its dependencies resolve.
    // Build a dependency graph, resolve in topological order,
    // and collect results.
    let form = r#"
(progn
  (fset 'neovm--pt-resolve (lambda (val) (cons 'resolved val)))
  (fset 'neovm--pt-reject (lambda (reason) (cons 'rejected reason)))

  ;; A task is (name deps compute-fn)
  ;; deps is a list of task names this task depends on
  ;; compute-fn takes an alist of (dep-name . dep-value) and returns a promise
  (fset 'neovm--pt-make-task
    (lambda (name deps compute-fn)
      (list name deps compute-fn)))

  ;; Execute tasks in dependency order
  (fset 'neovm--pt-execute
    (lambda (tasks)
      (let ((results nil)  ;; alist of (name . promise)
            (remaining (copy-sequence tasks))
            (max-iter 100)
            (iter 0))
        ;; Simple topological execution: repeat until all done or stuck
        (while (and remaining (< iter max-iter))
          (setq iter (1+ iter))
          (let ((executed-any nil))
            (let ((still-remaining nil))
              (dolist (task remaining)
                (let* ((name (nth 0 task))
                       (deps (nth 1 task))
                       (compute (nth 2 task))
                       ;; Check if all deps are resolved
                       (deps-ready t)
                       (deps-failed nil)
                       (dep-values nil))
                  (dolist (dep deps)
                    (let ((dep-result (cdr (assoc dep results))))
                      (cond
                        ((null dep-result) (setq deps-ready nil))
                        ((eq (car dep-result) 'rejected)
                         (setq deps-failed (cdr dep-result)))
                        (t (setq dep-values
                                 (cons (cons dep (cdr dep-result))
                                       dep-values))))))
                  (cond
                    (deps-failed
                     ;; A dependency failed - propagate
                     (setq results
                           (cons (cons name
                                       (funcall 'neovm--pt-reject
                                                (format "dep failed: %s" deps-failed)))
                                 results))
                     (setq executed-any t))
                    (deps-ready
                     ;; All deps resolved - execute
                     (setq results
                           (cons (cons name (funcall compute dep-values))
                                 results))
                     (setq executed-any t))
                    (t
                     ;; Still waiting
                     (setq still-remaining (cons task still-remaining))))))
              (setq remaining (nreverse still-remaining)))
            (unless executed-any
              (setq remaining nil)))) ;; stuck - break
        ;; Return results in task order
        (mapcar (lambda (task)
                  (let ((name (nth 0 task)))
                    (cons name (cdr (assoc name results)))))
                tasks))))

  (unwind-protect
      (list
        ;; Linear dependency chain: a -> b -> c
        (funcall 'neovm--pt-execute
                 (list
                  (funcall 'neovm--pt-make-task "a" nil
                           (lambda (_deps) (funcall 'neovm--pt-resolve 10)))
                  (funcall 'neovm--pt-make-task "b" '("a")
                           (lambda (deps)
                             (funcall 'neovm--pt-resolve
                                      (* (cdr (assoc "a" deps)) 2))))
                  (funcall 'neovm--pt-make-task "c" '("b")
                           (lambda (deps)
                             (funcall 'neovm--pt-resolve
                                      (+ (cdr (assoc "b" deps)) 5))))))

        ;; Diamond dependency: a -> b, a -> c, b+c -> d
        (funcall 'neovm--pt-execute
                 (list
                  (funcall 'neovm--pt-make-task "a" nil
                           (lambda (_) (funcall 'neovm--pt-resolve 5)))
                  (funcall 'neovm--pt-make-task "b" '("a")
                           (lambda (deps)
                             (funcall 'neovm--pt-resolve
                                      (+ (cdr (assoc "a" deps)) 10))))
                  (funcall 'neovm--pt-make-task "c" '("a")
                           (lambda (deps)
                             (funcall 'neovm--pt-resolve
                                      (* (cdr (assoc "a" deps)) 3))))
                  (funcall 'neovm--pt-make-task "d" '("b" "c")
                           (lambda (deps)
                             (funcall 'neovm--pt-resolve
                                      (+ (cdr (assoc "b" deps))
                                         (cdr (assoc "c" deps))))))))

        ;; Failure propagation: a ok, b fails, c depends on b
        (funcall 'neovm--pt-execute
                 (list
                  (funcall 'neovm--pt-make-task "a" nil
                           (lambda (_) (funcall 'neovm--pt-resolve 1)))
                  (funcall 'neovm--pt-make-task "b" '("a")
                           (lambda (_) (funcall 'neovm--pt-reject "b crashed")))
                  (funcall 'neovm--pt-make-task "c" '("b")
                           (lambda (deps)
                             (funcall 'neovm--pt-resolve 999)))))

        ;; Independent tasks (no deps)
        (funcall 'neovm--pt-execute
                 (list
                  (funcall 'neovm--pt-make-task "x" nil
                           (lambda (_) (funcall 'neovm--pt-resolve "hello")))
                  (funcall 'neovm--pt-make-task "y" nil
                           (lambda (_) (funcall 'neovm--pt-resolve "world")))
                  (funcall 'neovm--pt-make-task "z" nil
                           (lambda (_) (funcall 'neovm--pt-resolve 42))))))
    (fmakunbound 'neovm--pt-resolve)
    (fmakunbound 'neovm--pt-reject)
    (fmakunbound 'neovm--pt-make-task)
    (fmakunbound 'neovm--pt-execute)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" resolved . 10) (\"b\" resolved . 20) (\"c\" resolved . 25)) ((\"a\" resolved . 5) (\"b\" resolved . 15) (\"c\" resolved . 15) (\"d\" resolved . 30)) ((\"a\" resolved . 1) (\"b\" rejected . \"b crashed\") (\"c\" rejected . \"dep failed: b crashed\")) ((\"x\" resolved . \"hello\") (\"y\" resolved . \"world\") (\"z\" resolved . 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
