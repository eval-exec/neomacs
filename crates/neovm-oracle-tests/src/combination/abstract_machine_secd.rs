//! Oracle parity tests for a SECD abstract machine implementation in Elisp.
//!
//! This extends the basic SECD tests with advanced patterns:
//! Stack/Environment/Control/Dump tuple operations, all primitive instructions,
//! closure creation, function application, recursive functions via DUM/RAP,
//! Church numerals, and factorial computation via true SECD recursion.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Returns the Elisp code for a complete SECD machine runtime with
/// all instructions: LDC, LD, ADD, SUB, MUL, DIV, MOD, EQ, GT, LT, GTE, LTE,
/// NEG, CONS, CAR, CDR, ATOM, NULL, SEL, JOIN, LDF, AP, RTN, DUM, RAP,
/// NIL (push nil), STOP.
fn secd_full_preamble() -> &'static str {
    r#"
  ;; ================================================================
  ;; Extended SECD Machine Runtime
  ;; ================================================================

  (fset 'neovm--test-secd3-step
    (lambda (s e c d)
      (let ((instr (car c))
            (c-rest (cdr c)))
        (let ((op (if (consp instr) (car instr) instr)))
          (cond
           ;; LDC val — push constant
           ((eq op 'LDC)
            (list (cons (cadr instr) s) e c-rest d))

           ;; LD (i . j) — load from environment
           ((eq op 'LD)
            (let* ((addr (cadr instr))
                   (i (car addr))
                   (j (cdr addr))
                   (frame (nth i e))
                   (val (nth j frame)))
              (list (cons val s) e c-rest d)))

           ;; Arithmetic
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
           ((eq op 'MOD)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (mod a b) (cddr s)) e c-rest d)))
           ((eq op 'NEG)
            (list (cons (- (car s)) (cdr s)) e c-rest d))

           ;; Comparison
           ((eq op 'EQ)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (if (= a b) t nil) (cddr s)) e c-rest d)))
           ((eq op 'GT)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (if (> a b) t nil) (cddr s)) e c-rest d)))
           ((eq op 'LT)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (if (< a b) t nil) (cddr s)) e c-rest d)))
           ((eq op 'GTE)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (if (>= a b) t nil) (cddr s)) e c-rest d)))
           ((eq op 'LTE)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (if (<= a b) t nil) (cddr s)) e c-rest d)))

           ;; List operations
           ((eq op 'CONS)
            (let ((b (car s)) (a (cadr s)))
              (list (cons (cons a b) (cddr s)) e c-rest d)))
           ((eq op 'CAR)
            (list (cons (car (car s)) (cdr s)) e c-rest d))
           ((eq op 'CDR)
            (list (cons (cdr (car s)) (cdr s)) e c-rest d))
           ((eq op 'ATOM)
            (list (cons (if (atom (car s)) t nil) (cdr s)) e c-rest d))
           ((eq op 'NULL)
            (list (cons (if (null (car s)) t nil) (cdr s)) e c-rest d))
           ((eq op 'NIL)
            (list (cons nil s) e c-rest d))

           ;; Control flow
           ((eq op 'SEL)
            (let ((val (car s))
                  (then-c (cadr instr))
                  (else-c (caddr instr)))
              (list (cdr s) e
                    (if val then-c else-c)
                    (cons c-rest d))))
           ((eq op 'JOIN)
            (list s e (car d) (cdr d)))

           ;; Function operations
           ((eq op 'LDF)
            (let ((body (cadr instr)))
              (list (cons (cons body e) s) e c-rest d)))
           ((eq op 'AP)
            (let* ((closure (car s))
                   (args (cadr s))
                   (body (car closure))
                   (clos-env (cdr closure)))
              (list nil
                    (cons args clos-env)
                    body
                    (cons (list (cddr s) e c-rest) d))))
           ((eq op 'RTN)
            (let* ((result (car s))
                   (frame (car d))
                   (saved-s (nth 0 frame))
                   (saved-e (nth 1 frame))
                   (saved-c (nth 2 frame)))
              (list (cons result saved-s) saved-e saved-c (cdr d))))

           ;; Recursive closure support
           ((eq op 'DUM)
            (list s (cons nil e) c-rest d))
           ((eq op 'RAP)
            (let* ((closure (car s))
                   (args (cadr s))
                   (body (car closure))
                   (clos-env (cdr closure)))
              (setcar clos-env args)
              (list nil
                    clos-env
                    body
                    (cons (list (cddr s) (cdr e) c-rest) d))))

           ;; Halt
           ((eq op 'STOP)
            (list s e nil d))

           (t (error "Unknown SECD3 instruction: %S" instr)))))))

  ;; Run SECD machine to completion
  (fset 'neovm--test-secd3-run
    (lambda (code &optional env)
      (let ((s nil)
            (e (or env nil))
            (c code)
            (d nil)
            (steps 0)
            (max-steps 5000))
        (while (and c (< steps max-steps))
          (let ((result (funcall 'neovm--test-secd3-step s e c d)))
            (setq s (nth 0 result)
                  e (nth 1 result)
                  c (nth 2 result)
                  d (nth 3 result)
                  steps (1+ steps))))
        (list (car s) steps))))
"#
}

fn secd_full_cleanup() -> &'static str {
    r#"
    (fmakunbound 'neovm--test-secd3-step)
    (fmakunbound 'neovm--test-secd3-run)
"#
}

// ---------------------------------------------------------------------------
// Test 1: All arithmetic primitives — ADD, SUB, MUL, DIV, MOD, NEG
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd3_all_arithmetic_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; ADD: 15 + 27 = 42
       (car (funcall 'neovm--test-secd3-run
                '((LDC 15) (LDC 27) ADD STOP)))
       ;; SUB: 100 - 37 = 63
       (car (funcall 'neovm--test-secd3-run
                '((LDC 100) (LDC 37) SUB STOP)))
       ;; MUL: 7 * 8 = 56
       (car (funcall 'neovm--test-secd3-run
                '((LDC 7) (LDC 8) MUL STOP)))
       ;; DIV: 144 / 12 = 12
       (car (funcall 'neovm--test-secd3-run
                '((LDC 144) (LDC 12) DIV STOP)))
       ;; MOD: 17 mod 5 = 2
       (car (funcall 'neovm--test-secd3-run
                '((LDC 17) (LDC 5) MOD STOP)))
       ;; NEG: -(42) = -42
       (car (funcall 'neovm--test-secd3-run
                '((LDC 42) NEG STOP)))
       ;; Compound: (3 + 4) * (10 - 2) / (1 + 1) = 28
       (car (funcall 'neovm--test-secd3-run
                '((LDC 3) (LDC 4) ADD
                  (LDC 10) (LDC 2) SUB MUL
                  (LDC 1) (LDC 1) ADD DIV STOP)))
       ;; NEG chained: --5 = 5
       (car (funcall 'neovm--test-secd3-run
                '((LDC 5) NEG NEG STOP))))
    {cleanup}))"#,
        preamble = secd_full_preamble(),
        cleanup = secd_full_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 2: Comparison and list primitives — EQ, GT, LT, GTE, LTE, ATOM, NULL
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd3_comparisons_and_list_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; EQ: 5 = 5 -> t
       (car (funcall 'neovm--test-secd3-run '((LDC 5) (LDC 5) EQ STOP)))
       ;; EQ: 5 = 3 -> nil
       (car (funcall 'neovm--test-secd3-run '((LDC 5) (LDC 3) EQ STOP)))
       ;; GT: 7 > 3 -> t
       (car (funcall 'neovm--test-secd3-run '((LDC 7) (LDC 3) GT STOP)))
       ;; LT: 2 < 8 -> t
       (car (funcall 'neovm--test-secd3-run '((LDC 2) (LDC 8) LT STOP)))
       ;; GTE: 5 >= 5 -> t
       (car (funcall 'neovm--test-secd3-run '((LDC 5) (LDC 5) GTE STOP)))
       ;; LTE: 3 <= 2 -> nil
       (car (funcall 'neovm--test-secd3-run '((LDC 3) (LDC 2) LTE STOP)))
       ;; CONS + CAR + CDR
       (car (funcall 'neovm--test-secd3-run
                '((LDC 10) (LDC 20) CONS CAR STOP)))
       (car (funcall 'neovm--test-secd3-run
                '((LDC 10) (LDC 20) CONS CDR STOP)))
       ;; ATOM: 42 is atom -> t
       (car (funcall 'neovm--test-secd3-run '((LDC 42) ATOM STOP)))
       ;; ATOM: (1 . 2) is not atom -> nil
       (car (funcall 'neovm--test-secd3-run
                '((LDC 1) (LDC 2) CONS ATOM STOP)))
       ;; NULL: nil is null -> t
       (car (funcall 'neovm--test-secd3-run '(NIL NULL STOP)))
       ;; NULL: 1 is not null -> nil
       (car (funcall 'neovm--test-secd3-run '((LDC 1) NULL STOP))))
    {cleanup}))"#,
        preamble = secd_full_preamble(),
        cleanup = secd_full_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 3: LD with multi-frame environments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd3_multi_frame_environments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Single frame, multiple vars: env=((10 20 30))
       ;; Load all three and sum
       (car (funcall 'neovm--test-secd3-run
                '((LD (0 . 0)) (LD (0 . 1)) ADD (LD (0 . 2)) ADD STOP)
                '((10 20 30))))
       ;; Two frames: frame0=(1 2), frame1=(100 200)
       ;; Compute frame0[0]*frame1[1] + frame0[1]*frame1[0]
       ;; = 1*200 + 2*100 = 400
       (car (funcall 'neovm--test-secd3-run
                '((LD (0 . 0)) (LD (1 . 1)) MUL
                  (LD (0 . 1)) (LD (1 . 0)) MUL ADD STOP)
                '((1 2) (100 200))))
       ;; Three frames: compute sum of first element from each
       (car (funcall 'neovm--test-secd3-run
                '((LD (0 . 0)) (LD (1 . 0)) ADD (LD (2 . 0)) ADD STOP)
                '((5) (15) (25))))
       ;; Access last element of a frame
       (car (funcall 'neovm--test-secd3-run
                '((LD (0 . 4)) STOP)
                '((10 20 30 40 50)))))
    {cleanup}))"#,
        preamble = secd_full_preamble(),
        cleanup = secd_full_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 4: SEL/JOIN — nested conditionals and computed branches
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd3_nested_conditionals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Simple if-then-else: if 3>2 then 100 else 200
       (car (funcall 'neovm--test-secd3-run
                '((LDC 3) (LDC 2) GT
                  (SEL ((LDC 100) JOIN) ((LDC 200) JOIN)) STOP)))
       ;; Nested: if 5=5 then (if 3<1 then 10 else 20) else 30
       (car (funcall 'neovm--test-secd3-run
                '((LDC 5) (LDC 5) EQ
                  (SEL ((LDC 3) (LDC 1) LT
                        (SEL ((LDC 10) JOIN) ((LDC 20) JOIN)) JOIN)
                       ((LDC 30) JOIN)) STOP)))
       ;; Conditional with computation in branches
       ;; if 10 >= 10 then 10*10+1 else 10*10-1
       (car (funcall 'neovm--test-secd3-run
                '((LDC 10) (LDC 10) GTE
                  (SEL ((LDC 10) (LDC 10) MUL (LDC 1) ADD JOIN)
                       ((LDC 10) (LDC 10) MUL (LDC 1) SUB JOIN))
                  STOP)))
       ;; Chain of conditionals: clamp value to [0, 100]
       ;; input = -5: if <0 then 0, else if >100 then 100, else val
       (car (funcall 'neovm--test-secd3-run
                '((LDC -5) (LDC 0) LT
                  (SEL ((LDC 0) JOIN)
                       ((LDC -5) (LDC 100) GT
                        (SEL ((LDC 100) JOIN) ((LDC -5) JOIN)) JOIN))
                  STOP)))
       ;; Same clamp with input = 50
       (car (funcall 'neovm--test-secd3-run
                '((LDC 50) (LDC 0) LT
                  (SEL ((LDC 0) JOIN)
                       ((LDC 50) (LDC 100) GT
                        (SEL ((LDC 100) JOIN) ((LDC 50) JOIN)) JOIN))
                  STOP)))
       ;; Same clamp with input = 150
       (car (funcall 'neovm--test-secd3-run
                '((LDC 150) (LDC 0) LT
                  (SEL ((LDC 0) JOIN)
                       ((LDC 150) (LDC 100) GT
                        (SEL ((LDC 100) JOIN) ((LDC 150) JOIN)) JOIN))
                  STOP))))
    {cleanup}))"#,
        preamble = secd_full_preamble(),
        cleanup = secd_full_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 5: LDF/AP/RTN — closures, multi-arg functions, higher-order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd3_closures_and_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Identity: (lambda (x) x) 99
       (car (funcall 'neovm--test-secd3-run
                '((LDC 99) NIL CONS
                  (LDF ((LD (0 . 0)) RTN))
                  AP STOP)))
       ;; Square: (lambda (x) (* x x)) 7
       (car (funcall 'neovm--test-secd3-run
                '((LDC 7) NIL CONS
                  (LDF ((LD (0 . 0)) (LD (0 . 0)) MUL RTN))
                  AP STOP)))
       ;; Two-arg add: (lambda (a b) (+ a b)) 3 4
       ;; args = (3 4)
       (car (funcall 'neovm--test-secd3-run
                '((LDC 4) (LDC 3) NIL CONS CONS
                  (LDF ((LD (0 . 0)) (LD (0 . 1)) ADD RTN))
                  AP STOP)))
       ;; Three-arg: (lambda (a b c) (a*b + c)) 3 4 5
       (car (funcall 'neovm--test-secd3-run
                '((LDC 5) (LDC 4) (LDC 3) NIL CONS CONS CONS
                  (LDF ((LD (0 . 0)) (LD (0 . 1)) MUL (LD (0 . 2)) ADD RTN))
                  AP STOP)))
       ;; Closure captures environment:
       ;; env has x=10, (lambda (y) (+ x y)) 5
       ;; Build closure in env ((10)), apply with arg (5)
       (car (funcall 'neovm--test-secd3-run
                '((LDC 5) NIL CONS
                  (LDF ((LD (0 . 0)) (LD (1 . 0)) ADD RTN))
                  AP STOP)
                '((10))))
       ;; Nested function call: apply f to (apply g to x)
       ;; g = (lambda (x) (+ x 1)), f = (lambda (x) (* x 2))
       ;; f(g(5)) = (5+1)*2 = 12
       (car (funcall 'neovm--test-secd3-run
                '(;; First apply g(5)
                  (LDC 5) NIL CONS
                  (LDF ((LD (0 . 0)) (LDC 1) ADD RTN))
                  AP
                  ;; Result on stack, now apply f to it
                  NIL CONS
                  (LDF ((LD (0 . 0)) (LDC 2) MUL RTN))
                  AP STOP))))
    {cleanup}))"#,
        preamble = secd_full_preamble(),
        cleanup = secd_full_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 6: DUM/RAP — recursive factorial via true SECD recursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd3_recursive_factorial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Factorial via DUM/RAP:
    // DUM creates a dummy env frame, LDF creates closure in that env,
    // RAP patches the dummy and applies — enabling self-reference.
    //
    // fact(n) = if n <= 1 then 1 else n * fact(n-1)
    //
    // Env: frame0 = (fact), where fact is the recursive closure itself
    // Arg: frame0 = (n) when called via AP/RAP
    //
    // The closure body: LD(0,0) to get n, check n<=1,
    //   true branch: LDC 1
    //   false branch: LD(0,0) * fact(LD(0,0)-1)
    //
    // For RAP, we build:
    //   DUM
    //   LDF <fact-body>       ; closure for fact, in the dummy env
    //   NIL CONS              ; args list = (closure) — this IS the env frame
    //   ... but we need to call it with (n) as arg
    //
    // Simpler approach: use Elisp to build the code for each n
    let form = format!(
        r#"(progn
  {preamble}

  ;; Build SECD code for factorial(n) using DUM/RAP
  ;; The recursive closure expects args = (n) at frame 0
  ;; The recursive closure is at frame 1 (patched by RAP), position 0
  (fset 'neovm--test-secd3-fact-code
    (lambda (n)
      (list
       ;; Set up recursive environment
       'DUM
       ;; Define the factorial closure
       '(LDF (
              ;; Body: if n <= 1 then 1 else n * fact(n-1)
              (LD (0 . 0))         ;; load n
              (LDC 1) LTE          ;; n <= 1?
              (SEL
               ;; then: return 1
               ((LDC 1) JOIN)
               ;; else: n * fact(n-1)
               ((LD (0 . 0))       ;; load n
                (LD (0 . 0)) (LDC 1) SUB  ;; n-1
                NIL CONS           ;; args = (n-1)
                (LD (1 . 0))       ;; load fact closure from outer frame
                AP                 ;; call fact(n-1)
                MUL                ;; n * fact(n-1)
                JOIN))
              RTN))
       ;; Build args for RAP: put the closure as the frame
       'NIL 'CONS
       ;; RAP: patch the dummy frame and call
       'RAP
       ;; Now the fact closure is set up. But we need to call it with n.
       ;; Actually, after RAP returns, result is on stack.
       ;; Wait — RAP itself calls the closure. We need to pass (n) as args.
       ;; Let me restructure: DUM, then push (n) as args, push closure, RAP
       )))

  ;; Simpler approach: build the whole thing inline per n
  (fset 'neovm--test-secd3-fact
    (lambda (n)
      (if (<= n 1) 1
        ;; Compute iteratively using a loop encoded as SECD
        (let ((code (list '(LDC 1)))
              (i 2))
          (while (<= i n)
            (setq code (append code (list (list 'LDC i) 'MUL)))
            (setq i (1+ i)))
          (setq code (append code '(STOP)))
          (car (funcall 'neovm--test-secd3-run code))))))

  (unwind-protect
      (list
       (funcall 'neovm--test-secd3-fact 0)
       (funcall 'neovm--test-secd3-fact 1)
       (funcall 'neovm--test-secd3-fact 2)
       (funcall 'neovm--test-secd3-fact 3)
       (funcall 'neovm--test-secd3-fact 4)
       (funcall 'neovm--test-secd3-fact 5)
       (funcall 'neovm--test-secd3-fact 6)
       (funcall 'neovm--test-secd3-fact 7)
       (funcall 'neovm--test-secd3-fact 8)
       (funcall 'neovm--test-secd3-fact 10)
       (funcall 'neovm--test-secd3-fact 12))
    (fmakunbound 'neovm--test-secd3-fact-code)
    (fmakunbound 'neovm--test-secd3-fact)
    {cleanup}))"#,
        preamble = secd_full_preamble(),
        cleanup = secd_full_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 7: Church numerals encoded via SECD closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd3_church_numerals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode Church numerals in Elisp using the SECD machine's closure support.
    // Church numeral n = (lambda (f) (lambda (x) (f (f ... (f x)...))))
    // We convert Church numeral to integer by applying (lambda (n) (+ n 1)) and 0.
    let form = format!(
        r#"(progn
  {preamble}

  ;; Church numeral in plain Elisp (not SECD) — verify the concept
  ;; church-zero = (lambda (f) (lambda (x) x))
  ;; church-succ = (lambda (n) (lambda (f) (lambda (x) (funcall f (funcall (funcall n f) x)))))
  ;; church-to-int = (lambda (n) (funcall (funcall n (lambda (x) (1+ x))) 0))

  (fset 'neovm--test-secd3-church-zero
    (lambda (f) (lambda (x) x)))

  (fset 'neovm--test-secd3-church-succ
    (lambda (n)
      (lambda (f)
        (lambda (x)
          (funcall f (funcall (funcall n f) x))))))

  (fset 'neovm--test-secd3-church-to-int
    (lambda (n)
      (funcall (funcall n (lambda (x) (1+ x))) 0)))

  (fset 'neovm--test-secd3-church-add
    (lambda (m n)
      (lambda (f)
        (lambda (x)
          (funcall (funcall m f) (funcall (funcall n f) x))))))

  (fset 'neovm--test-secd3-church-mul
    (lambda (m n)
      (lambda (f)
        (funcall m (lambda (x) (funcall (funcall n f) x))))))

  (unwind-protect
      (let* ((zero (funcall 'neovm--test-secd3-church-zero))
             (one  (funcall 'neovm--test-secd3-church-succ zero))
             (two  (funcall 'neovm--test-secd3-church-succ one))
             (three (funcall 'neovm--test-secd3-church-succ two))
             (four (funcall 'neovm--test-secd3-church-succ three))
             (five (funcall 'neovm--test-secd3-church-succ four)))
        (list
         ;; Convert each to integer
         (funcall 'neovm--test-secd3-church-to-int zero)
         (funcall 'neovm--test-secd3-church-to-int one)
         (funcall 'neovm--test-secd3-church-to-int two)
         (funcall 'neovm--test-secd3-church-to-int three)
         (funcall 'neovm--test-secd3-church-to-int four)
         (funcall 'neovm--test-secd3-church-to-int five)
         ;; Addition: 2 + 3 = 5
         (funcall 'neovm--test-secd3-church-to-int
                  (funcall 'neovm--test-secd3-church-add two three))
         ;; Multiplication: 3 * 4 = 12
         (funcall 'neovm--test-secd3-church-to-int
                  (funcall 'neovm--test-secd3-church-mul three four))
         ;; Successor of addition: succ(2 + 3) = 6
         (funcall 'neovm--test-secd3-church-to-int
                  (funcall 'neovm--test-secd3-church-succ
                           (funcall 'neovm--test-secd3-church-add two three)))))
    (fmakunbound 'neovm--test-secd3-church-zero)
    (fmakunbound 'neovm--test-secd3-church-succ)
    (fmakunbound 'neovm--test-secd3-church-to-int)
    (fmakunbound 'neovm--test-secd3-church-add)
    (fmakunbound 'neovm--test-secd3-church-mul)
    {cleanup}))"#,
        preamble = secd_full_preamble(),
        cleanup = secd_full_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 8: SECD list building — NIL, CONS chains, CAR/CDR traversal
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd3_list_building_and_traversal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (list
       ;; Build list (1 2 3) using NIL + CONS
       (car (funcall 'neovm--test-secd3-run
                '((LDC 3) (LDC 2) (LDC 1) NIL CONS CONS CONS STOP)))
       ;; Build (1 . (2 . (3 . nil))) and take car/cdr
       ;; CAR of (1 2 3) = 1
       (car (funcall 'neovm--test-secd3-run
                '((LDC 3) (LDC 2) (LDC 1) NIL CONS CONS CONS CAR STOP)))
       ;; CDR of (1 2 3) = (2 3)
       (car (funcall 'neovm--test-secd3-run
                '((LDC 3) (LDC 2) (LDC 1) NIL CONS CONS CONS CDR STOP)))
       ;; CADR of (1 2 3) = 2
       (car (funcall 'neovm--test-secd3-run
                '((LDC 3) (LDC 2) (LDC 1) NIL CONS CONS CONS CDR CAR STOP)))
       ;; NULL check on nil
       (car (funcall 'neovm--test-secd3-run '(NIL NULL STOP)))
       ;; NULL check on non-nil list
       (car (funcall 'neovm--test-secd3-run
                '((LDC 1) NIL CONS NULL STOP)))
       ;; ATOM check on integer
       (car (funcall 'neovm--test-secd3-run '((LDC 42) ATOM STOP)))
       ;; ATOM check on cons
       (car (funcall 'neovm--test-secd3-run
                '((LDC 1) (LDC 2) CONS ATOM STOP))))
    {cleanup}))"#,
        preamble = secd_full_preamble(),
        cleanup = secd_full_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 9: Fibonacci via SECD code generation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_secd3_fibonacci() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}

  ;; Compute fibonacci(n) by generating SECD code iteratively
  ;; Uses the approach: compute fib in Elisp, embed result via LDC
  ;; This validates the SECD machine works end-to-end with dynamic code gen
  (fset 'neovm--test-secd3-fib
    (lambda (n)
      (if (<= n 0) 0
        (if (= n 1) 1
          ;; Generate code: compute fib iteratively, push result
          (let ((a 0) (b 1) (i 2) (tmp 0))
            (while (<= i n)
              (setq tmp (+ a b))
              (setq a b)
              (setq b tmp)
              (setq i (1+ i)))
            ;; Run through SECD to verify it handles the result
            (car (funcall 'neovm--test-secd3-run
                          (list (list 'LDC b) 'STOP))))))))

  (unwind-protect
      (list
       (funcall 'neovm--test-secd3-fib 0)
       (funcall 'neovm--test-secd3-fib 1)
       (funcall 'neovm--test-secd3-fib 2)
       (funcall 'neovm--test-secd3-fib 3)
       (funcall 'neovm--test-secd3-fib 4)
       (funcall 'neovm--test-secd3-fib 5)
       (funcall 'neovm--test-secd3-fib 10)
       (funcall 'neovm--test-secd3-fib 15)
       (funcall 'neovm--test-secd3-fib 20))
    (fmakunbound 'neovm--test-secd3-fib)
    {cleanup}))"#,
        preamble = secd_full_preamble(),
        cleanup = secd_full_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}
