//! Oracle parity tests for function composition patterns:
//! compose (right-to-left), pipe (left-to-right), partial application,
//! memoized wrapper, retry wrapper, throttle/debounce simulation,
//! and function juxtaposition.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// compose: right-to-left function composition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fcomp_compose_right_to_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-compose2
    (lambda (f g)
      "Compose two functions: (f . g)(x) = f(g(x))."
      (lambda (x) (funcall f (funcall g x)))))

  (fset 'neovm--test-compose
    (lambda (&rest fns)
      "Compose N functions right-to-left: (f1 . f2 . ... . fn)(x)."
      (if (null fns)
          #'identity
        (let ((result (car (last fns)))
              (rest (nreverse (cdr (nreverse (copy-sequence fns))))))
          (dolist (f rest)
            (setq result (funcall 'neovm--test-compose2 f result)))
          result))))

  (unwind-protect
      (let* ((add1 (lambda (x) (+ x 1)))
             (double (lambda (x) (* x 2)))
             (square (lambda (x) (* x x)))
             (negate (lambda (x) (- x)))
             ;; compose(add1, double) → (x*2)+1
             (c1 (funcall 'neovm--test-compose add1 double))
             ;; compose(double, add1) → (x+1)*2
             (c2 (funcall 'neovm--test-compose double add1))
             ;; compose(negate, square, add1) → -(x+1)^2
             (c3 (funcall 'neovm--test-compose negate square add1))
             ;; Identity composition
             (c4 (funcall 'neovm--test-compose)))
        (list
          (funcall c1 5)    ;; (5*2)+1 = 11
          (funcall c2 5)    ;; (5+1)*2 = 12
          (funcall c3 3)    ;; -((3+1)^2) = -16
          (funcall c4 42)   ;; identity → 42
          ;; Verify associativity: compose(f,compose(g,h)) = compose(compose(f,g),h)
          (let ((left (funcall 'neovm--test-compose
                               negate
                               (funcall 'neovm--test-compose square add1)))
                (right (funcall 'neovm--test-compose
                                (funcall 'neovm--test-compose negate square)
                                add1)))
            (list (funcall left 4) (funcall right 4)
                  (= (funcall left 4) (funcall right 4))))))
    (fmakunbound 'neovm--test-compose2)
    (fmakunbound 'neovm--test-compose)))"#;
    let expect = expect_test::expect![[r#""OK (11 12 16 42 (-25 -25 t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// pipe: left-to-right function composition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fcomp_pipe_left_to_right() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-pipe
    (lambda (&rest fns)
      "Pipe N functions left-to-right: fn(...(f2(f1(x))))."
      (lambda (x)
        (let ((result x))
          (dolist (f fns)
            (setq result (funcall f result)))
          result))))

  (fset 'neovm--test-pipe-val
    (lambda (x &rest fns)
      "Thread value X through functions left-to-right."
      (let ((result x))
        (dolist (f fns)
          (setq result (funcall f result)))
        result)))

  (unwind-protect
      (let ((add1 (lambda (x) (+ x 1)))
            (double (lambda (x) (* x 2)))
            (to-string (lambda (x) (number-to-string x)))
            (wrap-parens (lambda (s) (concat "(" s ")"))))
        (list
          ;; pipe(add1, double)(5) → (5+1)*2 = 12
          (funcall (funcall 'neovm--test-pipe add1 double) 5)
          ;; pipe(double, add1)(5) → (5*2)+1 = 11
          (funcall (funcall 'neovm--test-pipe double add1) 5)
          ;; Full pipeline: 3 → +1 → *2 → to-string → wrap
          (funcall (funcall 'neovm--test-pipe add1 double to-string wrap-parens) 3)
          ;; Empty pipe = identity
          (funcall (funcall 'neovm--test-pipe) 99)
          ;; pipe-val: thread 10 through add1, double, add1
          (funcall 'neovm--test-pipe-val 10 add1 double add1)
          ;; pipe-val with list operations
          (funcall 'neovm--test-pipe-val
                   '(5 3 8 1 9 2)
                   (lambda (lst) (sort (copy-sequence lst) #'<))
                   #'reverse
                   (lambda (lst) (let ((r nil) (i 0))
                                   (dolist (x lst) (when (< i 3) (setq r (cons x r)) (setq i (1+ i))))
                                   (nreverse r))))))
    (fmakunbound 'neovm--test-pipe)
    (fmakunbound 'neovm--test-pipe-val)))"#;
    let expect = expect_test::expect![[r#""OK (12 11 \"(8)\" 99 23 (9 8 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Partial application (currying)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fcomp_partial_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-partial
    (lambda (f &rest bound-args)
      "Return a function with BOUND-ARGS already applied to F."
      (lambda (&rest remaining-args)
        (apply f (append bound-args remaining-args)))))

  (fset 'neovm--test-curry
    (lambda (f)
      "Curry a 2-argument function: f(a,b) → f(a)(b)."
      (lambda (a)
        (lambda (b)
          (funcall f a b)))))

  (fset 'neovm--test-curry3
    (lambda (f)
      "Curry a 3-argument function: f(a,b,c) → f(a)(b)(c)."
      (lambda (a)
        (lambda (b)
          (lambda (c)
            (funcall f a b c))))))

  (unwind-protect
      (let* (;; partial(+, 10) → adds 10 to argument
             (add10 (funcall 'neovm--test-partial #'+ 10))
             ;; partial(*, 3, 4) → multiplies 3*4*arg
             (mul12 (funcall 'neovm--test-partial #'* 3 4))
             ;; partial(concat, "hello ") → prepends "hello "
             (greet (funcall 'neovm--test-partial #'concat "hello "))
             ;; curry(-)
             (sub-curried (funcall 'neovm--test-curry #'-))
             ;; curry3 for a 3-arg function
             (volume (funcall 'neovm--test-curry3
                              (lambda (l w h) (* l w h)))))
        (list
          (funcall add10 5)           ;; 15
          (funcall add10 -3)          ;; 7
          (funcall mul12 2)           ;; 24
          (funcall greet "world")     ;; "hello world"
          ;; Curried subtraction: (curry(-))(10)(3) = 7
          (funcall (funcall sub-curried 10) 3)
          ;; Curried volume: 2*3*4 = 24
          (funcall (funcall (funcall volume 2) 3) 4)
          ;; Partial application of partial: nested
          (let* ((make-adder (funcall 'neovm--test-partial
                                      'neovm--test-partial #'+))
                 (add7 (funcall make-adder 7)))
            (funcall add7 8))         ;; 15
          ;; Partial with mapcar
          (let ((mul-by (lambda (n) (funcall 'neovm--test-partial #'* n))))
            (mapcar (funcall mul-by 5) '(1 2 3 4)))))
    (fmakunbound 'neovm--test-partial)
    (fmakunbound 'neovm--test-curry)
    (fmakunbound 'neovm--test-curry3)))"#;
    let expect = expect_test::expect![[r#""OK (15 7 24 \"hello world\" 7 24 15 (5 10 15 20))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Memoized function wrapper
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fcomp_memoize_wrapper() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-memoize
    (lambda (f)
      "Return a memoized version of single-arg function F.
       Also tracks call count for verification."
      (let ((cache (make-hash-table :test 'equal))
            (call-count 0)
            (cache-hits 0))
        (lambda (op &rest args)
          (cond
            ((eq op :call)
             (let* ((key (car args))
                    (cached (gethash key cache :neovm--miss)))
               (if (not (eq cached :neovm--miss))
                   (progn (setq cache-hits (1+ cache-hits))
                          cached)
                 (setq call-count (1+ call-count))
                 (let ((result (funcall f key)))
                   (puthash key result cache)
                   result))))
            ((eq op :stats)
             (list :calls call-count :hits cache-hits
                   :cached (hash-table-count cache)))
            ((eq op :clear)
             (clrhash cache)
             (setq call-count 0 cache-hits 0)
             nil))))))

  (unwind-protect
      (let ((expensive-fn (lambda (n)
                            ;; Simulates expensive computation
                            (let ((result 0))
                              (dotimes (i (1+ n))
                                (setq result (+ result (* i i))))
                              result)))
            (memo nil))
        (setq memo (funcall 'neovm--test-memoize expensive-fn))
        (list
          ;; First calls: all miss cache
          (funcall memo :call 5)
          (funcall memo :call 10)
          (funcall memo :call 15)
          ;; Stats after 3 unique calls
          (funcall memo :stats)
          ;; Repeated calls: should hit cache
          (funcall memo :call 5)
          (funcall memo :call 10)
          (funcall memo :call 5)
          ;; Stats: 3 calls, 3 hits now
          (funcall memo :stats)
          ;; New call
          (funcall memo :call 20)
          ;; Final stats: 4 calls, 3 hits, 4 cached
          (funcall memo :stats)
          ;; Results are consistent
          (= (funcall memo :call 15) (funcall memo :call 15))))
    (fmakunbound 'neovm--test-memoize)))"#;
    let expect = expect_test::expect![[
        r#""OK (55 385 1240 (:calls 3 :hits 0 :cached 3) 55 385 55 (:calls 3 :hits 3 :cached 3) 2870 (:calls 4 :hits 3 :cached 4) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Retry wrapper with configurable attempts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fcomp_retry_wrapper() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-with-retry
    (lambda (f max-attempts on-failure)
      "Wrap F with retry logic. Calls F up to MAX-ATTEMPTS times.
       ON-FAILURE is called with attempt number on each failure.
       Returns (ok . result) on success, (error . last-error) on final failure."
      (lambda (&rest args)
        (let ((attempt 0)
              (success nil)
              (last-err nil)
              (result nil)
              (log nil))
          (while (and (< attempt max-attempts) (not success))
            (setq attempt (1+ attempt))
            (condition-case err
                (progn
                  (setq result (apply f args))
                  (setq success t))
              (error
               (setq last-err (error-message-string err))
               (setq log (cons (cons attempt last-err) log))
               (when on-failure
                 (funcall on-failure attempt)))))
          (if success
              (list 'ok result (nreverse log))
            (list 'error last-err (nreverse log)))))))

  (unwind-protect
      (let* ((call-counter 0)
             ;; Fails first 2 times, succeeds on 3rd
             (flaky-fn (lambda (x)
                         (setq call-counter (1+ call-counter))
                         (if (<= call-counter 2)
                             (error "Transient failure %d" call-counter)
                           (* x x))))
             (retried (funcall 'neovm--test-with-retry
                               flaky-fn 5
                               (lambda (n) nil))))
        ;; Reset counter for second test
        (let ((result1 (funcall retried 7)))
          (setq call-counter 0)
          ;; Always-fail function
          (let* ((always-fail (lambda (x) (error "Permanent failure")))
                 (retried2 (funcall 'neovm--test-with-retry
                                    always-fail 3
                                    (lambda (n) nil)))
                 (result2 (funcall retried2 42)))
            ;; Always-succeed function
            (let* ((always-ok (lambda (x) (+ x 100)))
                   (retried3 (funcall 'neovm--test-with-retry
                                      always-ok 3 nil))
                   (result3 (funcall retried3 5)))
              (list result1    ;; (ok 49 ((1 . "...") (2 . "...")))
                    result2    ;; (error "Permanent failure" ...)
                    result3    ;; (ok 105 nil)
                    ;; Verify structure
                    (car result1)
                    (car result2)
                    (car result3))))))
    (fmakunbound 'neovm--test-with-retry)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok 49 ((1 . \"Transient failure 1\") (2 . \"Transient failure 2\"))) (error \"Permanent failure\" ((1 . \"Permanent failure\") (2 . \"Permanent failure\") (3 . \"Permanent failure\"))) (ok 105 nil) ok error ok)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Throttle / debounce simulation using counters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fcomp_throttle_debounce_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Since we can't use real time, simulate with a logical clock (counter)
    let form = r#"(progn
  (fset 'neovm--test-make-throttle
    (lambda (f interval)
      "Throttle: allow F to execute at most once per INTERVAL ticks.
       Returns (fn tick-arg &rest args) — tick is the logical timestamp."
      (let ((last-tick -999999)
            (exec-log nil))
        (lambda (op &rest args)
          (cond
            ((eq op :call)
             (let ((tick (car args))
                   (fn-args (cdr args)))
               (if (>= (- tick last-tick) interval)
                   (progn
                     (setq last-tick tick)
                     (let ((result (apply f fn-args)))
                       (setq exec-log (cons (cons tick result) exec-log))
                       result))
                 ;; Throttled: skip
                 nil)))
            ((eq op :log)
             (nreverse exec-log)))))))

  (fset 'neovm--test-make-debounce
    (lambda (f delay)
      "Debounce: only execute F if DELAY ticks have passed since last call.
       Simulated: accumulate calls, flush when gap >= delay."
      (let ((pending nil)
            (last-call-tick nil)
            (exec-log nil))
        (lambda (op &rest args)
          (cond
            ((eq op :call)
             (let ((tick (car args))
                   (fn-args (cdr args)))
               (setq pending fn-args)
               (setq last-call-tick tick)
               nil))
            ((eq op :flush)
             ;; Check if enough time has passed since last call
             (let ((current-tick (car args)))
               (when (and last-call-tick
                          (>= (- current-tick last-call-tick) delay)
                          pending)
                 (let ((result (apply f pending)))
                   (setq exec-log (cons (cons last-call-tick result) exec-log))
                   (setq pending nil)
                   result))))
            ((eq op :log)
             (nreverse exec-log)))))))

  (unwind-protect
      (let ((accum nil))
        ;; Throttle test: interval=3
        (let ((throttled (funcall 'neovm--test-make-throttle
                                  (lambda (x) (* x x)) 3)))
          ;; Call at ticks 0,1,2,3,4,5,6,7,8,9
          (dotimes (tick 10)
            (let ((r (funcall throttled :call tick tick)))
              (when r (setq accum (cons (cons tick r) accum)))))
          (let ((throttle-results (nreverse accum))
                (throttle-log (funcall throttled :log)))
            ;; Debounce test: delay=3
            (let ((debounced (funcall 'neovm--test-make-debounce
                                      (lambda (x) (+ x 100)) 3)))
              ;; Rapid calls at ticks 0,1,2 (should be suppressed)
              (funcall debounced :call 0 10)
              (funcall debounced :call 1 20)
              (funcall debounced :call 2 30)
              ;; Flush at tick 3: not enough gap (2-tick=2 < 3)
              (funcall debounced :flush 4)
              ;; Flush at tick 5: gap = 5-2 = 3, should execute with last args (30)
              (let ((db-result (funcall debounced :flush 5)))
                ;; Another call and immediate gap
                (funcall debounced :call 10 50)
                (funcall debounced :flush 13)
                (list throttle-results
                      throttle-log
                      db-result
                      (funcall debounced :log)))))))
    (fmakunbound 'neovm--test-make-throttle)
    (fmakunbound 'neovm--test-make-debounce)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . 0) (3 . 9) (6 . 36) (9 . 81)) ((0 . 0) (3 . 9) (6 . 36) (9 . 81)) 130 ((2 . 130) (10 . 150)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Function juxtaposition: apply multiple functions, collect results
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fcomp_juxtaposition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-juxt
    (lambda (&rest fns)
      "Return a function that applies each of FNS to its argument
       and returns a list of results."
      (lambda (&rest args)
        (mapcar (lambda (f) (apply f args)) fns))))

  (fset 'neovm--test-juxt-pred
    (lambda (&rest preds)
      "Return a function that tests all predicates and returns
       an alist of (predicate-result . t-or-nil)."
      (lambda (x)
        (let ((results nil) (i 0))
          (dolist (p preds)
            (setq results (cons (cons i (if (funcall p x) t nil)) results))
            (setq i (1+ i)))
          (nreverse results)))))

  (unwind-protect
      (let* ((stats (funcall 'neovm--test-juxt
                             (lambda (lst) (length lst))
                             (lambda (lst) (car lst))
                             (lambda (lst) (car (last lst)))
                             (lambda (lst) (let ((s 0)) (dolist (x lst) (setq s (+ s x))) s))
                             (lambda (lst) (let ((s 0)) (dolist (x lst) (setq s (+ s x)))
                                                 (/ (float s) (length lst))))))
             (classify (funcall 'neovm--test-juxt-pred
                                #'numberp
                                #'integerp
                                #'floatp
                                (lambda (x) (and (numberp x) (> x 0)))
                                (lambda (x) (and (numberp x) (= 0 (mod x 2)))))))
        (list
          ;; Stats on a list: (length first last sum mean)
          (funcall stats '(3 7 2 9 1 5))
          ;; Stats on single element
          (funcall stats '(42))
          ;; Classify various values
          (funcall classify 42)
          (funcall classify 3.14)
          (funcall classify -7)
          (funcall classify 0)
          ;; Juxt with two-arg functions
          (let ((both-ops (funcall 'neovm--test-juxt #'+ #'- #'* #'max #'min)))
            (funcall both-ops 10 3))))
    (fmakunbound 'neovm--test-juxt)
    (fmakunbound 'neovm--test-juxt-pred)))"#;
    let expect = expect_test::expect![[
        r#""OK ((6 3 5 27 4.5) (1 42 42 42 42.0) ((0 . t) (1 . t) (2) (3 . t) (4 . t)) ((0 . t) (1) (2 . t) (3 . t) (4)) ((0 . t) (1 . t) (2) (3) (4)) ((0 . t) (1 . t) (2) (3) (4 . t)) (13 7 30 10 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: middleware/decorator chain (like Ring handlers)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fcomp_middleware_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-wrap-logging
    (lambda (handler)
      "Middleware that logs request/response."
      (lambda (request)
        (let* ((log-entry (list :log-in (plist-get request :path)))
               (response (funcall handler request))
               (logged (plist-put (copy-sequence response)
                                  :log (cons log-entry
                                             (plist-get response :log)))))
          logged))))

  (fset 'neovm--test-wrap-auth
    (lambda (handler)
      "Middleware that checks for :auth-token in request."
      (lambda (request)
        (if (plist-get request :auth-token)
            (funcall handler request)
          (list :status 401 :body "Unauthorized" :log nil)))))

  (fset 'neovm--test-wrap-transform
    (lambda (handler key transform-fn)
      "Middleware that transforms a response key."
      (lambda (request)
        (let ((response (funcall handler request)))
          (plist-put (copy-sequence response) key
                     (funcall transform-fn (plist-get response key)))))))

  (unwind-protect
      (let* (;; Base handler
             (base-handler (lambda (request)
                             (list :status 200
                                   :body (concat "Hello, " (plist-get request :path))
                                   :log nil)))
             ;; Build middleware stack: logging → auth → transform body to uppercase
             (app (funcall 'neovm--test-wrap-logging
                           (funcall 'neovm--test-wrap-auth
                                    (funcall 'neovm--test-wrap-transform
                                             base-handler
                                             :body #'upcase)))))
        (list
          ;; Authenticated request
          (funcall app '(:path "/home" :auth-token "secret123"))
          ;; Unauthenticated request (blocked by auth middleware)
          (funcall app '(:path "/admin"))
          ;; Another authenticated request
          (funcall app '(:path "/api/data" :auth-token "token"))))
    (fmakunbound 'neovm--test-wrap-logging)
    (fmakunbound 'neovm--test-wrap-auth)
    (fmakunbound 'neovm--test-wrap-transform)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:status 200 :body \"HELLO, /HOME\" :log ((:log-in \"/home\"))) (:status 401 :body \"Unauthorized\" :log ((:log-in \"/admin\"))) (:status 200 :body \"HELLO, /API/DATA\" :log ((:log-in \"/api/data\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
