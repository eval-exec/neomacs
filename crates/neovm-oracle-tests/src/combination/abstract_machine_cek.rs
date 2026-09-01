//! Oracle parity tests for a CEK abstract machine implementation in Elisp.
//!
//! The CEK machine (Control, Environment, Continuation) is an abstract
//! machine for evaluating lambda calculus expressions.  Unlike the SECD
//! machine it uses an explicit continuation instead of a dump/stack pair.
//!
//! Components:
//!   Control   — the expression being evaluated
//!   Environment — variable bindings (list of frames)
//!   Continuation — what to do next (stack of frames: arg, fn, if, halt)
//!
//! Values: integers, closures (list 'clo param body env), booleans.
//! Expressions: integer literals, (var name), (lam param body),
//! (app fn arg), (ifte cond then else), (add e1 e2), (mul e1 e2),
//! (sub e1 e2), (eq e1 e2), (lt e1 e2).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

/// Returns the Elisp source for the CEK machine runtime.
fn cek_machine_preamble() -> &'static str {
    r#"
  ;; ================================================================
  ;; CEK Machine Runtime
  ;; ================================================================

  ;; Environment operations
  (fset 'neovm--cek-env-empty (lambda () nil))
  (fset 'neovm--cek-env-extend
    (lambda (env name val)
      (cons (cons name val) env)))
  (fset 'neovm--cek-env-lookup
    (lambda (env name)
      (let ((pair (assq name env)))
        (if pair (cdr pair)
          (error "CEK: unbound variable: %S" name)))))

  ;; Value predicates
  (fset 'neovm--cek-closure-p
    (lambda (v)
      (and (consp v) (eq (car v) 'clo))))

  ;; Continuation frames:
  ;;   (halt)                — top-level
  ;;   (arg-k expr env k)   — evaluate the argument next
  ;;   (fn-k val k)         — apply function val to the argument
  ;;   (ifte-k then-e else-e env k) — branch based on value
  ;;   (binop-arg-k op e2 env k)    — evaluate second operand
  ;;   (binop-apply-k op v1 k)      — apply binary op with first value

  ;; Single step of the CEK machine.
  ;; Returns (list control env kont) or (list 'done value).
  (fset 'neovm--cek-step
    (lambda (ctrl env kont)
      (cond
       ;; Integer literal: pass to continuation
       ((integerp ctrl)
        (funcall 'neovm--cek-apply-kont kont ctrl))

       ;; Boolean literal
       ((or (eq ctrl t) (eq ctrl nil))
        (funcall 'neovm--cek-apply-kont kont ctrl))

       ;; Variable reference
       ((and (consp ctrl) (eq (car ctrl) 'var))
        (let ((val (funcall 'neovm--cek-env-lookup env (cadr ctrl))))
          (funcall 'neovm--cek-apply-kont kont val)))

       ;; Lambda: create closure
       ((and (consp ctrl) (eq (car ctrl) 'lam))
        (let ((clo (list 'clo (cadr ctrl) (caddr ctrl) env)))
          (funcall 'neovm--cek-apply-kont kont clo)))

       ;; Application: evaluate function first, then argument
       ((and (consp ctrl) (eq (car ctrl) 'app))
        (let ((fn-expr (cadr ctrl))
              (arg-expr (caddr ctrl)))
          ;; Evaluate fn-expr, with continuation to evaluate arg-expr after
          (list fn-expr env (list 'arg-k arg-expr env kont))))

       ;; If-then-else: evaluate condition
       ((and (consp ctrl) (eq (car ctrl) 'ifte))
        (let ((cond-e (cadr ctrl))
              (then-e (caddr ctrl))
              (else-e (cadddr ctrl)))
          (list cond-e env (list 'ifte-k then-e else-e env kont))))

       ;; Binary ops: evaluate first operand
       ((and (consp ctrl) (memq (car ctrl) '(add sub mul eq lt)))
        (let ((op (car ctrl))
              (e1 (cadr ctrl))
              (e2 (caddr ctrl)))
          (list e1 env (list 'binop-arg-k op e2 env kont))))

       (t (error "CEK: unknown expression: %S" ctrl)))))

  ;; Apply a continuation to a value
  (fset 'neovm--cek-apply-kont
    (lambda (kont val)
      (let ((tag (car kont)))
        (cond
         ;; Halt: we are done
         ((eq tag 'halt)
          (list 'done val))

         ;; arg-k: we just got the function, now evaluate the argument
         ((eq tag 'arg-k)
          (let ((arg-expr (cadr kont))
                (env (caddr kont))
                (k (cadddr kont)))
            (list arg-expr env (list 'fn-k val k))))

         ;; fn-k: we have both function and argument, apply
         ((eq tag 'fn-k)
          (let ((func (cadr kont))
                (k (caddr kont)))
            ;; func must be a closure (clo param body env)
            (let ((param (cadr func))
                  (body (caddr func))
                  (clo-env (cadddr func)))
              (let ((new-env (funcall 'neovm--cek-env-extend clo-env param val)))
                (list body new-env k)))))

         ;; ifte-k: we have the condition value
         ((eq tag 'ifte-k)
          (let ((then-e (cadr kont))
                (else-e (caddr kont))
                (env (cadddr kont))
                (k (car (cddddr kont))))
            (if val
                (list then-e env k)
              (list else-e env k))))

         ;; binop-arg-k: first operand evaluated, now evaluate second
         ((eq tag 'binop-arg-k)
          (let ((op (cadr kont))
                (e2 (caddr kont))
                (env (cadddr kont))
                (k (car (cddddr kont))))
            (list e2 env (list 'binop-apply-k op val k))))

         ;; binop-apply-k: both operands ready, compute result
         ((eq tag 'binop-apply-k)
          (let ((op (cadr kont))
                (v1 (caddr kont))
                (k (cadddr kont)))
            (let ((result
                   (cond
                    ((eq op 'add) (+ v1 val))
                    ((eq op 'sub) (- v1 val))
                    ((eq op 'mul) (* v1 val))
                    ((eq op 'eq) (if (= v1 val) t nil))
                    ((eq op 'lt) (if (< v1 val) t nil))
                    (t (error "CEK: unknown binop: %S" op)))))
              (funcall 'neovm--cek-apply-kont k result))))

         (t (error "CEK: unknown continuation: %S" kont))))))

  ;; Run the CEK machine to completion
  (fset 'neovm--cek-run
    (lambda (expr &optional env)
      (let ((ctrl expr)
            (e (or env (funcall 'neovm--cek-env-empty)))
            (k '(halt))
            (steps 0)
            (max-steps 2000))
        (catch 'cek-done
          (while (< steps max-steps)
            (let ((result (funcall 'neovm--cek-step ctrl e k)))
              (if (eq (car result) 'done)
                  (throw 'cek-done (list (cadr result) steps))
                (setq ctrl (nth 0 result)
                      e (nth 1 result)
                      k (nth 2 result)
                      steps (1+ steps)))))
          (list 'timeout steps)))))
"#
}

/// Cleanup code for the CEK machine definitions.
fn cek_machine_cleanup() -> &'static str {
    r#"
    (fmakunbound 'neovm--cek-env-empty)
    (fmakunbound 'neovm--cek-env-extend)
    (fmakunbound 'neovm--cek-env-lookup)
    (fmakunbound 'neovm--cek-closure-p)
    (fmakunbound 'neovm--cek-step)
    (fmakunbound 'neovm--cek-apply-kont)
    (fmakunbound 'neovm--cek-run)
"#
}

// ---------------------------------------------------------------------------
// Test 1: Integer literals and binary operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cek_basic_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Simple integer
       (car (funcall 'neovm--cek-run 42))
       ;; Addition
       (car (funcall 'neovm--cek-run '(add 3 4)))
       ;; Subtraction
       (car (funcall 'neovm--cek-run '(sub 10 3)))
       ;; Multiplication
       (car (funcall 'neovm--cek-run '(mul 6 7)))
       ;; Nested arithmetic: (2+3) * (4+5) = 45
       (car (funcall 'neovm--cek-run '(mul (add 2 3) (add 4 5))))
       ;; Deep nesting: ((1+2)*(3+4)) - (5*6) = 21 - 30 = -9
       (car (funcall 'neovm--cek-run '(sub (mul (add 1 2) (add 3 4))
                                            (mul 5 6))))
       ;; Comparison: eq
       (car (funcall 'neovm--cek-run '(eq 5 5)))
       (car (funcall 'neovm--cek-run '(eq 3 7)))
       ;; Comparison: lt
       (car (funcall 'neovm--cek-run '(lt 3 5)))
       (car (funcall 'neovm--cek-run '(lt 5 3))))
    {cleanup}))"#,
        preamble = cek_machine_preamble(),
        cleanup = cek_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 2: Closure values and lambda application
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cek_closure_and_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Identity function: (lambda (x) x) applied to 99
       (car (funcall 'neovm--cek-run '(app (lam x (var x)) 99)))
       ;; Constant function: (lambda (x) 7) applied to anything
       (car (funcall 'neovm--cek-run '(app (lam x 7) 123)))
       ;; Double: (lambda (x) (+ x x)) applied to 21
       (car (funcall 'neovm--cek-run '(app (lam x (add (var x) (var x))) 21)))
       ;; Square: (lambda (x) (* x x)) applied to 8
       (car (funcall 'neovm--cek-run '(app (lam n (mul (var n) (var n))) 8)))
       ;; Two nested lambdas (currying): ((lambda (a) (lambda (b) (+ a b))) 10) 20
       (car (funcall 'neovm--cek-run
                     '(app (app (lam a (lam b (add (var a) (var b)))) 10) 20)))
       ;; Triple nesting: (lambda (f) (lambda (x) (f (f x))))
       ;; Apply to (lambda (n) (+ n 1)) then to 0 => 2
       (car (funcall 'neovm--cek-run
                     '(app (app (lam f (lam x (app (var f) (app (var f) (var x)))))
                                (lam n (add (var n) 1)))
                           0))))
    {cleanup}))"#,
        preamble = cek_machine_preamble(),
        cleanup = cek_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 3: If-then-else control flow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cek_if_then_else() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Simple true branch
       (car (funcall 'neovm--cek-run '(ifte t 42 99)))
       ;; Simple false branch (nil)
       (car (funcall 'neovm--cek-run '(ifte nil 42 99)))
       ;; Condition from comparison
       (car (funcall 'neovm--cek-run '(ifte (eq 3 3) 100 200)))
       (car (funcall 'neovm--cek-run '(ifte (lt 5 2) 100 200)))
       ;; Nested if: if 3<5 then (if 1=1 then 111 else 222) else 333
       (car (funcall 'neovm--cek-run
                     '(ifte (lt 3 5)
                            (ifte (eq 1 1) 111 222)
                            333)))
       ;; If with computation in branches
       (car (funcall 'neovm--cek-run
                     '(ifte (lt 10 20)
                            (mul 7 8)
                            (add 1 1))))
       ;; Absolute value: if x<0 then (0-x) else x
       (car (funcall 'neovm--cek-run
                     '(app (lam x (ifte (lt (var x) 0)
                                        (sub 0 (var x))
                                        (var x)))
                           -42)))
       (car (funcall 'neovm--cek-run
                     '(app (lam x (ifte (lt (var x) 0)
                                        (sub 0 (var x))
                                        (var x)))
                           17))))
    {cleanup}))"#,
        preamble = cek_machine_preamble(),
        cleanup = cek_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 4: Recursive functions via Y-combinator-like pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cek_recursive_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Since our CEK machine does not have built-in recursion (no letrec),
    // we simulate recursion by passing the function to itself.
    // fact-helper = (lambda (self) (lambda (n) (if n=0 1 (* n (self self (n-1))))))
    // fact = (lambda (n) ((fact-helper fact-helper) n))
    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Factorial via self-application:
       ;; let mk = (lam self (lam n (ifte (eq n 0) 1 (mul n (app (app self self) (sub n 1))))))
       ;; fact(n) = ((mk mk) n)
       (car (funcall 'neovm--cek-run
                     '(app (app (lam self
                                  (lam n
                                    (ifte (eq (var n) 0)
                                          1
                                          (mul (var n)
                                               (app (app (var self) (var self))
                                                    (sub (var n) 1))))))
                                (lam self
                                  (lam n
                                    (ifte (eq (var n) 0)
                                          1
                                          (mul (var n)
                                               (app (app (var self) (var self))
                                                    (sub (var n) 1)))))))
                           5)))
       ;; Sum 1..n via self-application
       (car (funcall 'neovm--cek-run
                     '(app (app (lam self
                                  (lam n
                                    (ifte (eq (var n) 0)
                                          0
                                          (add (var n)
                                               (app (app (var self) (var self))
                                                    (sub (var n) 1))))))
                                (lam self
                                  (lam n
                                    (ifte (eq (var n) 0)
                                          0
                                          (add (var n)
                                               (app (app (var self) (var self))
                                                    (sub (var n) 1)))))))
                           10)))
       ;; Power: base^exp via self-application
       ;; pow(b,e) = if e=0 then 1 else b * pow(b, e-1)
       ;; Curried: mk = (lam self (lam b (lam e ...)))
       (car (funcall 'neovm--cek-run
                     '(app (app (app (lam self
                                       (lam b
                                         (lam e
                                           (ifte (eq (var e) 0)
                                                 1
                                                 (mul (var b)
                                                      (app (app (app (var self) (var self))
                                                                (var b))
                                                           (sub (var e) 1)))))))
                                     (lam self
                                       (lam b
                                         (lam e
                                           (ifte (eq (var e) 0)
                                                 1
                                                 (mul (var b)
                                                      (app (app (app (var self) (var self))
                                                                (var b))
                                                           (sub (var e) 1))))))))
                                2)
                           8))))
    {cleanup}))"#,
        preamble = cek_machine_preamble(),
        cleanup = cek_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 5: Church numerals in the CEK machine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cek_church_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Church numerals: n = (lam f (lam x (f (f ... (f x)...))))
    // church-to-int = apply n to (+1) and 0
    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let* (;; Church 0 = (lam f (lam x x))
             (c0 '(lam f (lam x (var x))))
             ;; Church 1 = (lam f (lam x (app f x)))
             (c1 '(lam f (lam x (app (var f) (var x)))))
             ;; Church 2 = (lam f (lam x (app f (app f x))))
             (c2 '(lam f (lam x (app (var f) (app (var f) (var x))))))
             ;; Church 3
             (c3 '(lam f (lam x (app (var f) (app (var f) (app (var f) (var x)))))))
             ;; Successor = (lam n (lam f (lam x (app f (app (app n f) x)))))
             (succ-e '(lam n (lam f (lam x (app (var f) (app (app (var n) (var f)) (var x)))))))
             ;; Add = (lam m (lam n (lam f (lam x (app (app m f) (app (app n f) x))))))
             (add-e '(lam m (lam n (lam f (lam x (app (app (var m) (var f))
                                                       (app (app (var n) (var f)) (var x))))))))
             ;; to-int = (lam n (app (app n (lam x (add x 1))) 0))
             (to-int-e '(lam n (app (app (var n) (lam x (add (var x) 1))) 0))))
        (list
         ;; Church 0 -> 0
         (car (funcall 'neovm--cek-run (list 'app to-int-e c0)))
         ;; Church 1 -> 1
         (car (funcall 'neovm--cek-run (list 'app to-int-e c1)))
         ;; Church 2 -> 2
         (car (funcall 'neovm--cek-run (list 'app to-int-e c2)))
         ;; Church 3 -> 3
         (car (funcall 'neovm--cek-run (list 'app to-int-e c3)))
         ;; succ(2) -> 3
         (car (funcall 'neovm--cek-run
                       (list 'app to-int-e (list 'app succ-e c2))))
         ;; add(2,3) -> 5
         (car (funcall 'neovm--cek-run
                       (list 'app to-int-e (list 'app (list 'app add-e c2) c3))))
         ;; succ(succ(0)) -> 2
         (car (funcall 'neovm--cek-run
                       (list 'app to-int-e (list 'app succ-e (list 'app succ-e c0)))))))
    {cleanup}))"#,
        preamble = cek_machine_preamble(),
        cleanup = cek_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 6: Step-by-step trace of CEK evaluation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cek_step_trace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that step counts are consistent and that intermediate
    // states behave correctly.
    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Integer literal takes 1 step
       (cadr (funcall 'neovm--cek-run 7))
       ;; Simple addition takes a few steps (eval 3, eval 4, add, apply)
       (let ((result (funcall 'neovm--cek-run '(add 3 4))))
         (list (car result) (> (cadr result) 0)))
       ;; Lambda application step count > arithmetic step count
       (let ((arith-steps (cadr (funcall 'neovm--cek-run '(add 1 2))))
             (app-steps (cadr (funcall 'neovm--cek-run '(app (lam x (var x)) 5)))))
         (list (> app-steps arith-steps)
               arith-steps
               app-steps))
       ;; Deeper nesting = more steps
       (let ((shallow (cadr (funcall 'neovm--cek-run '(add 1 2))))
             (deep (cadr (funcall 'neovm--cek-run '(add (add 1 2) (add 3 4))))))
         (> deep shallow))
       ;; If-then-else: both branches same step count for equal depth
       (let ((true-steps (cadr (funcall 'neovm--cek-run '(ifte t 1 2))))
             (false-steps (cadr (funcall 'neovm--cek-run '(ifte nil 1 2)))))
         (= true-steps false-steps)))
    {cleanup}))"#,
        preamble = cek_machine_preamble(),
        cleanup = cek_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 7: Environment extension and variable scoping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cek_environment_scoping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Variable from outer lambda shadows nothing
       (car (funcall 'neovm--cek-run '(app (lam x (add (var x) 10)) 5)))
       ;; Inner lambda captures outer variable (closure)
       ;; ((lambda (x) ((lambda (y) (+ x y)) 20)) 10) = 30
       (car (funcall 'neovm--cek-run
                     '(app (lam x (app (lam y (add (var x) (var y))) 20)) 10)))
       ;; Shadowing: inner x shadows outer x
       ;; ((lambda (x) ((lambda (x) x) 99)) 1) = 99
       (car (funcall 'neovm--cek-run
                     '(app (lam x (app (lam x (var x)) 99)) 1)))
       ;; Closure retains environment after outer function returns
       ;; let make-adder = (lambda (n) (lambda (m) (+ n m)))
       ;; (make-adder 100) 42 = 142
       (car (funcall 'neovm--cek-run
                     '(app (app (lam n (lam m (add (var n) (var m)))) 100) 42)))
       ;; Multiple closures from same factory, different environments
       ;; add5 = (make-adder 5), add10 = (make-adder 10)
       ;; (add5 3) = 8, but we can only run one at a time
       (car (funcall 'neovm--cek-run
                     '(app (app (lam n (lam m (add (var n) (var m)))) 5) 3)))
       (car (funcall 'neovm--cek-run
                     '(app (app (lam n (lam m (add (var n) (var m)))) 10) 3)))
       ;; Three levels of nesting: x=1, y=2, z=3 => x+y+z = 6
       (car (funcall 'neovm--cek-run
                     '(app (app (app (lam x (lam y (lam z
                                      (add (var x) (add (var y) (var z))))))
                                     1) 2) 3)))
       ;; Closure used in if-condition
       ;; ((lambda (pred) (if (pred 5) 100 200)) (lambda (n) (< n 10)))
       (car (funcall 'neovm--cek-run
                     '(app (lam pred
                             (ifte (app (var pred) 5) 100 200))
                           (lam n (lt (var n) 10))))))
    {cleanup}))"#,
        preamble = cek_machine_preamble(),
        cleanup = cek_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}
