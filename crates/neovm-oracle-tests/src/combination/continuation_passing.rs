//! Oracle parity tests for continuation-passing style (CPS) transformations
//! in Elisp: direct-style to CPS for arithmetic, explicit continuations,
//! CPS conditionals, CPS recursive functions (factorial, fibonacci),
//! CPS with multiple continuations (success/failure), and CPS trampoline
//! for stack-safe recursion.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Direct-style to CPS transformation for arithmetic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_direct_to_cps_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare direct-style arithmetic with CPS-transformed equivalents.
    // Every operation takes an extra continuation argument.
    let form = r#"(progn
  ;; CPS arithmetic primitives
  (fset 'neovm--cp-add (lambda (a b k) (funcall k (+ a b))))
  (fset 'neovm--cp-sub (lambda (a b k) (funcall k (- a b))))
  (fset 'neovm--cp-mul (lambda (a b k) (funcall k (* a b))))
  (fset 'neovm--cp-div (lambda (a b k) (funcall k (/ a b))))
  (fset 'neovm--cp-mod (lambda (a b k) (funcall k (% a b))))
  (fset 'neovm--cp-neg (lambda (a k) (funcall k (- a))))

  (unwind-protect
      (list
        ;; Direct: (+ 3 4) = 7
        ;; CPS: (add-k 3 4 identity)
        (funcall 'neovm--cp-add 3 4 #'identity)

        ;; Direct: (* (+ 2 3) (- 10 4)) = 30
        ;; CPS: add(2,3, k1 -> sub(10,4, k2 -> mul(k1,k2, id)))
        (funcall 'neovm--cp-add 2 3
                 (lambda (v1)
                   (funcall 'neovm--cp-sub 10 4
                            (lambda (v2)
                              (funcall 'neovm--cp-mul v1 v2 #'identity)))))

        ;; Direct: (% (+ (* 7 8) 3) 10) = 9
        (funcall 'neovm--cp-mul 7 8
                 (lambda (v1)
                   (funcall 'neovm--cp-add v1 3
                            (lambda (v2)
                              (funcall 'neovm--cp-mod v2 10 #'identity)))))

        ;; Direct: (- 0 (+ 5 (* 3 (- 8 2)))) = -23
        (funcall 'neovm--cp-sub 8 2
                 (lambda (v1)
                   (funcall 'neovm--cp-mul 3 v1
                            (lambda (v2)
                              (funcall 'neovm--cp-add 5 v2
                                       (lambda (v3)
                                         (funcall 'neovm--cp-neg v3 #'identity)))))))

        ;; Direct: (/ (* (+ 10 20) (- 100 70)) 5) = 180
        (funcall 'neovm--cp-add 10 20
                 (lambda (v1)
                   (funcall 'neovm--cp-sub 100 70
                            (lambda (v2)
                              (funcall 'neovm--cp-mul v1 v2
                                       (lambda (v3)
                                         (funcall 'neovm--cp-div v3 5 #'identity)))))))

        ;; Verify all match direct-style
        (= (funcall 'neovm--cp-add 3 4 #'identity) (+ 3 4))
        (= (funcall 'neovm--cp-mul 7 8
                     (lambda (v) (funcall 'neovm--cp-add v 3
                                          (lambda (v2) (funcall 'neovm--cp-mod v2 10 #'identity)))))
           (% (+ (* 7 8) 3) 10)))
    (fmakunbound 'neovm--cp-add)
    (fmakunbound 'neovm--cp-sub)
    (fmakunbound 'neovm--cp-mul)
    (fmakunbound 'neovm--cp-div)
    (fmakunbound 'neovm--cp-mod)
    (fmakunbound 'neovm--cp-neg)))"#;
    let expect = expect_test::expect![[r#""OK (7 30 9 -23 180 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPS-transformed functions with explicit continuations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_explicit_continuations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define higher-level CPS functions that compose continuations.
    let form = r#"(progn
  ;; CPS compose: (f . g)(x) = f(g(x))
  ;; In CPS: g-k(x, k1 -> f-k(k1, k))
  (fset 'neovm--cp-compose-k
    (lambda (f-k g-k)
      "Return a CPS function that composes F-K after G-K."
      (lambda (x k)
        (funcall g-k x (lambda (intermediate) (funcall f-k intermediate k))))))

  ;; CPS pipe: thread value through a list of CPS functions
  (fset 'neovm--cp-pipe-k
    (lambda (val fns k)
      "Thread VAL through CPS functions FNS, pass final result to K."
      (if (null fns)
          (funcall k val)
        (funcall (car fns) val
                 (lambda (next-val)
                   (funcall 'neovm--cp-pipe-k next-val (cdr fns) k))))))

  ;; CPS apply-n-times: apply f-k to val N times
  (fset 'neovm--cp-apply-n
    (lambda (f-k val n k)
      (if (<= n 0)
          (funcall k val)
        (funcall f-k val
                 (lambda (result)
                   (funcall 'neovm--cp-apply-n f-k result (1- n) k))))))

  (unwind-protect
      (let* ((double-k (lambda (x k) (funcall k (* x 2))))
             (inc-k (lambda (x k) (funcall k (1+ x))))
             (square-k (lambda (x k) (funcall k (* x x))))
             (to-string-k (lambda (x k) (funcall k (number-to-string x)))))
        (list
          ;; Compose: square after double: square(double(3)) = 36
          (let ((sq-dbl (funcall 'neovm--cp-compose-k square-k double-k)))
            (funcall sq-dbl 3 #'identity))

          ;; Compose: double after square: double(square(3)) = 18
          (let ((dbl-sq (funcall 'neovm--cp-compose-k double-k square-k)))
            (funcall dbl-sq 3 #'identity))

          ;; Triple compose: to-string(square(double(5))) = "100"
          (let* ((sq-dbl (funcall 'neovm--cp-compose-k square-k double-k))
                 (str-sq-dbl (funcall 'neovm--cp-compose-k to-string-k sq-dbl)))
            (funcall str-sq-dbl 5 #'identity))

          ;; Pipe: 3 -> double -> inc -> square -> to-string = "49"
          (funcall 'neovm--cp-pipe-k 3
                   (list double-k inc-k square-k to-string-k)
                   #'identity)

          ;; Pipe: empty pipeline returns value unchanged
          (funcall 'neovm--cp-pipe-k 42 nil #'identity)

          ;; Pipe: single function
          (funcall 'neovm--cp-pipe-k 5 (list square-k) #'identity)

          ;; Apply-n: double 3 times: 2 -> 4 -> 8 -> 16
          (funcall 'neovm--cp-apply-n double-k 2 3 #'identity)

          ;; Apply-n: inc 5 times: 10 -> 15
          (funcall 'neovm--cp-apply-n inc-k 10 5 #'identity)

          ;; Apply-n: 0 times returns original
          (funcall 'neovm--cp-apply-n double-k 7 0 #'identity)))
    (fmakunbound 'neovm--cp-compose-k)
    (fmakunbound 'neovm--cp-pipe-k)
    (fmakunbound 'neovm--cp-apply-n)))"#;
    let expect = expect_test::expect![[r#""OK (36 18 \"100\" \"49\" 42 25 16 15 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPS for conditional expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_conditional_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CPS if, cond, and case expressions.
    let form = r#"(progn
  ;; CPS if: test-k evaluates test, then branch on result
  (fset 'neovm--cp-if-k
    (lambda (test-k then-k else-k k)
      (funcall test-k
               (lambda (cond-val)
                 (if cond-val
                     (funcall then-k k)
                   (funcall else-k k))))))

  ;; CPS cond: list of (test-k . body-k) pairs, plus default
  (fset 'neovm--cp-cond-k
    (lambda (clauses default-k k)
      "CPS cond: try clauses in order, fire first matching body."
      (if (null clauses)
          (funcall default-k k)
        (let ((test-k (caar clauses))
              (body-k (cdar clauses))
              (rest (cdr clauses)))
          (funcall test-k
                   (lambda (result)
                     (if result
                         (funcall body-k k)
                       (funcall 'neovm--cp-cond-k rest default-k k))))))))

  ;; CPS comparison helpers
  (fset 'neovm--cp-lt-k (lambda (a b k) (funcall k (< a b))))
  (fset 'neovm--cp-gt-k (lambda (a b k) (funcall k (> a b))))
  (fset 'neovm--cp-eq-k (lambda (a b k) (funcall k (= a b))))

  (unwind-protect
      (list
        ;; Simple if: (if (< 3 5) "yes" "no")
        (funcall 'neovm--cp-if-k
                 (lambda (k) (funcall 'neovm--cp-lt-k 3 5 k))
                 (lambda (k) (funcall k "yes"))
                 (lambda (k) (funcall k "no"))
                 #'identity)

        ;; If with false condition
        (funcall 'neovm--cp-if-k
                 (lambda (k) (funcall 'neovm--cp-gt-k 3 5 k))
                 (lambda (k) (funcall k "yes"))
                 (lambda (k) (funcall k "no"))
                 #'identity)

        ;; Nested if: classify number
        ;; x < 0 -> "negative", x = 0 -> "zero", x > 0 -> "positive"
        (let ((x 0))
          (funcall 'neovm--cp-if-k
                   (lambda (k) (funcall 'neovm--cp-lt-k x 0 k))
                   (lambda (k) (funcall k "negative"))
                   (lambda (k)
                     (funcall 'neovm--cp-if-k
                              (lambda (k2) (funcall 'neovm--cp-eq-k x 0 k2))
                              (lambda (k2) (funcall k2 "zero"))
                              (lambda (k2) (funcall k2 "positive"))
                              k))
                   #'identity))

        ;; CPS cond: grade classification
        (let ((score 85))
          (funcall 'neovm--cp-cond-k
                   (list (cons (lambda (k) (funcall 'neovm--cp-lt-k score 60 k))
                               (lambda (k) (funcall k "F")))
                         (cons (lambda (k) (funcall 'neovm--cp-lt-k score 70 k))
                               (lambda (k) (funcall k "D")))
                         (cons (lambda (k) (funcall 'neovm--cp-lt-k score 80 k))
                               (lambda (k) (funcall k "C")))
                         (cons (lambda (k) (funcall 'neovm--cp-lt-k score 90 k))
                               (lambda (k) (funcall k "B"))))
                   (lambda (k) (funcall k "A"))
                   #'identity))

        ;; CPS cond: multiple scores
        (let ((test-scores '(95 72 58 83 67)))
          (let ((grades nil)
                (remaining test-scores))
            (while remaining
              (let ((score (car remaining)))
                (push
                 (funcall 'neovm--cp-cond-k
                          (list (cons (lambda (k) (funcall 'neovm--cp-lt-k score 60 k))
                                      (lambda (k) (funcall k "F")))
                                (cons (lambda (k) (funcall 'neovm--cp-lt-k score 70 k))
                                      (lambda (k) (funcall k "D")))
                                (cons (lambda (k) (funcall 'neovm--cp-lt-k score 80 k))
                                      (lambda (k) (funcall k "C")))
                                (cons (lambda (k) (funcall 'neovm--cp-lt-k score 90 k))
                                      (lambda (k) (funcall k "B"))))
                          (lambda (k) (funcall k "A"))
                          #'identity)
                 grades))
              (setq remaining (cdr remaining)))
            (nreverse grades)))

        ;; If with computed branches
        (funcall 'neovm--cp-if-k
                 (lambda (k) (funcall k (> (length "hello") 3)))
                 (lambda (k) (funcall k (concat "long-" (number-to-string (length "hello")))))
                 (lambda (k) (funcall k "short"))
                 #'identity))
    (fmakunbound 'neovm--cp-if-k)
    (fmakunbound 'neovm--cp-cond-k)
    (fmakunbound 'neovm--cp-lt-k)
    (fmakunbound 'neovm--cp-gt-k)
    (fmakunbound 'neovm--cp-eq-k)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"yes\" \"no\" \"zero\" \"B\" (\"A\" \"C\" \"F\" \"B\" \"D\") \"long-5\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: CPS for recursive functions (factorial, fibonacci)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_recursive_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CPS factorial and fibonacci, where the continuation captures
    // the "rest of the computation" after the recursive call.
    let form = r#"(progn
  ;; CPS factorial: fact(n, k) = if n<2 then k(1) else fact(n-1, r -> k(n*r))
  (fset 'neovm--cp-fact
    (lambda (n k)
      (if (< n 2)
          (funcall k 1)
        (funcall 'neovm--cp-fact (1- n)
                 (lambda (r) (funcall k (* n r)))))))

  ;; CPS fibonacci: fib(n, k) = if n<2 then k(n)
  ;;   else fib(n-1, a -> fib(n-2, b -> k(a+b)))
  (fset 'neovm--cp-fib
    (lambda (n k)
      (if (< n 2)
          (funcall k n)
        (funcall 'neovm--cp-fib (1- n)
                 (lambda (a)
                   (funcall 'neovm--cp-fib (- n 2)
                            (lambda (b)
                              (funcall k (+ a b)))))))))

  ;; CPS power: pow(base, exp, k)
  (fset 'neovm--cp-pow
    (lambda (base exp k)
      (if (= exp 0)
          (funcall k 1)
        (funcall 'neovm--cp-pow base (1- exp)
                 (lambda (r) (funcall k (* base r)))))))

  ;; CPS sum of list
  (fset 'neovm--cp-sum-list
    (lambda (lst k)
      (if (null lst)
          (funcall k 0)
        (funcall 'neovm--cp-sum-list (cdr lst)
                 (lambda (rest-sum) (funcall k (+ (car lst) rest-sum)))))))

  ;; CPS length of list
  (fset 'neovm--cp-length
    (lambda (lst k)
      (if (null lst)
          (funcall k 0)
        (funcall 'neovm--cp-length (cdr lst)
                 (lambda (rest-len) (funcall k (1+ rest-len)))))))

  (unwind-protect
      (list
        ;; Factorial
        (funcall 'neovm--cp-fact 0 #'identity)
        (funcall 'neovm--cp-fact 1 #'identity)
        (funcall 'neovm--cp-fact 5 #'identity)
        (funcall 'neovm--cp-fact 10 #'identity)

        ;; Fibonacci
        (funcall 'neovm--cp-fib 0 #'identity)
        (funcall 'neovm--cp-fib 1 #'identity)
        (funcall 'neovm--cp-fib 5 #'identity)
        (funcall 'neovm--cp-fib 10 #'identity)
        (funcall 'neovm--cp-fib 15 #'identity)

        ;; Power
        (funcall 'neovm--cp-pow 2 0 #'identity)
        (funcall 'neovm--cp-pow 2 10 #'identity)
        (funcall 'neovm--cp-pow 3 5 #'identity)

        ;; Sum of list
        (funcall 'neovm--cp-sum-list '(1 2 3 4 5) #'identity)
        (funcall 'neovm--cp-sum-list nil #'identity)
        (funcall 'neovm--cp-sum-list '(10 20 30 40 50 60 70 80 90 100) #'identity)

        ;; Length of list
        (funcall 'neovm--cp-length '(a b c d e) #'identity)
        (funcall 'neovm--cp-length nil #'identity)

        ;; Verify against direct-style
        (= (funcall 'neovm--cp-fact 7 #'identity) (* 7 6 5 4 3 2 1))
        (= (funcall 'neovm--cp-pow 2 8 #'identity) 256))
    (fmakunbound 'neovm--cp-fact)
    (fmakunbound 'neovm--cp-fib)
    (fmakunbound 'neovm--cp-pow)
    (fmakunbound 'neovm--cp-sum-list)
    (fmakunbound 'neovm--cp-length)))"#;
    let expect =
        expect_test::expect![[r#""ERR (error \"Lisp nesting exceeds ‘max-lisp-eval-depth’\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: CPS with multiple continuations (success/failure)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_multiple_continuations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two-continuation CPS: each function receives a success continuation
    // and a failure continuation. Errors propagate through failure-k,
    // successes through success-k.
    let form = r#"(progn
  ;; Safe division: success or failure continuation
  (fset 'neovm--cp-safe-div
    (lambda (a b sk fk)
      (if (= b 0)
          (funcall fk (format "division by zero: %d / %d" a b))
        (funcall sk (/ a b)))))

  ;; Safe sqrt (integer approximation): fails on negative
  (fset 'neovm--cp-safe-isqrt
    (lambda (n sk fk)
      (if (< n 0)
          (funcall fk (format "negative sqrt: %d" n))
        (funcall sk (floor (sqrt n))))))

  ;; Parse integer from string: fails on non-numeric
  (fset 'neovm--cp-parse-int
    (lambda (s sk fk)
      (let ((n (string-to-number s)))
        (if (and (= n 0) (not (string= s "0")))
            (funcall fk (format "not a number: %s" s))
          (funcall sk n)))))

  ;; Chain: pipe value through list of 2-continuation functions
  (fset 'neovm--cp-chain-2k
    (lambda (val fns sk fk)
      (if (null fns)
          (funcall sk val)
        (funcall (car fns) val
                 (lambda (next-val)
                   (funcall 'neovm--cp-chain-2k next-val (cdr fns) sk fk))
                 fk))))

  ;; Try-catch in 2k CPS: run body, if failure call handler
  (fset 'neovm--cp-try-2k
    (lambda (body-fn handler-fn sk)
      (funcall body-fn sk
               (lambda (err) (funcall handler-fn err sk)))))

  (unwind-protect
      (list
        ;; Successful division
        (funcall 'neovm--cp-safe-div 10 3
                 (lambda (v) (list :ok v))
                 (lambda (e) (list :err e)))

        ;; Division by zero
        (funcall 'neovm--cp-safe-div 10 0
                 (lambda (v) (list :ok v))
                 (lambda (e) (list :err e)))

        ;; Chain: 100 / 5 / 4 = 5
        (funcall 'neovm--cp-chain-2k 100
                 (list (lambda (v sk fk) (funcall 'neovm--cp-safe-div v 5 sk fk))
                       (lambda (v sk fk) (funcall 'neovm--cp-safe-div v 4 sk fk)))
                 (lambda (v) (list :ok v))
                 (lambda (e) (list :err e)))

        ;; Chain with failure in middle: 100 / 5 / 0 / 3
        (funcall 'neovm--cp-chain-2k 100
                 (list (lambda (v sk fk) (funcall 'neovm--cp-safe-div v 5 sk fk))
                       (lambda (v sk fk) (funcall 'neovm--cp-safe-div v 0 sk fk))
                       (lambda (v sk fk) (funcall 'neovm--cp-safe-div v 3 sk fk)))
                 (lambda (v) (list :ok v))
                 (lambda (e) (list :err e)))

        ;; Parse then sqrt: "49" -> 49 -> 7
        (funcall 'neovm--cp-chain-2k "49"
                 (list (lambda (v sk fk) (funcall 'neovm--cp-parse-int v sk fk))
                       (lambda (v sk fk) (funcall 'neovm--cp-safe-isqrt v sk fk)))
                 (lambda (v) (list :ok v))
                 (lambda (e) (list :err e)))

        ;; Parse fails: "abc" -> error
        (funcall 'neovm--cp-chain-2k "abc"
                 (list (lambda (v sk fk) (funcall 'neovm--cp-parse-int v sk fk))
                       (lambda (v sk fk) (funcall 'neovm--cp-safe-isqrt v sk fk)))
                 (lambda (v) (list :ok v))
                 (lambda (e) (list :err e)))

        ;; Try-catch: recover from division by zero
        (funcall 'neovm--cp-try-2k
                 (lambda (sk fk) (funcall 'neovm--cp-safe-div 10 0 sk fk))
                 (lambda (err sk) (funcall sk (list :recovered-from err)))
                 #'identity)

        ;; Nested try-catch: inner catches, outer sees recovered value
        (funcall 'neovm--cp-try-2k
                 (lambda (sk fk)
                   (funcall 'neovm--cp-try-2k
                            (lambda (sk2 fk2)
                              (funcall 'neovm--cp-safe-div 42 0 sk2 fk2))
                            (lambda (err sk2) (funcall sk2 -1))
                            sk))
                 (lambda (err sk) (funcall sk :outer-caught))
                 #'identity)

        ;; All-or-nothing: process list of parse operations
        (let ((inputs '("10" "20" "30")))
          (funcall 'neovm--cp-chain-2k
                   inputs
                   (list (lambda (lst sk fk)
                           ;; Parse all strings, fail on first error
                           (let ((parsed nil)
                                 (remaining lst)
                                 (error-found nil))
                             (while (and remaining (not error-found))
                               (funcall 'neovm--cp-parse-int (car remaining)
                                        (lambda (n) (push n parsed))
                                        (lambda (e) (setq error-found e)))
                               (setq remaining (cdr remaining)))
                             (if error-found
                                 (funcall fk error-found)
                               (funcall sk (nreverse parsed))))))
                   (lambda (v) (list :ok v))
                   (lambda (e) (list :err e)))))
    (fmakunbound 'neovm--cp-safe-div)
    (fmakunbound 'neovm--cp-safe-isqrt)
    (fmakunbound 'neovm--cp-parse-int)
    (fmakunbound 'neovm--cp-chain-2k)
    (fmakunbound 'neovm--cp-try-2k)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:ok 3) (:err \"division by zero: 10 / 0\") (:ok 5) (:err \"division by zero: 20 / 0\") (:ok 7) (:err \"not a number: abc\") (:recovered-from \"division by zero: 10 / 0\") -1 (:ok (10 20 30)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: CPS trampoline for stack-safe recursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_trampoline_stack_safe() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Trampolined CPS: instead of direct recursive calls (which build stack),
    // return thunks. The trampoline bounces until a final value is produced.
    let form = r#"(progn
  ;; Trampoline: run thunks until non-function result
  (fset 'neovm--cp-trampoline
    (lambda (thunk)
      (let ((result thunk))
        (while (functionp result)
          (setq result (funcall result)))
        result)))

  ;; Trampolined CPS factorial
  (fset 'neovm--cp-tfact
    (lambda (n k)
      (if (< n 2)
          (lambda () (funcall k 1))
        (lambda ()
          (funcall 'neovm--cp-tfact (1- n)
                   (lambda (r)
                     (lambda () (funcall k (* n r)))))))))

  ;; Trampolined CPS sum 1..n
  (fset 'neovm--cp-tsum
    (lambda (n k)
      (if (= n 0)
          (lambda () (funcall k 0))
        (lambda ()
          (funcall 'neovm--cp-tsum (1- n)
                   (lambda (r)
                     (lambda () (funcall k (+ n r)))))))))

  ;; Trampolined CPS fibonacci
  (fset 'neovm--cp-tfib
    (lambda (n k)
      (if (< n 2)
          (lambda () (funcall k n))
        (lambda ()
          (funcall 'neovm--cp-tfib (1- n)
                   (lambda (a)
                     (lambda ()
                       (funcall 'neovm--cp-tfib (- n 2)
                                (lambda (b)
                                  (lambda () (funcall k (+ a b))))))))))))

  ;; Trampolined CPS map
  (fset 'neovm--cp-tmap
    (lambda (f lst k)
      (if (null lst)
          (lambda () (funcall k nil))
        (lambda ()
          (funcall 'neovm--cp-tmap f (cdr lst)
                   (lambda (rest)
                     (lambda () (funcall k (cons (funcall f (car lst)) rest)))))))))

  ;; Trampolined CPS filter
  (fset 'neovm--cp-tfilter
    (lambda (pred lst k)
      (if (null lst)
          (lambda () (funcall k nil))
        (lambda ()
          (funcall 'neovm--cp-tfilter pred (cdr lst)
                   (lambda (rest)
                     (lambda ()
                       (if (funcall pred (car lst))
                           (funcall k (cons (car lst) rest))
                         (funcall k rest)))))))))

  (unwind-protect
      (list
        ;; Factorial via trampoline
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tfact 0 #'identity))
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tfact 1 #'identity))
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tfact 5 #'identity))
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tfact 10 #'identity))

        ;; Sum via trampoline
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tsum 0 #'identity))
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tsum 10 #'identity))
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tsum 100 #'identity))

        ;; Fibonacci via trampoline
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tfib 0 #'identity))
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tfib 1 #'identity))
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tfib 10 #'identity))

        ;; Map via trampoline: square each
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tmap
                          (lambda (x) (* x x))
                          '(1 2 3 4 5)
                          #'identity))

        ;; Filter via trampoline: keep evens
        (funcall 'neovm--cp-trampoline
                 (funcall 'neovm--cp-tfilter
                          (lambda (x) (= (% x 2) 0))
                          '(1 2 3 4 5 6 7 8 9 10)
                          #'identity))

        ;; Verify consistency with direct computation
        (= (funcall 'neovm--cp-trampoline
                     (funcall 'neovm--cp-tfact 8 #'identity))
           (* 8 7 6 5 4 3 2 1))
        (= (funcall 'neovm--cp-trampoline
                     (funcall 'neovm--cp-tsum 50 #'identity))
           (/ (* 50 51) 2)))
    (fmakunbound 'neovm--cp-trampoline)
    (fmakunbound 'neovm--cp-tfact)
    (fmakunbound 'neovm--cp-tsum)
    (fmakunbound 'neovm--cp-tfib)
    (fmakunbound 'neovm--cp-tmap)
    (fmakunbound 'neovm--cp-tfilter)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 1 120 3628800 0 55 5050 0 1 55 (1 4 9 16 25) (2 4 6 8 10) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: CPS-based list processing with accumulator pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_accumulator_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CPS with accumulator-passing for tail-recursive-like behavior.
    // Combines CPS with accumulator to avoid deep continuation nesting.
    let form = r#"(progn
  ;; CPS fold-left with accumulator (tail-position CPS)
  (fset 'neovm--cp-foldl
    (lambda (f acc lst k)
      "Left fold in CPS: f takes (acc elem k)."
      (if (null lst)
          (funcall k acc)
        (funcall f acc (car lst)
                 (lambda (new-acc)
                   (funcall 'neovm--cp-foldl f new-acc (cdr lst) k))))))

  ;; CPS reverse via fold
  (fset 'neovm--cp-reverse
    (lambda (lst k)
      (funcall 'neovm--cp-foldl
               (lambda (acc elem k) (funcall k (cons elem acc)))
               nil lst k)))

  ;; CPS flatten nested list (one level)
  (fset 'neovm--cp-flatten1
    (lambda (lst k)
      (funcall 'neovm--cp-foldl
               (lambda (acc elem k)
                 (if (listp elem)
                     (funcall k (append acc elem))
                   (funcall k (append acc (list elem)))))
               nil lst k)))

  ;; CPS group-by: group elements by a key function
  (fset 'neovm--cp-group-by
    (lambda (key-fn lst k)
      (funcall 'neovm--cp-foldl
               (lambda (groups elem k)
                 (let* ((key (funcall key-fn elem))
                        (existing (assoc key groups)))
                   (if existing
                       (progn
                         (setcdr existing (cons elem (cdr existing)))
                         (funcall k groups))
                     (funcall k (cons (list key elem) groups)))))
               nil lst
               ;; Reverse the value lists in each group
               (lambda (groups)
                 (funcall k
                          (mapcar (lambda (g) (cons (car g) (nreverse (cdr g))))
                                  (nreverse groups)))))))

  ;; CPS zip two lists
  (fset 'neovm--cp-zip
    (lambda (lst1 lst2 k)
      (if (or (null lst1) (null lst2))
          (funcall k nil)
        (funcall 'neovm--cp-zip (cdr lst1) (cdr lst2)
                 (lambda (rest)
                   (funcall k (cons (cons (car lst1) (car lst2)) rest)))))))

  (unwind-protect
      (list
        ;; Fold: sum
        (funcall 'neovm--cp-foldl
                 (lambda (acc x k) (funcall k (+ acc x)))
                 0 '(1 2 3 4 5) #'identity)

        ;; Fold: product
        (funcall 'neovm--cp-foldl
                 (lambda (acc x k) (funcall k (* acc x)))
                 1 '(1 2 3 4 5) #'identity)

        ;; Fold: max
        (funcall 'neovm--cp-foldl
                 (lambda (acc x k) (funcall k (if (> x acc) x acc)))
                 0 '(3 1 4 1 5 9 2 6 5 3) #'identity)

        ;; Fold: count elements satisfying predicate
        (funcall 'neovm--cp-foldl
                 (lambda (acc x k) (funcall k (if (> x 3) (1+ acc) acc)))
                 0 '(1 2 3 4 5 6 7) #'identity)

        ;; Reverse
        (funcall 'neovm--cp-reverse '(1 2 3 4 5) #'identity)

        ;; Flatten
        (funcall 'neovm--cp-flatten1 '((1 2) 3 (4 5 6) 7 (8)) #'identity)

        ;; Group-by: even/odd
        (funcall 'neovm--cp-group-by
                 (lambda (x) (if (= (% x 2) 0) 'even 'odd))
                 '(1 2 3 4 5 6 7 8)
                 #'identity)

        ;; Zip
        (funcall 'neovm--cp-zip '(a b c) '(1 2 3) #'identity)
        (funcall 'neovm--cp-zip '(a b c) '(1 2) #'identity)
        (funcall 'neovm--cp-zip nil '(1 2 3) #'identity)

        ;; Complex pipeline: reverse, then fold to build string
        (funcall 'neovm--cp-reverse '("world" "brave" "new" "hello")
                 (lambda (reversed)
                   (funcall 'neovm--cp-foldl
                            (lambda (acc s k)
                              (funcall k (if (string-empty-p acc) s (concat acc " " s))))
                            "" reversed #'identity))))
    (fmakunbound 'neovm--cp-foldl)
    (fmakunbound 'neovm--cp-reverse)
    (fmakunbound 'neovm--cp-flatten1)
    (fmakunbound 'neovm--cp-group-by)
    (fmakunbound 'neovm--cp-zip)))"#;
    let expect = expect_test::expect![[
        r#""OK (15 120 9 4 (5 4 3 2 1) (1 2 3 4 5 6 7 8) ((odd 1 3 5 7) (even 2 4 6 8)) ((a . 1) (b . 2) (c . 3)) ((a . 1) (b . 2)) nil \"hello new brave world\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
