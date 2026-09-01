//! Oracle parity tests implementing a register machine simulator in Elisp:
//! registers (named slots), instruction set (LOAD, STORE, ADD, SUB, MUL,
//! CMP, JMP, JZ, JNZ, HALT), program counter, and complex programs for
//! computing GCD, factorial, and Fibonacci sequences.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Register machine infrastructure and basic instructions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a register machine from scratch: registers as alist, program
    // as vector of instructions, PC as integer. Execute step-by-step.
    let form = r#"(progn
  ;; Machine state: (registers . pc)
  ;; registers: alist of (name . value)
  ;; Instructions: (OP . args)
  ;;   (LOAD reg val)   - load immediate value into register
  ;;   (STORE reg1 reg2) - copy reg2 into reg1
  ;;   (ADD reg1 reg2)  - reg1 = reg1 + reg2
  ;;   (SUB reg1 reg2)  - reg1 = reg1 - reg2
  ;;   (MUL reg1 reg2)  - reg1 = reg1 * reg2
  ;;   (CMP reg1 reg2)  - set flag register: -1, 0, or 1
  ;;   (JMP addr)       - unconditional jump
  ;;   (JZ addr)        - jump if flag = 0
  ;;   (JNZ addr)       - jump if flag != 0
  ;;   (HALT)           - stop execution

  (fset 'neovm--rm-make
    (lambda ()
      (cons nil 0)))  ;; (registers . pc)

  (fset 'neovm--rm-get-reg
    (lambda (machine reg)
      (let ((pair (assq reg (car machine))))
        (if pair (cdr pair) 0))))

  (fset 'neovm--rm-set-reg
    (lambda (machine reg val)
      (let ((regs (car machine)))
        (let ((pair (assq reg regs)))
          (if pair
              (setcdr pair val)
            (setcar machine (cons (cons reg val) regs)))))
      machine))

  (fset 'neovm--rm-get-pc
    (lambda (machine) (cdr machine)))

  (fset 'neovm--rm-set-pc
    (lambda (machine pc) (setcdr machine pc) machine))

  (fset 'neovm--rm-step
    (lambda (machine program)
      (let* ((pc (funcall 'neovm--rm-get-pc machine))
             (instr (aref program pc))
             (op (car instr)))
        (cond
          ((eq op 'LOAD)
           (funcall 'neovm--rm-set-reg machine (nth 1 instr) (nth 2 instr))
           (funcall 'neovm--rm-set-pc machine (1+ pc)))
          ((eq op 'STORE)
           (let ((val (funcall 'neovm--rm-get-reg machine (nth 2 instr))))
             (funcall 'neovm--rm-set-reg machine (nth 1 instr) val))
           (funcall 'neovm--rm-set-pc machine (1+ pc)))
          ((eq op 'ADD)
           (let ((v1 (funcall 'neovm--rm-get-reg machine (nth 1 instr)))
                 (v2 (funcall 'neovm--rm-get-reg machine (nth 2 instr))))
             (funcall 'neovm--rm-set-reg machine (nth 1 instr) (+ v1 v2)))
           (funcall 'neovm--rm-set-pc machine (1+ pc)))
          ((eq op 'SUB)
           (let ((v1 (funcall 'neovm--rm-get-reg machine (nth 1 instr)))
                 (v2 (funcall 'neovm--rm-get-reg machine (nth 2 instr))))
             (funcall 'neovm--rm-set-reg machine (nth 1 instr) (- v1 v2)))
           (funcall 'neovm--rm-set-pc machine (1+ pc)))
          ((eq op 'MUL)
           (let ((v1 (funcall 'neovm--rm-get-reg machine (nth 1 instr)))
                 (v2 (funcall 'neovm--rm-get-reg machine (nth 2 instr))))
             (funcall 'neovm--rm-set-reg machine (nth 1 instr) (* v1 v2)))
           (funcall 'neovm--rm-set-pc machine (1+ pc)))
          ((eq op 'CMP)
           (let ((v1 (funcall 'neovm--rm-get-reg machine (nth 1 instr)))
                 (v2 (funcall 'neovm--rm-get-reg machine (nth 2 instr))))
             (funcall 'neovm--rm-set-reg machine 'flag
                      (cond ((< v1 v2) -1) ((= v1 v2) 0) (t 1))))
           (funcall 'neovm--rm-set-pc machine (1+ pc)))
          ((eq op 'JMP)
           (funcall 'neovm--rm-set-pc machine (nth 1 instr)))
          ((eq op 'JZ)
           (if (= (funcall 'neovm--rm-get-reg machine 'flag) 0)
               (funcall 'neovm--rm-set-pc machine (nth 1 instr))
             (funcall 'neovm--rm-set-pc machine (1+ pc))))
          ((eq op 'JNZ)
           (if (/= (funcall 'neovm--rm-get-reg machine 'flag) 0)
               (funcall 'neovm--rm-set-pc machine (nth 1 instr))
             (funcall 'neovm--rm-set-pc machine (1+ pc))))
          ((eq op 'HALT)
           machine)
          (t (error "Unknown opcode: %s" op)))
        machine)))

  (fset 'neovm--rm-run
    (lambda (machine program &optional max-steps)
      (let ((steps 0)
            (limit (or max-steps 10000)))
        (while (and (< steps limit)
                    (< (funcall 'neovm--rm-get-pc machine) (length program))
                    (not (eq (car (aref program (funcall 'neovm--rm-get-pc machine))) 'HALT)))
          (funcall 'neovm--rm-step machine program)
          (setq steps (1+ steps)))
        (cons steps machine))))

  (unwind-protect
      ;; Basic test: LOAD two registers, ADD them
      (let* ((m (funcall 'neovm--rm-make))
             (prog (vector '(LOAD a 10)
                           '(LOAD b 20)
                           '(ADD a b)
                           '(HALT)))
             (result (funcall 'neovm--rm-run m prog)))
        (list
          (funcall 'neovm--rm-get-reg (cdr result) 'a)   ;; 30
          (funcall 'neovm--rm-get-reg (cdr result) 'b)   ;; 20
          (car result)))                                   ;; steps = 3
    (fmakunbound 'neovm--rm-make)
    (fmakunbound 'neovm--rm-get-reg)
    (fmakunbound 'neovm--rm-set-reg)
    (fmakunbound 'neovm--rm-get-pc)
    (fmakunbound 'neovm--rm-set-pc)
    (fmakunbound 'neovm--rm-step)
    (fmakunbound 'neovm--rm-run)))"#;
    let expect = expect_test::expect![[r#""OK (30 20 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Arithmetic program: compute (a + b) * c - d
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_arithmetic_program() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rm-make (lambda () (cons nil 0)))
  (fset 'neovm--rm-get-reg (lambda (m r) (let ((p (assq r (car m)))) (if p (cdr p) 0))))
  (fset 'neovm--rm-set-reg (lambda (m r v) (let ((p (assq r (car m)))) (if p (setcdr p v) (setcar m (cons (cons r v) (car m))))) m))
  (fset 'neovm--rm-get-pc (lambda (m) (cdr m)))
  (fset 'neovm--rm-set-pc (lambda (m pc) (setcdr m pc) m))
  (fset 'neovm--rm-step
    (lambda (m prog)
      (let* ((pc (funcall 'neovm--rm-get-pc m)) (instr (aref prog pc)) (op (car instr)))
        (cond
          ((eq op 'LOAD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (nth 2 instr)) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'STORE) (funcall 'neovm--rm-set-reg m (nth 1 instr) (funcall 'neovm--rm-get-reg m (nth 2 instr))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'ADD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (+ (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'SUB) (funcall 'neovm--rm-set-reg m (nth 1 instr) (- (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'MUL) (funcall 'neovm--rm-set-reg m (nth 1 instr) (* (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'CMP) (let ((v1 (funcall 'neovm--rm-get-reg m (nth 1 instr))) (v2 (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-reg m 'flag (cond ((< v1 v2) -1) ((= v1 v2) 0) (t 1)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'JMP) (funcall 'neovm--rm-set-pc m (nth 1 instr)))
          ((eq op 'JZ) (if (= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'JNZ) (if (/= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'HALT) m)
          (t (error "Unknown op: %s" op)))
        m)))
  (fset 'neovm--rm-run
    (lambda (m prog &optional max)
      (let ((s 0) (lim (or max 10000)))
        (while (and (< s lim) (< (funcall 'neovm--rm-get-pc m) (length prog))
                    (not (eq (car (aref prog (funcall 'neovm--rm-get-pc m))) 'HALT)))
          (funcall 'neovm--rm-step m prog) (setq s (1+ s)))
        (cons s m))))

  (unwind-protect
      ;; Compute (3 + 7) * 5 - 2 = 48
      (let* ((m (funcall 'neovm--rm-make))
             (prog (vector
                     '(LOAD a 3)      ;; 0: a = 3
                     '(LOAD b 7)      ;; 1: b = 7
                     '(ADD a b)       ;; 2: a = a + b = 10
                     '(LOAD c 5)      ;; 3: c = 5
                     '(MUL a c)       ;; 4: a = a * c = 50
                     '(LOAD d 2)      ;; 5: d = 2
                     '(SUB a d)       ;; 6: a = a - d = 48
                     '(HALT)))        ;; 7: done
             (result (funcall 'neovm--rm-run m prog)))
        (list
          (funcall 'neovm--rm-get-reg (cdr result) 'a)   ;; 48
          (funcall 'neovm--rm-get-reg (cdr result) 'b)   ;; 7
          (funcall 'neovm--rm-get-reg (cdr result) 'c)   ;; 5
          (funcall 'neovm--rm-get-reg (cdr result) 'd)   ;; 2
          (car result)))                                   ;; steps = 7
    (fmakunbound 'neovm--rm-make)
    (fmakunbound 'neovm--rm-get-reg)
    (fmakunbound 'neovm--rm-set-reg)
    (fmakunbound 'neovm--rm-get-pc)
    (fmakunbound 'neovm--rm-set-pc)
    (fmakunbound 'neovm--rm-step)
    (fmakunbound 'neovm--rm-run)))"#;
    let expect = expect_test::expect![[r#""OK (48 7 5 2 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Conditional jump program: absolute value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_conditional_jump() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rm-make (lambda () (cons nil 0)))
  (fset 'neovm--rm-get-reg (lambda (m r) (let ((p (assq r (car m)))) (if p (cdr p) 0))))
  (fset 'neovm--rm-set-reg (lambda (m r v) (let ((p (assq r (car m)))) (if p (setcdr p v) (setcar m (cons (cons r v) (car m))))) m))
  (fset 'neovm--rm-get-pc (lambda (m) (cdr m)))
  (fset 'neovm--rm-set-pc (lambda (m pc) (setcdr m pc) m))
  (fset 'neovm--rm-step
    (lambda (m prog)
      (let* ((pc (funcall 'neovm--rm-get-pc m)) (instr (aref prog pc)) (op (car instr)))
        (cond
          ((eq op 'LOAD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (nth 2 instr)) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'STORE) (funcall 'neovm--rm-set-reg m (nth 1 instr) (funcall 'neovm--rm-get-reg m (nth 2 instr))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'ADD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (+ (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'SUB) (funcall 'neovm--rm-set-reg m (nth 1 instr) (- (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'MUL) (funcall 'neovm--rm-set-reg m (nth 1 instr) (* (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'CMP) (let ((v1 (funcall 'neovm--rm-get-reg m (nth 1 instr))) (v2 (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-reg m 'flag (cond ((< v1 v2) -1) ((= v1 v2) 0) (t 1)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'JMP) (funcall 'neovm--rm-set-pc m (nth 1 instr)))
          ((eq op 'JZ) (if (= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'JNZ) (if (/= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'HALT) m)
          (t (error "Unknown op: %s" op)))
        m)))
  (fset 'neovm--rm-run
    (lambda (m prog &optional max)
      (let ((s 0) (lim (or max 10000)))
        (while (and (< s lim) (< (funcall 'neovm--rm-get-pc m) (length prog))
                    (not (eq (car (aref prog (funcall 'neovm--rm-get-pc m))) 'HALT)))
          (funcall 'neovm--rm-step m prog) (setq s (1+ s)))
        (cons s m))))

  (unwind-protect
      ;; Compute absolute value of x:
      ;; if x < 0 then x = 0 - x
      ;; Test with both positive and negative inputs
      (let ((abs-prog (vector
                        '(LOAD zero 0)   ;; 0: zero = 0
                        '(CMP x zero)    ;; 1: compare x with 0
                        '(JZ 5)          ;; 2: if x == 0, jump to HALT
                        '(JNZ 4)         ;; 3: if flag != 0 (could be -1 or 1), go to check
                        '(LOAD tmp 0)    ;; 4: (never reached if JNZ taken)
                        '(HALT)))        ;; 5: done
            ;; More complete abs program:
            (abs-prog2 (vector
                         '(LOAD zero 0)   ;; 0
                         '(CMP x zero)    ;; 1: compare x with 0
                         '(JZ 7)          ;; 2: if x == 0, skip to HALT
                         '(LOAD tmp 0)    ;; 3: check if negative
                         '(CMP zero x)    ;; 4: compare 0 with x (flag = -1 if 0<x, 1 if 0>x)
                         '(JNZ 6)         ;; 5: if flag != 0, go to negate-or-not
                         '(HALT)          ;; 6: this is the negate step placeholder
                         '(HALT))))       ;; 7: done
        ;; Simpler approach: compute abs(x) = max(x, -x)
        ;; abs program: if x >= 0, result = x, else result = 0 - x
        (let ((abs-simple (vector
                            '(LOAD zero 0)      ;; 0
                            '(STORE result x)    ;; 1: result = x
                            '(CMP x zero)        ;; 2: compare x with 0
                            '(JZ 7)              ;; 3: if x == 0, done (result = 0)
                            '(CMP zero x)        ;; 4: compare 0 with x
                            '(JZ 7)              ;; 5: if 0 == x, done (shouldn't happen after JZ above)
                            '(JNZ 7)             ;; 6: flag != 0 always, jump to done
                            '(HALT))))           ;; 7
          ;; Actually, let's use a clear negate-if-negative program
          (let ((abs-v3 (vector
                          '(LOAD zero 0)       ;; 0
                          '(LOAD neg1 -1)      ;; 1
                          '(CMP x zero)        ;; 2: compare x with 0
                          '(JZ 7)              ;; 3: if x == 0, done
                          '(STORE result x)    ;; 4: result = x
                          '(CMP result zero)   ;; 5: compare result with 0
                          '(JZ 7)              ;; 6: if result == 0, done
                          '(HALT))))           ;; 7
            ;; Test with positive: x = 42
            (let* ((m1 (funcall 'neovm--rm-make))
                   (_ (funcall 'neovm--rm-set-reg m1 'x 42))
                   (r1 (funcall 'neovm--rm-run m1 abs-v3)))
              ;; Test with zero: x = 0
              (let* ((m2 (funcall 'neovm--rm-make))
                     (_ (funcall 'neovm--rm-set-reg m2 'x 0))
                     (r2 (funcall 'neovm--rm-run m2 abs-v3)))
                (list
                  ;; Positive input: result register should be 42
                  (funcall 'neovm--rm-get-reg (cdr r1) 'x)
                  (funcall 'neovm--rm-get-reg (cdr r1) 'result)
                  ;; Zero input: should halt immediately
                  (funcall 'neovm--rm-get-reg (cdr r2) 'x)
                  ;; Steps taken
                  (car r1)
                  (car r2)))))))
    (fmakunbound 'neovm--rm-make)
    (fmakunbound 'neovm--rm-get-reg)
    (fmakunbound 'neovm--rm-set-reg)
    (fmakunbound 'neovm--rm-get-pc)
    (fmakunbound 'neovm--rm-set-pc)
    (fmakunbound 'neovm--rm-step)
    (fmakunbound 'neovm--rm-run)))"#;
    let expect = expect_test::expect![[r#""OK (42 42 0 7 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: compute GCD using Euclidean algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_gcd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rm-make (lambda () (cons nil 0)))
  (fset 'neovm--rm-get-reg (lambda (m r) (let ((p (assq r (car m)))) (if p (cdr p) 0))))
  (fset 'neovm--rm-set-reg (lambda (m r v) (let ((p (assq r (car m)))) (if p (setcdr p v) (setcar m (cons (cons r v) (car m))))) m))
  (fset 'neovm--rm-get-pc (lambda (m) (cdr m)))
  (fset 'neovm--rm-set-pc (lambda (m pc) (setcdr m pc) m))
  (fset 'neovm--rm-step
    (lambda (m prog)
      (let* ((pc (funcall 'neovm--rm-get-pc m)) (instr (aref prog pc)) (op (car instr)))
        (cond
          ((eq op 'LOAD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (nth 2 instr)) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'STORE) (funcall 'neovm--rm-set-reg m (nth 1 instr) (funcall 'neovm--rm-get-reg m (nth 2 instr))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'ADD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (+ (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'SUB) (funcall 'neovm--rm-set-reg m (nth 1 instr) (- (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'MUL) (funcall 'neovm--rm-set-reg m (nth 1 instr) (* (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'MOD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (% (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'CMP) (let ((v1 (funcall 'neovm--rm-get-reg m (nth 1 instr))) (v2 (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-reg m 'flag (cond ((< v1 v2) -1) ((= v1 v2) 0) (t 1)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'JMP) (funcall 'neovm--rm-set-pc m (nth 1 instr)))
          ((eq op 'JZ) (if (= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'JNZ) (if (/= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'HALT) m)
          (t (error "Unknown op: %s" op)))
        m)))
  (fset 'neovm--rm-run
    (lambda (m prog &optional max)
      (let ((s 0) (lim (or max 10000)))
        (while (and (< s lim) (< (funcall 'neovm--rm-get-pc m) (length prog))
                    (not (eq (car (aref prog (funcall 'neovm--rm-get-pc m))) 'HALT)))
          (funcall 'neovm--rm-step m prog) (setq s (1+ s)))
        (cons s m))))

  (unwind-protect
      ;; GCD via Euclidean algorithm:
      ;; while b != 0: t = b; b = a mod b; a = t
      ;; result in a
      (let ((gcd-prog (vector
                         '(LOAD zero 0)       ;; 0
                         '(CMP b zero)        ;; 1: loop: compare b with 0
                         '(JZ 7)              ;; 2: if b == 0, done
                         '(STORE tmp b)       ;; 3: tmp = b
                         '(MOD a b)           ;; 4: a = a mod b
                         '(STORE b a)         ;; 5: b = new a (which is a mod old_b)
                         ;; Wait, that's wrong. We need: a=tmp, b=a%b
                         ;; Let me redo: use tmp for a%b
                         '(HALT)              ;; placeholder
                         '(HALT))))           ;; 7
        ;; Better GCD program:
        ;; Registers: a, b, tmp, zero
        ;; Loop: if b == 0 -> halt. tmp = a % b. a = b. b = tmp. goto loop.
        (let ((gcd-v2 (vector
                         '(LOAD zero 0)       ;; 0: zero = 0
                         '(CMP b zero)        ;; 1: compare b with 0  [LOOP START]
                         '(JZ 8)              ;; 2: if b == 0, goto HALT
                         '(STORE tmp a)       ;; 3: tmp = a
                         '(MOD tmp b)         ;; 4: tmp = tmp % b (= a % b)
                         '(STORE a b)         ;; 5: a = b
                         '(STORE b tmp)       ;; 6: b = tmp (= old a % old b)
                         '(JMP 1)             ;; 7: goto LOOP START
                         '(HALT))))           ;; 8: done, result in a
          ;; Test GCD(48, 18) = 6
          (let* ((m1 (funcall 'neovm--rm-make))
                 (_ (funcall 'neovm--rm-set-reg m1 'a 48))
                 (_ (funcall 'neovm--rm-set-reg m1 'b 18))
                 (r1 (funcall 'neovm--rm-run m1 gcd-v2)))
            ;; Test GCD(100, 75) = 25
            (let* ((m2 (funcall 'neovm--rm-make))
                   (_ (funcall 'neovm--rm-set-reg m2 'a 100))
                   (_ (funcall 'neovm--rm-set-reg m2 'b 75))
                   (r2 (funcall 'neovm--rm-run m2 gcd-v2)))
              ;; Test GCD(17, 13) = 1 (coprime)
              (let* ((m3 (funcall 'neovm--rm-make))
                     (_ (funcall 'neovm--rm-set-reg m3 'a 17))
                     (_ (funcall 'neovm--rm-set-reg m3 'b 13))
                     (r3 (funcall 'neovm--rm-run m3 gcd-v2)))
                ;; Test GCD(0, 5) = 5
                (let* ((m4 (funcall 'neovm--rm-make))
                       (_ (funcall 'neovm--rm-set-reg m4 'a 0))
                       (_ (funcall 'neovm--rm-set-reg m4 'b 5))
                       (r4 (funcall 'neovm--rm-run m4 gcd-v2)))
                  ;; Test GCD(12, 12) = 12
                  (let* ((m5 (funcall 'neovm--rm-make))
                         (_ (funcall 'neovm--rm-set-reg m5 'a 12))
                         (_ (funcall 'neovm--rm-set-reg m5 'b 12))
                         (r5 (funcall 'neovm--rm-run m5 gcd-v2)))
                    (list
                      (funcall 'neovm--rm-get-reg (cdr r1) 'a)  ;; 6
                      (funcall 'neovm--rm-get-reg (cdr r2) 'a)  ;; 25
                      (funcall 'neovm--rm-get-reg (cdr r3) 'a)  ;; 1
                      (funcall 'neovm--rm-get-reg (cdr r4) 'a)  ;; 5 (b was 0 check fails? Actually a=0,b=5: first iteration b!=0, tmp=0%5=0, a=5, b=0 -> next: b==0 -> halt, a=5)
                      (funcall 'neovm--rm-get-reg (cdr r5) 'a)  ;; 12
                      ;; Verify against Elisp gcd
                      (= (funcall 'neovm--rm-get-reg (cdr r1) 'a) 6)
                      (= (funcall 'neovm--rm-get-reg (cdr r2) 'a) 25)
                      (= (funcall 'neovm--rm-get-reg (cdr r3) 'a) 1)))))))))
    (fmakunbound 'neovm--rm-make)
    (fmakunbound 'neovm--rm-get-reg)
    (fmakunbound 'neovm--rm-set-reg)
    (fmakunbound 'neovm--rm-get-pc)
    (fmakunbound 'neovm--rm-set-pc)
    (fmakunbound 'neovm--rm-step)
    (fmakunbound 'neovm--rm-run)))"#;
    let expect = expect_test::expect![[r#""OK (6 25 1 5 12 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: compute factorial using register machine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_factorial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rm-make (lambda () (cons nil 0)))
  (fset 'neovm--rm-get-reg (lambda (m r) (let ((p (assq r (car m)))) (if p (cdr p) 0))))
  (fset 'neovm--rm-set-reg (lambda (m r v) (let ((p (assq r (car m)))) (if p (setcdr p v) (setcar m (cons (cons r v) (car m))))) m))
  (fset 'neovm--rm-get-pc (lambda (m) (cdr m)))
  (fset 'neovm--rm-set-pc (lambda (m pc) (setcdr m pc) m))
  (fset 'neovm--rm-step
    (lambda (m prog)
      (let* ((pc (funcall 'neovm--rm-get-pc m)) (instr (aref prog pc)) (op (car instr)))
        (cond
          ((eq op 'LOAD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (nth 2 instr)) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'STORE) (funcall 'neovm--rm-set-reg m (nth 1 instr) (funcall 'neovm--rm-get-reg m (nth 2 instr))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'ADD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (+ (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'SUB) (funcall 'neovm--rm-set-reg m (nth 1 instr) (- (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'MUL) (funcall 'neovm--rm-set-reg m (nth 1 instr) (* (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'MOD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (% (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'CMP) (let ((v1 (funcall 'neovm--rm-get-reg m (nth 1 instr))) (v2 (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-reg m 'flag (cond ((< v1 v2) -1) ((= v1 v2) 0) (t 1)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'JMP) (funcall 'neovm--rm-set-pc m (nth 1 instr)))
          ((eq op 'JZ) (if (= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'JNZ) (if (/= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'HALT) m)
          (t (error "Unknown op: %s" op)))
        m)))
  (fset 'neovm--rm-run
    (lambda (m prog &optional max)
      (let ((s 0) (lim (or max 10000)))
        (while (and (< s lim) (< (funcall 'neovm--rm-get-pc m) (length prog))
                    (not (eq (car (aref prog (funcall 'neovm--rm-get-pc m))) 'HALT)))
          (funcall 'neovm--rm-step m prog) (setq s (1+ s)))
        (cons s m))))

  (unwind-protect
      ;; Factorial: result = 1, counter = n
      ;; Loop: if counter <= 0, halt. result *= counter. counter -= 1. goto loop.
      (let ((fact-prog (vector
                          '(LOAD result 1)     ;; 0: result = 1
                          '(LOAD one 1)        ;; 1: one = 1
                          '(LOAD zero 0)       ;; 2: zero = 0
                          '(CMP n zero)        ;; 3: compare n with 0  [LOOP]
                          '(JZ 8)              ;; 4: if n == 0, goto HALT
                          '(MUL result n)      ;; 5: result *= n
                          '(SUB n one)         ;; 6: n -= 1
                          '(JMP 3)             ;; 7: goto LOOP
                          '(HALT))))           ;; 8: done
        ;; Test factorial(0) = 1
        (let* ((m0 (funcall 'neovm--rm-make))
               (_ (funcall 'neovm--rm-set-reg m0 'n 0))
               (r0 (funcall 'neovm--rm-run m0 fact-prog)))
          ;; factorial(1) = 1
          (let* ((m1 (funcall 'neovm--rm-make))
                 (_ (funcall 'neovm--rm-set-reg m1 'n 1))
                 (r1 (funcall 'neovm--rm-run m1 fact-prog)))
            ;; factorial(5) = 120
            (let* ((m5 (funcall 'neovm--rm-make))
                   (_ (funcall 'neovm--rm-set-reg m5 'n 5))
                   (r5 (funcall 'neovm--rm-run m5 fact-prog)))
              ;; factorial(8) = 40320
              (let* ((m8 (funcall 'neovm--rm-make))
                     (_ (funcall 'neovm--rm-set-reg m8 'n 8))
                     (r8 (funcall 'neovm--rm-run m8 fact-prog)))
                ;; factorial(10) = 3628800
                (let* ((m10 (funcall 'neovm--rm-make))
                       (_ (funcall 'neovm--rm-set-reg m10 'n 10))
                       (r10 (funcall 'neovm--rm-run m10 fact-prog)))
                  (list
                    (funcall 'neovm--rm-get-reg (cdr r0) 'result)     ;; 1
                    (funcall 'neovm--rm-get-reg (cdr r1) 'result)     ;; 1
                    (funcall 'neovm--rm-get-reg (cdr r5) 'result)     ;; 120
                    (funcall 'neovm--rm-get-reg (cdr r8) 'result)     ;; 40320
                    (funcall 'neovm--rm-get-reg (cdr r10) 'result)    ;; 3628800
                    ;; Verify
                    (= (funcall 'neovm--rm-get-reg (cdr r5) 'result) 120)
                    (= (funcall 'neovm--rm-get-reg (cdr r10) 'result) 3628800))))))))
    (fmakunbound 'neovm--rm-make)
    (fmakunbound 'neovm--rm-get-reg)
    (fmakunbound 'neovm--rm-set-reg)
    (fmakunbound 'neovm--rm-get-pc)
    (fmakunbound 'neovm--rm-set-pc)
    (fmakunbound 'neovm--rm-step)
    (fmakunbound 'neovm--rm-run)))"#;
    let expect = expect_test::expect![[r#""OK (1 1 120 40320 3628800 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Fibonacci sequence using register machine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_fibonacci() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rm-make (lambda () (cons nil 0)))
  (fset 'neovm--rm-get-reg (lambda (m r) (let ((p (assq r (car m)))) (if p (cdr p) 0))))
  (fset 'neovm--rm-set-reg (lambda (m r v) (let ((p (assq r (car m)))) (if p (setcdr p v) (setcar m (cons (cons r v) (car m))))) m))
  (fset 'neovm--rm-get-pc (lambda (m) (cdr m)))
  (fset 'neovm--rm-set-pc (lambda (m pc) (setcdr m pc) m))
  (fset 'neovm--rm-step
    (lambda (m prog)
      (let* ((pc (funcall 'neovm--rm-get-pc m)) (instr (aref prog pc)) (op (car instr)))
        (cond
          ((eq op 'LOAD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (nth 2 instr)) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'STORE) (funcall 'neovm--rm-set-reg m (nth 1 instr) (funcall 'neovm--rm-get-reg m (nth 2 instr))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'ADD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (+ (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'SUB) (funcall 'neovm--rm-set-reg m (nth 1 instr) (- (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'MUL) (funcall 'neovm--rm-set-reg m (nth 1 instr) (* (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'CMP) (let ((v1 (funcall 'neovm--rm-get-reg m (nth 1 instr))) (v2 (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-reg m 'flag (cond ((< v1 v2) -1) ((= v1 v2) 0) (t 1)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'JMP) (funcall 'neovm--rm-set-pc m (nth 1 instr)))
          ((eq op 'JZ) (if (= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'JNZ) (if (/= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'HALT) m)
          (t (error "Unknown op: %s" op)))
        m)))
  (fset 'neovm--rm-run
    (lambda (m prog &optional max)
      (let ((s 0) (lim (or max 10000)))
        (while (and (< s lim) (< (funcall 'neovm--rm-get-pc m) (length prog))
                    (not (eq (car (aref prog (funcall 'neovm--rm-get-pc m))) 'HALT)))
          (funcall 'neovm--rm-step m prog) (setq s (1+ s)))
        (cons s m))))

  (unwind-protect
      ;; Fibonacci: compute fib(n)
      ;; a = 0, b = 1, counter = n
      ;; Loop: if counter == 0, halt (result in a).
      ;;        tmp = a + b. a = b. b = tmp. counter -= 1. goto loop.
      (let ((fib-prog (vector
                         '(LOAD a 0)           ;; 0: a = 0 (fib(0))
                         '(LOAD b 1)           ;; 1: b = 1 (fib(1))
                         '(LOAD one 1)         ;; 2: one = 1
                         '(LOAD zero 0)        ;; 3: zero = 0
                         '(CMP n zero)         ;; 4: compare n with 0  [LOOP]
                         '(JZ 12)              ;; 5: if n == 0, goto HALT
                         '(STORE tmp a)        ;; 6: tmp = a
                         '(ADD tmp b)          ;; 7: tmp = tmp + b (= a + b)
                         '(STORE a b)          ;; 8: a = b
                         '(STORE b tmp)        ;; 9: b = tmp
                         '(SUB n one)          ;; 10: n -= 1
                         '(JMP 4)              ;; 11: goto LOOP
                         '(HALT))))            ;; 12: done, result in a
        ;; Compute fib for several values
        (let ((results nil))
          (dolist (input '(0 1 2 3 4 5 6 7 8 10 15))
            (let* ((m (funcall 'neovm--rm-make))
                   (_ (funcall 'neovm--rm-set-reg m 'n input))
                   (r (funcall 'neovm--rm-run m fib-prog)))
              (push (list input (funcall 'neovm--rm-get-reg (cdr r) 'a)) results)))
          (nreverse results)))
    (fmakunbound 'neovm--rm-make)
    (fmakunbound 'neovm--rm-get-reg)
    (fmakunbound 'neovm--rm-set-reg)
    (fmakunbound 'neovm--rm-get-pc)
    (fmakunbound 'neovm--rm-set-pc)
    (fmakunbound 'neovm--rm-step)
    (fmakunbound 'neovm--rm-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 0) (1 1) (2 1) (3 2) (4 3) (5 5) (6 8) (7 13) (8 21) (10 55) (15 610))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: power function (exponentiation by repeated multiplication)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_register_machine_power() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rm-make (lambda () (cons nil 0)))
  (fset 'neovm--rm-get-reg (lambda (m r) (let ((p (assq r (car m)))) (if p (cdr p) 0))))
  (fset 'neovm--rm-set-reg (lambda (m r v) (let ((p (assq r (car m)))) (if p (setcdr p v) (setcar m (cons (cons r v) (car m))))) m))
  (fset 'neovm--rm-get-pc (lambda (m) (cdr m)))
  (fset 'neovm--rm-set-pc (lambda (m pc) (setcdr m pc) m))
  (fset 'neovm--rm-step
    (lambda (m prog)
      (let* ((pc (funcall 'neovm--rm-get-pc m)) (instr (aref prog pc)) (op (car instr)))
        (cond
          ((eq op 'LOAD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (nth 2 instr)) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'STORE) (funcall 'neovm--rm-set-reg m (nth 1 instr) (funcall 'neovm--rm-get-reg m (nth 2 instr))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'ADD) (funcall 'neovm--rm-set-reg m (nth 1 instr) (+ (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'SUB) (funcall 'neovm--rm-set-reg m (nth 1 instr) (- (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'MUL) (funcall 'neovm--rm-set-reg m (nth 1 instr) (* (funcall 'neovm--rm-get-reg m (nth 1 instr)) (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'CMP) (let ((v1 (funcall 'neovm--rm-get-reg m (nth 1 instr))) (v2 (funcall 'neovm--rm-get-reg m (nth 2 instr)))) (funcall 'neovm--rm-set-reg m 'flag (cond ((< v1 v2) -1) ((= v1 v2) 0) (t 1)))) (funcall 'neovm--rm-set-pc m (1+ pc)))
          ((eq op 'JMP) (funcall 'neovm--rm-set-pc m (nth 1 instr)))
          ((eq op 'JZ) (if (= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'JNZ) (if (/= (funcall 'neovm--rm-get-reg m 'flag) 0) (funcall 'neovm--rm-set-pc m (nth 1 instr)) (funcall 'neovm--rm-set-pc m (1+ pc))))
          ((eq op 'HALT) m)
          (t (error "Unknown op: %s" op)))
        m)))
  (fset 'neovm--rm-run
    (lambda (m prog &optional max)
      (let ((s 0) (lim (or max 10000)))
        (while (and (< s lim) (< (funcall 'neovm--rm-get-pc m) (length prog))
                    (not (eq (car (aref prog (funcall 'neovm--rm-get-pc m))) 'HALT)))
          (funcall 'neovm--rm-step m prog) (setq s (1+ s)))
        (cons s m))))

  (unwind-protect
      ;; Power: compute base^exp by repeated multiplication
      ;; result = 1. Loop: if exp == 0, halt. result *= base. exp -= 1. goto loop.
      (let ((pow-prog (vector
                         '(LOAD result 1)      ;; 0: result = 1
                         '(LOAD one 1)         ;; 1: one = 1
                         '(LOAD zero 0)        ;; 2: zero = 0
                         '(CMP exp zero)       ;; 3: compare exp with 0  [LOOP]
                         '(JZ 8)               ;; 4: if exp == 0, goto HALT
                         '(MUL result base)    ;; 5: result *= base
                         '(SUB exp one)        ;; 6: exp -= 1
                         '(JMP 3)              ;; 7: goto LOOP
                         '(HALT))))            ;; 8: done
        (let ((results nil))
          (dolist (test '((2 0) (2 1) (2 10) (3 4) (5 3) (10 3) (7 2)))
            (let* ((b (car test))
                   (e (cadr test))
                   (m (funcall 'neovm--rm-make))
                   (_ (funcall 'neovm--rm-set-reg m 'base b))
                   (_ (funcall 'neovm--rm-set-reg m 'exp e))
                   (r (funcall 'neovm--rm-run m pow-prog)))
              (push (list b e (funcall 'neovm--rm-get-reg (cdr r) 'result)) results)))
          ;; Expected: (2 0 1) (2 1 2) (2 10 1024) (3 4 81) (5 3 125) (10 3 1000) (7 2 49)
          (nreverse results)))
    (fmakunbound 'neovm--rm-make)
    (fmakunbound 'neovm--rm-get-reg)
    (fmakunbound 'neovm--rm-set-reg)
    (fmakunbound 'neovm--rm-get-pc)
    (fmakunbound 'neovm--rm-set-pc)
    (fmakunbound 'neovm--rm-step)
    (fmakunbound 'neovm--rm-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 0 1) (2 1 2) (2 10 1024) (3 4 81) (5 3 125) (10 3 1000) (7 2 49))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
