//! Oracle parity tests implementing a register-based virtual machine in Elisp:
//! register file operations, MOV/ADD/SUB/MUL/CMP instructions, conditional
//! jumps (JZ/JNZ/JGT/JLT), function call with register save/restore, stack
//! frame management, Fibonacci and factorial in register VM, instruction
//! encoding/decoding.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Register file: create, read, write, bulk operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regvm_register_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Register file: vector of N registers, all initialized to 0
  (fset 'neovm--regvm-make-regs
    (lambda (n)
      (make-vector n 0)))

  (fset 'neovm--regvm-get
    (lambda (regs idx)
      (aref regs idx)))

  (fset 'neovm--regvm-set
    (lambda (regs idx val)
      (aset regs idx val)
      regs))

  (fset 'neovm--regvm-dump
    (lambda (regs)
      "Return alist of (index . value) for non-zero registers."
      (let ((result nil) (i (1- (length regs))))
        (while (>= i 0)
          (unless (= (aref regs i) 0)
            (setq result (cons (cons i (aref regs i)) result)))
          (setq i (1- i)))
        result)))

  (fset 'neovm--regvm-copy
    (lambda (regs)
      (copy-sequence regs)))

  (unwind-protect
      (let ((regs (funcall 'neovm--regvm-make-regs 8)))
        ;; Initial state: all zeros
        (let ((dump0 (funcall 'neovm--regvm-dump regs)))
          ;; Set some registers
          (funcall 'neovm--regvm-set regs 0 42)
          (funcall 'neovm--regvm-set regs 3 -17)
          (funcall 'neovm--regvm-set regs 7 999)
          (let ((dump1 (funcall 'neovm--regvm-dump regs)))
            ;; Copy and modify independently
            (let ((copy (funcall 'neovm--regvm-copy regs)))
              (funcall 'neovm--regvm-set copy 0 0)
              (funcall 'neovm--regvm-set copy 5 100)
              (list
                dump0
                dump1
                (funcall 'neovm--regvm-dump copy)
                ;; Original unchanged after modifying copy
                (funcall 'neovm--regvm-dump regs)
                ;; Individual reads
                (funcall 'neovm--regvm-get regs 0)
                (funcall 'neovm--regvm-get regs 3)
                (funcall 'neovm--regvm-get regs 7)
                (funcall 'neovm--regvm-get regs 1))))))
    (fmakunbound 'neovm--regvm-make-regs)
    (fmakunbound 'neovm--regvm-get)
    (fmakunbound 'neovm--regvm-set)
    (fmakunbound 'neovm--regvm-dump)
    (fmakunbound 'neovm--regvm-copy)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil ((0 . 42) (3 . -17) (7 . 999)) ((3 . -17) (5 . 100) (7 . 999)) ((0 . 42) (3 . -17) (7 . 999)) 42 -17 999 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ALU instructions: MOV, ADD, SUB, MUL, CMP with flag register
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regvm_alu_instructions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; VM state: (vector-of-regs pc flag-register)
  ;; Registers: R0-R7 (indices 0-7)
  ;; Flag: -1 (less), 0 (equal), 1 (greater)

  (fset 'neovm--regvm-make-vm
    (lambda ()
      (list (make-vector 8 0) 0 0)))

  (fset 'neovm--regvm-exec-one
    (lambda (vm instr)
      "Execute one instruction on VM. Returns modified VM."
      (let ((regs (nth 0 vm))
            (pc (nth 1 vm))
            (flag (nth 2 vm))
            (op (car instr)))
        (cond
          ;; MOV Rd, imm
          ((eq op 'MOV-IMM)
           (aset regs (nth 1 instr) (nth 2 instr)))
          ;; MOV Rd, Rs
          ((eq op 'MOV)
           (aset regs (nth 1 instr) (aref regs (nth 2 instr))))
          ;; ADD Rd, Rs (Rd = Rd + Rs)
          ((eq op 'ADD)
           (aset regs (nth 1 instr)
                 (+ (aref regs (nth 1 instr)) (aref regs (nth 2 instr)))))
          ;; SUB Rd, Rs (Rd = Rd - Rs)
          ((eq op 'SUB)
           (aset regs (nth 1 instr)
                 (- (aref regs (nth 1 instr)) (aref regs (nth 2 instr)))))
          ;; MUL Rd, Rs (Rd = Rd * Rs)
          ((eq op 'MUL)
           (aset regs (nth 1 instr)
                 (* (aref regs (nth 1 instr)) (aref regs (nth 2 instr)))))
          ;; CMP Ra, Rb -> set flag
          ((eq op 'CMP)
           (let ((a (aref regs (nth 1 instr)))
                 (b (aref regs (nth 2 instr))))
             (setf (nth 2 vm)
                   (cond ((< a b) -1)
                         ((= a b) 0)
                         (t 1)))))
          ;; ADD-IMM Rd, imm (Rd = Rd + imm)
          ((eq op 'ADD-IMM)
           (aset regs (nth 1 instr)
                 (+ (aref regs (nth 1 instr)) (nth 2 instr)))))
        (setf (nth 1 vm) (1+ pc))
        vm)))

  (unwind-protect
      (let ((vm (funcall 'neovm--regvm-make-vm)))
        ;; MOV-IMM R0, 10
        (funcall 'neovm--regvm-exec-one vm '(MOV-IMM 0 10))
        ;; MOV-IMM R1, 20
        (funcall 'neovm--regvm-exec-one vm '(MOV-IMM 1 20))
        ;; ADD R0, R1 -> R0 = 30
        (funcall 'neovm--regvm-exec-one vm '(ADD 0 1))
        (let ((after-add (aref (nth 0 vm) 0)))
          ;; MOV R2, R0 -> R2 = 30
          (funcall 'neovm--regvm-exec-one vm '(MOV 2 0))
          ;; SUB R0, R1 -> R0 = 30-20 = 10
          (funcall 'neovm--regvm-exec-one vm '(SUB 0 1))
          (let ((after-sub (aref (nth 0 vm) 0)))
            ;; MUL R1, R2 -> R1 = 20*30 = 600
            (funcall 'neovm--regvm-exec-one vm '(MUL 1 2))
            (let ((after-mul (aref (nth 0 vm) 1)))
              ;; CMP R0, R1 -> R0=10 < R1=600 -> flag=-1
              (funcall 'neovm--regvm-exec-one vm '(CMP 0 1))
              (let ((flag1 (nth 2 vm)))
                ;; CMP R0, R0 -> flag=0
                (funcall 'neovm--regvm-exec-one vm '(CMP 0 0))
                (let ((flag2 (nth 2 vm)))
                  ;; CMP R1, R0 -> flag=1
                  (funcall 'neovm--regvm-exec-one vm '(CMP 1 0))
                  (let ((flag3 (nth 2 vm)))
                    ;; ADD-IMM R0, 5 -> R0 = 10+5 = 15
                    (funcall 'neovm--regvm-exec-one vm '(ADD-IMM 0 5))
                    (list after-add after-sub after-mul
                          flag1 flag2 flag3
                          (aref (nth 0 vm) 0)
                          (aref (nth 0 vm) 2)
                          (nth 1 vm)))))))))
    (fmakunbound 'neovm--regvm-make-vm)
    (fmakunbound 'neovm--regvm-exec-one)))"#;
    let expect = expect_test::expect![[r#""OK (30 10 600 -1 0 1 15 30 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Conditional jumps: JZ, JNZ, JGT, JLT with program execution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regvm_conditional_jumps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Full VM with program vector and execution loop
  (fset 'neovm--regvm2-run
    (lambda (program regs-init max-steps)
      "Run register VM program. regs-init is alist of (reg . val).
       Returns (final-regs steps-executed halted)."
      (let ((regs (make-vector 8 0))
            (pc 0)
            (flag 0)
            (steps 0)
            (halted nil))
        ;; Initialize registers
        (dolist (pair regs-init)
          (aset regs (car pair) (cdr pair)))
        (while (and (not halted) (< steps max-steps) (< pc (length program)))
          (let* ((instr (aref program pc))
                 (op (car instr)))
            (cond
              ((eq op 'MOV-IMM) (aset regs (nth 1 instr) (nth 2 instr)) (setq pc (1+ pc)))
              ((eq op 'MOV) (aset regs (nth 1 instr) (aref regs (nth 2 instr))) (setq pc (1+ pc)))
              ((eq op 'ADD) (aset regs (nth 1 instr) (+ (aref regs (nth 1 instr)) (aref regs (nth 2 instr)))) (setq pc (1+ pc)))
              ((eq op 'SUB) (aset regs (nth 1 instr) (- (aref regs (nth 1 instr)) (aref regs (nth 2 instr)))) (setq pc (1+ pc)))
              ((eq op 'MUL) (aset regs (nth 1 instr) (* (aref regs (nth 1 instr)) (aref regs (nth 2 instr)))) (setq pc (1+ pc)))
              ((eq op 'ADD-IMM) (aset regs (nth 1 instr) (+ (aref regs (nth 1 instr)) (nth 2 instr))) (setq pc (1+ pc)))
              ((eq op 'CMP)
               (let ((a (aref regs (nth 1 instr)))
                     (b (aref regs (nth 2 instr))))
                 (setq flag (cond ((< a b) -1) ((= a b) 0) (t 1))))
               (setq pc (1+ pc)))
              ((eq op 'CMP-IMM)
               (let ((a (aref regs (nth 1 instr)))
                     (b (nth 2 instr)))
                 (setq flag (cond ((< a b) -1) ((= a b) 0) (t 1))))
               (setq pc (1+ pc)))
              ((eq op 'JMP) (setq pc (nth 1 instr)))
              ((eq op 'JZ) (setq pc (if (= flag 0) (nth 1 instr) (1+ pc))))
              ((eq op 'JNZ) (setq pc (if (/= flag 0) (nth 1 instr) (1+ pc))))
              ((eq op 'JGT) (setq pc (if (= flag 1) (nth 1 instr) (1+ pc))))
              ((eq op 'JLT) (setq pc (if (= flag -1) (nth 1 instr) (1+ pc))))
              ((eq op 'HALT) (setq halted t))
              (t (setq halted t))))
          (setq steps (1+ steps)))
        (let ((result nil) (i 7))
          (while (>= i 0)
            (setq result (cons (aref regs i) result))
            (setq i (1- i)))
          (list result steps halted)))))

  (unwind-protect
      (list
        ;; Simple: load and halt
        (funcall 'neovm--regvm2-run
                 (vector '(MOV-IMM 0 42) '(HALT))
                 nil 100)
        ;; Conditional: if R0 > R1 then R2=1 else R2=0
        ;; R0=10, R1=5
        (funcall 'neovm--regvm2-run
                 (vector '(CMP 0 1)       ;; 0: compare R0, R1
                         '(JGT 4)         ;; 1: if R0>R1 goto 4
                         '(MOV-IMM 2 0)   ;; 2: R2=0 (false branch)
                         '(JMP 5)         ;; 3: skip true branch
                         '(MOV-IMM 2 1)   ;; 4: R2=1 (true branch)
                         '(HALT))         ;; 5: done
                 '((0 . 10) (1 . 5)) 100)
        ;; Same program but R0=3, R1=5 -> should take false branch
        (funcall 'neovm--regvm2-run
                 (vector '(CMP 0 1)
                         '(JGT 4)
                         '(MOV-IMM 2 0)
                         '(JMP 5)
                         '(MOV-IMM 2 1)
                         '(HALT))
                 '((0 . 3) (1 . 5)) 100)
        ;; Loop countdown: R0=5, decrement until 0, count iterations in R1
        (funcall 'neovm--regvm2-run
                 (vector '(CMP-IMM 0 0)   ;; 0: R0 == 0?
                         '(JZ 5)          ;; 1: if zero, done
                         '(ADD-IMM 0 -1)  ;; 2: R0--
                         '(ADD-IMM 1 1)   ;; 3: R1++ (counter)
                         '(JMP 0)         ;; 4: loop
                         '(HALT))         ;; 5: done
                 '((0 . 5)) 200))
    (fmakunbound 'neovm--regvm2-run)))"#;
    let expect = expect_test::expect![[
        r#""OK (((42 0 0 0 0 0 0 0) 2 t) ((10 5 1 0 0 0 0 0) 4 t) ((3 5 0 0 0 0 0 0) 5 t) ((0 5 0 0 0 0 0 0) 28 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Function call with register save/restore (call stack)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regvm_call_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; VM with call stack for function calls
  ;; CALL addr: push (pc+1, regs-snapshot) onto call stack, jump to addr
  ;; RET: pop call stack, restore pc (regs NOT restored, for return value)
  ;; SAVE-REG r: push register value onto data stack
  ;; LOAD-REG r: pop data stack into register

  (fset 'neovm--regvm3-run
    (lambda (program regs-init max-steps)
      (let ((regs (make-vector 8 0))
            (pc 0)
            (flag 0)
            (call-stack nil)
            (data-stack nil)
            (steps 0)
            (halted nil))
        (dolist (pair regs-init) (aset regs (car pair) (cdr pair)))
        (while (and (not halted) (< steps max-steps) (< pc (length program)))
          (let* ((instr (aref program pc))
                 (op (car instr)))
            (cond
              ((eq op 'MOV-IMM) (aset regs (nth 1 instr) (nth 2 instr)) (setq pc (1+ pc)))
              ((eq op 'MOV) (aset regs (nth 1 instr) (aref regs (nth 2 instr))) (setq pc (1+ pc)))
              ((eq op 'ADD) (aset regs (nth 1 instr) (+ (aref regs (nth 1 instr)) (aref regs (nth 2 instr)))) (setq pc (1+ pc)))
              ((eq op 'SUB) (aset regs (nth 1 instr) (- (aref regs (nth 1 instr)) (aref regs (nth 2 instr)))) (setq pc (1+ pc)))
              ((eq op 'MUL) (aset regs (nth 1 instr) (* (aref regs (nth 1 instr)) (aref regs (nth 2 instr)))) (setq pc (1+ pc)))
              ((eq op 'ADD-IMM) (aset regs (nth 1 instr) (+ (aref regs (nth 1 instr)) (nth 2 instr))) (setq pc (1+ pc)))
              ((eq op 'CMP)
               (let ((a (aref regs (nth 1 instr))) (b (aref regs (nth 2 instr))))
                 (setq flag (cond ((< a b) -1) ((= a b) 0) (t 1))))
               (setq pc (1+ pc)))
              ((eq op 'CMP-IMM)
               (let ((a (aref regs (nth 1 instr))) (b (nth 2 instr)))
                 (setq flag (cond ((< a b) -1) ((= a b) 0) (t 1))))
               (setq pc (1+ pc)))
              ((eq op 'JMP) (setq pc (nth 1 instr)))
              ((eq op 'JZ) (setq pc (if (= flag 0) (nth 1 instr) (1+ pc))))
              ((eq op 'JNZ) (setq pc (if (/= flag 0) (nth 1 instr) (1+ pc))))
              ((eq op 'JGT) (setq pc (if (= flag 1) (nth 1 instr) (1+ pc))))
              ((eq op 'JLT) (setq pc (if (= flag -1) (nth 1 instr) (1+ pc))))
              ((eq op 'CALL)
               (setq call-stack (cons (1+ pc) call-stack))
               (setq pc (nth 1 instr)))
              ((eq op 'RET)
               (if call-stack
                   (progn (setq pc (car call-stack))
                          (setq call-stack (cdr call-stack)))
                 (setq halted t)))
              ((eq op 'PUSH)
               (setq data-stack (cons (aref regs (nth 1 instr)) data-stack))
               (setq pc (1+ pc)))
              ((eq op 'POP)
               (aset regs (nth 1 instr) (car data-stack))
               (setq data-stack (cdr data-stack))
               (setq pc (1+ pc)))
              ((eq op 'HALT) (setq halted t))
              (t (setq halted t))))
          (setq steps (1+ steps)))
        (let ((r nil) (i 7))
          (while (>= i 0)
            (setq r (cons (aref regs i) r))
            (setq i (1- i)))
          (list r steps halted (length call-stack) (length data-stack))))))

  (unwind-protect
      (list
        ;; Call a "function" at addr 4 that doubles R0, return
        ;; Main: set R0=7, call double, halt
        ;; Double (at 4): ADD R0, R0; RET
        (funcall 'neovm--regvm3-run
                 (vector '(MOV-IMM 0 7)   ;; 0
                         '(CALL 4)         ;; 1
                         '(MOV 1 0)        ;; 2: save result to R1
                         '(HALT)           ;; 3
                         '(ADD 0 0)        ;; 4: double
                         '(RET))           ;; 5
                 nil 100)
        ;; Nested calls: main -> f -> g -> return
        ;; f: adds 10 to R0, calls g, returns
        ;; g: multiplies R0 by 2, returns
        (funcall 'neovm--regvm3-run
                 (vector '(MOV-IMM 0 3)   ;; 0: R0=3
                         '(CALL 5)         ;; 1: call f
                         '(MOV 2 0)        ;; 2: R2 = final result
                         '(HALT)           ;; 3
                         '(HALT)           ;; 4: unused
                         '(ADD-IMM 0 10)   ;; 5: f: R0+=10 -> 13
                         '(CALL 9)         ;; 6: call g
                         '(RET)            ;; 7: return to main (idx 2)
                         '(HALT)           ;; 8: unused
                         '(ADD 0 0)        ;; 9: g: R0*=2 -> 26
                         '(RET))           ;; 10: return to f (idx 7)
                 nil 100)
        ;; PUSH/POP to save/restore registers across call
        (funcall 'neovm--regvm3-run
                 (vector '(MOV-IMM 0 100)  ;; 0: R0=100
                         '(PUSH 0)         ;; 1: save R0
                         '(MOV-IMM 0 999)  ;; 2: clobber R0
                         '(MOV 1 0)        ;; 3: R1=999
                         '(POP 0)          ;; 4: restore R0=100
                         '(HALT))          ;; 5
                 nil 100))
    (fmakunbound 'neovm--regvm3-run)))"#;
    let expect = expect_test::expect![[
        r#""OK (((14 14 0 0 0 0 0 0) 6 t 0 0) ((26 0 26 0 0 0 0 0) 9 t 0 0) ((100 999 0 0 0 0 0 0) 6 t 0 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fibonacci in register VM
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regvm_fibonacci() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--regvm-fib-run
    (lambda (n)
      "Compute fib(n) using register VM loop.
       R0=n (input), R1=fib(n-1), R2=fib(n-2), R3=temp, R4=counter."
      (let ((regs (make-vector 8 0))
            (pc 0) (flag 0) (steps 0) (halted nil))
        (aset regs 0 n)
        (let ((program
               (vector
                 '(CMP-IMM 0 0)    ;; 0: n==0?
                 '(JZ 12)          ;; 1: if n==0, result is 0 (R1=0)
                 '(CMP-IMM 0 1)    ;; 2: n==1?
                 '(JZ 11)          ;; 3: if n==1, goto set R1=1
                 '(MOV-IMM 1 1)    ;; 4: fib_prev = 1
                 '(MOV-IMM 2 0)    ;; 5: fib_prev2 = 0
                 '(MOV-IMM 4 2)    ;; 6: counter = 2
                 ;; Loop:
                 '(MOV 3 1)        ;; 7: temp = fib_prev
                 '(ADD 1 2)        ;; 8: fib_prev += fib_prev2
                 '(MOV 2 3)        ;; 9: fib_prev2 = temp
                 '(ADD-IMM 4 1)    ;; 10: counter++
                 '(CMP 4 0)        ;; 11: counter vs n
                 '(JGT 14)         ;; 12: if counter>n, done
                 '(JMP 7)          ;; 13: loop
                 '(HALT))))        ;; 14: R1 = result
          (while (and (not halted) (< steps 500) (< pc (length program)))
            (let* ((instr (aref program pc))
                   (op (car instr)))
              (cond
                ((eq op 'MOV-IMM) (aset regs (nth 1 instr) (nth 2 instr)) (setq pc (1+ pc)))
                ((eq op 'MOV) (aset regs (nth 1 instr) (aref regs (nth 2 instr))) (setq pc (1+ pc)))
                ((eq op 'ADD) (aset regs (nth 1 instr) (+ (aref regs (nth 1 instr)) (aref regs (nth 2 instr)))) (setq pc (1+ pc)))
                ((eq op 'ADD-IMM) (aset regs (nth 1 instr) (+ (aref regs (nth 1 instr)) (nth 2 instr))) (setq pc (1+ pc)))
                ((eq op 'CMP)
                 (let ((a (aref regs (nth 1 instr))) (b (aref regs (nth 2 instr))))
                   (setq flag (cond ((< a b) -1) ((= a b) 0) (t 1))))
                 (setq pc (1+ pc)))
                ((eq op 'CMP-IMM)
                 (let ((a (aref regs (nth 1 instr))) (b (nth 2 instr)))
                   (setq flag (cond ((< a b) -1) ((= a b) 0) (t 1))))
                 (setq pc (1+ pc)))
                ((eq op 'JMP) (setq pc (nth 1 instr)))
                ((eq op 'JZ) (setq pc (if (= flag 0) (nth 1 instr) (1+ pc))))
                ((eq op 'JGT) (setq pc (if (= flag 1) (nth 1 instr) (1+ pc))))
                ((eq op 'HALT) (setq halted t))
                (t (setq halted t))))
            (setq steps (1+ steps)))
          (list (aref regs 1) steps)))))

  (unwind-protect
      (list
        (funcall 'neovm--regvm-fib-run 0)
        (funcall 'neovm--regvm-fib-run 1)
        (funcall 'neovm--regvm-fib-run 2)
        (funcall 'neovm--regvm-fib-run 3)
        (funcall 'neovm--regvm-fib-run 5)
        (funcall 'neovm--regvm-fib-run 8)
        (funcall 'neovm--regvm-fib-run 10)
        (funcall 'neovm--regvm-fib-run 15)
        ;; Verify against direct computation
        (let ((results nil))
          (dolist (n '(0 1 2 3 4 5 6 7 8 9 10))
            (setq results (cons (car (funcall 'neovm--regvm-fib-run n)) results)))
          (nreverse results)))
    (fmakunbound 'neovm--regvm-fib-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 11) (0 21) (1 14) (2 21) (5 35) (21 56) (55 70) (610 105) (0 0 1 2 3 5 8 13 21 34 55))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Factorial in register VM
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regvm_factorial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--regvm-fact-run
    (lambda (n)
      "Compute n! using register VM.
       R0=n (counter), R1=accumulator (result)."
      (let ((regs (make-vector 8 0))
            (pc 0) (flag 0) (steps 0) (halted nil))
        (aset regs 0 n)
        (let ((program
               (vector
                 '(MOV-IMM 1 1)    ;; 0: acc = 1
                 '(CMP-IMM 0 0)    ;; 1: n == 0?
                 '(JZ 7)           ;; 2: if n==0, done
                 '(MUL 1 0)        ;; 3: acc *= n
                 '(ADD-IMM 0 -1)   ;; 4: n--
                 '(CMP-IMM 0 0)    ;; 5: n == 0?
                 '(JNZ 3)          ;; 6: if n!=0, loop
                 '(HALT))))        ;; 7: done, result in R1
          (while (and (not halted) (< steps 500) (< pc (length program)))
            (let* ((instr (aref program pc))
                   (op (car instr)))
              (cond
                ((eq op 'MOV-IMM) (aset regs (nth 1 instr) (nth 2 instr)) (setq pc (1+ pc)))
                ((eq op 'MUL) (aset regs (nth 1 instr) (* (aref regs (nth 1 instr)) (aref regs (nth 2 instr)))) (setq pc (1+ pc)))
                ((eq op 'ADD-IMM) (aset regs (nth 1 instr) (+ (aref regs (nth 1 instr)) (nth 2 instr))) (setq pc (1+ pc)))
                ((eq op 'CMP-IMM)
                 (let ((a (aref regs (nth 1 instr))) (b (nth 2 instr)))
                   (setq flag (cond ((< a b) -1) ((= a b) 0) (t 1))))
                 (setq pc (1+ pc)))
                ((eq op 'JZ) (setq pc (if (= flag 0) (nth 1 instr) (1+ pc))))
                ((eq op 'JNZ) (setq pc (if (/= flag 0) (nth 1 instr) (1+ pc))))
                ((eq op 'HALT) (setq halted t))
                (t (setq halted t))))
            (setq steps (1+ steps)))
          (list (aref regs 1) steps)))))

  (unwind-protect
      (list
        (funcall 'neovm--regvm-fact-run 0)
        (funcall 'neovm--regvm-fact-run 1)
        (funcall 'neovm--regvm-fact-run 2)
        (funcall 'neovm--regvm-fact-run 3)
        (funcall 'neovm--regvm-fact-run 5)
        (funcall 'neovm--regvm-fact-run 7)
        (funcall 'neovm--regvm-fact-run 10)
        ;; Verify: n! values
        (mapcar (lambda (n) (car (funcall 'neovm--regvm-fact-run n)))
                '(0 1 2 3 4 5 6 7 8 9 10)))
    (fmakunbound 'neovm--regvm-fact-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 4) (1 8) (2 12) (6 16) (120 24) (5040 32) (3628800 44) (1 1 2 6 24 120 720 5040 40320 362880 3628800))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Instruction encoding/decoding: pack instructions into integers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regvm_instruction_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Instruction format (32-bit integer):
  ;; Bits 0-7:   opcode (0-255)
  ;; Bits 8-11:  Rd (destination register, 0-15)
  ;; Bits 12-15: Rs (source register, 0-15)
  ;; Bits 16-31: immediate value (signed 16-bit)

  (defvar neovm--regvm-opcodes
    '((NOP . 0) (MOV-IMM . 1) (MOV . 2) (ADD . 3)
      (SUB . 4) (MUL . 5) (CMP . 6) (CMP-IMM . 7)
      (JMP . 8) (JZ . 9) (JNZ . 10) (JGT . 11)
      (JLT . 12) (HALT . 13) (ADD-IMM . 14)
      (PUSH . 15) (POP . 16) (CALL . 17) (RET . 18)))

  (fset 'neovm--regvm-encode
    (lambda (op rd rs imm)
      "Encode instruction to integer."
      (let ((opcode (cdr (assq op neovm--regvm-opcodes))))
        (logior opcode
                (ash (logand rd 15) 8)
                (ash (logand rs 15) 12)
                (ash (logand imm 65535) 16)))))

  (fset 'neovm--regvm-decode
    (lambda (word)
      "Decode integer to instruction fields."
      (let* ((opcode (logand word 255))
             (rd (logand (ash word -8) 15))
             (rs (logand (ash word -12) 15))
             (imm (logand (ash word -16) 65535))
             (op-name (car (rassq opcode neovm--regvm-opcodes))))
        ;; Sign-extend immediate if bit 15 is set
        (when (>= imm 32768)
          (setq imm (- imm 65536)))
        (list op-name rd rs imm))))

  (unwind-protect
      (list
        ;; Encode and decode round-trip
        (funcall 'neovm--regvm-decode
                 (funcall 'neovm--regvm-encode 'MOV-IMM 0 0 42))
        (funcall 'neovm--regvm-decode
                 (funcall 'neovm--regvm-encode 'ADD 3 5 0))
        (funcall 'neovm--regvm-decode
                 (funcall 'neovm--regvm-encode 'CMP 2 7 0))
        (funcall 'neovm--regvm-decode
                 (funcall 'neovm--regvm-encode 'JMP 0 0 100))
        (funcall 'neovm--regvm-decode
                 (funcall 'neovm--regvm-encode 'HALT 0 0 0))
        ;; Negative immediate (sign extension)
        (funcall 'neovm--regvm-decode
                 (funcall 'neovm--regvm-encode 'ADD-IMM 1 0 -1))
        (funcall 'neovm--regvm-decode
                 (funcall 'neovm--regvm-encode 'ADD-IMM 2 0 -100))
        ;; Encode a full program as vector of integers
        (let ((prog (vector
                      (funcall 'neovm--regvm-encode 'MOV-IMM 0 0 10)
                      (funcall 'neovm--regvm-encode 'MOV-IMM 1 0 20)
                      (funcall 'neovm--regvm-encode 'ADD 0 1 0)
                      (funcall 'neovm--regvm-encode 'HALT 0 0 0))))
          (list (length prog)
                (mapcar (lambda (w) (car (funcall 'neovm--regvm-decode w)))
                        (append prog nil)))))
    (fmakunbound 'neovm--regvm-encode)
    (fmakunbound 'neovm--regvm-decode)
    (makunbound 'neovm--regvm-opcodes)))"#;
    let expect = expect_test::expect![[
        r#""OK ((MOV-IMM 0 0 42) (ADD 3 5 0) (CMP 2 7 0) (JMP 0 0 100) (HALT 0 0 0) (ADD-IMM 1 0 -1) (ADD-IMM 2 0 -100) (4 (MOV-IMM MOV-IMM ADD HALT)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
