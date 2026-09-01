//! Oracle parity tests for a verification system implemented in Elisp.
//!
//! Implements precondition/postcondition checking, invariant maintenance,
//! design-by-contract patterns, assertion with error reporting,
//! Hoare triple simulation, loop invariant verification, and weakest
//! precondition computation over a simple imperative language.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Test 1: Precondition/postcondition checking with detailed reports
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_verification_pre_post_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Verification framework: wrap a function with pre/post checks
  ;; that produce structured error reports on failure
  (fset 'neovm--vfy-check
    (lambda (name impl preconditions postconditions)
      (lambda (&rest args)
        (let ((pre-failures nil))
          ;; Check all preconditions, collect failures
          (dolist (pre preconditions)
            (let ((check-name (car pre))
                  (check-fn (cdr pre)))
              (unless (apply check-fn args)
                (push (list 'pre-fail check-name args) pre-failures))))
          (if pre-failures
              (list 'rejected name (nreverse pre-failures))
            ;; Run the implementation
            (let ((result (apply impl args)))
              ;; Check all postconditions
              (let ((post-failures nil))
                (dolist (post postconditions)
                  (let ((check-name (car post))
                        (check-fn (cdr post)))
                    (unless (funcall check-fn result args)
                      (push (list 'post-fail check-name result args)
                            post-failures))))
                (if post-failures
                    (list 'violated name (nreverse post-failures))
                  (list 'verified result)))))))))

  (unwind-protect
      (let* (;; Integer square root with verification
             (isqrt
              (funcall 'neovm--vfy-check
                "isqrt"
                (lambda (n) (let ((r 0)) (while (<= (* (1+ r) (1+ r)) n) (setq r (1+ r))) r))
                (list (cons "non-negative" (lambda (n) (and (integerp n) (>= n 0))))
                      (cons "not-too-large" (lambda (n) (< n 1000000))))
                (list (cons "r*r<=n" (lambda (result args)
                                        (<= (* result result) (car args))))
                      (cons "(r+1)^2>n" (lambda (result args)
                                           (> (* (1+ result) (1+ result)) (car args)))))))
             ;; GCD with verification
             (verified-gcd
              (funcall 'neovm--vfy-check
                "gcd"
                (lambda (a b) (let ((x a) (y b)) (while (/= y 0) (let ((tmp (mod x y))) (setq x y) (setq y tmp))) x))
                (list (cons "positive-a" (lambda (a b) (> a 0)))
                      (cons "positive-b" (lambda (a b) (> b 0))))
                (list (cons "divides-a" (lambda (result args) (= (mod (car args) result) 0)))
                      (cons "divides-b" (lambda (result args) (= (mod (cadr args) result) 0)))))))
        (list
         ;; isqrt successes
         (funcall isqrt 0)
         (funcall isqrt 1)
         (funcall isqrt 25)
         (funcall isqrt 26)
         (funcall isqrt 100)
         ;; isqrt precondition failure
         (funcall isqrt -5)
         ;; gcd successes
         (funcall verified-gcd 12 8)
         (funcall verified-gcd 100 75)
         (funcall verified-gcd 17 13)
         ;; gcd precondition failure
         (funcall verified-gcd 0 5)))
    (fmakunbound 'neovm--vfy-check)))"#;
    let expect = expect_test::expect![[
        r#""OK ((verified 0) (verified 1) (verified 5) (verified 5) (verified 10) (rejected \"isqrt\" ((pre-fail \"non-negative\" (-5)))) (verified 4) (verified 25) (verified 1) (rejected \"gcd\" ((pre-fail \"positive-a\" (0 5)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 2: Invariant maintenance on a data structure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_verification_invariant_maintenance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A bounded counter with min/max invariants checked after every operation.
    let form = r#"(let ((make-counter nil)
                        (counter-inc nil)
                        (counter-dec nil)
                        (counter-set nil)
                        (counter-value nil)
                        (counter-check-invariant nil))
  (setq counter-check-invariant
        (lambda (ctr)
          (let ((val (cdr (assq 'value ctr)))
                (lo (cdr (assq 'min ctr)))
                (hi (cdr (assq 'max ctr))))
            (cond
             ((< val lo) (list 'invariant-broken (format "value %d < min %d" val lo)))
             ((> val hi) (list 'invariant-broken (format "value %d > max %d" val hi)))
             (t (list 'invariant-ok))))))
  (setq make-counter
        (lambda (lo hi init)
          (let ((ctr (list (cons 'value init) (cons 'min lo) (cons 'max hi))))
            (let ((check (funcall counter-check-invariant ctr)))
              (if (eq (car check) 'invariant-ok)
                  (list 'ok ctr)
                (list 'error check))))))
  (setq counter-value (lambda (ctr) (cdr (assq 'value ctr))))
  (setq counter-inc
        (lambda (ctr)
          (let* ((new-val (1+ (cdr (assq 'value ctr))))
                 (new-ctr (list (cons 'value new-val)
                                (cons 'min (cdr (assq 'min ctr)))
                                (cons 'max (cdr (assq 'max ctr))))))
            (let ((check (funcall counter-check-invariant new-ctr)))
              (if (eq (car check) 'invariant-ok)
                  (list 'ok new-ctr)
                (list 'error check))))))
  (setq counter-dec
        (lambda (ctr)
          (let* ((new-val (1- (cdr (assq 'value ctr))))
                 (new-ctr (list (cons 'value new-val)
                                (cons 'min (cdr (assq 'min ctr)))
                                (cons 'max (cdr (assq 'max ctr))))))
            (let ((check (funcall counter-check-invariant new-ctr)))
              (if (eq (car check) 'invariant-ok)
                  (list 'ok new-ctr)
                (list 'error check))))))
  ;; Exercise
  (let* ((r0 (funcall make-counter 0 5 0))
         (c0 (cadr r0))
         ;; Increment several times
         (r1 (funcall counter-inc c0))
         (c1 (cadr r1))
         (r2 (funcall counter-inc c1))
         (c2 (cadr r2))
         (r3 (funcall counter-inc c2))
         (c3 (cadr r3))
         ;; Decrement
         (r4 (funcall counter-dec c3))
         (c4 (cadr r4))
         ;; Try to go below min
         (rdec0 (funcall counter-dec c0))
         ;; Increment to max
         (r5 (funcall counter-inc c3))
         (c5 (cadr r5))
         (r6 (funcall counter-inc c5))
         (c6 (cadr r6))
         ;; Try to exceed max
         (r7 (funcall counter-inc c6)))
    (list
      ;; Creation
      (car r0)
      (funcall counter-value c0)
      ;; Increments
      (car r1) (funcall counter-value c1)
      (car r2) (funcall counter-value c2)
      (car r3) (funcall counter-value c3)
      ;; Decrement
      (car r4) (funcall counter-value c4)
      ;; Under min
      rdec0
      ;; To max
      (car r5) (car r6)
      (funcall counter-value c6)
      ;; Over max
      r7
      ;; Invalid creation: init outside bounds
      (funcall make-counter 10 20 5))))"#;
    let expect = expect_test::expect![[
        r#""OK (ok 0 ok 1 ok 2 ok 3 ok 2 (error (invariant-broken \"value -1 < min 0\")) ok ok 5 (error (invariant-broken \"value 6 > max 5\")) (error (invariant-broken \"value 5 < min 10\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 3: Design-by-contract patterns with blame tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_verification_design_by_contract() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Contract system that tracks who violated the contract:
    // "caller" for precondition violations, "callee" for postcondition violations.
    let form = r#"(progn
  (fset 'neovm--vfy-contract
    (lambda (name pre-fn post-fn impl)
      (lambda (&rest args)
        (let ((pre-result (apply pre-fn args)))
          (if pre-result
              (list 'blame 'caller name pre-result)
            (let ((result (apply impl args)))
              (let ((post-result (funcall post-fn result args)))
                (if post-result
                    (list 'blame 'callee name post-result)
                  (list 'success result)))))))))

  (unwind-protect
      (let* (;; Binary search contract: sorted array, valid target, correct result
             (binary-search-contract
              (funcall 'neovm--vfy-contract
                "binary-search"
                ;; Pre: first arg must be a sorted vector, second an integer
                (lambda (vec target)
                  (cond
                   ((not (vectorp vec)) "first arg must be vector")
                   ((not (integerp target)) "target must be integer")
                   ;; Check sorted
                   ((let ((sorted t) (i 1))
                      (while (and sorted (< i (length vec)))
                        (when (< (aref vec i) (aref vec (1- i)))
                          (setq sorted nil))
                        (setq i (1+ i)))
                      (not sorted))
                    "vector must be sorted")
                   (t nil)))
                ;; Post: if result is non-nil index, vec[index] = target
                (lambda (result args)
                  (if (null result) nil
                    (if (not (= (aref (car args) result) (cadr args)))
                        (format "vec[%d]=%d but target=%d" result
                                (aref (car args) result) (cadr args))
                      nil)))
                ;; Implementation: standard binary search
                (lambda (vec target)
                  (let ((lo 0) (hi (1- (length vec))) (found nil))
                    (while (and (<= lo hi) (not found))
                      (let ((mid (/ (+ lo hi) 2)))
                        (cond
                         ((= (aref vec mid) target) (setq found mid))
                         ((< (aref vec mid) target) (setq lo (1+ mid)))
                         (t (setq hi (1- mid))))))
                    found)))))
        (list
         ;; Successful searches
         (funcall binary-search-contract [1 3 5 7 9 11] 5)
         (funcall binary-search-contract [1 3 5 7 9 11] 1)
         (funcall binary-search-contract [1 3 5 7 9 11] 11)
         ;; Not found (success with nil result)
         (funcall binary-search-contract [1 3 5 7 9 11] 4)
         ;; Caller blame: unsorted input
         (funcall binary-search-contract [5 3 1 7] 3)
         ;; Caller blame: wrong type
         (funcall binary-search-contract "not-a-vector" 1)
         ;; Single element searches
         (funcall binary-search-contract [42] 42)
         (funcall binary-search-contract [42] 99)
         ;; Empty vector
         (funcall binary-search-contract [] 1)))
    (fmakunbound 'neovm--vfy-contract)))"#;
    let expect = expect_test::expect![[
        r#""OK ((success 2) (success 0) (success 5) (success nil) (blame caller \"binary-search\" \"vector must be sorted\") (blame caller \"binary-search\" \"first arg must be vector\") (success 0) (success nil) (success nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 4: Assertion system with structured error reports
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_verification_assertion_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((assert-true nil)
                        (assert-equal nil)
                        (assert-all nil)
                        (run-assertions nil))
  ;; Each assertion returns (pass desc) or (fail desc expected actual)
  (setq assert-true
        (lambda (desc value)
          (if value
              (list 'pass desc)
            (list 'fail desc t value))))
  (setq assert-equal
        (lambda (desc expected actual)
          (if (equal expected actual)
              (list 'pass desc)
            (list 'fail desc expected actual))))
  (setq assert-all
        (lambda (assertions)
          (let ((passes 0) (failures nil))
            (dolist (a assertions)
              (if (eq (car a) 'pass)
                  (setq passes (1+ passes))
                (push a failures)))
            (list 'summary passes (length failures) (nreverse failures)))))
  ;; Run a test suite
  (let* ((results
          (list
           ;; Arithmetic assertions
           (funcall assert-equal "1+1=2" 2 (+ 1 1))
           (funcall assert-equal "2*3=6" 6 (* 2 3))
           (funcall assert-equal "10/3=3" 3 (/ 10 3))
           ;; String assertions
           (funcall assert-equal "concat" "foobar" (concat "foo" "bar"))
           (funcall assert-true "non-empty" (> (length "hello") 0))
           ;; Deliberately failing
           (funcall assert-equal "bad-math" 7 (+ 2 2))
           (funcall assert-true "nil-is-true" nil)
           ;; List assertions
           (funcall assert-equal "car" 1 (car '(1 2 3)))
           (funcall assert-equal "length" 4 (length '(a b c d)))
           ;; Type assertions
           (funcall assert-true "is-number" (numberp 42))
           (funcall assert-true "is-string" (stringp "hi"))
           (funcall assert-true "is-list" (listp '(1))))))
    (list
     ;; Individual results
     (nth 0 results)
     (nth 5 results)
     (nth 6 results)
     ;; Summary
     (funcall assert-all results))))"#;
    let expect = expect_test::expect![[
        r#""OK ((pass \"1+1=2\") (fail \"bad-math\" 7 4) (fail \"nil-is-true\" t nil) (summary 10 2 ((fail \"bad-math\" 7 4) (fail \"nil-is-true\" t nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 5: Hoare triple simulation {P} S {Q}
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_verification_hoare_triples() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate Hoare triples over an alist-based state.
    // {P} S {Q} means: if P(state) then after running S(state)->state', Q(state').
    let form = r#"(progn
  (fset 'neovm--vfy-hoare
    (lambda (precond stmt postcond state)
      (if (not (funcall precond state))
          (list 'precondition-not-met state)
        (let ((new-state (funcall stmt state)))
          (if (funcall postcond new-state)
              (list 'valid new-state)
            (list 'postcondition-violated new-state))))))

  ;; State is an alist: ((x . val) (y . val) ...)
  (fset 'neovm--vfy-state-get
    (lambda (state var) (cdr (assq var state))))
  (fset 'neovm--vfy-state-set
    (lambda (state var val)
      (cons (cons var val) (assq-delete-all var (copy-alist state)))))

  (unwind-protect
      (let ((init-state '((x . 5) (y . 10))))
        (list
         ;; {x > 0} x := x + 1 {x > 1}
         (funcall 'neovm--vfy-hoare
                  (lambda (s) (> (funcall 'neovm--vfy-state-get s 'x) 0))
                  (lambda (s) (funcall 'neovm--vfy-state-set s 'x
                                       (1+ (funcall 'neovm--vfy-state-get s 'x))))
                  (lambda (s) (> (funcall 'neovm--vfy-state-get s 'x) 1))
                  init-state)
         ;; {x = 5} y := x * 2 {y = 10}
         (funcall 'neovm--vfy-hoare
                  (lambda (s) (= (funcall 'neovm--vfy-state-get s 'x) 5))
                  (lambda (s) (funcall 'neovm--vfy-state-set s 'y
                                       (* 2 (funcall 'neovm--vfy-state-get s 'x))))
                  (lambda (s) (= (funcall 'neovm--vfy-state-get s 'y) 10))
                  init-state)
         ;; {true} x := 0 {x = 0}
         (funcall 'neovm--vfy-hoare
                  (lambda (_s) t)
                  (lambda (s) (funcall 'neovm--vfy-state-set s 'x 0))
                  (lambda (s) (= (funcall 'neovm--vfy-state-get s 'x) 0))
                  init-state)
         ;; {x > 0, y > 0} z := x + y {z > x AND z > y}
         (funcall 'neovm--vfy-hoare
                  (lambda (s) (and (> (funcall 'neovm--vfy-state-get s 'x) 0)
                                   (> (funcall 'neovm--vfy-state-get s 'y) 0)))
                  (lambda (s) (funcall 'neovm--vfy-state-set s 'z
                                       (+ (funcall 'neovm--vfy-state-get s 'x)
                                          (funcall 'neovm--vfy-state-get s 'y))))
                  (lambda (s) (and (> (funcall 'neovm--vfy-state-get s 'z)
                                      (funcall 'neovm--vfy-state-get s 'x))
                                   (> (funcall 'neovm--vfy-state-get s 'z)
                                      (funcall 'neovm--vfy-state-get s 'y))))
                  init-state)
         ;; Precondition not met: {x < 0} x := x {x < 0}
         (funcall 'neovm--vfy-hoare
                  (lambda (s) (< (funcall 'neovm--vfy-state-get s 'x) 0))
                  (lambda (s) s)
                  (lambda (s) (< (funcall 'neovm--vfy-state-get s 'x) 0))
                  init-state)
         ;; Postcondition violated: {true} x := x + 1 {x = 5} (x was 5, now 6)
         (funcall 'neovm--vfy-hoare
                  (lambda (_s) t)
                  (lambda (s) (funcall 'neovm--vfy-state-set s 'x
                                       (1+ (funcall 'neovm--vfy-state-get s 'x))))
                  (lambda (s) (= (funcall 'neovm--vfy-state-get s 'x) 5))
                  init-state)))
    (fmakunbound 'neovm--vfy-hoare)
    (fmakunbound 'neovm--vfy-state-get)
    (fmakunbound 'neovm--vfy-state-set)))"#;
    let expect = expect_test::expect![[
        r#""OK ((valid ((x . 6) (y . 10))) (valid ((y . 10) (x . 5))) (valid ((x . 0) (y . 10))) (valid ((z . 15) (x . 5) (y . 10))) (precondition-not-met ((x . 5) (y . 10))) (postcondition-violated ((x . 6) (y . 10))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 6: Loop invariant verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_verification_loop_invariant() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify loop invariants hold at every iteration.
    // Returns a trace of invariant checks.
    let form = r#"(progn
  ;; verify-loop: runs body repeatedly, checking invariant at each step
  ;; body: state -> state
  ;; guard: state -> bool (continue while true)
  ;; invariant: state -> bool
  ;; Returns (valid final-state iterations) or (violated state iteration)
  (fset 'neovm--vfy-loop
    (lambda (init-state guard body invariant max-iters)
      ;; Check invariant before loop
      (if (not (funcall invariant init-state))
          (list 'violated init-state 0 "initial")
        (let ((state init-state)
              (iter 0)
              (error nil))
          (while (and (not error) (funcall guard state) (< iter max-iters))
            (setq state (funcall body state))
            (setq iter (1+ iter))
            (unless (funcall invariant state)
              (setq error (list 'violated state iter "mid-loop"))))
          (or error
              (list 'valid state iter))))))

  (unwind-protect
      (list
       ;; Sum 1..n: invariant = (acc = i*(i-1)/2) — sum of first (i-1) naturals
       ;; State: (i . acc), loop while i <= n, body: acc += i; i++
       (funcall 'neovm--vfy-loop
                '(1 . 0)
                (lambda (s) (<= (car s) 10))
                (lambda (s) (cons (1+ (car s)) (+ (cdr s) (car s))))
                (lambda (s) (let ((i (car s)) (acc (cdr s)))
                              (= acc (/ (* (1- i) i) 2))))
                100)
       ;; Factorial: state = (i . acc), invariant: acc = (i-1)!
       ;; body: acc *= i; i++
       (funcall 'neovm--vfy-loop
                '(1 . 1)
                (lambda (s) (<= (car s) 7))
                (lambda (s) (cons (1+ (car s)) (* (cdr s) (car s))))
                ;; Invariant: acc = product of 1..(i-1)
                ;; We verify by computing (i-1)! independently
                (lambda (s) (let ((i (car s)) (acc (cdr s))
                                  (expected 1) (k 1))
                              (while (< k i)
                                (setq expected (* expected k))
                                (setq k (1+ k)))
                              (= acc expected)))
                100)
       ;; Euclidean algorithm: gcd(a,b) invariant: gcd(a,b) = gcd(orig-a, orig-b)
       ;; State: (a b orig-gcd), loop while b > 0
       (let* ((a 48) (b 18)
              ;; Pre-compute gcd for invariant check
              (g (let ((x a) (y b))
                   (while (/= y 0) (let ((tmp (mod x y))) (setq x y y tmp))) x)))
         (funcall 'neovm--vfy-loop
                  (list a b g)
                  (lambda (s) (/= (nth 1 s) 0))
                  (lambda (s) (list (nth 1 s) (mod (nth 0 s) (nth 1 s)) (nth 2 s)))
                  ;; Invariant: current gcd(a,b) = original gcd
                  (lambda (s) (let ((x (nth 0 s)) (y (nth 1 s)) (orig-g (nth 2 s)))
                                (let ((g x))
                                  (when (/= y 0)
                                    (let ((tx x) (ty y))
                                      (while (/= ty 0)
                                        (let ((tmp (mod tx ty)))
                                          (setq tx ty ty tmp)))
                                      (setq g tx)))
                                  (= g orig-g))))
                  100))
       ;; Deliberately broken invariant: increment but claim value stays same
       (funcall 'neovm--vfy-loop
                0
                (lambda (s) (< s 5))
                (lambda (s) (1+ s))
                (lambda (s) (= s 0))   ;; Wrong: only true at start
                100))
    (fmakunbound 'neovm--vfy-loop)))"#;
    let expect = expect_test::expect![[
        r#""OK ((valid (11 . 55) 10) (valid (8 . 5040) 7) (valid (6 0 6) 3) (violated 1 1 \"mid-loop\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 7: Weakest precondition computation over simple statements
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_verification_weakest_precondition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute weakest preconditions for a tiny imperative language:
    // Statements: (assign var expr), (seq s1 s2), (if cond s1 s2), (skip)
    // Expressions: integer, (var x), (add e1 e2), (mul e1 e2), (sub e1 e2)
    //
    // wp(assign x e, Q) = Q[x := e]
    // wp(seq s1 s2, Q) = wp(s1, wp(s2, Q))
    // wp(if c s1 s2, Q) = (c => wp(s1,Q)) AND (!c => wp(s2,Q))
    // wp(skip, Q) = Q
    //
    // We represent predicates as lambdas over state and evaluate them.
    let form = r#"(progn
  ;; Expression evaluator over state (alist)
  (fset 'neovm--wp-eval-expr
    (lambda (expr state)
      (cond
       ((integerp expr) expr)
       ((and (consp expr) (eq (car expr) 'var))
        (cdr (assq (cadr expr) state)))
       ((and (consp expr) (eq (car expr) 'add))
        (+ (funcall 'neovm--wp-eval-expr (cadr expr) state)
           (funcall 'neovm--wp-eval-expr (caddr expr) state)))
       ((and (consp expr) (eq (car expr) 'sub))
        (- (funcall 'neovm--wp-eval-expr (cadr expr) state)
           (funcall 'neovm--wp-eval-expr (caddr expr) state)))
       ((and (consp expr) (eq (car expr) 'mul))
        (* (funcall 'neovm--wp-eval-expr (cadr expr) state)
           (funcall 'neovm--wp-eval-expr (caddr expr) state)))
       (t (error "wp-eval: unknown expr %S" expr)))))

  ;; Statement executor
  (fset 'neovm--wp-exec-stmt
    (lambda (stmt state)
      (cond
       ((eq (car stmt) 'skip) state)
       ((eq (car stmt) 'assign)
        (let* ((var (cadr stmt))
               (expr (caddr stmt))
               (val (funcall 'neovm--wp-eval-expr expr state)))
          (cons (cons var val) (assq-delete-all var (copy-alist state)))))
       ((eq (car stmt) 'seq)
        (let ((s1 (funcall 'neovm--wp-exec-stmt (cadr stmt) state)))
          (funcall 'neovm--wp-exec-stmt (caddr stmt) s1)))
       ((eq (car stmt) 'if-stmt)
        (let ((cond-val (funcall 'neovm--wp-eval-expr (cadr stmt) state)))
          (if (and cond-val (not (= cond-val 0)))
              (funcall 'neovm--wp-exec-stmt (caddr stmt) state)
            (funcall 'neovm--wp-exec-stmt (cadddr stmt) state))))
       (t (error "wp-exec: unknown stmt %S" stmt)))))

  ;; Verify a Hoare triple {P} S {Q} by:
  ;; 1. Checking P(state)
  ;; 2. Executing S
  ;; 3. Checking Q(state')
  (fset 'neovm--wp-verify-triple
    (lambda (pre stmt post state)
      (if (not (funcall pre state))
          (list 'precondition-false)
        (let ((state2 (funcall 'neovm--wp-exec-stmt stmt state)))
          (if (funcall post state2)
              (list 'triple-holds state2)
            (list 'triple-fails state2))))))

  (unwind-protect
      (let ((s0 '((x . 5) (y . 10) (z . 0))))
        (list
         ;; {true} x := 7 {x = 7}
         (funcall 'neovm--wp-verify-triple
                  (lambda (_s) t)
                  '(assign x 7)
                  (lambda (s) (= (cdr (assq 'x s)) 7))
                  s0)
         ;; {x = 5} y := x + 1 {y = 6}
         (funcall 'neovm--wp-verify-triple
                  (lambda (s) (= (cdr (assq 'x s)) 5))
                  '(assign y (add (var x) 1))
                  (lambda (s) (= (cdr (assq 'y s)) 6))
                  s0)
         ;; Sequential: {x=5} x:=x+1; y:=x*2 {y=12}
         (funcall 'neovm--wp-verify-triple
                  (lambda (s) (= (cdr (assq 'x s)) 5))
                  '(seq (assign x (add (var x) 1))
                        (assign y (mul (var x) 2)))
                  (lambda (s) (= (cdr (assq 'y s)) 12))
                  s0)
         ;; Conditional: if x>0 then z:=x else z:=-x  => z=|x|
         ;; For x=5: z should be 5
         (funcall 'neovm--wp-verify-triple
                  (lambda (_s) t)
                  '(if-stmt (var x) (assign z (var x)) (assign z (sub 0 (var x))))
                  (lambda (s) (= (cdr (assq 'z s)) (abs (cdr (assq 'x s0)))))
                  s0)
         ;; Skip
         (funcall 'neovm--wp-verify-triple
                  (lambda (s) (= (cdr (assq 'x s)) 5))
                  '(skip)
                  (lambda (s) (= (cdr (assq 'x s)) 5))
                  s0)
         ;; Failed postcondition: {x=5} x:=x+1 {x=5} (x is now 6)
         (funcall 'neovm--wp-verify-triple
                  (lambda (s) (= (cdr (assq 'x s)) 5))
                  '(assign x (add (var x) 1))
                  (lambda (s) (= (cdr (assq 'x s)) 5))
                  s0)
         ;; Complex: swap x and y via tmp
         ;; {x=5, y=10} tmp:=x; x:=y; y:=tmp {x=10, y=5}
         (funcall 'neovm--wp-verify-triple
                  (lambda (s) (and (= (cdr (assq 'x s)) 5) (= (cdr (assq 'y s)) 10)))
                  '(seq (assign z (var x))
                        (seq (assign x (var y))
                             (assign y (var z))))
                  (lambda (s) (and (= (cdr (assq 'x s)) 10) (= (cdr (assq 'y s)) 5)))
                  s0)))
    (fmakunbound 'neovm--wp-eval-expr)
    (fmakunbound 'neovm--wp-exec-stmt)
    (fmakunbound 'neovm--wp-verify-triple)))"#;
    let expect = expect_test::expect![[
        r#""OK ((triple-holds ((x . 7) (y . 10) (z . 0))) (triple-holds ((y . 6) (x . 5) (z . 0))) (triple-holds ((y . 12) (x . 6) (z . 0))) (triple-holds ((z . 5) (x . 5) (y . 10))) (triple-holds ((x . 5) (y . 10) (z . 0))) (triple-fails ((x . 6) (y . 10) (z . 0))) (triple-holds ((y . 5) (x . 10) (z . 5))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
