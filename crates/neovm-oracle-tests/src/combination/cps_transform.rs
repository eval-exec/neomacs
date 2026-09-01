//! Oracle parity tests for continuation-passing style (CPS) transformation
//! in Elisp: CPS-transform simple expressions (arithmetic, let, if),
//! trampolined CPS for stack safety, CPS-based exception handling,
//! CPS-based coroutines, and a CPS-based interpreter for a small language.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic CPS transformation: arithmetic and let expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_basic_arithmetic_and_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CPS-transform arithmetic: instead of returning values directly,
    // pass results to continuation functions.
    // Direct: (+ (* 2 3) (* 4 5))
    // CPS:    (mul-k 2 3 (lambda (v1) (mul-k 4 5 (lambda (v2) (add-k v1 v2 k)))))
    let form = r#"(progn
  ;; CPS arithmetic primitives: each takes operands + continuation
  (fset 'neovm--cps-add-k
    (lambda (a b k) (funcall k (+ a b))))

  (fset 'neovm--cps-sub-k
    (lambda (a b k) (funcall k (- a b))))

  (fset 'neovm--cps-mul-k
    (lambda (a b k) (funcall k (* a b))))

  (fset 'neovm--cps-div-k
    (lambda (a b k) (funcall k (/ a b))))

  ;; CPS let-binding: evaluate expr with continuation that binds result
  (fset 'neovm--cps-let-k
    (lambda (expr-fn body-fn k)
      "Evaluate EXPR-FN in CPS, bind result, pass to BODY-FN in CPS."
      (funcall expr-fn
               (lambda (val)
                 (funcall body-fn val k)))))

  (unwind-protect
      (list
        ;; Simple: (+ 1 2) in CPS
        (funcall 'neovm--cps-add-k 1 2 #'identity)

        ;; Nested: (+ (* 2 3) (* 4 5)) in CPS
        (funcall 'neovm--cps-mul-k 2 3
                 (lambda (v1)
                   (funcall 'neovm--cps-mul-k 4 5
                            (lambda (v2)
                              (funcall 'neovm--cps-add-k v1 v2 #'identity)))))

        ;; Complex: (- (+ (* 2 3) 4) (/ 10 2)) in CPS
        (funcall 'neovm--cps-mul-k 2 3
                 (lambda (v1)
                   (funcall 'neovm--cps-add-k v1 4
                            (lambda (v2)
                              (funcall 'neovm--cps-div-k 10 2
                                       (lambda (v3)
                                         (funcall 'neovm--cps-sub-k v2 v3 #'identity)))))))

        ;; CPS let: (let ((x (* 3 4))) (+ x 10))
        (funcall 'neovm--cps-let-k
                 (lambda (k) (funcall 'neovm--cps-mul-k 3 4 k))
                 (lambda (x k) (funcall 'neovm--cps-add-k x 10 k))
                 #'identity)

        ;; Nested CPS let: (let ((x (* 2 5))) (let ((y (+ x 3))) (* y y)))
        (funcall 'neovm--cps-let-k
                 (lambda (k) (funcall 'neovm--cps-mul-k 2 5 k))
                 (lambda (x k)
                   (funcall 'neovm--cps-let-k
                            (lambda (k2) (funcall 'neovm--cps-add-k x 3 k2))
                            (lambda (y k3) (funcall 'neovm--cps-mul-k y y k3))
                            k))
                 #'identity)

        ;; Chain of operations: ((1 + 2) * 3 - 4) / 5
        (funcall 'neovm--cps-add-k 1 2
                 (lambda (v1)
                   (funcall 'neovm--cps-mul-k v1 3
                            (lambda (v2)
                              (funcall 'neovm--cps-sub-k v2 4
                                       (lambda (v3)
                                         (funcall 'neovm--cps-div-k v3 5 #'identity))))))))
    (fmakunbound 'neovm--cps-add-k)
    (fmakunbound 'neovm--cps-sub-k)
    (fmakunbound 'neovm--cps-mul-k)
    (fmakunbound 'neovm--cps-div-k)
    (fmakunbound 'neovm--cps-let-k)))"#;
    let expect = expect_test::expect![[r#""OK (3 26 5 22 169 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPS with if-expressions and boolean logic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_conditional_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CPS-transform conditional expressions:
    // (if test then else) becomes (test-k (lambda (b) (if b (then-k k) (else-k k))))
    let form = r#"(progn
  ;; CPS if: evaluate test, then branch based on result
  (fset 'neovm--cps-if-k
    (lambda (test-fn then-fn else-fn k)
      (funcall test-fn
               (lambda (condition)
                 (if condition
                     (funcall then-fn k)
                   (funcall else-fn k))))))

  ;; CPS comparison operators
  (fset 'neovm--cps-lt-k
    (lambda (a b k) (funcall k (< a b))))
  (fset 'neovm--cps-gt-k
    (lambda (a b k) (funcall k (> a b))))
  (fset 'neovm--cps-eq-k
    (lambda (a b k) (funcall k (= a b))))

  ;; CPS and/or
  (fset 'neovm--cps-and-k
    (lambda (a-fn b-fn k)
      (funcall a-fn
               (lambda (va)
                 (if va
                     (funcall b-fn k)
                   (funcall k nil))))))

  (fset 'neovm--cps-or-k
    (lambda (a-fn b-fn k)
      (funcall a-fn
               (lambda (va)
                 (if va
                     (funcall k va)
                   (funcall b-fn k))))))

  (unwind-protect
      (list
        ;; Simple if: (if (< 3 5) "yes" "no")
        (funcall 'neovm--cps-if-k
                 (lambda (k) (funcall 'neovm--cps-lt-k 3 5 k))
                 (lambda (k) (funcall k "yes"))
                 (lambda (k) (funcall k "no"))
                 #'identity)

        ;; Nested if: (if (> x 0) (if (< x 10) "single-digit" "big") "negative")
        ;; with x = 7
        (let ((x 7))
          (funcall 'neovm--cps-if-k
                   (lambda (k) (funcall 'neovm--cps-gt-k x 0 k))
                   (lambda (k)
                     (funcall 'neovm--cps-if-k
                              (lambda (k2) (funcall 'neovm--cps-lt-k x 10 k2))
                              (lambda (k2) (funcall k2 "single-digit"))
                              (lambda (k2) (funcall k2 "big"))
                              k))
                   (lambda (k) (funcall k "negative"))
                   #'identity))

        ;; CPS and: (and (> 5 3) (< 5 10))
        (funcall 'neovm--cps-and-k
                 (lambda (k) (funcall 'neovm--cps-gt-k 5 3 k))
                 (lambda (k) (funcall 'neovm--cps-lt-k 5 10 k))
                 #'identity)

        ;; CPS or: (or (= 3 5) (= 3 3))
        (funcall 'neovm--cps-or-k
                 (lambda (k) (funcall 'neovm--cps-eq-k 3 5 k))
                 (lambda (k) (funcall 'neovm--cps-eq-k 3 3 k))
                 #'identity)

        ;; abs(x) in CPS: (if (< x 0) (- 0 x) x)
        (let ((x -42))
          (funcall 'neovm--cps-if-k
                   (lambda (k) (funcall 'neovm--cps-lt-k x 0 k))
                   (lambda (k) (funcall k (- 0 x)))
                   (lambda (k) (funcall k x))
                   #'identity))

        ;; Fibonacci-like decision tree in CPS
        ;; fib-category(n): n<2 -> "base", n<10 -> "small", else "large"
        (let ((n 15))
          (funcall 'neovm--cps-if-k
                   (lambda (k) (funcall 'neovm--cps-lt-k n 2 k))
                   (lambda (k) (funcall k "base"))
                   (lambda (k)
                     (funcall 'neovm--cps-if-k
                              (lambda (k2) (funcall 'neovm--cps-lt-k n 10 k2))
                              (lambda (k2) (funcall k2 "small"))
                              (lambda (k2) (funcall k2 "large"))
                              k))
                   #'identity)))
    (fmakunbound 'neovm--cps-if-k)
    (fmakunbound 'neovm--cps-lt-k)
    (fmakunbound 'neovm--cps-gt-k)
    (fmakunbound 'neovm--cps-eq-k)
    (fmakunbound 'neovm--cps-and-k)
    (fmakunbound 'neovm--cps-or-k)))"#;
    let expect = expect_test::expect![[r#""OK (\"yes\" \"single-digit\" t t 42 \"large\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trampolined CPS: stack-safe recursion via thunks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_trampolined_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Trampoline: instead of directly calling continuations (which builds
    // stack), return thunks (zero-arg lambdas). The trampoline loop
    // bounces until it gets a non-function value.
    let form = r#"(progn
  ;; Trampoline: run thunks until result is not a function
  (fset 'neovm--cps-trampoline
    (lambda (thunk)
      (let ((result thunk))
        (while (functionp result)
          (setq result (funcall result)))
        result)))

  ;; Mark a value as a final result (wrap in a cons to distinguish)
  (fset 'neovm--cps-done
    (lambda (val) (cons :done val)))

  (fset 'neovm--cps-done-p
    (lambda (val) (and (consp val) (eq (car val) :done))))

  ;; Trampolined trampoline that handles :done markers
  (fset 'neovm--cps-bounce
    (lambda (thunk)
      (let ((result thunk))
        (while (functionp result)
          (setq result (funcall result)))
        (if (funcall 'neovm--cps-done-p result)
            (cdr result)
          result))))

  ;; Trampolined factorial in CPS
  (fset 'neovm--cps-fact-k
    (lambda (n k)
      "CPS factorial returning thunks for trampoline."
      (if (< n 2)
          (lambda () (funcall k 1))
        (lambda ()
          (funcall 'neovm--cps-fact-k (1- n)
                   (lambda (r)
                     (lambda () (funcall k (* n r)))))))))

  ;; Trampolined sum 1..n in CPS
  (fset 'neovm--cps-sum-k
    (lambda (n k)
      (if (= n 0)
          (lambda () (funcall k 0))
        (lambda ()
          (funcall 'neovm--cps-sum-k (1- n)
                   (lambda (r)
                     (lambda () (funcall k (+ n r)))))))))

  ;; Trampolined Fibonacci (double recursion, CPS)
  (fset 'neovm--cps-fib-k
    (lambda (n k)
      (if (< n 2)
          (lambda () (funcall k n))
        (lambda ()
          (funcall 'neovm--cps-fib-k (- n 1)
                   (lambda (a)
                     (lambda ()
                       (funcall 'neovm--cps-fib-k (- n 2)
                                (lambda (b)
                                  (lambda () (funcall k (+ a b))))))))))))

  (unwind-protect
      (list
        ;; Factorial
        (funcall 'neovm--cps-trampoline
                 (funcall 'neovm--cps-fact-k 0 #'identity))
        (funcall 'neovm--cps-trampoline
                 (funcall 'neovm--cps-fact-k 1 #'identity))
        (funcall 'neovm--cps-trampoline
                 (funcall 'neovm--cps-fact-k 5 #'identity))
        (funcall 'neovm--cps-trampoline
                 (funcall 'neovm--cps-fact-k 10 #'identity))
        ;; Sum
        (funcall 'neovm--cps-trampoline
                 (funcall 'neovm--cps-sum-k 0 #'identity))
        (funcall 'neovm--cps-trampoline
                 (funcall 'neovm--cps-sum-k 10 #'identity))
        (funcall 'neovm--cps-trampoline
                 (funcall 'neovm--cps-sum-k 100 #'identity))
        ;; Fibonacci
        (funcall 'neovm--cps-trampoline
                 (funcall 'neovm--cps-fib-k 0 #'identity))
        (funcall 'neovm--cps-trampoline
                 (funcall 'neovm--cps-fib-k 1 #'identity))
        (funcall 'neovm--cps-trampoline
                 (funcall 'neovm--cps-fib-k 10 #'identity))
        (funcall 'neovm--cps-trampoline
                 (funcall 'neovm--cps-fib-k 15 #'identity)))
    (fmakunbound 'neovm--cps-trampoline)
    (fmakunbound 'neovm--cps-done)
    (fmakunbound 'neovm--cps-done-p)
    (fmakunbound 'neovm--cps-bounce)
    (fmakunbound 'neovm--cps-fact-k)
    (fmakunbound 'neovm--cps-sum-k)
    (fmakunbound 'neovm--cps-fib-k)))"#;
    let expect = expect_test::expect![[r#""OK (1 1 120 3628800 0 55 5050 0 1 55 610)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPS-based exception handling: error/success continuations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_exception_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Instead of a single continuation k, pass two: success-k and error-k.
    // Operations can signal errors via error-k instead of success-k.
    let form = r#"(progn
  ;; CPS safe-divide: divides or calls error-k on zero divisor
  (fset 'neovm--cps-safe-div
    (lambda (a b success-k error-k)
      (if (= b 0)
          (funcall error-k (list 'division-by-zero a b))
        (funcall success-k (/ a b)))))

  ;; CPS safe-sqrt: errors on negative input
  (fset 'neovm--cps-safe-sqrt
    (lambda (x success-k error-k)
      (if (< x 0)
          (funcall error-k (list 'negative-sqrt x))
        (funcall success-k (sqrt x)))))

  ;; CPS try-catch: wraps a CPS computation, catches errors
  (fset 'neovm--cps-try-catch
    (lambda (body-fn handler-fn final-k)
      "Run BODY-FN in CPS with error handler. On success pass to FINAL-K.
       On error, call HANDLER-FN with the error, then pass result to FINAL-K."
      (funcall body-fn
               final-k
               (lambda (err) (funcall handler-fn err final-k)))))

  ;; CPS pipeline: chain operations, short-circuit on error
  (fset 'neovm--cps-chain
    (lambda (val fns success-k error-k)
      "Thread VAL through list of CPS functions (each takes val, sk, ek)."
      (if (null fns)
          (funcall success-k val)
        (funcall (car fns) val
                 (lambda (next-val)
                   (funcall 'neovm--cps-chain next-val (cdr fns) success-k error-k))
                 error-k))))

  (unwind-protect
      (list
        ;; Successful division
        (funcall 'neovm--cps-safe-div 10 2
                 (lambda (v) (list :ok v))
                 (lambda (e) (list :error e)))

        ;; Division by zero
        (funcall 'neovm--cps-safe-div 10 0
                 (lambda (v) (list :ok v))
                 (lambda (e) (list :error e)))

        ;; Try-catch: successful computation
        (funcall 'neovm--cps-try-catch
                 (lambda (sk ek)
                   (funcall 'neovm--cps-safe-div 100 5 sk ek))
                 (lambda (err k) (funcall k (list :caught err)))
                 #'identity)

        ;; Try-catch: catching division by zero
        (funcall 'neovm--cps-try-catch
                 (lambda (sk ek)
                   (funcall 'neovm--cps-safe-div 100 0 sk ek))
                 (lambda (err k) (funcall k (list :caught err)))
                 #'identity)

        ;; Chain: 100 -> /5 -> /4 -> /1 = 5
        (funcall 'neovm--cps-chain 100
                 (list (lambda (v sk ek) (funcall 'neovm--cps-safe-div v 5 sk ek))
                       (lambda (v sk ek) (funcall 'neovm--cps-safe-div v 4 sk ek))
                       (lambda (v sk ek) (funcall 'neovm--cps-safe-div v 1 sk ek)))
                 (lambda (v) (list :ok v))
                 (lambda (e) (list :error e)))

        ;; Chain with error in middle: 100 -> /5 -> /0 -> /1
        (funcall 'neovm--cps-chain 100
                 (list (lambda (v sk ek) (funcall 'neovm--cps-safe-div v 5 sk ek))
                       (lambda (v sk ek) (funcall 'neovm--cps-safe-div v 0 sk ek))
                       (lambda (v sk ek) (funcall 'neovm--cps-safe-div v 1 sk ek)))
                 (lambda (v) (list :ok v))
                 (lambda (e) (list :error e)))

        ;; Nested try-catch: inner catches, outer sees success
        (funcall 'neovm--cps-try-catch
                 (lambda (outer-sk outer-ek)
                   (funcall 'neovm--cps-try-catch
                            (lambda (inner-sk inner-ek)
                              (funcall 'neovm--cps-safe-div 10 0 inner-sk inner-ek))
                            (lambda (err k)
                              (funcall k 999))  ;; recovery value
                            outer-sk))
                 (lambda (err k) (funcall k (list :outer-caught err)))
                 #'identity))
    (fmakunbound 'neovm--cps-safe-div)
    (fmakunbound 'neovm--cps-safe-sqrt)
    (fmakunbound 'neovm--cps-try-catch)
    (fmakunbound 'neovm--cps-chain)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:ok 5) (:error (division-by-zero 10 0)) 20 (:caught (division-by-zero 100 0)) (:ok 5) (:error (division-by-zero 20 0)) 999)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPS-based coroutines: yield/resume via continuation capture
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_coroutines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate coroutines by capturing continuations at yield points.
    // A coroutine is a state machine: each "yield" saves the remaining
    // computation as a continuation and returns the yielded value.
    let form = r#"(progn
  ;; A coroutine step result: either (:yield value continuation) or (:done value)
  (fset 'neovm--cps-yield
    (lambda (val k) (list :yield val k)))

  (fset 'neovm--cps-return
    (lambda (val) (list :done val)))

  ;; Resume a coroutine: feed a value into its continuation
  (fset 'neovm--cps-resume
    (lambda (coro input)
      (if (eq (car coro) :yield)
          (funcall (caddr coro) input)
        coro)))

  ;; Collect all yields from a coroutine, always feeding nil on resume
  (fset 'neovm--cps-collect-yields
    (lambda (coro)
      (let ((result nil) (current coro))
        (while (eq (car current) :yield)
          (setq result (cons (cadr current) result))
          (setq current (funcall 'neovm--cps-resume current nil)))
        ;; Include final :done value
        (list :yields (nreverse result)
              :final (cadr current)))))

  ;; Example coroutine: counts from start, yielding each number
  (fset 'neovm--cps-count-coro
    (lambda (start count)
      "Coroutine that yields start, start+1, ..., start+count-1, then returns total."
      (let ((i 0) (total 0))
        (fset 'neovm--cps-count-step
          (lambda (n idx cnt tot k)
            (if (>= idx cnt)
                (funcall 'neovm--cps-return tot)
              (funcall 'neovm--cps-yield (+ n idx)
                       (lambda (_input)
                         (funcall 'neovm--cps-count-step n (1+ idx) cnt (+ tot (+ n idx)) k))))))
        (funcall 'neovm--cps-count-step start 0 count 0 nil))))

  ;; Example coroutine: accumulator that yields running sums
  (fset 'neovm--cps-accum-coro
    (lambda (values)
      "Coroutine yielding running sums of VALUES list."
      (fset 'neovm--cps-accum-step
        (lambda (remaining acc)
          (if (null remaining)
              (funcall 'neovm--cps-return acc)
            (let ((new-acc (+ acc (car remaining))))
              (funcall 'neovm--cps-yield new-acc
                       (lambda (_input)
                         (funcall 'neovm--cps-accum-step (cdr remaining) new-acc)))))))
      (funcall 'neovm--cps-accum-step values 0)))

  (unwind-protect
      (list
        ;; Count coroutine: 0,1,2,3,4
        (funcall 'neovm--cps-collect-yields
                 (funcall 'neovm--cps-count-coro 0 5))

        ;; Count coroutine: 10,11,12
        (funcall 'neovm--cps-collect-yields
                 (funcall 'neovm--cps-count-coro 10 3))

        ;; Count coroutine: empty (0 iterations)
        (funcall 'neovm--cps-collect-yields
                 (funcall 'neovm--cps-count-coro 0 0))

        ;; Accumulator coroutine
        (funcall 'neovm--cps-collect-yields
                 (funcall 'neovm--cps-accum-coro '(1 2 3 4 5)))

        ;; Manual stepping: yield, resume, yield, resume
        (let ((c (funcall 'neovm--cps-count-coro 100 3)))
          (let ((s1 c))                                 ;; (:yield 100 k)
            (let ((v1 (cadr s1)))
              (let ((s2 (funcall 'neovm--cps-resume s1 nil)))  ;; (:yield 101 k)
                (let ((v2 (cadr s2)))
                  (let ((s3 (funcall 'neovm--cps-resume s2 nil))) ;; (:yield 102 k)
                    (let ((v3 (cadr s3)))
                      (let ((s4 (funcall 'neovm--cps-resume s3 nil))) ;; (:done 303)
                        (list v1 v2 v3 s4))))))))))
    (fmakunbound 'neovm--cps-yield)
    (fmakunbound 'neovm--cps-return)
    (fmakunbound 'neovm--cps-resume)
    (fmakunbound 'neovm--cps-collect-yields)
    (fmakunbound 'neovm--cps-count-coro)
    (fmakunbound 'neovm--cps-count-step)
    (fmakunbound 'neovm--cps-accum-coro)
    (fmakunbound 'neovm--cps-accum-step)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:yields (0 1 2 3 4) :final 10) (:yields (10 11 12) :final 33) (:yields nil :final 0) (:yields (1 3 6 10 15) :final 15) (100 101 102 (:done 303)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPS-based interpreter for a small expression language
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_interpreter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A CPS interpreter for a small language with:
    // - numbers (self-evaluating)
    // - variables (lookup in environment)
    // - (add e1 e2), (mul e1 e2), (sub e1 e2)
    // - (if-pos e1 e2 e3)  -- if e1 > 0 then e2 else e3
    // - (let1 var e1 e2)   -- let var = e1 in e2
    // - (seq e1 e2)        -- evaluate e1, discard, evaluate e2
    let form = r#"(progn
  (fset 'neovm--cps-interp
    (lambda (expr env k)
      "CPS interpreter: evaluate EXPR in ENV, pass result to K."
      (cond
        ;; Number: self-evaluating
        ((numberp expr)
         (funcall k expr))
        ;; Symbol: variable lookup
        ((symbolp expr)
         (let ((binding (assq expr env)))
           (if binding
               (funcall k (cdr binding))
             (funcall k (list :error 'unbound expr)))))
        ;; Compound expression
        ((consp expr)
         (let ((op (car expr)))
           (cond
             ;; (add e1 e2)
             ((eq op 'add)
              (funcall 'neovm--cps-interp (nth 1 expr) env
                       (lambda (v1)
                         (funcall 'neovm--cps-interp (nth 2 expr) env
                                  (lambda (v2)
                                    (funcall k (+ v1 v2)))))))
             ;; (sub e1 e2)
             ((eq op 'sub)
              (funcall 'neovm--cps-interp (nth 1 expr) env
                       (lambda (v1)
                         (funcall 'neovm--cps-interp (nth 2 expr) env
                                  (lambda (v2)
                                    (funcall k (- v1 v2)))))))
             ;; (mul e1 e2)
             ((eq op 'mul)
              (funcall 'neovm--cps-interp (nth 1 expr) env
                       (lambda (v1)
                         (funcall 'neovm--cps-interp (nth 2 expr) env
                                  (lambda (v2)
                                    (funcall k (* v1 v2)))))))
             ;; (if-pos e1 e2 e3)
             ((eq op 'if-pos)
              (funcall 'neovm--cps-interp (nth 1 expr) env
                       (lambda (test-val)
                         (if (> test-val 0)
                             (funcall 'neovm--cps-interp (nth 2 expr) env k)
                           (funcall 'neovm--cps-interp (nth 3 expr) env k)))))
             ;; (let1 var e1 e2)
             ((eq op 'let1)
              (funcall 'neovm--cps-interp (nth 2 expr) env
                       (lambda (val)
                         (funcall 'neovm--cps-interp (nth 3 expr)
                                  (cons (cons (nth 1 expr) val) env)
                                  k))))
             ;; (seq e1 e2)
             ((eq op 'seq)
              (funcall 'neovm--cps-interp (nth 1 expr) env
                       (lambda (_)
                         (funcall 'neovm--cps-interp (nth 2 expr) env k))))
             (t (funcall k (list :error 'unknown-op op))))))
        (t (funcall k (list :error 'bad-expr expr))))))

  ;; Helper to run the interpreter
  (fset 'neovm--cps-run
    (lambda (expr &optional env)
      (funcall 'neovm--cps-interp expr (or env nil) #'identity)))

  (unwind-protect
      (list
        ;; Simple number
        (funcall 'neovm--cps-run 42)
        ;; Addition
        (funcall 'neovm--cps-run '(add 10 20))
        ;; Nested arithmetic: (2 * 3) + (10 - 4)
        (funcall 'neovm--cps-run '(add (mul 2 3) (sub 10 4)))
        ;; Variable binding: let x = 5 in x + x
        (funcall 'neovm--cps-run '(let1 x 5 (add x x)))
        ;; Nested let: let x=3 in let y=x*x in x+y
        (funcall 'neovm--cps-run '(let1 x 3 (let1 y (mul x x) (add x y))))
        ;; Conditional: if-pos 1 then 100 else 200
        (funcall 'neovm--cps-run '(if-pos 1 100 200))
        ;; Conditional: if-pos -1 then 100 else 200
        (funcall 'neovm--cps-run '(if-pos (sub 0 1) 100 200))
        ;; Complex: abs(x) = if-pos x then x else 0-x
        ;; with x = -7
        (funcall 'neovm--cps-run
                 '(let1 x (sub 0 7)
                    (if-pos x x (sub 0 x))))
        ;; Seq: evaluate two things, return second
        (funcall 'neovm--cps-run '(seq (add 1 2) (mul 3 4)))
        ;; Full program: compute max(a,b) where a=15, b=23
        ;; max = if-pos (a-b) then a else b
        (funcall 'neovm--cps-run
                 '(let1 a 15
                    (let1 b 23
                      (if-pos (sub a b) a b))))
        ;; Variable from environment
        (funcall 'neovm--cps-run 'z '((z . 999))))
    (fmakunbound 'neovm--cps-interp)
    (fmakunbound 'neovm--cps-run)))"#;
    let expect = expect_test::expect![[r#""OK (42 30 12 10 12 100 200 7 12 23 999)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPS transformation of map/filter/fold
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_higher_order_transforms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // CPS versions of map, filter, and fold that thread continuations
    // through list processing
    let form = r#"(progn
  ;; CPS map: apply f-k (a CPS function) to each element
  (fset 'neovm--cps-map-k
    (lambda (f-k lst k)
      "Map F-K over LST in CPS. F-K takes (elem k) and calls k with result."
      (if (null lst)
          (funcall k nil)
        (funcall f-k (car lst)
                 (lambda (head)
                   (funcall 'neovm--cps-map-k f-k (cdr lst)
                            (lambda (tail)
                              (funcall k (cons head tail)))))))))

  ;; CPS filter: keep elements where pred-k calls k with non-nil
  (fset 'neovm--cps-filter-k
    (lambda (pred-k lst k)
      (if (null lst)
          (funcall k nil)
        (funcall pred-k (car lst)
                 (lambda (keep)
                   (funcall 'neovm--cps-filter-k pred-k (cdr lst)
                            (lambda (rest)
                              (if keep
                                  (funcall k (cons (car lst) rest))
                                (funcall k rest)))))))))

  ;; CPS fold-left
  (fset 'neovm--cps-foldl-k
    (lambda (f-k acc lst k)
      "Left fold in CPS. F-K takes (acc elem k)."
      (if (null lst)
          (funcall k acc)
        (funcall f-k acc (car lst)
                 (lambda (new-acc)
                   (funcall 'neovm--cps-foldl-k f-k new-acc (cdr lst) k))))))

  ;; CPS flatmap: map then flatten
  (fset 'neovm--cps-flatmap-k
    (lambda (f-k lst k)
      "FlatMap: f-k returns a list for each element, results are concatenated."
      (funcall 'neovm--cps-map-k f-k lst
               (lambda (lists)
                 (funcall k (apply #'append lists))))))

  (unwind-protect
      (list
        ;; Map: square each number
        (funcall 'neovm--cps-map-k
                 (lambda (x k) (funcall k (* x x)))
                 '(1 2 3 4 5)
                 #'identity)

        ;; Map: convert to strings
        (funcall 'neovm--cps-map-k
                 (lambda (x k) (funcall k (number-to-string x)))
                 '(10 20 30)
                 #'identity)

        ;; Filter: keep even numbers
        (funcall 'neovm--cps-filter-k
                 (lambda (x k) (funcall k (= (% x 2) 0)))
                 '(1 2 3 4 5 6 7 8 9 10)
                 #'identity)

        ;; Filter: keep strings longer than 3
        (funcall 'neovm--cps-filter-k
                 (lambda (s k) (funcall k (> (length s) 3)))
                 '("hi" "hello" "yo" "world" "ok" "great")
                 #'identity)

        ;; Fold: sum
        (funcall 'neovm--cps-foldl-k
                 (lambda (acc x k) (funcall k (+ acc x)))
                 0 '(1 2 3 4 5)
                 #'identity)

        ;; Fold: build string
        (funcall 'neovm--cps-foldl-k
                 (lambda (acc x k) (funcall k (concat acc (if (string-empty-p acc) "" "-") x)))
                 "" '("hello" "brave" "new" "world")
                 #'identity)

        ;; Pipeline: filter even -> map square -> fold sum
        (funcall 'neovm--cps-filter-k
                 (lambda (x k) (funcall k (= (% x 2) 0)))
                 '(1 2 3 4 5 6 7 8 9 10)
                 (lambda (evens)
                   (funcall 'neovm--cps-map-k
                            (lambda (x k) (funcall k (* x x)))
                            evens
                            (lambda (squares)
                              (funcall 'neovm--cps-foldl-k
                                       (lambda (acc x k) (funcall k (+ acc x)))
                                       0 squares #'identity)))))

        ;; Flatmap: each number n -> list of n copies
        (funcall 'neovm--cps-flatmap-k
                 (lambda (n k) (funcall k (make-list n n)))
                 '(1 2 3)
                 #'identity))
    (fmakunbound 'neovm--cps-map-k)
    (fmakunbound 'neovm--cps-filter-k)
    (fmakunbound 'neovm--cps-foldl-k)
    (fmakunbound 'neovm--cps-flatmap-k)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 4 9 16 25) (\"10\" \"20\" \"30\") (2 4 6 8 10) (\"hello\" \"world\" \"great\") 15 \"hello-brave-new-world\" 220 (1 2 2 3 3 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
