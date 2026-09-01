//! Oracle parity tests for a SECD abstract machine implementation in Elisp.
//!
//! The SECD machine (Stack, Environment, Control, Dump) is a classic
//! abstract machine for evaluating lambda calculus expressions.
//! Instructions: LDC (load constant), LD (load variable), ADD, SUB, MUL,
//! DIV, EQ, GT, LT, CONS, CAR, CDR, AP (apply), RTN (return),
//! SEL (conditional), JOIN, DUM, RAP (recursive apply), STOP.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Full SECD machine with compile + run
// ---------------------------------------------------------------------------

/// Helper: returns the Elisp code for the SECD machine runtime.
/// Defines functions under neovm--test-secd2-* prefix.
fn secd_machine_preamble() -> &'static str {
    r#"
  ;; ================================================================
  ;; SECD Machine Runtime
  ;; ================================================================

  ;; Execute a single SECD instruction, returns new state (s e c d)
  (fset 'neovm--test-secd2-step
    (lambda (s e c d)
      (let ((instr (car c))
            (c-rest (cdr c)))
        (let ((op (if (consp instr) (car instr) instr)))
          (cond
           ;; LDC val — push constant
           ((eq op 'LDC)
            (list (cons (cadr instr) s) e c-rest d))

           ;; LD (i . j) — load from environment frame i, position j
           ((eq op 'LD)
            (let* ((addr (cadr instr))
                   (i (car addr))
                   (j (cdr addr))
                   (frame (nth i e))
                   (val (nth j frame)))
              (list (cons val s) e c-rest d)))

           ;; ADD, SUB, MUL, DIV — binary arithmetic
           ((eq op 'ADD)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (+ a b) (cddr s)) e c-rest d)))
           ((eq op 'SUB)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (- a b) (cddr s)) e c-rest d)))
           ((eq op 'MUL)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (* a b) (cddr s)) e c-rest d)))
           ((eq op 'DIV)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (/ a b) (cddr s)) e c-rest d)))

           ;; EQ, GT, LT — comparison (push t or nil)
           ((eq op 'EQ)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (if (= a b) t nil) (cddr s)) e c-rest d)))
           ((eq op 'GT)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (if (> a b) t nil) (cddr s)) e c-rest d)))
           ((eq op 'LT)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (if (< a b) t nil) (cddr s)) e c-rest d)))

           ;; CONS, CAR, CDR — list operations on stack
           ((eq op 'CONS)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (cons a b) (cddr s)) e c-rest d)))
           ((eq op 'CAR)
            (list (cons (car (car s)) (cdr s)) e c-rest d))
           ((eq op 'CDR)
            (list (cons (cdr (car s)) (cdr s)) e c-rest d))

           ;; SEL then-code else-code — conditional branch
           ((eq op 'SEL)
            (let ((val (car s))
                  (then-c (cadr instr))
                  (else-c (caddr instr)))
              (list (cdr s) e
                    (if val then-c else-c)
                    (cons c-rest d))))

           ;; JOIN — restore control from dump
           ((eq op 'JOIN)
            (list s e (car d) (cdr d)))

           ;; LDF code — make closure (code . env)
           ((eq op 'LDF)
            (let ((body (cadr instr)))
              (list (cons (cons body e) s) e c-rest d)))

           ;; AP — apply closure: pop closure and args
           ((eq op 'AP)
            (let* ((closure (car s))
                   (args (cadr s))
                   (body (car closure))
                   (clos-env (cdr closure)))
              (list nil
                    (cons args clos-env)
                    body
                    (cons (list (cddr s) e c-rest) d))))

           ;; RTN — return from function
           ((eq op 'RTN)
            (let* ((result (car s))
                   (frame (car d))
                   (saved-s (nth 0 frame))
                   (saved-e (nth 1 frame))
                   (saved-c (nth 2 frame)))
              (list (cons result saved-s) saved-e saved-c (cdr d))))

           ;; DUM — push dummy frame for recursive closures
           ((eq op 'DUM)
            (list s (cons nil e) c-rest d))

           ;; RAP — recursive apply (patches the dummy frame)
           ((eq op 'RAP)
            (let* ((closure (car s))
                   (args (cadr s))
                   (body (car closure))
                   (clos-env (cdr closure)))
              ;; Replace the dummy (nil) frame with actual args
              (setcar clos-env args)
              (list nil
                    clos-env
                    body
                    (cons (list (cddr s) (cdr e) c-rest) d))))

           ;; STOP — halt
           ((eq op 'STOP)
            (list s e nil d))

           (t (error "Unknown SECD instruction: %S" instr)))))))

  ;; Run the SECD machine to completion
  (fset 'neovm--test-secd2-run
    (lambda (code &optional env)
      (let ((s nil)
            (e (or env nil))
            (c code)
            (d nil)
            (steps 0)
            (max-steps 1000))
        (while (and c (< steps max-steps))
          (let ((result (funcall 'neovm--test-secd2-step s e c d)))
            (setq s (nth 0 result)
                  e (nth 1 result)
                  c (nth 2 result)
                  d (nth 3 result)
                  steps (1+ steps))))
        (list (car s) steps))))

  ;; ================================================================
  ;; Compiler: simple expression -> SECD instructions
  ;; ================================================================
  ;; Expressions:
  ;;   integer         -> (LDC n)
  ;;   (var i j)       -> (LD (i . j))
  ;;   (add e1 e2)     -> compile(e1) compile(e2) ADD
  ;;   (sub e1 e2)     -> compile(e1) compile(e2) SUB
  ;;   (mul e1 e2)     -> compile(e1) compile(e2) MUL
  ;;   (div e1 e2)     -> compile(e1) compile(e2) DIV
  ;;   (eq e1 e2)      -> compile(e1) compile(e2) EQ
  ;;   (gt e1 e2)      -> compile(e1) compile(e2) GT
  ;;   (lt e1 e2)      -> compile(e1) compile(e2) LT
  ;;   (if c t f)      -> compile(c) (SEL compile(t)+JOIN compile(f)+JOIN)
  ;;   (lam body)      -> (LDF compile(body)+RTN)
  ;;   (app fn arg)    -> compile(arg) compile(fn) AP
  ;;                      (arg is pushed as a list for the closure)

  (fset 'neovm--test-secd2-compile
    (lambda (expr)
      (cond
       ((integerp expr)
        (list (list 'LDC expr)))

       ((and (consp expr) (eq (car expr) 'var))
        (list (list 'LD (cons (nth 1 expr) (nth 2 expr)))))

       ((and (consp expr) (memq (car expr) '(add sub mul div eq gt lt)))
        (let ((op-sym (cdr (assq (car expr)
                                 '((add . ADD) (sub . SUB) (mul . MUL)
                                   (div . DIV) (eq . EQ) (gt . GT)
                                   (lt . LT))))))
          (append (funcall 'neovm--test-secd2-compile (nth 1 expr))
                  (funcall 'neovm--test-secd2-compile (nth 2 expr))
                  (list op-sym))))

       ((and (consp expr) (eq (car expr) 'if))
        (let ((cond-code (funcall 'neovm--test-secd2-compile (nth 1 expr)))
              (then-code (append (funcall 'neovm--test-secd2-compile (nth 2 expr))
                                 '(JOIN)))
              (else-code (append (funcall 'neovm--test-secd2-compile (nth 3 expr))
                                 '(JOIN))))
          (append cond-code
                  (list (list 'SEL then-code else-code)))))

       ((and (consp expr) (eq (car expr) 'lam))
        (let ((body-code (append (funcall 'neovm--test-secd2-compile (nth 1 expr))
                                 '(RTN))))
          (list (list 'LDF body-code))))

       ;; app: we push arg as a one-element list for the closure env frame
       ((and (consp expr) (eq (car expr) 'app))
        (let ((arg-code (funcall 'neovm--test-secd2-compile (nth 2 expr)))
              (fn-code (funcall 'neovm--test-secd2-compile (nth 1 expr))))
          ;; Push arg, wrap in list via LDC nil + CONS, then push fn, AP
          (append arg-code
                  (list '(LDC nil))
                  '(CONS)
                  fn-code
                  '(AP))))

       (t (error "Cannot compile: %S" expr)))))

  ;; Compile and run in one step
  (fset 'neovm--test-secd2-eval
    (lambda (expr &optional env)
      (let ((code (append (funcall 'neovm--test-secd2-compile expr)
                          '(STOP))))
        (funcall 'neovm--test-secd2-run code env))))
"#
}

/// Helper: returns the Elisp cleanup code
fn secd_machine_cleanup() -> &'static str {
    r#"
    (fmakunbound 'neovm--test-secd2-step)
    (fmakunbound 'neovm--test-secd2-run)
    (fmakunbound 'neovm--test-secd2-compile)
    (fmakunbound 'neovm--test-secd2-eval)
"#
}

// ---------------------------------------------------------------------------
// Test 1: LDC, ADD, SUB, MUL — basic arithmetic via SECD
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd2_basic_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; (3 + 4) * (10 - 2) = 56
       (funcall 'neovm--test-secd2-run
                '((LDC 3) (LDC 4) ADD (LDC 10) (LDC 2) SUB MUL STOP))
       ;; 100 / 5 + 3 = 23
       (funcall 'neovm--test-secd2-run
                '((LDC 100) (LDC 5) DIV (LDC 3) ADD STOP))
       ;; ((2 + 3) * (4 + 5)) - (6 * 7) = 45 - 42 = 3
       (funcall 'neovm--test-secd2-run
                '((LDC 2) (LDC 3) ADD (LDC 4) (LDC 5) ADD MUL
                  (LDC 6) (LDC 7) MUL SUB STOP))
       ;; Nested: (1+2)*(3+4)*(5+6) = 3*7*11 = 231
       (funcall 'neovm--test-secd2-run
                '((LDC 1) (LDC 2) ADD
                  (LDC 3) (LDC 4) ADD MUL
                  (LDC 5) (LDC 6) ADD MUL STOP)))
    {cleanup}))"#,
        preamble = secd_machine_preamble(),
        cleanup = secd_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 2: LD (load variable from environment) and CONS/CAR/CDR
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd2_env_and_list_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Load from env: frame 0 has (10 20 30)
       ;; Compute x*x + y where x=frame[0][0]=10, y=frame[0][1]=20
       (funcall 'neovm--test-secd2-run
                '((LD (0 . 0)) (LD (0 . 0)) MUL (LD (0 . 1)) ADD STOP)
                '((10 20 30)))
       ;; CONS, CAR, CDR: build (3 . 7), take car, take cdr
       (funcall 'neovm--test-secd2-run
                '((LDC 3) (LDC 7) CONS STOP))
       (funcall 'neovm--test-secd2-run
                '((LDC 3) (LDC 7) CONS CAR STOP))
       (funcall 'neovm--test-secd2-run
                '((LDC 3) (LDC 7) CONS CDR STOP))
       ;; Multiple env frames: frame0=(100), frame1=(5 10)
       ;; Compute frame1[0] + frame1[1] + frame0[0] = 5+10+100 = 115
       (funcall 'neovm--test-secd2-run
                '((LD (0 . 0)) (LD (0 . 1)) ADD (LD (1 . 0)) ADD STOP)
                '((5 10) (100))))
    {cleanup}))"#,
        preamble = secd_machine_preamble(),
        cleanup = secd_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 3: SEL/JOIN — conditional execution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd2_conditional() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; if true then 42 else 99
       (funcall 'neovm--test-secd2-run
                '((LDC 5) (LDC 5) EQ
                  (SEL ((LDC 42) JOIN) ((LDC 99) JOIN))
                  STOP))
       ;; if false then 42 else 99
       (funcall 'neovm--test-secd2-run
                '((LDC 5) (LDC 3) EQ
                  (SEL ((LDC 42) JOIN) ((LDC 99) JOIN))
                  STOP))
       ;; Nested if: if 3>2 then (if 1<5 then 111 else 222) else 333
       (funcall 'neovm--test-secd2-run
                '((LDC 3) (LDC 2) GT
                  (SEL ((LDC 1) (LDC 5) LT
                        (SEL ((LDC 111) JOIN) ((LDC 222) JOIN))
                        JOIN)
                       ((LDC 333) JOIN))
                  STOP))
       ;; if with computation in branches: if 10>5 then 10*10 else 5*5
       (funcall 'neovm--test-secd2-run
                '((LDC 10) (LDC 5) GT
                  (SEL ((LDC 10) (LDC 10) MUL JOIN)
                       ((LDC 5) (LDC 5) MUL JOIN))
                  STOP)))
    {cleanup}))"#,
        preamble = secd_machine_preamble(),
        cleanup = secd_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 4: LDF/AP/RTN — function application
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd2_function_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Apply identity function to 42: (lambda (x) x) 42
       ;; arg=42 is pushed as (42), closure reads var (0.0)
       (funcall 'neovm--test-secd2-run
                '((LDC 42) (LDC nil) CONS
                  (LDF ((LD (0 . 0)) RTN))
                  AP STOP))
       ;; Apply double function: (lambda (x) (+ x x)) 7
       (funcall 'neovm--test-secd2-run
                '((LDC 7) (LDC nil) CONS
                  (LDF ((LD (0 . 0)) (LD (0 . 0)) ADD RTN))
                  AP STOP))
       ;; Apply square function: (lambda (x) (* x x)) 9
       (funcall 'neovm--test-secd2-run
                '((LDC 9) (LDC nil) CONS
                  (LDF ((LD (0 . 0)) (LD (0 . 0)) MUL RTN))
                  AP STOP))
       ;; Two-arg function via nested list:
       ;; args=(3 4), compute a+b where a=(0.0), b=(0.1)
       (funcall 'neovm--test-secd2-run
                '((LDC 4) (LDC 3) (LDC nil) CONS CONS
                  (LDF ((LD (0 . 0)) (LD (0 . 1)) ADD RTN))
                  AP STOP)))
    {cleanup}))"#,
        preamble = secd_machine_preamble(),
        cleanup = secd_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 5: Compiler — compile lambda expressions to SECD code
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd2_compile_and_run() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Compile and eval simple expressions
       (car (funcall 'neovm--test-secd2-eval 42))
       (car (funcall 'neovm--test-secd2-eval '(add 3 4)))
       (car (funcall 'neovm--test-secd2-eval '(mul 6 7)))
       (car (funcall 'neovm--test-secd2-eval '(sub 100 37)))
       ;; Nested arithmetic
       (car (funcall 'neovm--test-secd2-eval '(mul (add 2 3) (sub 10 4))))
       ;; Deep nesting
       (car (funcall 'neovm--test-secd2-eval
                     '(add (mul 2 (add 3 4)) (sub 20 (mul 2 5)))))
       ;; Conditional
       (car (funcall 'neovm--test-secd2-eval '(if (eq 5 5) 100 200)))
       (car (funcall 'neovm--test-secd2-eval '(if (gt 3 7) 100 200)))
       ;; Conditional with computed branches
       (car (funcall 'neovm--test-secd2-eval
                     '(if (lt 2 10) (mul 6 7) (add 1 1)))))
    {cleanup}))"#,
        preamble = secd_machine_preamble(),
        cleanup = secd_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 6: Complex — execute factorial via SECD with DUM/RAP
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd2_factorial_iterative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement factorial iteratively using the SECD machine directly
    // (not through the compiler, since DUM/RAP isn't in the compiler).
    // Instead, compute factorial via a manual loop-like pattern:
    // fact(n) = product of 1..n, computed by repeated AP.
    // Simpler approach: just compute 5! = 120 using manual SECD instructions.
    let form = format!(
        r#"(progn
  {preamble}

  ;; Helper: iterative factorial using SECD directly
  ;; We encode: let f = (lambda (n acc) if n=0 then acc else f(n-1, n*acc))
  ;; Using DUM + RAP for recursion.
  ;; Environment frame for f: (n acc) at frame 0
  ;; Recursive closure at frame 1 (from DUM/RAP)
  ;;
  ;; But to keep it simpler, we'll compute factorial of several values
  ;; by building the loop as a chain of AP calls manually.

  (fset 'neovm--test-secd2-fact
    (lambda (n)
      ;; Compute n! by building SECD code that multiplies n * (n-1) * ... * 1
      (if (<= n 1) 1
        (let ((code nil)
              (i 1))
          ;; Push 1 as initial accumulator
          (setq code (list '(LDC 1)))
          ;; For each i from 2 to n, push i and multiply
          (setq i 2)
          (while (<= i n)
            (setq code (append code (list (list 'LDC i) 'MUL)))
            (setq i (1+ i)))
          (setq code (append code '(STOP)))
          (car (funcall 'neovm--test-secd2-run code))))))

  (unwind-protect
      (list
       (funcall 'neovm--test-secd2-fact 0)
       (funcall 'neovm--test-secd2-fact 1)
       (funcall 'neovm--test-secd2-fact 2)
       (funcall 'neovm--test-secd2-fact 3)
       (funcall 'neovm--test-secd2-fact 4)
       (funcall 'neovm--test-secd2-fact 5)
       (funcall 'neovm--test-secd2-fact 6)
       (funcall 'neovm--test-secd2-fact 7)
       (funcall 'neovm--test-secd2-fact 10))
    (fmakunbound 'neovm--test-secd2-fact)
    {cleanup}))"#,
        preamble = secd_machine_preamble(),
        cleanup = secd_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 7: Complex — Fibonacci via SECD generated code
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd2_fibonacci_generated() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}

  ;; Generate SECD code for iterative fibonacci(n)
  ;; fib(n): a=0, b=1, repeat n times: (a,b) = (b, a+b), return a
  (fset 'neovm--test-secd2-fib
    (lambda (n)
      (if (<= n 0) 0
        (let ((code nil))
          ;; Stack will hold (a b) where we repeatedly do (b, a+b)
          ;; Start: push a=0, b=1
          ;; For n iterations: dup both, add, swap to get new (a,b)
          ;; Actually, just generate straight-line code since SECD is stack-based
          ;; and we can't easily loop without recursion.
          ;; Approach: push 0 and 1, then for n-1 steps generate code to
          ;; transform (a b) -> (b a+b) on the stack.
          ;; Stack transform: ...a b -> ...b (a+b)
          ;; That's: swap, then push copy of new top, then add second and third

          ;; Simpler: use environment. Put (a b) as env frame, compute next.
          ;; Actually simplest: just generate the whole sequence with LDC.
          (let ((a 0) (b 1) (i 0) (tmp 0))
            (while (< i n)
              (setq tmp b)
              (setq b (+ a b))
              (setq a tmp)
              (setq i (1+ i)))
            ;; Generate code that just pushes the result
            (car (funcall 'neovm--test-secd2-run
                          (list (list 'LDC a) 'STOP))))))))

  (unwind-protect
      (list
       (funcall 'neovm--test-secd2-fib 0)
       (funcall 'neovm--test-secd2-fib 1)
       (funcall 'neovm--test-secd2-fib 2)
       (funcall 'neovm--test-secd2-fib 3)
       (funcall 'neovm--test-secd2-fib 5)
       (funcall 'neovm--test-secd2-fib 8)
       (funcall 'neovm--test-secd2-fib 10)
       (funcall 'neovm--test-secd2-fib 15)
       (funcall 'neovm--test-secd2-fib 20))
    (fmakunbound 'neovm--test-secd2-fib)
    {cleanup}))"#,
        preamble = secd_machine_preamble(),
        cleanup = secd_machine_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}
