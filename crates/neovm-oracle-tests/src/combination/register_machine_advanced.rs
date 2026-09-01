//! Advanced oracle parity tests for register machine simulation:
//! full instruction set (LOAD, STORE, ADD, SUB, MUL, DIV, MOD, CMP,
//! JMP, JZ, JNZ, JGT, JLT, PUSH, POP, CALL, RET, HALT),
//! fetch-decode-execute cycle, stack operations, subroutine calls with
//! return addresses, factorial/fibonacci/GCD as machine programs,
//! step counting, register dump at each step, and overflow detection.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Shared machine definition: full instruction set with stack and subroutines
// ---------------------------------------------------------------------------

/// Returns the Elisp code defining the full register machine.
fn rm_defs() -> &'static str {
    r#"
  ;; Machine state: ((registers . pc) . stack)
  ;; registers: alist of (name . value)
  ;; stack: list (top is car)
  (fset 'neovm--rma-make
    (lambda () (cons (cons nil 0) nil)))

  (fset 'neovm--rma-regs (lambda (m) (caar m)))
  (fset 'neovm--rma-pc (lambda (m) (cdar m)))
  (fset 'neovm--rma-stack (lambda (m) (cdr m)))

  (fset 'neovm--rma-get-reg
    (lambda (m r)
      (let ((p (assq r (funcall 'neovm--rma-regs m))))
        (if p (cdr p) 0))))

  (fset 'neovm--rma-set-reg
    (lambda (m r v)
      (let ((regs (funcall 'neovm--rma-regs m)))
        (let ((p (assq r regs)))
          (if p (setcdr p v)
            (setcar (car m) (cons (cons r v) regs)))))
      m))

  (fset 'neovm--rma-set-pc
    (lambda (m pc) (setcdr (car m) pc) m))

  (fset 'neovm--rma-push
    (lambda (m val) (setcdr m (cons val (cdr m))) m))

  (fset 'neovm--rma-pop
    (lambda (m)
      (let ((top (cadr m)))
        (setcdr m (cddr m))
        top)))

  (fset 'neovm--rma-step
    (lambda (m prog)
      (let* ((pc (funcall 'neovm--rma-pc m))
             (instr (aref prog pc))
             (op (car instr)))
        (cond
          ((eq op 'LOAD)
           (funcall 'neovm--rma-set-reg m (nth 1 instr) (nth 2 instr))
           (funcall 'neovm--rma-set-pc m (1+ pc)))
          ((eq op 'STORE)
           (funcall 'neovm--rma-set-reg m (nth 1 instr)
                    (funcall 'neovm--rma-get-reg m (nth 2 instr)))
           (funcall 'neovm--rma-set-pc m (1+ pc)))
          ((eq op 'ADD)
           (funcall 'neovm--rma-set-reg m (nth 1 instr)
                    (+ (funcall 'neovm--rma-get-reg m (nth 1 instr))
                       (funcall 'neovm--rma-get-reg m (nth 2 instr))))
           (funcall 'neovm--rma-set-pc m (1+ pc)))
          ((eq op 'SUB)
           (funcall 'neovm--rma-set-reg m (nth 1 instr)
                    (- (funcall 'neovm--rma-get-reg m (nth 1 instr))
                       (funcall 'neovm--rma-get-reg m (nth 2 instr))))
           (funcall 'neovm--rma-set-pc m (1+ pc)))
          ((eq op 'MUL)
           (funcall 'neovm--rma-set-reg m (nth 1 instr)
                    (* (funcall 'neovm--rma-get-reg m (nth 1 instr))
                       (funcall 'neovm--rma-get-reg m (nth 2 instr))))
           (funcall 'neovm--rma-set-pc m (1+ pc)))
          ((eq op 'DIV)
           (funcall 'neovm--rma-set-reg m (nth 1 instr)
                    (/ (funcall 'neovm--rma-get-reg m (nth 1 instr))
                       (funcall 'neovm--rma-get-reg m (nth 2 instr))))
           (funcall 'neovm--rma-set-pc m (1+ pc)))
          ((eq op 'MOD)
           (funcall 'neovm--rma-set-reg m (nth 1 instr)
                    (% (funcall 'neovm--rma-get-reg m (nth 1 instr))
                       (funcall 'neovm--rma-get-reg m (nth 2 instr))))
           (funcall 'neovm--rma-set-pc m (1+ pc)))
          ((eq op 'CMP)
           (let ((v1 (funcall 'neovm--rma-get-reg m (nth 1 instr)))
                 (v2 (funcall 'neovm--rma-get-reg m (nth 2 instr))))
             (funcall 'neovm--rma-set-reg m 'flag
                      (cond ((< v1 v2) -1) ((= v1 v2) 0) (t 1))))
           (funcall 'neovm--rma-set-pc m (1+ pc)))
          ((eq op 'JMP)
           (funcall 'neovm--rma-set-pc m (nth 1 instr)))
          ((eq op 'JZ)
           (if (= (funcall 'neovm--rma-get-reg m 'flag) 0)
               (funcall 'neovm--rma-set-pc m (nth 1 instr))
             (funcall 'neovm--rma-set-pc m (1+ pc))))
          ((eq op 'JNZ)
           (if (/= (funcall 'neovm--rma-get-reg m 'flag) 0)
               (funcall 'neovm--rma-set-pc m (nth 1 instr))
             (funcall 'neovm--rma-set-pc m (1+ pc))))
          ((eq op 'JGT)
           (if (= (funcall 'neovm--rma-get-reg m 'flag) 1)
               (funcall 'neovm--rma-set-pc m (nth 1 instr))
             (funcall 'neovm--rma-set-pc m (1+ pc))))
          ((eq op 'JLT)
           (if (= (funcall 'neovm--rma-get-reg m 'flag) -1)
               (funcall 'neovm--rma-set-pc m (nth 1 instr))
             (funcall 'neovm--rma-set-pc m (1+ pc))))
          ((eq op 'PUSH)
           (funcall 'neovm--rma-push m (funcall 'neovm--rma-get-reg m (nth 1 instr)))
           (funcall 'neovm--rma-set-pc m (1+ pc)))
          ((eq op 'POP)
           (funcall 'neovm--rma-set-reg m (nth 1 instr)
                    (funcall 'neovm--rma-pop m))
           (funcall 'neovm--rma-set-pc m (1+ pc)))
          ((eq op 'CALL)
           ;; Push return address (next instruction), jump to target
           (funcall 'neovm--rma-push m (1+ pc))
           (funcall 'neovm--rma-set-pc m (nth 1 instr)))
          ((eq op 'RET)
           ;; Pop return address and jump there
           (let ((ret-addr (funcall 'neovm--rma-pop m)))
             (funcall 'neovm--rma-set-pc m ret-addr)))
          ((eq op 'HALT) m)
          (t (error "Unknown opcode: %s" op)))
        m)))

  (fset 'neovm--rma-run
    (lambda (m prog &optional max-steps)
      (let ((steps 0) (limit (or max-steps 10000)))
        (while (and (< steps limit)
                    (< (funcall 'neovm--rma-pc m) (length prog))
                    (not (eq (car (aref prog (funcall 'neovm--rma-pc m))) 'HALT)))
          (funcall 'neovm--rma-step m prog)
          (setq steps (1+ steps)))
        (cons steps m))))
"#
}

fn rm_cleanup() -> &'static str {
    r#"
    (fmakunbound 'neovm--rma-make)
    (fmakunbound 'neovm--rma-regs)
    (fmakunbound 'neovm--rma-pc)
    (fmakunbound 'neovm--rma-stack)
    (fmakunbound 'neovm--rma-get-reg)
    (fmakunbound 'neovm--rma-set-reg)
    (fmakunbound 'neovm--rma-set-pc)
    (fmakunbound 'neovm--rma-push)
    (fmakunbound 'neovm--rma-pop)
    (fmakunbound 'neovm--rma-step)
    (fmakunbound 'neovm--rma-run)
"#
}

// ---------------------------------------------------------------------------
// DIV and MOD instructions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_div_mod_instructions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      (let ((results nil))
        ;; Test DIV instruction: integer division
        (let* ((m (funcall 'neovm--rma-make))
               (prog (vector '(LOAD a 100)
                              '(LOAD b 7)
                              '(STORE q a)
                              '(DIV q b)       ;; q = 100 / 7 = 14
                              '(STORE r a)
                              '(MOD r b)       ;; r = 100 % 7 = 2
                              '(HALT)))
               (result (funcall 'neovm--rma-run m prog)))
          (push (list :div-mod
                      :q (funcall 'neovm--rma-get-reg (cdr result) 'q)
                      :r (funcall 'neovm--rma-get-reg (cdr result) 'r)
                      :steps (car result)
                      ;; Verify: q*b + r == a
                      :verify (= (+ (* (funcall 'neovm--rma-get-reg (cdr result) 'q)
                                       (funcall 'neovm--rma-get-reg (cdr result) 'b))
                                    (funcall 'neovm--rma-get-reg (cdr result) 'r))
                                 (funcall 'neovm--rma-get-reg (cdr result) 'a)))
                results))

        ;; Negative division
        (let* ((m (funcall 'neovm--rma-make))
               (prog (vector '(LOAD a -17)
                              '(LOAD b 5)
                              '(STORE q a)
                              '(DIV q b)       ;; -17 / 5 = -3 (truncate toward 0)
                              '(STORE r a)
                              '(MOD r b)       ;; -17 % 5 = -2
                              '(HALT)))
               (result (funcall 'neovm--rma-run m prog)))
          (push (list :neg-div
                      :q (funcall 'neovm--rma-get-reg (cdr result) 'q)
                      :r (funcall 'neovm--rma-get-reg (cdr result) 'r))
                results))

        (nreverse results))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// JGT and JLT instructions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_jgt_jlt_instructions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; Find the maximum of three values using JGT/JLT
      (let* ((m (funcall 'neovm--rma-make))
             (prog (vector
                     '(LOAD a 15)         ;; 0
                     '(LOAD b 42)         ;; 1
                     '(LOAD c 27)         ;; 2
                     '(STORE max a)       ;; 3: max = a
                     '(CMP b max)         ;; 4: compare b with max
                     '(JGT 7)             ;; 5: if b > max, go to update
                     '(JMP 8)             ;; 6: skip update
                     '(STORE max b)       ;; 7: max = b
                     '(CMP c max)         ;; 8: compare c with max
                     '(JGT 11)            ;; 9: if c > max, go to update
                     '(JMP 12)            ;; 10: skip
                     '(STORE max c)       ;; 11: max = c
                     '(HALT)))            ;; 12
             (result (funcall 'neovm--rma-run m prog)))
        (list
          :max (funcall 'neovm--rma-get-reg (cdr result) 'max)
          :a (funcall 'neovm--rma-get-reg (cdr result) 'a)
          :b (funcall 'neovm--rma-get-reg (cdr result) 'b)
          :c (funcall 'neovm--rma-get-reg (cdr result) 'c)
          :steps (car result)))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// PUSH and POP: stack operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_stack_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      (let ((results nil))
        ;; Push several values, pop in reverse order
        (let* ((m (funcall 'neovm--rma-make))
               (prog (vector
                       '(LOAD a 10)     ;; 0
                       '(LOAD b 20)     ;; 1
                       '(LOAD c 30)     ;; 2
                       '(PUSH a)        ;; 3: stack = (10)
                       '(PUSH b)        ;; 4: stack = (20 10)
                       '(PUSH c)        ;; 5: stack = (30 20 10)
                       '(POP x)         ;; 6: x = 30, stack = (20 10)
                       '(POP y)         ;; 7: y = 20, stack = (10)
                       '(POP z)         ;; 8: z = 10, stack = ()
                       '(HALT)))        ;; 9
               (result (funcall 'neovm--rma-run m prog)))
          (push (list :stack-lifo
                      :x (funcall 'neovm--rma-get-reg (cdr result) 'x)
                      :y (funcall 'neovm--rma-get-reg (cdr result) 'y)
                      :z (funcall 'neovm--rma-get-reg (cdr result) 'z))
                results))

        ;; Stack-based expression: (3 + 4) * (5 - 2)
        ;; Push 3, Push 4, Pop both, add, push result
        ;; Push 5, Push 2, Pop both, sub, push result
        ;; Pop both, multiply
        (let* ((m (funcall 'neovm--rma-make))
               (prog (vector
                       '(LOAD a 3)      ;; 0
                       '(LOAD b 4)      ;; 1
                       '(ADD a b)       ;; 2: a = 7
                       '(PUSH a)        ;; 3: stack = (7)
                       '(LOAD a 5)      ;; 4
                       '(LOAD b 2)      ;; 5
                       '(SUB a b)       ;; 6: a = 3
                       '(POP b)         ;; 7: b = 7, stack = ()
                       '(MUL a b)       ;; 8: a = 3 * 7 = 21
                       '(HALT)))        ;; 9
               (result (funcall 'neovm--rma-run m prog)))
          (push (list :stack-expr
                      :result (funcall 'neovm--rma-get-reg (cdr result) 'a)
                      :steps (car result))
                results))

        (nreverse results))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// CALL and RET: subroutine calls with return addresses
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_subroutine_call_ret() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; Program with a subroutine that doubles a register value
      ;; Main: load 5 into 'arg', call double, result in 'arg'
      ;; Double subroutine at addr 5: arg = arg + arg, ret
      (let* ((m (funcall 'neovm--rma-make))
             (prog (vector
                     '(LOAD arg 5)       ;; 0: arg = 5
                     '(CALL 5)           ;; 1: call double (pushes 2)
                     '(STORE result arg) ;; 2: result = 10
                     '(LOAD arg 7)       ;; 3: arg = 7
                     '(CALL 5)           ;; 4: call double (pushes 5)
                     ;; ---- subroutine: double ----
                     '(ADD arg arg)      ;; 5: arg = arg + arg
                     '(RET)              ;; 6: return
                     ;; ---- back in main ----
                     ;; (PC=5 after CALL 4, which pushed 5, RET pops 5 -> PC=5, but
                     ;;  actually CALL pushes (1+ pc)=5, so we return to addr 5 which
                     ;;  is the subroutine again! Need to fix layout.)
                     '(HALT)))           ;; 7
             ;; Better layout: subroutine at end
             (prog2 (vector
                      '(LOAD arg 5)       ;; 0: arg = 5
                      '(CALL 7)           ;; 1: call double, pushes 2
                      '(STORE r1 arg)     ;; 2: r1 = 10 (returned)
                      '(LOAD arg 7)       ;; 3: arg = 7
                      '(CALL 7)           ;; 4: call double, pushes 5
                      '(STORE r2 arg)     ;; 5: r2 = 14
                      '(HALT)             ;; 6: done
                      ;; ---- subroutine: double (addr 7) ----
                      '(ADD arg arg)      ;; 7: arg *= 2
                      '(RET)))            ;; 8: return
             (result (funcall 'neovm--rma-run m prog2)))
        (list :r1 (funcall 'neovm--rma-get-reg (cdr result) 'r1)
              :r2 (funcall 'neovm--rma-get-reg (cdr result) 'r2)
              :steps (car result)))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Nested subroutine calls
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_nested_subroutines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; subroutine A calls subroutine B
      ;; A: multiplies 'arg' by 3 (by calling B twice + add)
      ;; B: doubles 'arg'
      ;; Main: arg=4, call A -> arg=12
      (let* ((m (funcall 'neovm--rma-make))
             (prog (vector
                     ;; Main
                     '(LOAD arg 4)       ;; 0: arg = 4
                     '(CALL 4)           ;; 1: call triple_via_double, pushes 2
                     '(STORE result arg) ;; 2: result = 12
                     '(HALT)             ;; 3: done
                     ;; ---- subroutine: triple_via_double (addr 4) ----
                     ;; triple = double + original = 2*arg + arg = 3*arg
                     '(PUSH arg)         ;; 4: save original arg
                     '(CALL 9)           ;; 5: call double, pushes 6
                     '(POP tmp)          ;; 6: tmp = original arg
                     '(ADD arg tmp)      ;; 7: arg = 2*arg + arg = 3*arg
                     '(RET)              ;; 8: return
                     ;; ---- subroutine: double (addr 9) ----
                     '(ADD arg arg)      ;; 9: arg *= 2
                     '(RET)))            ;; 10: return
             (result (funcall 'neovm--rma-run m prog)))
        (list :result (funcall 'neovm--rma-get-reg (cdr result) 'result)
              :steps (car result)))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Factorial using CALL/RET (iterative with subroutine)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_factorial_with_subroutine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; Main calls factorial subroutine.
      ;; Factorial is iterative: result=1, loop: if n==0 ret. result*=n. n-=1. goto loop.
      (let ((prog (vector
                    ;; Main
                    '(LOAD n 7)          ;; 0: compute 7!
                    '(CALL 4)            ;; 1: call factorial, pushes 2
                    '(STORE answer result) ;; 2
                    '(HALT)              ;; 3
                    ;; ---- factorial subroutine (addr 4) ----
                    '(LOAD result 1)     ;; 4
                    '(LOAD one 1)        ;; 5
                    '(LOAD zero 0)       ;; 6
                    '(CMP n zero)        ;; 7: [LOOP]
                    '(JZ 12)             ;; 8: if n == 0, goto RET
                    '(MUL result n)      ;; 9: result *= n
                    '(SUB n one)         ;; 10: n -= 1
                    '(JMP 7)             ;; 11: goto LOOP
                    '(RET))))            ;; 12: return
        (let ((results nil))
          ;; Test 7! = 5040
          (let* ((m (funcall 'neovm--rma-make))
                 (_ (funcall 'neovm--rma-set-reg m 'n 7))
                 ;; Override: set n via program
                 (r (funcall 'neovm--rma-run m prog)))
            (push (list :fact-7
                        :answer (funcall 'neovm--rma-get-reg (cdr r) 'answer)
                        :steps (car r))
                  results))
          ;; Test 0! = 1
          (let* ((m (funcall 'neovm--rma-make))
                 (prog0 (vector
                          '(LOAD n 0)
                          '(CALL 4)
                          '(STORE answer result)
                          '(HALT)
                          '(LOAD result 1)
                          '(LOAD one 1)
                          '(LOAD zero 0)
                          '(CMP n zero)
                          '(JZ 12)
                          '(MUL result n)
                          '(SUB n one)
                          '(JMP 7)
                          '(RET)))
                 (r (funcall 'neovm--rma-run m prog0)))
            (push (list :fact-0
                        :answer (funcall 'neovm--rma-get-reg (cdr r) 'answer))
                  results))
          ;; Test 10! = 3628800
          (let* ((m (funcall 'neovm--rma-make))
                 (prog10 (vector
                           '(LOAD n 10)
                           '(CALL 4)
                           '(STORE answer result)
                           '(HALT)
                           '(LOAD result 1)
                           '(LOAD one 1)
                           '(LOAD zero 0)
                           '(CMP n zero)
                           '(JZ 12)
                           '(MUL result n)
                           '(SUB n one)
                           '(JMP 7)
                           '(RET)))
                 (r (funcall 'neovm--rma-run m prog10)))
            (push (list :fact-10
                        :answer (funcall 'neovm--rma-get-reg (cdr r) 'answer)
                        :verify (= (funcall 'neovm--rma-get-reg (cdr r) 'answer) 3628800))
                  results))
          (nreverse results)))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Fibonacci with step-by-step register dump
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_fibonacci_with_trace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  ;; Step-by-step runner that records register values at each step
  (fset 'neovm--rma-run-trace
    (lambda (m prog reg-names max-steps)
      (let ((steps 0) (trace nil) (limit (or max-steps 500)))
        (while (and (< steps limit)
                    (< (funcall 'neovm--rma-pc m) (length prog))
                    (not (eq (car (aref prog (funcall 'neovm--rma-pc m))) 'HALT)))
          (funcall 'neovm--rma-step m prog)
          (setq steps (1+ steps))
          ;; Record register snapshot every few steps
          (when (= (% steps 5) 0)
            (let ((snapshot nil))
              (dolist (r reg-names)
                (push (cons r (funcall 'neovm--rma-get-reg m r)) snapshot))
              (push (cons steps (nreverse snapshot)) trace))))
        (list :steps steps
              :trace (nreverse trace)
              :final (mapcar (lambda (r) (cons r (funcall 'neovm--rma-get-reg m r)))
                             reg-names)))))

  (unwind-protect
      ;; Fibonacci: fib(n) using iterative register machine
      (let ((fib-prog (vector
                        '(LOAD a 0)        ;; 0
                        '(LOAD b 1)        ;; 1
                        '(LOAD one 1)      ;; 2
                        '(LOAD zero 0)     ;; 3
                        '(CMP n zero)      ;; 4: [LOOP]
                        '(JZ 12)           ;; 5: if n==0, done
                        '(STORE tmp a)     ;; 6
                        '(ADD tmp b)       ;; 7: tmp = a + b
                        '(STORE a b)       ;; 8: a = b
                        '(STORE b tmp)     ;; 9: b = tmp
                        '(SUB n one)       ;; 10: n -= 1
                        '(JMP 4)           ;; 11: goto LOOP
                        '(HALT))))         ;; 12
        (let ((results nil))
          ;; fib(10) with trace
          (let* ((m (funcall 'neovm--rma-make))
                 (_ (funcall 'neovm--rma-set-reg m 'n 10))
                 (r (funcall 'neovm--rma-run-trace m fib-prog '(a b n) 500)))
            (push (list :fib-10 r) results))
          ;; fib(15)
          (let* ((m (funcall 'neovm--rma-make))
                 (_ (funcall 'neovm--rma-set-reg m 'n 15))
                 (r (funcall 'neovm--rma-run-trace m fib-prog '(a b n) 500)))
            (push (list :fib-15-final (nth 5 r)) results))
          (nreverse results)))
    (fmakunbound 'neovm--rma-run-trace)
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// GCD with full instruction set (using DIV and MOD)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_gcd_with_div_mod() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; GCD subroutine using MOD:
      ;; while b != 0: tmp = a MOD b. a = b. b = tmp.
      (let ((gcd-prog (vector
                        '(LOAD zero 0)       ;; 0
                        '(CMP b zero)        ;; 1: [LOOP]
                        '(JZ 8)              ;; 2: if b==0, done
                        '(STORE tmp a)       ;; 3
                        '(MOD tmp b)         ;; 4: tmp = a % b
                        '(STORE a b)         ;; 5: a = b
                        '(STORE b tmp)       ;; 6: b = a % old_b
                        '(JMP 1)             ;; 7: goto LOOP
                        '(HALT))))           ;; 8
        (let ((results nil)
              (test-cases '((48 18 6) (100 75 25) (17 13 1)
                            (0 5 5) (12 12 12) (1071 462 21)
                            (270 192 6) (1 1 1))))
          (dolist (tc test-cases)
            (let* ((m (funcall 'neovm--rma-make))
                   (_ (funcall 'neovm--rma-set-reg m 'a (car tc)))
                   (_ (funcall 'neovm--rma-set-reg m 'b (cadr tc)))
                   (r (funcall 'neovm--rma-run m gcd-prog)))
              (push (list (car tc) (cadr tc)
                          :gcd (funcall 'neovm--rma-get-reg (cdr r) 'a)
                          :expected (nth 2 tc)
                          :correct (= (funcall 'neovm--rma-get-reg (cdr r) 'a) (nth 2 tc))
                          :steps (car r))
                    results)))
          (nreverse results)))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Overflow detection: multiply until overflow threshold
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_overflow_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; Repeatedly multiply by 2 and count steps until exceeding a threshold
      (let ((prog (vector
                    '(LOAD val 1)         ;; 0
                    '(LOAD two 2)         ;; 1
                    '(LOAD zero 0)        ;; 2
                    '(LOAD limit 10000)   ;; 3
                    '(LOAD count 0)       ;; 4
                    '(LOAD one 1)         ;; 5
                    '(CMP val limit)      ;; 6: [LOOP]
                    '(JGT 11)             ;; 7: if val > limit, done
                    '(MUL val two)        ;; 8: val *= 2
                    '(ADD count one)      ;; 9: count++
                    '(JMP 6)              ;; 10: goto LOOP
                    '(HALT))))            ;; 11
        (let* ((m (funcall 'neovm--rma-make))
               (r (funcall 'neovm--rma-run m prog)))
          (list :final-val (funcall 'neovm--rma-get-reg (cdr r) 'val)
                :count (funcall 'neovm--rma-get-reg (cdr r) 'count)
                :steps (car r)
                ;; val should be 2^count > 10000
                :verify-power (= (funcall 'neovm--rma-get-reg (cdr r) 'val)
                                 (expt 2 (funcall 'neovm--rma-get-reg (cdr r) 'count))))))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Bubble sort using register machine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_bubble_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; Sort 5 values in registers using bubble sort logic
      ;; Registers r0..r4 hold values, use nested loops with CMP/JGT to swap
      ;; Simplified: just do enough passes to sort 5 elements
      (let ((results nil))
        ;; Manual swap-based sort of 3 values
        (let* ((m (funcall 'neovm--rma-make))
               (prog (vector
                       '(LOAD a 30)        ;; 0
                       '(LOAD b 10)        ;; 1
                       '(LOAD c 20)        ;; 2
                       ;; Sort a,b: if a > b, swap
                       '(CMP a b)          ;; 3
                       '(JGT 6)            ;; 4: if a > b, swap
                       '(JMP 9)            ;; 5: skip swap
                       '(STORE tmp a)      ;; 6: tmp = a
                       '(STORE a b)        ;; 7: a = b
                       '(STORE b tmp)      ;; 8: b = tmp
                       ;; Sort b,c: if b > c, swap
                       '(CMP b c)          ;; 9
                       '(JGT 12)           ;; 10
                       '(JMP 15)           ;; 11
                       '(STORE tmp b)      ;; 12
                       '(STORE b c)        ;; 13
                       '(STORE c tmp)      ;; 14
                       ;; Sort a,b again (second pass)
                       '(CMP a b)          ;; 15
                       '(JGT 18)           ;; 16
                       '(JMP 21)           ;; 17
                       '(STORE tmp a)      ;; 18
                       '(STORE a b)        ;; 19
                       '(STORE b tmp)      ;; 20
                       '(HALT)))           ;; 21
               (r (funcall 'neovm--rma-run m prog)))
          (push (list :sorted
                      (funcall 'neovm--rma-get-reg (cdr r) 'a)
                      (funcall 'neovm--rma-get-reg (cdr r) 'b)
                      (funcall 'neovm--rma-get-reg (cdr r) 'c)
                      :steps (car r)
                      :in-order (<= (funcall 'neovm--rma-get-reg (cdr r) 'a)
                                    (funcall 'neovm--rma-get-reg (cdr r) 'b)
                                    (funcall 'neovm--rma-get-reg (cdr r) 'c)))
                results))

        ;; Already sorted input
        (let* ((m (funcall 'neovm--rma-make))
               (prog (vector
                       '(LOAD a 10) '(LOAD b 20) '(LOAD c 30)
                       '(CMP a b) '(JGT 6) '(JMP 9)
                       '(STORE tmp a) '(STORE a b) '(STORE b tmp)
                       '(CMP b c) '(JGT 12) '(JMP 15)
                       '(STORE tmp b) '(STORE b c) '(STORE c tmp)
                       '(CMP a b) '(JGT 18) '(JMP 21)
                       '(STORE tmp a) '(STORE a b) '(STORE b tmp)
                       '(HALT)))
               (r (funcall 'neovm--rma-run m prog)))
          (push (list :already-sorted
                      (funcall 'neovm--rma-get-reg (cdr r) 'a)
                      (funcall 'neovm--rma-get-reg (cdr r) 'b)
                      (funcall 'neovm--rma-get-reg (cdr r) 'c)
                      :steps (car r))
                results))

        (nreverse results))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Power function with subroutine call
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_power_subroutine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; Main calls power subroutine with base in 'base' and exp in 'exp'
      ;; Power subroutine (iterative): result=1, loop: if exp==0 ret. result*=base. exp-=1.
      (let ((prog (vector
                    ;; Main
                    '(LOAD base 3)        ;; 0
                    '(LOAD exp 5)         ;; 1
                    '(CALL 5)             ;; 2: call power, pushes 3
                    '(STORE answer result) ;; 3: answer = 3^5 = 243
                    '(HALT)               ;; 4
                    ;; ---- power subroutine (addr 5) ----
                    '(LOAD result 1)      ;; 5
                    '(LOAD one 1)         ;; 6
                    '(LOAD zero 0)        ;; 7
                    '(CMP exp zero)       ;; 8: [LOOP]
                    '(JZ 13)              ;; 9: if exp==0, return
                    '(MUL result base)    ;; 10: result *= base
                    '(SUB exp one)        ;; 11: exp -= 1
                    '(JMP 8)              ;; 12: goto LOOP
                    '(RET))))             ;; 13
        (let ((results nil)
              (tests '((2 10 1024) (3 5 243) (5 3 125) (7 2 49) (2 0 1) (10 3 1000))))
          (dolist (tc tests)
            (let* ((m (funcall 'neovm--rma-make))
                   (p (vector
                        (list 'LOAD 'base (car tc))
                        (list 'LOAD 'exp (cadr tc))
                        '(CALL 5)
                        '(STORE answer result)
                        '(HALT)
                        '(LOAD result 1)
                        '(LOAD one 1)
                        '(LOAD zero 0)
                        '(CMP exp zero)
                        '(JZ 13)
                        '(MUL result base)
                        '(SUB exp one)
                        '(JMP 8)
                        '(RET)))
                   (r (funcall 'neovm--rma-run m p)))
              (push (list (car tc) (cadr tc)
                          :got (funcall 'neovm--rma-get-reg (cdr r) 'answer)
                          :expected (nth 2 tc)
                          :ok (= (funcall 'neovm--rma-get-reg (cdr r) 'answer) (nth 2 tc)))
                    results)))
          (nreverse results)))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Sum of 1..N using loop with step counting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_sum_1_to_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; Compute sum = 1 + 2 + ... + N using a register machine loop
      (let ((sum-prog (vector
                        '(LOAD sum 0)       ;; 0
                        '(LOAD i 1)         ;; 1
                        '(LOAD one 1)       ;; 2
                        '(CMP i n)          ;; 3: [LOOP]
                        '(JGT 8)            ;; 4: if i > n, done
                        '(ADD sum i)        ;; 5: sum += i
                        '(ADD i one)        ;; 6: i++
                        '(JMP 3)            ;; 7: goto LOOP
                        '(HALT))))          ;; 8
        (let ((results nil))
          (dolist (n-val '(0 1 5 10 20 50 100))
            (let* ((m (funcall 'neovm--rma-make))
                   (_ (funcall 'neovm--rma-set-reg m 'n n-val))
                   (r (funcall 'neovm--rma-run m sum-prog)))
              (push (list n-val
                          :sum (funcall 'neovm--rma-get-reg (cdr r) 'sum)
                          :expected (/ (* n-val (1+ n-val)) 2)
                          :correct (= (funcall 'neovm--rma-get-reg (cdr r) 'sum)
                                      (/ (* n-val (1+ n-val)) 2))
                          :steps (car r))
                    results)))
          (nreverse results)))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Collatz conjecture: count steps to reach 1
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_collatz_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; Collatz: if n is even, n/=2; if odd, n=3*n+1. Count steps to reach 1.
      ;; We use MOD to test even/odd, DIV for /2, MUL+ADD for 3n+1
      (let ((collatz-prog (vector
                            '(LOAD one 1)       ;; 0
                            '(LOAD two 2)       ;; 1
                            '(LOAD three 3)     ;; 2
                            '(LOAD zero 0)      ;; 3
                            '(LOAD count 0)     ;; 4
                            ;; [LOOP]
                            '(CMP n one)        ;; 5
                            '(JZ 16)            ;; 6: if n==1, done
                            '(STORE tmp n)      ;; 7
                            '(MOD tmp two)      ;; 8: tmp = n % 2
                            '(CMP tmp zero)     ;; 9
                            '(JZ 13)            ;; 10: if even, goto even-branch
                            ;; odd branch: n = 3*n + 1
                            '(MUL n three)      ;; 11
                            '(ADD n one)        ;; 12
                            '(JMP 14)           ;; 13-alt: skip to count++ (reuse addr)
                            ;; even branch: n = n / 2
                            '(DIV n two)        ;; 13
                            ;; count++
                            '(ADD count one)    ;; 14
                            '(JMP 5)            ;; 15: goto LOOP
                            '(HALT))))          ;; 16
        ;; Oops, addr 13 is used for both jump target and even branch.
        ;; Fix: adjust addresses.
        (let ((collatz-v2 (vector
                            '(LOAD one 1)       ;; 0
                            '(LOAD two 2)       ;; 1
                            '(LOAD three 3)     ;; 2
                            '(LOAD zero 0)      ;; 3
                            '(LOAD count 0)     ;; 4
                            '(CMP n one)        ;; 5: [LOOP]
                            '(JZ 15)            ;; 6: if n==1, done
                            '(STORE tmp n)      ;; 7
                            '(MOD tmp two)      ;; 8
                            '(CMP tmp zero)     ;; 9
                            '(JZ 13)            ;; 10: if even
                            ;; odd: n = 3n+1
                            '(MUL n three)      ;; 11
                            '(ADD n one)        ;; 12
                            ;; even: n = n/2  (odd branch falls through to ADD count)
                            '(DIV n two)        ;; 13
                            ;; This is wrong: odd branch should skip DIV.
                            '(HALT)             ;; placeholder
                            '(HALT))))          ;; 15
          ;; Correct version with proper jumps:
          (let ((collatz-v3 (vector
                              '(LOAD one 1)       ;; 0
                              '(LOAD two 2)       ;; 1
                              '(LOAD three 3)     ;; 2
                              '(LOAD zero 0)      ;; 3
                              '(LOAD count 0)     ;; 4
                              '(CMP n one)        ;; 5: [LOOP]
                              '(JZ 16)            ;; 6: if n==1, done
                              '(STORE tmp n)      ;; 7
                              '(MOD tmp two)      ;; 8
                              '(CMP tmp zero)     ;; 9
                              '(JZ 14)            ;; 10: if even, goto EVEN
                              ;; ODD: n = 3n+1
                              '(MUL n three)      ;; 11
                              '(ADD n one)        ;; 12
                              '(JMP 15)           ;; 13: goto COUNT
                              ;; EVEN: n = n/2
                              '(DIV n two)        ;; 14
                              ;; COUNT:
                              '(ADD count one)    ;; 15
                              '(JMP 5)            ;; 16-wrong... HALT should be elsewhere
                              '(HALT))))          ;; 17
            ;; Fix: HALT at 17, JZ should go to 17
            (let ((cz (vector
                        '(LOAD one 1)       ;; 0
                        '(LOAD two 2)       ;; 1
                        '(LOAD three 3)     ;; 2
                        '(LOAD zero 0)      ;; 3
                        '(LOAD count 0)     ;; 4
                        '(CMP n one)        ;; 5: [LOOP]
                        '(JZ 17)            ;; 6: if n==1, HALT
                        '(STORE tmp n)      ;; 7
                        '(MOD tmp two)      ;; 8
                        '(CMP tmp zero)     ;; 9
                        '(JZ 14)            ;; 10: if even
                        '(MUL n three)      ;; 11: odd
                        '(ADD n one)        ;; 12
                        '(JMP 15)           ;; 13: goto COUNT
                        '(DIV n two)        ;; 14: even
                        '(ADD count one)    ;; 15: COUNT
                        '(JMP 5)            ;; 16: goto LOOP
                        '(HALT))))          ;; 17
              (let ((results nil))
                (dolist (start '(1 2 3 6 7 10 27))
                  (let* ((m (funcall 'neovm--rma-make))
                         (_ (funcall 'neovm--rma-set-reg m 'n start))
                         (r (funcall 'neovm--rma-run m cz 500)))
                    (push (list start
                                :steps-to-1 (funcall 'neovm--rma-get-reg (cdr r) 'count)
                                :final-n (funcall 'neovm--rma-get-reg (cdr r) 'n))
                          results)))
                (nreverse results)))))))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Multiple subroutine calls: compute expression using helpers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_multi_subroutine_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; Compute: square(3) + square(4) using a square subroutine
      ;; square(x) = x * x
      (let ((prog (vector
                    ;; Main
                    '(LOAD x 3)           ;; 0
                    '(CALL 9)             ;; 1: call square, pushes 2
                    '(STORE s1 result)    ;; 2: s1 = 9
                    '(LOAD x 4)           ;; 3
                    '(CALL 9)             ;; 4: call square, pushes 5
                    '(STORE s2 result)    ;; 5: s2 = 16
                    '(STORE total s1)     ;; 6
                    '(ADD total s2)       ;; 7: total = 9 + 16 = 25
                    '(HALT)               ;; 8
                    ;; ---- square subroutine (addr 9) ----
                    '(STORE result x)     ;; 9: result = x
                    '(MUL result x)       ;; 10: result = x * x
                    '(RET))))             ;; 11
        (let* ((m (funcall 'neovm--rma-make))
               (r (funcall 'neovm--rma-run m prog)))
          (list :s1 (funcall 'neovm--rma-get-reg (cdr r) 's1)
                :s2 (funcall 'neovm--rma-get-reg (cdr r) 's2)
                :total (funcall 'neovm--rma-get-reg (cdr r) 'total)
                :steps (car r)
                :verify (= (funcall 'neovm--rma-get-reg (cdr r) 'total) 25))))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Counting bits set: population count via register machine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_adv_popcount() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {defs}
  (unwind-protect
      ;; Count bits set in register 'val' using shift and mask
      ;; While val > 0: count += (val AND 1); val >>= 1
      ;; We simulate ash with DIV by 2 (for positive numbers)
      (let ((popcount-prog (vector
                             '(LOAD count 0)     ;; 0
                             '(LOAD one 1)       ;; 1
                             '(LOAD two 2)       ;; 2
                             '(LOAD zero 0)      ;; 3
                             '(CMP val zero)     ;; 4: [LOOP]
                             '(JZ 11)            ;; 5: if val==0, done
                             '(STORE tmp val)    ;; 6
                             '(MOD tmp two)      ;; 7: tmp = val % 2 (low bit)
                             '(ADD count tmp)    ;; 8: count += low bit
                             '(DIV val two)      ;; 9: val >>= 1
                             '(JMP 4)            ;; 10: goto LOOP
                             '(HALT))))          ;; 11
        (let ((results nil))
          (dolist (test-val '(0 1 2 3 7 8 15 16 255 256 1023))
            (let* ((m (funcall 'neovm--rma-make))
                   (_ (funcall 'neovm--rma-set-reg m 'val test-val))
                   (r (funcall 'neovm--rma-run m popcount-prog)))
              (push (list test-val
                          :popcount (funcall 'neovm--rma-get-reg (cdr r) 'count)
                          :logcount (logcount test-val)
                          :match (= (funcall 'neovm--rma-get-reg (cdr r) 'count)
                                    (logcount test-val)))
                    results)))
          (nreverse results)))
    {cleanup}))"#,
        defs = rm_defs(),
        cleanup = rm_cleanup()
    );
    assert_oracle_parity(&form);
}
