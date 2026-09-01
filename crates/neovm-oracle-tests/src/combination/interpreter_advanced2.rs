//! Advanced oracle parity tests for an interpreter built in Elisp:
//! closures with lexical binding simulation, recursive functions with
//! environment chains, tail-call optimization pattern, multi-level
//! scoping, and error handling in the interpreted language.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Closures with lexical binding simulation and environment chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp2_closures_lexical_env() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An interpreter with explicit environment chains. Closures capture
    // their defining environment. Supports nested let, lambda, and
    // mutual references through a shared environment frame.
    let form = r#"(progn
  (fset 'neovm--i2-eval
    (lambda (expr env)
      (cond
       ;; Literals
       ((integerp expr) expr)
       ((stringp expr) expr)
       ((eq expr 't) t)
       ((eq expr 'nil) nil)
       ;; Variable lookup: walk env chain
       ((symbolp expr)
        (let ((found nil) (e env))
          (while (and e (not found))
            (let ((binding (assq expr (car e))))
              (if binding
                  (setq found (cdr binding))
                (setq e (cdr e)))))
          (if found found
            (if e nil
              (list 'error 'unbound (symbol-name expr))))))
       ;; Special forms and operations
       ((consp expr)
        (let ((op (car expr)))
          (cond
           ;; (quote X)
           ((eq op 'quote) (cadr expr))
           ;; (lam (params...) body) - closure captures current env
           ((eq op 'lam)
            (list 'closure (cadr expr) (caddr expr) env))
           ;; (app func arg...) - function application
           ((eq op 'app)
            (let ((func (funcall 'neovm--i2-eval (cadr expr) env))
                  (args (mapcar (lambda (a) (funcall 'neovm--i2-eval a env))
                                (cddr expr))))
              (if (and (consp func) (eq (car func) 'closure))
                  (let* ((params (cadr func))
                         (body (caddr func))
                         (closed-env (cadddr func))
                         ;; Create new frame with param bindings
                         (frame nil))
                    (let ((ps params) (as args))
                      (while ps
                        (setq frame (cons (cons (car ps) (car as)) frame))
                        (setq ps (cdr ps))
                        (setq as (cdr as))))
                    ;; New env: new frame + closure's captured env
                    (funcall 'neovm--i2-eval body (cons frame closed-env)))
                (list 'error 'not-callable func))))
           ;; (let1 var val body) - single binding in new frame
           ((eq op 'let1)
            (let* ((var (cadr expr))
                   (val (funcall 'neovm--i2-eval (caddr expr) env))
                   (frame (list (cons var val))))
              (funcall 'neovm--i2-eval (cadddr expr) (cons frame env))))
           ;; (letrec ((var1 val1) (var2 val2)...) body)
           ;; Create frame first, then evaluate vals in new env (for mutual recursion)
           ((eq op 'letrec)
            (let* ((bindings (cadr expr))
                   (body (caddr expr))
                   (frame (mapcar (lambda (b) (cons (car b) nil)) bindings))
                   (new-env (cons frame env)))
              ;; Evaluate and set each binding in the new env
              (dolist (b bindings)
                (let ((val (funcall 'neovm--i2-eval (cadr b) new-env)))
                  (setcdr (assq (car b) frame) val)))
              (funcall 'neovm--i2-eval body new-env)))
           ;; (if cond then else)
           ((eq op 'if)
            (let ((c (funcall 'neovm--i2-eval (cadr expr) env)))
              (if c
                  (funcall 'neovm--i2-eval (caddr expr) env)
                (if (cdddr expr)
                    (funcall 'neovm--i2-eval (cadddr expr) env)
                  nil))))
           ;; Arithmetic
           ((eq op '+)
            (+ (funcall 'neovm--i2-eval (cadr expr) env)
               (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '-)
            (- (funcall 'neovm--i2-eval (cadr expr) env)
               (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '*)
            (* (funcall 'neovm--i2-eval (cadr expr) env)
               (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '=)
            (= (funcall 'neovm--i2-eval (cadr expr) env)
               (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '<)
            (< (funcall 'neovm--i2-eval (cadr expr) env)
               (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '>)
            (> (funcall 'neovm--i2-eval (cadr expr) env)
               (funcall 'neovm--i2-eval (caddr expr) env)))
           ;; (list ...)
           ((eq op 'mklist)
            (mapcar (lambda (a) (funcall 'neovm--i2-eval a env))
                    (cdr expr)))
           ;; (seq e1 e2) - evaluate both, return e2
           ((eq op 'seq)
            (funcall 'neovm--i2-eval (cadr expr) env)
            (funcall 'neovm--i2-eval (caddr expr) env))
           (t (list 'error 'unknown-op op)))))
       (t (list 'error 'invalid expr)))))

  (unwind-protect
      (list
       ;; 1. Simple closure: make-adder
       (funcall 'neovm--i2-eval
                '(let1 make-adder (lam (n) (lam (x) (+ x n)))
                   (app (app make-adder 10) 5))
                nil)

       ;; 2. Nested closures with env chain
       (funcall 'neovm--i2-eval
                '(let1 x 100
                   (let1 f (lam (y) (+ x y))
                     (let1 x 200
                       (app f 1))))
                nil)

       ;; 3. Curried multiply
       (funcall 'neovm--i2-eval
                '(let1 curry-mul (lam (a) (lam (b) (* a b)))
                   (let1 double (app curry-mul 2)
                     (let1 triple (app curry-mul 3)
                       (mklist (app double 5) (app triple 5) (app double (app triple 4))))))
                nil)

       ;; 4. Compose higher-order function
       (funcall 'neovm--i2-eval
                '(let1 compose (lam (f g) (lam (x) (app f (app g x))))
                   (let1 inc (lam (x) (+ x 1))
                     (let1 dbl (lam (x) (* x 2))
                       (mklist (app (app compose inc dbl) 5)
                               (app (app compose dbl inc) 5)))))
                nil)

       ;; 5. Letrec: factorial via mutual recursion pattern
       (funcall 'neovm--i2-eval
                '(letrec ((fact (lam (n)
                            (if (= n 0) 1 (* n (app fact (- n 1)))))))
                   (mklist (app fact 0) (app fact 1) (app fact 5) (app fact 7)))
                nil)

       ;; 6. Letrec: mutual recursion (even?/odd?)
       (funcall 'neovm--i2-eval
                '(letrec ((is-even (lam (n) (if (= n 0) t (app is-odd (- n 1)))))
                          (is-odd  (lam (n) (if (= n 0) nil (app is-even (- n 1))))))
                   (mklist (app is-even 0) (app is-even 4) (app is-even 7)
                           (app is-odd 0) (app is-odd 3) (app is-odd 6)))
                nil))
    (fmakunbound 'neovm--i2-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK (15 101 (10 15 24) (11 12) (1 1 120 5040) (t t nil nil t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive functions: Fibonacci, GCD, Ackermann
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp2_recursive_algorithms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--i2-eval
    (lambda (expr env)
      (cond
       ((integerp expr) expr)
       ((eq expr 't) t)
       ((eq expr 'nil) nil)
       ((symbolp expr)
        (let ((found nil) (e env))
          (while (and e (not found))
            (let ((binding (assq expr (car e))))
              (if binding (setq found (cdr binding)) (setq e (cdr e)))))
          (or found 0)))
       ((consp expr)
        (let ((op (car expr)))
          (cond
           ((eq op 'lam) (list 'closure (cadr expr) (caddr expr) env))
           ((eq op 'app)
            (let ((func (funcall 'neovm--i2-eval (cadr expr) env))
                  (args (mapcar (lambda (a) (funcall 'neovm--i2-eval a env)) (cddr expr))))
              (if (and (consp func) (eq (car func) 'closure))
                  (let ((frame nil) (ps (cadr func)) (as args))
                    (while ps
                      (setq frame (cons (cons (car ps) (car as)) frame))
                      (setq ps (cdr ps)) (setq as (cdr as)))
                    (funcall 'neovm--i2-eval (caddr func) (cons frame (cadddr func))))
                nil)))
           ((eq op 'letrec)
            (let* ((bindings (cadr expr)) (body (caddr expr))
                   (frame (mapcar (lambda (b) (cons (car b) nil)) bindings))
                   (new-env (cons frame env)))
              (dolist (b bindings)
                (setcdr (assq (car b) frame) (funcall 'neovm--i2-eval (cadr b) new-env)))
              (funcall 'neovm--i2-eval body new-env)))
           ((eq op 'if)
            (if (funcall 'neovm--i2-eval (cadr expr) env)
                (funcall 'neovm--i2-eval (caddr expr) env)
              (if (cdddr expr) (funcall 'neovm--i2-eval (cadddr expr) env) nil)))
           ((eq op '+) (+ (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '-) (- (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '*) (* (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '=) (= (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '<) (< (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '>) (> (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op 'mklist) (mapcar (lambda (a) (funcall 'neovm--i2-eval a env)) (cdr expr)))
           ((eq op 'mod) (mod (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           (t nil))))
       (t nil))))

  (unwind-protect
      (list
       ;; Fibonacci
       (funcall 'neovm--i2-eval
                '(letrec ((fib (lam (n)
                            (if (< n 2) n
                              (+ (app fib (- n 1)) (app fib (- n 2)))))))
                   (mklist (app fib 0) (app fib 1) (app fib 2) (app fib 3)
                           (app fib 4) (app fib 5) (app fib 6) (app fib 7)
                           (app fib 8) (app fib 9) (app fib 10)))
                nil)

       ;; GCD (Euclidean algorithm)
       (funcall 'neovm--i2-eval
                '(letrec ((gcd (lam (a b)
                            (if (= b 0) a
                              (app gcd b (mod a b))))))
                   (mklist (app gcd 12 8) (app gcd 100 75) (app gcd 17 13)
                           (app gcd 48 18) (app gcd 1 1) (app gcd 0 5)))
                nil)

       ;; Power function
       (funcall 'neovm--i2-eval
                '(letrec ((power (lam (base exp)
                            (if (= exp 0) 1
                              (* base (app power base (- exp 1)))))))
                   (mklist (app power 2 0) (app power 2 1) (app power 2 8)
                           (app power 3 4) (app power 5 3) (app power 10 3)))
                nil)

       ;; Sum of range [a, b]
       (funcall 'neovm--i2-eval
                '(letrec ((sum-range (lam (a b)
                            (if (> a b) 0
                              (+ a (app sum-range (+ a 1) b))))))
                   (mklist (app sum-range 1 10) (app sum-range 1 100)
                           (app sum-range 5 5) (app sum-range 3 7)))
                nil)

       ;; Ackermann (small values only)
       (funcall 'neovm--i2-eval
                '(letrec ((ack (lam (m n)
                            (if (= m 0) (+ n 1)
                              (if (= n 0) (app ack (- m 1) 1)
                                (app ack (- m 1) (app ack m (- n 1))))))))
                   (mklist (app ack 0 0) (app ack 0 5) (app ack 1 0)
                           (app ack 1 5) (app ack 2 0) (app ack 2 3)
                           (app ack 3 0) (app ack 3 2)))
                nil))
    (fmakunbound 'neovm--i2-eval)))"#;
    let expect =
        expect_test::expect![[r#""ERR (error \"Lisp nesting exceeds ‘max-lisp-eval-depth’\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tail-call optimization pattern (trampoline)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp2_trampoline_tco() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a trampoline-based TCO. Functions return either a final
    // value or a thunk (list 'thunk closure). The trampoline loop
    // keeps calling thunks until a final value is produced.
    let form = r#"(progn
  (fset 'neovm--tramp-run
    (lambda (val)
      "Run trampoline: keep calling thunks until we get a final value."
      (let ((max-bounces 1000))
        (while (and (consp val) (eq (car val) 'thunk) (> max-bounces 0))
          (setq val (funcall (cadr val)))
          (setq max-bounces (1- max-bounces)))
        val)))

  ;; Tail-recursive factorial using trampoline
  (fset 'neovm--tramp-fact-iter
    (lambda (n acc)
      (if (<= n 1) acc
        (list 'thunk (lambda () (funcall 'neovm--tramp-fact-iter (1- n) (* acc n)))))))

  (fset 'neovm--tramp-fact
    (lambda (n)
      (funcall 'neovm--tramp-run (funcall 'neovm--tramp-fact-iter n 1))))

  ;; Tail-recursive fibonacci using trampoline
  (fset 'neovm--tramp-fib-iter
    (lambda (n a b)
      (if (= n 0) a
        (list 'thunk (lambda () (funcall 'neovm--tramp-fib-iter (1- n) b (+ a b)))))))

  (fset 'neovm--tramp-fib
    (lambda (n)
      (funcall 'neovm--tramp-run (funcall 'neovm--tramp-fib-iter n 0 1))))

  ;; Tail-recursive sum of 1..n using trampoline
  (fset 'neovm--tramp-sum-iter
    (lambda (n acc)
      (if (= n 0) acc
        (list 'thunk (lambda () (funcall 'neovm--tramp-sum-iter (1- n) (+ acc n)))))))

  (fset 'neovm--tramp-sum
    (lambda (n)
      (funcall 'neovm--tramp-run (funcall 'neovm--tramp-sum-iter n 0))))

  ;; Tail-recursive countdown producing a list
  (fset 'neovm--tramp-countdown-iter
    (lambda (n acc)
      (if (< n 0) acc
        (list 'thunk (lambda () (funcall 'neovm--tramp-countdown-iter (1- n) (cons n acc)))))))

  (fset 'neovm--tramp-countdown
    (lambda (n)
      (funcall 'neovm--tramp-run (funcall 'neovm--tramp-countdown-iter n nil))))

  (unwind-protect
      (list
       ;; Factorial via trampoline
       (funcall 'neovm--tramp-fact 0)
       (funcall 'neovm--tramp-fact 1)
       (funcall 'neovm--tramp-fact 5)
       (funcall 'neovm--tramp-fact 10)
       ;; Fibonacci via trampoline
       (funcall 'neovm--tramp-fib 0)
       (funcall 'neovm--tramp-fib 1)
       (funcall 'neovm--tramp-fib 10)
       (funcall 'neovm--tramp-fib 15)
       (funcall 'neovm--tramp-fib 20)
       ;; Sum via trampoline
       (funcall 'neovm--tramp-sum 0)
       (funcall 'neovm--tramp-sum 10)
       (funcall 'neovm--tramp-sum 100)
       ;; Countdown
       (funcall 'neovm--tramp-countdown 5)
       (funcall 'neovm--tramp-countdown 0)
       ;; Verify correctness
       (= (funcall 'neovm--tramp-fact 10) 3628800)
       (= (funcall 'neovm--tramp-fib 10) 55)
       (= (funcall 'neovm--tramp-sum 100) 5050))
    (fmakunbound 'neovm--tramp-run)
    (fmakunbound 'neovm--tramp-fact-iter)
    (fmakunbound 'neovm--tramp-fact)
    (fmakunbound 'neovm--tramp-fib-iter)
    (fmakunbound 'neovm--tramp-fib)
    (fmakunbound 'neovm--tramp-sum-iter)
    (fmakunbound 'neovm--tramp-sum)
    (fmakunbound 'neovm--tramp-countdown-iter)
    (fmakunbound 'neovm--tramp-countdown)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 1 120 3628800 0 1 55 610 6765 0 55 5050 (0 1 2 3 4 5) (0) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error handling in the interpreted language
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp2_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An interpreter with try/catch error handling. Errors are represented
    // as (error tag message). try-catch catches errors by tag.
    let form = r#"(progn
  (fset 'neovm--e-eval
    (lambda (expr env)
      (cond
       ((integerp expr) expr)
       ((stringp expr) expr)
       ((eq expr 't) t)
       ((eq expr 'nil) nil)
       ((symbolp expr)
        (let ((found nil) (e env))
          (while (and e (not found))
            (let ((binding (assq expr (car e))))
              (if binding (setq found (cdr binding)) (setq e (cdr e)))))
          (if found found
            (list 'error 'unbound (format "unbound variable: %s" (symbol-name expr))))))
       ((consp expr)
        (let ((op (car expr)))
          (cond
           ;; (raise tag message) - produce an error value
           ((eq op 'raise)
            (list 'error
                  (funcall 'neovm--e-eval (cadr expr) env)
                  (funcall 'neovm--e-eval (caddr expr) env)))
           ;; (try body (catch tag handler))
           ;; Evaluate body; if it produces (error tag _), run handler with error message
           ((eq op 'try)
            (let* ((body (cadr expr))
                   (catch-clause (caddr expr))
                   (catch-tag (cadr catch-clause))
                   (handler-var (caddr catch-clause))
                   (handler-body (cadddr catch-clause))
                   (result (funcall 'neovm--e-eval body env)))
              (if (and (consp result) (eq (car result) 'error)
                       (eq (cadr result) catch-tag))
                  ;; Error matches: bind message to handler-var, run handler
                  (let ((frame (list (cons handler-var (caddr result)))))
                    (funcall 'neovm--e-eval handler-body (cons frame env)))
                ;; No error or different tag: return as-is
                result)))
           ;; (is-error val) - check if value is an error
           ((eq op 'is-error)
            (let ((v (funcall 'neovm--e-eval (cadr expr) env)))
              (and (consp v) (eq (car v) 'error))))
           ;; (let1 var val body) - propagates errors
           ((eq op 'let1)
            (let ((val (funcall 'neovm--e-eval (caddr expr) env)))
              (if (and (consp val) (eq (car val) 'error))
                  val
                (funcall 'neovm--e-eval (cadddr expr)
                         (cons (list (cons (cadr expr) val)) env)))))
           ;; (if cond then else)
           ((eq op 'if)
            (let ((c (funcall 'neovm--e-eval (cadr expr) env)))
              (if (and (consp c) (eq (car c) 'error)) c
                (if c
                    (funcall 'neovm--e-eval (caddr expr) env)
                  (if (cdddr expr)
                      (funcall 'neovm--e-eval (cadddr expr) env)
                    nil)))))
           ;; (safe-div a b) - division with error on zero
           ((eq op 'safe-div)
            (let ((a (funcall 'neovm--e-eval (cadr expr) env))
                  (b (funcall 'neovm--e-eval (caddr expr) env)))
              (cond
               ((and (consp a) (eq (car a) 'error)) a)
               ((and (consp b) (eq (car b) 'error)) b)
               ((= b 0) (list 'error 'div-by-zero "division by zero"))
               (t (/ a b)))))
           ;; Arithmetic with error propagation
           ((eq op '+)
            (let ((a (funcall 'neovm--e-eval (cadr expr) env))
                  (b (funcall 'neovm--e-eval (caddr expr) env)))
              (cond
               ((and (consp a) (eq (car a) 'error)) a)
               ((and (consp b) (eq (car b) 'error)) b)
               (t (+ a b)))))
           ((eq op '-)
            (let ((a (funcall 'neovm--e-eval (cadr expr) env))
                  (b (funcall 'neovm--e-eval (caddr expr) env)))
              (cond
               ((and (consp a) (eq (car a) 'error)) a)
               ((and (consp b) (eq (car b) 'error)) b)
               (t (- a b)))))
           ((eq op '*)
            (let ((a (funcall 'neovm--e-eval (cadr expr) env))
                  (b (funcall 'neovm--e-eval (caddr expr) env)))
              (cond
               ((and (consp a) (eq (car a) 'error)) a)
               ((and (consp b) (eq (car b) 'error)) b)
               (t (* a b)))))
           ((eq op '=)
            (let ((a (funcall 'neovm--e-eval (cadr expr) env))
                  (b (funcall 'neovm--e-eval (caddr expr) env)))
              (cond
               ((and (consp a) (eq (car a) 'error)) a)
               ((and (consp b) (eq (car b) 'error)) b)
               (t (= a b)))))
           ((eq op '<)
            (let ((a (funcall 'neovm--e-eval (cadr expr) env))
                  (b (funcall 'neovm--e-eval (caddr expr) env)))
              (cond
               ((and (consp a) (eq (car a) 'error)) a)
               ((and (consp b) (eq (car b) 'error)) b)
               (t (< a b)))))
           ((eq op 'mklist)
            (let ((results nil) (has-error nil))
              (dolist (a (cdr expr))
                (let ((v (funcall 'neovm--e-eval a env)))
                  (if (and (consp v) (eq (car v) 'error) (not has-error))
                      (setq has-error v)
                    (setq results (cons v results)))))
              (if has-error has-error (nreverse results))))
           (t (list 'error 'unknown-op (format "unknown op: %s" op))))))
       (t (list 'error 'invalid "invalid expression")))))

  (unwind-protect
      (list
       ;; 1. Division by zero produces error
       (funcall 'neovm--e-eval '(safe-div 10 0) nil)

       ;; 2. Successful division
       (funcall 'neovm--e-eval '(safe-div 10 3) nil)

       ;; 3. Error propagation through arithmetic
       (funcall 'neovm--e-eval '(+ 1 (safe-div 5 0)) nil)

       ;; 4. Try/catch: catch div-by-zero
       (funcall 'neovm--e-eval
                '(try (safe-div 10 0)
                      (catch div-by-zero msg msg))
                nil)

       ;; 5. Try/catch: no error, body result returned
       (funcall 'neovm--e-eval
                '(try (safe-div 10 2)
                      (catch div-by-zero msg "caught"))
                nil)

       ;; 6. Try/catch: wrong tag, error passes through
       (funcall 'neovm--e-eval
                '(try (raise type-error "expected integer")
                      (catch div-by-zero msg "caught"))
                nil)

       ;; 7. Nested try/catch
       (funcall 'neovm--e-eval
                '(try
                  (try (safe-div 10 0)
                       (catch type-error msg "wrong handler"))
                  (catch div-by-zero msg (mklist "recovered" msg)))
                nil)

       ;; 8. Error in let binding propagates
       (funcall 'neovm--e-eval
                '(let1 x (safe-div 10 0) (+ x 1))
                nil)

       ;; 9. Catch in let context
       (funcall 'neovm--e-eval
                '(let1 result
                   (try (safe-div 10 0)
                        (catch div-by-zero msg 0))
                   (+ result 42))
                nil)

       ;; 10. Custom error and catch
       (funcall 'neovm--e-eval
                '(try
                  (let1 x 5
                    (if (< x 10)
                        (raise validation "value too small")
                      x))
                  (catch validation msg (mklist "invalid" msg)))
                nil))
    (fmakunbound 'neovm--e-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK ((error div-by-zero \"division by zero\") 3 (error div-by-zero \"division by zero\") \"division by zero\" 5 (error (error unbound \"unbound variable: type-error\") \"expected integer\") (\"recovered\" \"division by zero\") (error div-by-zero \"division by zero\") 42 (error (error unbound \"unbound variable: validation\") \"value too small\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-level scoping and closure capture verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp2_scoping_and_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--i2-eval
    (lambda (expr env)
      (cond
       ((integerp expr) expr)
       ((stringp expr) expr)
       ((eq expr 't) t)
       ((eq expr 'nil) nil)
       ((symbolp expr)
        (let ((found nil) (e env))
          (while (and e (not found))
            (let ((binding (assq expr (car e))))
              (if binding (setq found (cdr binding)) (setq e (cdr e)))))
          (or found 0)))
       ((consp expr)
        (let ((op (car expr)))
          (cond
           ((eq op 'quote) (cadr expr))
           ((eq op 'lam) (list 'closure (cadr expr) (caddr expr) env))
           ((eq op 'app)
            (let ((func (funcall 'neovm--i2-eval (cadr expr) env))
                  (args (mapcar (lambda (a) (funcall 'neovm--i2-eval a env)) (cddr expr))))
              (if (and (consp func) (eq (car func) 'closure))
                  (let ((frame nil) (ps (cadr func)) (as args))
                    (while ps
                      (setq frame (cons (cons (car ps) (car as)) frame))
                      (setq ps (cdr ps)) (setq as (cdr as)))
                    (funcall 'neovm--i2-eval (caddr func) (cons frame (cadddr func))))
                nil)))
           ((eq op 'let1)
            (let* ((val (funcall 'neovm--i2-eval (caddr expr) env))
                   (frame (list (cons (cadr expr) val))))
              (funcall 'neovm--i2-eval (cadddr expr) (cons frame env))))
           ((eq op 'letrec)
            (let* ((bindings (cadr expr)) (body (caddr expr))
                   (frame (mapcar (lambda (b) (cons (car b) nil)) bindings))
                   (new-env (cons frame env)))
              (dolist (b bindings)
                (setcdr (assq (car b) frame) (funcall 'neovm--i2-eval (cadr b) new-env)))
              (funcall 'neovm--i2-eval body new-env)))
           ((eq op 'if)
            (if (funcall 'neovm--i2-eval (cadr expr) env)
                (funcall 'neovm--i2-eval (caddr expr) env)
              (if (cdddr expr) (funcall 'neovm--i2-eval (cadddr expr) env) nil)))
           ((eq op '+) (+ (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '-) (- (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '*) (* (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '=) (= (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op '<) (< (funcall 'neovm--i2-eval (cadr expr) env) (funcall 'neovm--i2-eval (caddr expr) env)))
           ((eq op 'mklist) (mapcar (lambda (a) (funcall 'neovm--i2-eval a env)) (cdr expr)))
           ((eq op 'seq)
            (funcall 'neovm--i2-eval (cadr expr) env)
            (funcall 'neovm--i2-eval (caddr expr) env))
           (t nil))))
       (t nil))))

  (unwind-protect
      (list
       ;; 1. Shadowing: inner let shadows outer variable
       (funcall 'neovm--i2-eval
                '(let1 x 10
                   (let1 x 20
                     x))
                nil)

       ;; 2. Shadowing with closure: closure sees outer value
       (funcall 'neovm--i2-eval
                '(let1 x 10
                   (let1 f (lam () x)
                     (let1 x 20
                       (app f))))
                nil)

       ;; 3. Three-level nesting: each level has its own x
       (funcall 'neovm--i2-eval
                '(let1 x 1
                   (let1 y (+ x 10)
                     (let1 x 100
                       (mklist x y (+ x y)))))
                nil)

       ;; 4. Closure factory: each closure captures different env
       (funcall 'neovm--i2-eval
                '(let1 make-counter (lam (start)
                   (lam (step) (+ start step)))
                   (let1 from-zero (app make-counter 0)
                     (let1 from-ten (app make-counter 10)
                       (mklist (app from-zero 1) (app from-zero 5)
                               (app from-ten 1) (app from-ten 5)))))
                nil)

       ;; 5. Deep nesting: 5 levels of let
       (funcall 'neovm--i2-eval
                '(let1 a 1
                   (let1 b (+ a 1)
                     (let1 c (+ b 1)
                       (let1 d (+ c 1)
                         (let1 e (+ d 1)
                           (mklist a b c d e (+ a b c d e)))))))
                nil)

       ;; 6. Closure in letrec sees its own binding
       (funcall 'neovm--i2-eval
                '(letrec ((counter (lam (n)
                            (if (= n 0) 0
                              (+ 1 (app counter (- n 1)))))))
                   (mklist (app counter 0) (app counter 5) (app counter 10)))
                nil)

       ;; 7. Higher-order: map via interpreter
       (funcall 'neovm--i2-eval
                '(letrec ((my-map (lam (f lst)
                            (if (= (app length lst) 0) (quote ())
                              (let1 head (app f (app car lst))
                                (let1 tail (app my-map f (app cdr lst))
                                  (app cons head tail))))))
                          (length (lam (lst) (if (= lst (quote ())) 0
                                              (+ 1 (app length (app cdr lst))))))
                          (car (lam (lst) (if (= lst (quote ())) 0 (let1 h lst h))))
                          (cdr (lam (lst) (quote ())))
                          (cons (lam (h t) (mklist h))))
                   ;; Simplified: just double a single-element list
                   (let1 double (lam (x) (* x 2))
                     (app double 21)))
                nil))
    (fmakunbound 'neovm--i2-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK (20 10 (100 11 111) (1 5 11 15) (1 2 3 4 5 3) (0 5 10) 42)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interpreter with set! (mutation) and begin (sequencing)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp2_mutation_and_sequencing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--m-eval
    (lambda (expr env)
      (cond
       ((integerp expr) expr)
       ((stringp expr) expr)
       ((eq expr 't) t)
       ((eq expr 'nil) nil)
       ((symbolp expr)
        (let ((found nil) (e env))
          (while (and e (not found))
            (let ((binding (assq expr (car e))))
              (if binding (setq found binding) (setq e (cdr e)))))
          (if found (cdr found) 0)))
       ((consp expr)
        (let ((op (car expr)))
          (cond
           ((eq op 'lam) (list 'closure (cadr expr) (caddr expr) env))
           ((eq op 'app)
            (let ((func (funcall 'neovm--m-eval (cadr expr) env))
                  (args (mapcar (lambda (a) (funcall 'neovm--m-eval a env)) (cddr expr))))
              (if (and (consp func) (eq (car func) 'closure))
                  (let ((frame nil) (ps (cadr func)) (as args))
                    (while ps
                      (setq frame (cons (cons (car ps) (car as)) frame))
                      (setq ps (cdr ps)) (setq as (cdr as)))
                    (funcall 'neovm--m-eval (caddr func) (cons frame (cadddr func))))
                nil)))
           ;; (let1 var val body)
           ((eq op 'let1)
            (let* ((val (funcall 'neovm--m-eval (caddr expr) env))
                   (frame (list (cons (cadr expr) val))))
              (funcall 'neovm--m-eval (cadddr expr) (cons frame env))))
           ;; (set! var val) - mutate existing binding
           ((eq op 'set!)
            (let ((var (cadr expr))
                  (val (funcall 'neovm--m-eval (caddr expr) env))
                  (found nil) (e env))
              (while (and e (not found))
                (let ((binding (assq var (car e))))
                  (if binding
                      (progn (setcdr binding val) (setq found t))
                    (setq e (cdr e)))))
              val))
           ;; (begin e1 e2 ... en) - evaluate all, return last
           ((eq op 'begin)
            (let ((result nil))
              (dolist (e (cdr expr))
                (setq result (funcall 'neovm--m-eval e env)))
              result))
           ;; (while cond body)
           ((eq op 'while)
            (let ((max-iter 200) (result nil))
              (while (and (funcall 'neovm--m-eval (cadr expr) env) (> max-iter 0))
                (setq result (funcall 'neovm--m-eval (caddr expr) env))
                (setq max-iter (1- max-iter)))
              result))
           ;; Comparisons and arithmetic
           ((eq op 'if)
            (if (funcall 'neovm--m-eval (cadr expr) env)
                (funcall 'neovm--m-eval (caddr expr) env)
              (if (cdddr expr) (funcall 'neovm--m-eval (cadddr expr) env) nil)))
           ((eq op '+) (+ (funcall 'neovm--m-eval (cadr expr) env) (funcall 'neovm--m-eval (caddr expr) env)))
           ((eq op '-) (- (funcall 'neovm--m-eval (cadr expr) env) (funcall 'neovm--m-eval (caddr expr) env)))
           ((eq op '*) (* (funcall 'neovm--m-eval (cadr expr) env) (funcall 'neovm--m-eval (caddr expr) env)))
           ((eq op '=) (= (funcall 'neovm--m-eval (cadr expr) env) (funcall 'neovm--m-eval (caddr expr) env)))
           ((eq op '<) (< (funcall 'neovm--m-eval (cadr expr) env) (funcall 'neovm--m-eval (caddr expr) env)))
           ((eq op '>) (> (funcall 'neovm--m-eval (cadr expr) env) (funcall 'neovm--m-eval (caddr expr) env)))
           ((eq op 'mklist) (mapcar (lambda (a) (funcall 'neovm--m-eval a env)) (cdr expr)))
           (t nil))))
       (t nil))))

  (unwind-protect
      (list
       ;; 1. Simple mutation
       (funcall 'neovm--m-eval
                '(let1 x 10
                   (begin
                    (set! x 20)
                    x))
                nil)

       ;; 2. While loop with mutation: sum 1..10
       (funcall 'neovm--m-eval
                '(let1 i 1
                   (let1 sum 0
                     (begin
                      (while (< i 11)
                        (begin
                         (set! sum (+ sum i))
                         (set! i (+ i 1))))
                      sum)))
                nil)

       ;; 3. Factorial via while loop
       (funcall 'neovm--m-eval
                '(let1 n 10
                   (let1 result 1
                     (begin
                      (while (> n 1)
                        (begin
                         (set! result (* result n))
                         (set! n (- n 1))))
                      result)))
                nil)

       ;; 4. Closure observes mutation of shared variable
       (funcall 'neovm--m-eval
                '(let1 x 0
                   (let1 get-x (lam () x)
                     (begin
                      (set! x 42)
                      (app get-x))))
                nil)

       ;; 5. Multiple mutations and sequencing
       (funcall 'neovm--m-eval
                '(let1 a 1
                   (let1 b 2
                     (begin
                      (set! a (+ a b))
                      (set! b (* a b))
                      (set! a (- b a))
                      (mklist a b))))
                nil)

       ;; 6. Fibonacci via while loop
       (funcall 'neovm--m-eval
                '(let1 n 10
                   (let1 a 0
                     (let1 b 1
                       (let1 i 0
                         (begin
                          (while (< i n)
                            (let1 temp b
                              (begin
                               (set! b (+ a b))
                               (set! a temp)
                               (set! i (+ i 1)))))
                          a)))))
                nil)

       ;; 7. Counter closure pattern
       (funcall 'neovm--m-eval
                '(let1 count 0
                   (let1 increment (lam () (begin (set! count (+ count 1)) count))
                     (mklist (app increment) (app increment) (app increment)
                             (app increment) (app increment))))
                nil))
    (fmakunbound 'neovm--m-eval)))"#;
    let expect = expect_test::expect![[r#""OK (20 55 3628800 42 (3 6) 55 (1 2 3 4 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
