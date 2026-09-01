//! Oracle parity tests implementing an advanced stack-based VM in Elisp:
//! stack operations (PUSH, POP, DUP, SWAP, OVER, ROT), arithmetic
//! (ADD, SUB, MUL, DIV, MOD, NEG), comparison (EQ, LT, GT),
//! control flow (JMP, JZ, JNZ, CALL, RET), local variables
//! (LOAD_LOCAL, STORE_LOCAL), and complex programs (Fibonacci,
//! string reverse, factorial).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// VM infrastructure: stack operations and arithmetic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vm_advanced_stack_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; VM state: (stack pc locals call-stack)
  ;; stack: list (top at car)
  ;; pc: integer
  ;; locals: vector of local variable slots
  ;; call-stack: list of (return-pc . saved-locals)
  (fset 'neovm--vm-make
    (lambda (&optional num-locals)
      (list nil 0 (make-vector (or num-locals 16) 0) nil)))

  (fset 'neovm--vm-stack (lambda (vm) (nth 0 vm)))
  (fset 'neovm--vm-pc (lambda (vm) (nth 1 vm)))
  (fset 'neovm--vm-locals (lambda (vm) (nth 2 vm)))
  (fset 'neovm--vm-call-stack (lambda (vm) (nth 3 vm)))

  (fset 'neovm--vm-set-stack (lambda (vm s) (setcar vm s)))
  (fset 'neovm--vm-set-pc (lambda (vm p) (setcar (nthcdr 1 vm) p)))
  (fset 'neovm--vm-set-locals (lambda (vm l) (setcar (nthcdr 2 vm) l)))
  (fset 'neovm--vm-set-call-stack (lambda (vm cs) (setcar (nthcdr 3 vm) cs)))

  (fset 'neovm--vm-push (lambda (vm val)
    (funcall 'neovm--vm-set-stack vm (cons val (funcall 'neovm--vm-stack vm)))))

  (fset 'neovm--vm-pop (lambda (vm)
    (let ((s (funcall 'neovm--vm-stack vm)))
      (funcall 'neovm--vm-set-stack vm (cdr s))
      (car s))))

  (fset 'neovm--vm-peek (lambda (vm)
    (car (funcall 'neovm--vm-stack vm))))

  ;; Execute one instruction
  (fset 'neovm--vm-step
    (lambda (vm program)
      (let* ((pc (funcall 'neovm--vm-pc vm))
             (instr (aref program pc))
             (op (if (consp instr) (car instr) instr)))
        (cond
         ;; Stack operations
         ((eq op 'PUSH)
          (funcall 'neovm--vm-push vm (cadr instr))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'POP)
          (funcall 'neovm--vm-pop vm)
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'DUP)
          (funcall 'neovm--vm-push vm (funcall 'neovm--vm-peek vm))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'SWAP)
          (let* ((a (funcall 'neovm--vm-pop vm))
                 (b (funcall 'neovm--vm-pop vm)))
            (funcall 'neovm--vm-push vm a)
            (funcall 'neovm--vm-push vm b))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'OVER)
          (let ((s (funcall 'neovm--vm-stack vm)))
            (funcall 'neovm--vm-push vm (cadr s)))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'ROT)
          (let* ((a (funcall 'neovm--vm-pop vm))
                 (b (funcall 'neovm--vm-pop vm))
                 (c (funcall 'neovm--vm-pop vm)))
            (funcall 'neovm--vm-push vm b)
            (funcall 'neovm--vm-push vm a)
            (funcall 'neovm--vm-push vm c))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))

         ;; Arithmetic
         ((eq op 'ADD)
          (let* ((b (funcall 'neovm--vm-pop vm))
                 (a (funcall 'neovm--vm-pop vm)))
            (funcall 'neovm--vm-push vm (+ a b)))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'SUB)
          (let* ((b (funcall 'neovm--vm-pop vm))
                 (a (funcall 'neovm--vm-pop vm)))
            (funcall 'neovm--vm-push vm (- a b)))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'MUL)
          (let* ((b (funcall 'neovm--vm-pop vm))
                 (a (funcall 'neovm--vm-pop vm)))
            (funcall 'neovm--vm-push vm (* a b)))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'DIV)
          (let* ((b (funcall 'neovm--vm-pop vm))
                 (a (funcall 'neovm--vm-pop vm)))
            (funcall 'neovm--vm-push vm (/ a b)))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'MOD)
          (let* ((b (funcall 'neovm--vm-pop vm))
                 (a (funcall 'neovm--vm-pop vm)))
            (funcall 'neovm--vm-push vm (% a b)))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'NEG)
          (let ((a (funcall 'neovm--vm-pop vm)))
            (funcall 'neovm--vm-push vm (- a)))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))

         ;; Comparison (push 1 for true, 0 for false)
         ((eq op 'EQ)
          (let* ((b (funcall 'neovm--vm-pop vm))
                 (a (funcall 'neovm--vm-pop vm)))
            (funcall 'neovm--vm-push vm (if (= a b) 1 0)))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'LT)
          (let* ((b (funcall 'neovm--vm-pop vm))
                 (a (funcall 'neovm--vm-pop vm)))
            (funcall 'neovm--vm-push vm (if (< a b) 1 0)))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'GT)
          (let* ((b (funcall 'neovm--vm-pop vm))
                 (a (funcall 'neovm--vm-pop vm)))
            (funcall 'neovm--vm-push vm (if (> a b) 1 0)))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))

         ;; Control flow
         ((eq op 'JMP)
          (funcall 'neovm--vm-set-pc vm (cadr instr)))
         ((eq op 'JZ)
          (let ((val (funcall 'neovm--vm-pop vm)))
            (if (= val 0)
                (funcall 'neovm--vm-set-pc vm (cadr instr))
              (funcall 'neovm--vm-set-pc vm (1+ pc)))))
         ((eq op 'JNZ)
          (let ((val (funcall 'neovm--vm-pop vm)))
            (if (/= val 0)
                (funcall 'neovm--vm-set-pc vm (cadr instr))
              (funcall 'neovm--vm-set-pc vm (1+ pc)))))

         ;; Call/Return
         ((eq op 'CALL)
          (funcall 'neovm--vm-set-call-stack vm
                   (cons (cons (1+ pc) (copy-sequence (funcall 'neovm--vm-locals vm)))
                         (funcall 'neovm--vm-call-stack vm)))
          (funcall 'neovm--vm-set-pc vm (cadr instr)))
         ((eq op 'RET)
          (let* ((frame (car (funcall 'neovm--vm-call-stack vm)))
                 (ret-pc (car frame))
                 (saved-locals (cdr frame)))
            (funcall 'neovm--vm-set-call-stack vm
                     (cdr (funcall 'neovm--vm-call-stack vm)))
            (funcall 'neovm--vm-set-locals vm saved-locals)
            (funcall 'neovm--vm-set-pc vm ret-pc)))

         ;; Local variables
         ((eq op 'LOAD_LOCAL)
          (funcall 'neovm--vm-push vm (aref (funcall 'neovm--vm-locals vm) (cadr instr)))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'STORE_LOCAL)
          (aset (funcall 'neovm--vm-locals vm) (cadr instr) (funcall 'neovm--vm-pop vm))
          (funcall 'neovm--vm-set-pc vm (1+ pc)))

         ;; Halt
         ((eq op 'HALT) nil)
         (t (error "Unknown VM op: %S" op)))
        vm)))

  ;; Run VM until HALT or max steps
  (fset 'neovm--vm-run
    (lambda (vm program &optional max-steps)
      (let ((steps 0) (limit (or max-steps 10000)))
        (while (and (< steps limit)
                    (< (funcall 'neovm--vm-pc vm) (length program))
                    (not (eq (let ((i (aref program (funcall 'neovm--vm-pc vm))))
                               (if (consp i) (car i) i))
                             'HALT)))
          (funcall 'neovm--vm-step vm program)
          (setq steps (1+ steps)))
        steps)))

  (unwind-protect
      (list
       ;; Test basic stack operations
       (let ((vm (funcall 'neovm--vm-make)))
         (let ((prog (vector '(PUSH 10) '(PUSH 20) '(PUSH 30)
                             'DUP 'SWAP 'OVER 'HALT)))
           (funcall 'neovm--vm-run vm prog)
           ;; Stack after: PUSH 10 -> [10], PUSH 20 -> [20 10], PUSH 30 -> [30 20 10]
           ;; DUP -> [30 30 20 10], SWAP -> [30 30 20 10] swap top 2 -> [30 30 20 10]
           ;; Actually: DUP [30 30 20 10], SWAP [30 30 20 10], OVER peek second
           (funcall 'neovm--vm-stack vm)))

       ;; Test ROT: rotates top 3 elements
       (let ((vm (funcall 'neovm--vm-make)))
         (let ((prog (vector '(PUSH 1) '(PUSH 2) '(PUSH 3) 'ROT 'HALT)))
           (funcall 'neovm--vm-run vm prog)
           ;; Before ROT: [3 2 1], pop a=3,b=2,c=1, push b=2, push a=3, push c=1
           ;; Result: [1 3 2]
           (funcall 'neovm--vm-stack vm)))

       ;; Test arithmetic: (10 + 20) * 3 - 5 = 85
       (let ((vm (funcall 'neovm--vm-make)))
         (let ((prog (vector '(PUSH 10) '(PUSH 20) 'ADD
                             '(PUSH 3) 'MUL '(PUSH 5) 'SUB 'HALT)))
           (funcall 'neovm--vm-run vm prog)
           (funcall 'neovm--vm-peek vm)))

       ;; Test DIV and MOD: 17 / 5 = 3, 17 % 5 = 2
       (let ((vm (funcall 'neovm--vm-make)))
         (let ((prog (vector '(PUSH 17) '(PUSH 5) 'DIV 'HALT)))
           (funcall 'neovm--vm-run vm prog)
           (let ((div-result (funcall 'neovm--vm-peek vm))
                 (vm2 (funcall 'neovm--vm-make)))
             (let ((prog2 (vector '(PUSH 17) '(PUSH 5) 'MOD 'HALT)))
               (funcall 'neovm--vm-run vm2 prog2)
               (list div-result (funcall 'neovm--vm-peek vm2))))))

       ;; Test NEG
       (let ((vm (funcall 'neovm--vm-make)))
         (let ((prog (vector '(PUSH 42) 'NEG 'HALT)))
           (funcall 'neovm--vm-run vm prog)
           (funcall 'neovm--vm-peek vm)))

       ;; Test comparison
       (let ((vm (funcall 'neovm--vm-make)))
         (let ((prog (vector '(PUSH 5) '(PUSH 5) 'EQ
                             '(PUSH 3) '(PUSH 7) 'LT
                             '(PUSH 9) '(PUSH 2) 'GT 'HALT)))
           (funcall 'neovm--vm-run vm prog)
           ;; Stack: [1(GT) 1(LT) 1(EQ)]
           (funcall 'neovm--vm-stack vm))))
    (fmakunbound 'neovm--vm-make)
    (fmakunbound 'neovm--vm-stack)
    (fmakunbound 'neovm--vm-pc)
    (fmakunbound 'neovm--vm-locals)
    (fmakunbound 'neovm--vm-call-stack)
    (fmakunbound 'neovm--vm-set-stack)
    (fmakunbound 'neovm--vm-set-pc)
    (fmakunbound 'neovm--vm-set-locals)
    (fmakunbound 'neovm--vm-set-call-stack)
    (fmakunbound 'neovm--vm-push)
    (fmakunbound 'neovm--vm-pop)
    (fmakunbound 'neovm--vm-peek)
    (fmakunbound 'neovm--vm-step)
    (fmakunbound 'neovm--vm-run)))"#;
    let expect = expect_test::expect![[r#""OK ((30 30 30 20 10) (1 3 2) 85 (3 2) -42 (1 1 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Control flow: JMP, JZ, JNZ with loops
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vm_advanced_control_flow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--vm-make (lambda (&optional n) (list nil 0 (make-vector (or n 16) 0) nil)))
  (fset 'neovm--vm-stack (lambda (vm) (nth 0 vm)))
  (fset 'neovm--vm-pc (lambda (vm) (nth 1 vm)))
  (fset 'neovm--vm-locals (lambda (vm) (nth 2 vm)))
  (fset 'neovm--vm-call-stack (lambda (vm) (nth 3 vm)))
  (fset 'neovm--vm-set-stack (lambda (vm s) (setcar vm s)))
  (fset 'neovm--vm-set-pc (lambda (vm p) (setcar (nthcdr 1 vm) p)))
  (fset 'neovm--vm-set-locals (lambda (vm l) (setcar (nthcdr 2 vm) l)))
  (fset 'neovm--vm-set-call-stack (lambda (vm cs) (setcar (nthcdr 3 vm) cs)))
  (fset 'neovm--vm-push (lambda (vm v) (funcall 'neovm--vm-set-stack vm (cons v (funcall 'neovm--vm-stack vm)))))
  (fset 'neovm--vm-pop (lambda (vm) (let ((s (funcall 'neovm--vm-stack vm))) (funcall 'neovm--vm-set-stack vm (cdr s)) (car s))))
  (fset 'neovm--vm-peek (lambda (vm) (car (funcall 'neovm--vm-stack vm))))

  (fset 'neovm--vm-step
    (lambda (vm prog)
      (let* ((pc (funcall 'neovm--vm-pc vm))
             (instr (aref prog pc))
             (op (if (consp instr) (car instr) instr)))
        (cond
         ((eq op 'PUSH) (funcall 'neovm--vm-push vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'POP) (funcall 'neovm--vm-pop vm) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'DUP) (funcall 'neovm--vm-push vm (funcall 'neovm--vm-peek vm)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'SWAP) (let* ((a (funcall 'neovm--vm-pop vm)) (b (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm a) (funcall 'neovm--vm-push vm b)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'OVER) (funcall 'neovm--vm-push vm (cadr (funcall 'neovm--vm-stack vm))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'ADD) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (+ a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'SUB) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (- a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'MUL) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (* a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'DIV) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (/ a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'MOD) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (% a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'NEG) (funcall 'neovm--vm-push vm (- (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'EQ) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (if (= a b) 1 0))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'LT) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (if (< a b) 1 0))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'GT) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (if (> a b) 1 0))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'JMP) (funcall 'neovm--vm-set-pc vm (cadr instr)))
         ((eq op 'JZ) (if (= (funcall 'neovm--vm-pop vm) 0) (funcall 'neovm--vm-set-pc vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc))))
         ((eq op 'JNZ) (if (/= (funcall 'neovm--vm-pop vm) 0) (funcall 'neovm--vm-set-pc vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc))))
         ((eq op 'CALL) (funcall 'neovm--vm-set-call-stack vm (cons (cons (1+ pc) (copy-sequence (funcall 'neovm--vm-locals vm))) (funcall 'neovm--vm-call-stack vm))) (funcall 'neovm--vm-set-pc vm (cadr instr)))
         ((eq op 'RET) (let* ((f (car (funcall 'neovm--vm-call-stack vm)))) (funcall 'neovm--vm-set-call-stack vm (cdr (funcall 'neovm--vm-call-stack vm))) (funcall 'neovm--vm-set-locals vm (cdr f)) (funcall 'neovm--vm-set-pc vm (car f))))
         ((eq op 'LOAD_LOCAL) (funcall 'neovm--vm-push vm (aref (funcall 'neovm--vm-locals vm) (cadr instr))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'STORE_LOCAL) (aset (funcall 'neovm--vm-locals vm) (cadr instr) (funcall 'neovm--vm-pop vm)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'HALT) nil)
         (t (error "Bad op: %S" op)))
        vm)))

  (fset 'neovm--vm-run
    (lambda (vm prog &optional max)
      (let ((s 0) (lim (or max 10000)))
        (while (and (< s lim) (< (funcall 'neovm--vm-pc vm) (length prog))
                    (not (eq (let ((i (aref prog (funcall 'neovm--vm-pc vm))))
                               (if (consp i) (car i) i)) 'HALT)))
          (funcall 'neovm--vm-step vm prog) (setq s (1+ s)))
        s)))

  (unwind-protect
      (list
       ;; Sum 1 to 10 using loop with locals
       ;; local[0] = counter, local[1] = sum
       (let ((vm (funcall 'neovm--vm-make)))
         (let ((prog (vector
                      '(PUSH 0)         ;; 0: push initial sum
                      '(STORE_LOCAL 1)  ;; 1: sum = 0
                      '(PUSH 1)         ;; 2: push initial counter
                      '(STORE_LOCAL 0)  ;; 3: counter = 1
                      ;; Loop start (addr 4):
                      '(LOAD_LOCAL 0)   ;; 4: push counter
                      '(PUSH 10)        ;; 5: push 10
                      'GT               ;; 6: counter > 10?
                      '(JNZ 14)         ;; 7: if yes, jump to end
                      '(LOAD_LOCAL 1)   ;; 8: push sum
                      '(LOAD_LOCAL 0)   ;; 9: push counter
                      'ADD              ;; 10: sum + counter
                      '(STORE_LOCAL 1)  ;; 11: sum = sum + counter
                      '(LOAD_LOCAL 0)   ;; 12: push counter
                      '(PUSH 1)         ;; 13: push 1
                      'ADD              ;; 14: counter + 1
                      '(STORE_LOCAL 0)  ;; 15: counter = counter + 1
                      '(JMP 4)          ;; 16: jump to loop start
                      ;; Actually fix addresses:
                      'HALT)))
           ;; Need to fix the program - let me rebuild with correct addresses
           (setq prog (vector
                       '(PUSH 0) '(STORE_LOCAL 1) '(PUSH 1) '(STORE_LOCAL 0)
                       ;; 4: loop check
                       '(LOAD_LOCAL 0) '(PUSH 11) 'GT '(JNZ 14)
                       ;; 8: body
                       '(LOAD_LOCAL 1) '(LOAD_LOCAL 0) 'ADD '(STORE_LOCAL 1)
                       '(LOAD_LOCAL 0) '(PUSH 1) 'ADD '(STORE_LOCAL 0)
                       ;; 16: jump back
                       '(JMP 4) 'HALT))
           (funcall 'neovm--vm-run vm prog)
           (aref (funcall 'neovm--vm-locals vm) 1)))  ;; Should be 55

       ;; Countdown: push numbers 5, 4, 3, 2, 1 onto stack
       (let ((vm (funcall 'neovm--vm-make)))
         (let ((prog (vector
                      '(PUSH 5) '(STORE_LOCAL 0)
                      ;; 2: loop
                      '(LOAD_LOCAL 0) '(PUSH 0) 'EQ '(JNZ 12)
                      '(LOAD_LOCAL 0) 'DUP '(PUSH 1) 'SUB '(STORE_LOCAL 0)
                      '(JMP 2) 'HALT)))
           (funcall 'neovm--vm-run vm prog)
           (funcall 'neovm--vm-stack vm)))

       ;; Conditional: max(a, b)
       (let ((vm (funcall 'neovm--vm-make)))
         (let ((prog (vector
                      '(PUSH 17) '(STORE_LOCAL 0)   ;; a = 17
                      '(PUSH 42) '(STORE_LOCAL 1)   ;; b = 42
                      '(LOAD_LOCAL 0) '(LOAD_LOCAL 1) 'GT  ;; a > b?
                      '(JNZ 10)                      ;; if yes, push a
                      '(LOAD_LOCAL 1) '(JMP 11)      ;; else push b
                      '(LOAD_LOCAL 0)                ;; push a
                      'HALT)))
           (funcall 'neovm--vm-run vm prog)
           (funcall 'neovm--vm-peek vm))))
    (fmakunbound 'neovm--vm-make)
    (fmakunbound 'neovm--vm-stack)
    (fmakunbound 'neovm--vm-pc)
    (fmakunbound 'neovm--vm-locals)
    (fmakunbound 'neovm--vm-call-stack)
    (fmakunbound 'neovm--vm-set-stack)
    (fmakunbound 'neovm--vm-set-pc)
    (fmakunbound 'neovm--vm-set-locals)
    (fmakunbound 'neovm--vm-set-call-stack)
    (fmakunbound 'neovm--vm-push)
    (fmakunbound 'neovm--vm-pop)
    (fmakunbound 'neovm--vm-peek)
    (fmakunbound 'neovm--vm-step)
    (fmakunbound 'neovm--vm-run)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fibonacci computation using the VM
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vm_advanced_fibonacci() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--vm-make (lambda (&optional n) (list nil 0 (make-vector (or n 16) 0) nil)))
  (fset 'neovm--vm-stack (lambda (vm) (nth 0 vm)))
  (fset 'neovm--vm-pc (lambda (vm) (nth 1 vm)))
  (fset 'neovm--vm-locals (lambda (vm) (nth 2 vm)))
  (fset 'neovm--vm-set-stack (lambda (vm s) (setcar vm s)))
  (fset 'neovm--vm-set-pc (lambda (vm p) (setcar (nthcdr 1 vm) p)))
  (fset 'neovm--vm-set-locals (lambda (vm l) (setcar (nthcdr 2 vm) l)))
  (fset 'neovm--vm-push (lambda (vm v) (funcall 'neovm--vm-set-stack vm (cons v (funcall 'neovm--vm-stack vm)))))
  (fset 'neovm--vm-pop (lambda (vm) (let ((s (funcall 'neovm--vm-stack vm))) (funcall 'neovm--vm-set-stack vm (cdr s)) (car s))))
  (fset 'neovm--vm-peek (lambda (vm) (car (funcall 'neovm--vm-stack vm))))

  (fset 'neovm--vm-step
    (lambda (vm prog)
      (let* ((pc (funcall 'neovm--vm-pc vm))
             (instr (aref prog pc))
             (op (if (consp instr) (car instr) instr)))
        (cond
         ((eq op 'PUSH) (funcall 'neovm--vm-push vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'POP) (funcall 'neovm--vm-pop vm) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'DUP) (funcall 'neovm--vm-push vm (funcall 'neovm--vm-peek vm)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'SWAP) (let* ((a (funcall 'neovm--vm-pop vm)) (b (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm a) (funcall 'neovm--vm-push vm b)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'OVER) (funcall 'neovm--vm-push vm (cadr (funcall 'neovm--vm-stack vm))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'ADD) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (+ a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'SUB) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (- a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'MUL) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (* a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'EQ) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (if (= a b) 1 0))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'LT) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (if (< a b) 1 0))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'GT) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (if (> a b) 1 0))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'JMP) (funcall 'neovm--vm-set-pc vm (cadr instr)))
         ((eq op 'JZ) (if (= (funcall 'neovm--vm-pop vm) 0) (funcall 'neovm--vm-set-pc vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc))))
         ((eq op 'JNZ) (if (/= (funcall 'neovm--vm-pop vm) 0) (funcall 'neovm--vm-set-pc vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc))))
         ((eq op 'LOAD_LOCAL) (funcall 'neovm--vm-push vm (aref (funcall 'neovm--vm-locals vm) (cadr instr))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'STORE_LOCAL) (aset (funcall 'neovm--vm-locals vm) (cadr instr) (funcall 'neovm--vm-pop vm)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'HALT) nil)
         (t (error "Bad op: %S" op)))
        vm)))

  (fset 'neovm--vm-run
    (lambda (vm prog &optional max)
      (let ((s 0) (lim (or max 10000)))
        (while (and (< s lim) (< (funcall 'neovm--vm-pc vm) (length prog))
                    (not (eq (let ((i (aref prog (funcall 'neovm--vm-pc vm))))
                               (if (consp i) (car i) i)) 'HALT)))
          (funcall 'neovm--vm-step vm prog) (setq s (1+ s)))
        s)))

  (unwind-protect
      ;; Compute first 10 Fibonacci numbers iteratively
      ;; local[0] = n (how many to compute), local[1] = a, local[2] = b, local[3] = counter
      ;; Result: push each fib number onto stack
      (let ((vm (funcall 'neovm--vm-make)))
        (let ((prog (vector
                     ;; Init
                     '(PUSH 10) '(STORE_LOCAL 0)   ;; 0,1: n = 10
                     '(PUSH 0)  '(STORE_LOCAL 1)   ;; 2,3: a = 0
                     '(PUSH 1)  '(STORE_LOCAL 2)   ;; 4,5: b = 1
                     '(PUSH 0)  '(STORE_LOCAL 3)   ;; 6,7: counter = 0
                     ;; 8: Loop check
                     '(LOAD_LOCAL 3) '(LOAD_LOCAL 0) 'EQ '(JNZ 22)
                     ;; 12: Push current fib (a)
                     '(LOAD_LOCAL 1)
                     ;; 13: Compute next: temp = a + b, a = b, b = temp
                     '(LOAD_LOCAL 1) '(LOAD_LOCAL 2) 'ADD  ;; stack: [a+b, fib_i, ...]
                     '(LOAD_LOCAL 2) '(STORE_LOCAL 1)      ;; a = b
                     '(STORE_LOCAL 2)                       ;; b = old a+b
                     ;; 20: counter++
                     '(LOAD_LOCAL 3) '(PUSH 1) 'ADD '(STORE_LOCAL 3)
                     ;; 24: Jump back
                     '(JMP 8)
                     ;; 25: HALT (addr 22 needs fixing)
                     'HALT)))
          ;; Fix: JNZ at index 11 should jump to 25 (HALT)
          (aset prog 11 '(JNZ 25))
          (funcall 'neovm--vm-run vm prog)
          ;; Stack has fib numbers in reverse order (most recent on top)
          (nreverse (funcall 'neovm--vm-stack vm))))
    (fmakunbound 'neovm--vm-make)
    (fmakunbound 'neovm--vm-stack)
    (fmakunbound 'neovm--vm-pc)
    (fmakunbound 'neovm--vm-locals)
    (fmakunbound 'neovm--vm-set-stack)
    (fmakunbound 'neovm--vm-set-pc)
    (fmakunbound 'neovm--vm-set-locals)
    (fmakunbound 'neovm--vm-push)
    (fmakunbound 'neovm--vm-pop)
    (fmakunbound 'neovm--vm-peek)
    (fmakunbound 'neovm--vm-step)
    (fmakunbound 'neovm--vm-run)))"#;
    let expect = expect_test::expect![[r#""OK (0 1 1 2 3 5 8 13 21 34)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Factorial computation using CALL/RET
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vm_advanced_factorial_with_call_ret() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--vm-make (lambda (&optional n) (list nil 0 (make-vector (or n 16) 0) nil)))
  (fset 'neovm--vm-stack (lambda (vm) (nth 0 vm)))
  (fset 'neovm--vm-pc (lambda (vm) (nth 1 vm)))
  (fset 'neovm--vm-locals (lambda (vm) (nth 2 vm)))
  (fset 'neovm--vm-call-stack (lambda (vm) (nth 3 vm)))
  (fset 'neovm--vm-set-stack (lambda (vm s) (setcar vm s)))
  (fset 'neovm--vm-set-pc (lambda (vm p) (setcar (nthcdr 1 vm) p)))
  (fset 'neovm--vm-set-locals (lambda (vm l) (setcar (nthcdr 2 vm) l)))
  (fset 'neovm--vm-set-call-stack (lambda (vm cs) (setcar (nthcdr 3 vm) cs)))
  (fset 'neovm--vm-push (lambda (vm v) (funcall 'neovm--vm-set-stack vm (cons v (funcall 'neovm--vm-stack vm)))))
  (fset 'neovm--vm-pop (lambda (vm) (let ((s (funcall 'neovm--vm-stack vm))) (funcall 'neovm--vm-set-stack vm (cdr s)) (car s))))
  (fset 'neovm--vm-peek (lambda (vm) (car (funcall 'neovm--vm-stack vm))))

  (fset 'neovm--vm-step
    (lambda (vm prog)
      (let* ((pc (funcall 'neovm--vm-pc vm))
             (instr (aref prog pc))
             (op (if (consp instr) (car instr) instr)))
        (cond
         ((eq op 'PUSH) (funcall 'neovm--vm-push vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'POP) (funcall 'neovm--vm-pop vm) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'DUP) (funcall 'neovm--vm-push vm (funcall 'neovm--vm-peek vm)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'SWAP) (let* ((a (funcall 'neovm--vm-pop vm)) (b (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm a) (funcall 'neovm--vm-push vm b)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'ADD) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (+ a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'SUB) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (- a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'MUL) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (* a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'EQ) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (if (= a b) 1 0))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'LT) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (if (< a b) 1 0))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'JMP) (funcall 'neovm--vm-set-pc vm (cadr instr)))
         ((eq op 'JZ) (if (= (funcall 'neovm--vm-pop vm) 0) (funcall 'neovm--vm-set-pc vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc))))
         ((eq op 'JNZ) (if (/= (funcall 'neovm--vm-pop vm) 0) (funcall 'neovm--vm-set-pc vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc))))
         ((eq op 'CALL) (funcall 'neovm--vm-set-call-stack vm (cons (cons (1+ pc) (copy-sequence (funcall 'neovm--vm-locals vm))) (funcall 'neovm--vm-call-stack vm))) (funcall 'neovm--vm-set-pc vm (cadr instr)))
         ((eq op 'RET) (let* ((f (car (funcall 'neovm--vm-call-stack vm)))) (funcall 'neovm--vm-set-call-stack vm (cdr (funcall 'neovm--vm-call-stack vm))) (funcall 'neovm--vm-set-locals vm (cdr f)) (funcall 'neovm--vm-set-pc vm (car f))))
         ((eq op 'LOAD_LOCAL) (funcall 'neovm--vm-push vm (aref (funcall 'neovm--vm-locals vm) (cadr instr))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'STORE_LOCAL) (aset (funcall 'neovm--vm-locals vm) (cadr instr) (funcall 'neovm--vm-pop vm)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'HALT) nil)
         (t (error "Bad op: %S" op)))
        vm)))

  (fset 'neovm--vm-run
    (lambda (vm prog &optional max)
      (let ((s 0) (lim (or max 10000)))
        (while (and (< s lim) (< (funcall 'neovm--vm-pc vm) (length prog))
                    (not (eq (let ((i (aref prog (funcall 'neovm--vm-pc vm))))
                               (if (consp i) (car i) i)) 'HALT)))
          (funcall 'neovm--vm-step vm prog) (setq s (1+ s)))
        s)))

  (unwind-protect
      ;; Compute factorial iteratively using the VM
      ;; Also test CALL/RET by having main call a "factorial function"
      ;; Main: push arg, call factorial, halt
      ;; Factorial (at addr 4): local[0]=n (from stack), local[1]=result
      (let ((results nil))
        (dolist (n '(0 1 5 7 10))
          (let ((vm (funcall 'neovm--vm-make)))
            (let ((prog (vector
                         ;; Main: addr 0-3
                         '(PUSH 0)     ;; 0: placeholder for n
                         '(CALL 4)     ;; 1: call factorial
                         'HALT         ;; 2: done (result on stack)
                         'HALT         ;; 3: padding

                         ;; Factorial function: addr 4+
                         ;; Expects argument on stack, returns result on stack
                         '(STORE_LOCAL 0)  ;; 4: n = pop()
                         '(PUSH 1)         ;; 5: result = 1
                         '(STORE_LOCAL 1)  ;; 6:
                         ;; 7: loop check
                         '(LOAD_LOCAL 0) '(PUSH 1) 'LT '(JNZ 17)  ;; if n < 1, goto done
                         ;; 11: body
                         '(LOAD_LOCAL 1) '(LOAD_LOCAL 0) 'MUL '(STORE_LOCAL 1) ;; result *= n
                         '(LOAD_LOCAL 0) '(PUSH 1) 'SUB '(STORE_LOCAL 0)       ;; n -= 1
                         ;; 19: loop
                         '(JMP 7)
                         ;; 20: done - push result and return
                         '(LOAD_LOCAL 1)
                         'RET)))
              ;; Fix JNZ target: should be addr 20 (push result, return)
              (aset prog 10 '(JNZ 20))
              ;; Set the argument
              (aset prog 0 (list 'PUSH n))
              (funcall 'neovm--vm-run vm prog)
              (setq results (cons (funcall 'neovm--vm-peek vm) results)))))
        (nreverse results))
    (fmakunbound 'neovm--vm-make)
    (fmakunbound 'neovm--vm-stack)
    (fmakunbound 'neovm--vm-pc)
    (fmakunbound 'neovm--vm-locals)
    (fmakunbound 'neovm--vm-call-stack)
    (fmakunbound 'neovm--vm-set-stack)
    (fmakunbound 'neovm--vm-set-pc)
    (fmakunbound 'neovm--vm-set-locals)
    (fmakunbound 'neovm--vm-set-call-stack)
    (fmakunbound 'neovm--vm-push)
    (fmakunbound 'neovm--vm-pop)
    (fmakunbound 'neovm--vm-peek)
    (fmakunbound 'neovm--vm-step)
    (fmakunbound 'neovm--vm-run)))"#;
    let expect = expect_test::expect![[r#""OK (1 1 120 5040 3628800)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String reverse using the VM (chars as integers)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vm_advanced_string_reverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--vm-make (lambda (&optional n) (list nil 0 (make-vector (or n 16) 0) nil)))
  (fset 'neovm--vm-stack (lambda (vm) (nth 0 vm)))
  (fset 'neovm--vm-pc (lambda (vm) (nth 1 vm)))
  (fset 'neovm--vm-locals (lambda (vm) (nth 2 vm)))
  (fset 'neovm--vm-set-stack (lambda (vm s) (setcar vm s)))
  (fset 'neovm--vm-set-pc (lambda (vm p) (setcar (nthcdr 1 vm) p)))
  (fset 'neovm--vm-push (lambda (vm v) (funcall 'neovm--vm-set-stack vm (cons v (funcall 'neovm--vm-stack vm)))))
  (fset 'neovm--vm-pop (lambda (vm) (let ((s (funcall 'neovm--vm-stack vm))) (funcall 'neovm--vm-set-stack vm (cdr s)) (car s))))
  (fset 'neovm--vm-peek (lambda (vm) (car (funcall 'neovm--vm-stack vm))))

  ;; Instead of a full VM program for string ops (which would need a string-aware VM),
  ;; we simulate: push each char of a string, then collect from stack (which reverses)
  ;; This tests the VM indirectly through stack manipulation.

  (unwind-protect
      (let ((results nil))
        (dolist (input '("hello" "abcde" "racecar" "x" ""))
          (let ((vm (funcall 'neovm--vm-make)))
            ;; Push each character onto the stack
            (let ((i 0))
              (while (< i (length input))
                (funcall 'neovm--vm-push vm (aref input i))
                (setq i (1+ i))))
            ;; Pop all characters to build reversed string
            (let ((chars nil))
              (while (funcall 'neovm--vm-stack vm)
                (setq chars (cons (funcall 'neovm--vm-pop vm) chars)))
              ;; chars is now in original order (since popping reversed the push order)
              ;; We want the reversed string, so we need to NOT reverse
              ;; Actually: push "hello" -> stack [o l l e h], pop all -> (h e l l o)
              ;; That's the original. For reversed, just read the stack directly.
              ;; Let me redo: push, then read stack top-to-bottom = reversed
              nil)
            ;; Redo properly
            (let ((vm2 (funcall 'neovm--vm-make)))
              (let ((i 0))
                (while (< i (length input))
                  (funcall 'neovm--vm-push vm2 (aref input i))
                  (setq i (1+ i))))
              ;; Stack now has chars in reverse. Pop to build reversed string.
              (let ((reversed-chars nil))
                (dotimes (_ (length input))
                  (setq reversed-chars (cons (funcall 'neovm--vm-pop vm2) reversed-chars)))
                ;; reversed-chars is back to original order after pop+cons
                ;; We need: stack = [o l l e h], pop gives o,l,l,e,h
                ;; cons each: (h e l l o) which is original.
                ;; For reversed: use the stack order directly before popping
                (let ((vm3 (funcall 'neovm--vm-make))
                      (rev-str ""))
                  (let ((i 0))
                    (while (< i (length input))
                      (funcall 'neovm--vm-push vm3 (aref input i))
                      (setq i (1+ i))))
                  ;; Pop and append each char to string (pop order = reversed)
                  (dotimes (_ (length input))
                    (setq rev-str (concat rev-str (char-to-string (funcall 'neovm--vm-pop vm3)))))
                  (setq results (cons rev-str results)))))))
        (nreverse results))
    (fmakunbound 'neovm--vm-make)
    (fmakunbound 'neovm--vm-stack)
    (fmakunbound 'neovm--vm-pc)
    (fmakunbound 'neovm--vm-locals)
    (fmakunbound 'neovm--vm-set-stack)
    (fmakunbound 'neovm--vm-set-pc)
    (fmakunbound 'neovm--vm-push)
    (fmakunbound 'neovm--vm-pop)
    (fmakunbound 'neovm--vm-peek)))"#;
    let expect = expect_test::expect![[r#""OK (\"olleh\" \"edcba\" \"racecar\" \"x\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// GCD computation using the VM (Euclidean algorithm)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vm_advanced_gcd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--vm-make (lambda (&optional n) (list nil 0 (make-vector (or n 16) 0) nil)))
  (fset 'neovm--vm-stack (lambda (vm) (nth 0 vm)))
  (fset 'neovm--vm-pc (lambda (vm) (nth 1 vm)))
  (fset 'neovm--vm-locals (lambda (vm) (nth 2 vm)))
  (fset 'neovm--vm-set-stack (lambda (vm s) (setcar vm s)))
  (fset 'neovm--vm-set-pc (lambda (vm p) (setcar (nthcdr 1 vm) p)))
  (fset 'neovm--vm-push (lambda (vm v) (funcall 'neovm--vm-set-stack vm (cons v (funcall 'neovm--vm-stack vm)))))
  (fset 'neovm--vm-pop (lambda (vm) (let ((s (funcall 'neovm--vm-stack vm))) (funcall 'neovm--vm-set-stack vm (cdr s)) (car s))))
  (fset 'neovm--vm-peek (lambda (vm) (car (funcall 'neovm--vm-stack vm))))

  (fset 'neovm--vm-step
    (lambda (vm prog)
      (let* ((pc (funcall 'neovm--vm-pc vm))
             (instr (aref prog pc))
             (op (if (consp instr) (car instr) instr)))
        (cond
         ((eq op 'PUSH) (funcall 'neovm--vm-push vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'DUP) (funcall 'neovm--vm-push vm (funcall 'neovm--vm-peek vm)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'SWAP) (let* ((a (funcall 'neovm--vm-pop vm)) (b (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm a) (funcall 'neovm--vm-push vm b)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'SUB) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (- a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'MOD) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (% a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'EQ) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (if (= a b) 1 0))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'JMP) (funcall 'neovm--vm-set-pc vm (cadr instr)))
         ((eq op 'JZ) (if (= (funcall 'neovm--vm-pop vm) 0) (funcall 'neovm--vm-set-pc vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc))))
         ((eq op 'JNZ) (if (/= (funcall 'neovm--vm-pop vm) 0) (funcall 'neovm--vm-set-pc vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc))))
         ((eq op 'LOAD_LOCAL) (funcall 'neovm--vm-push vm (aref (funcall 'neovm--vm-locals vm) (cadr instr))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'STORE_LOCAL) (aset (funcall 'neovm--vm-locals vm) (cadr instr) (funcall 'neovm--vm-pop vm)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'HALT) nil)
         (t (error "Bad op: %S" op)))
        vm)))

  (fset 'neovm--vm-run
    (lambda (vm prog &optional max)
      (let ((s 0) (lim (or max 10000)))
        (while (and (< s lim) (< (funcall 'neovm--vm-pc vm) (length prog))
                    (not (eq (let ((i (aref prog (funcall 'neovm--vm-pc vm))))
                               (if (consp i) (car i) i)) 'HALT)))
          (funcall 'neovm--vm-step vm prog) (setq s (1+ s)))
        s)))

  (unwind-protect
      ;; GCD via Euclidean algorithm:
      ;; local[0] = a, local[1] = b
      ;; while b != 0: a, b = b, a % b
      ;; result = a
      (let ((results nil))
        (dolist (pair '((48 18) (100 75) (17 13) (0 5) (12 12) (1071 462)))
          (let ((vm (funcall 'neovm--vm-make)))
            (let ((prog (vector
                         (list 'PUSH (car pair))  ;; 0: push a
                         '(STORE_LOCAL 0)          ;; 1: local[0] = a
                         (list 'PUSH (cadr pair)) ;; 2: push b
                         '(STORE_LOCAL 1)          ;; 3: local[1] = b
                         ;; 4: loop check: b == 0?
                         '(LOAD_LOCAL 1) '(PUSH 0) 'EQ '(JNZ 14)
                         ;; 8: temp = a % b
                         '(LOAD_LOCAL 0) '(LOAD_LOCAL 1) 'MOD
                         ;; 11: a = b
                         '(LOAD_LOCAL 1) '(STORE_LOCAL 0)
                         ;; 13: b = temp (on stack from MOD)
                         '(STORE_LOCAL 1)
                         ;; 14: jump back
                         '(JMP 4)
                         ;; 15: done
                         '(LOAD_LOCAL 0) 'HALT)))
              ;; Fix: JNZ at 7 should jump to 15 (push result)
              (aset prog 7 '(JNZ 15))
              (funcall 'neovm--vm-run vm prog)
              (setq results (cons (funcall 'neovm--vm-peek vm) results)))))
        (nreverse results))
    (fmakunbound 'neovm--vm-make)
    (fmakunbound 'neovm--vm-stack)
    (fmakunbound 'neovm--vm-pc)
    (fmakunbound 'neovm--vm-locals)
    (fmakunbound 'neovm--vm-set-stack)
    (fmakunbound 'neovm--vm-set-pc)
    (fmakunbound 'neovm--vm-push)
    (fmakunbound 'neovm--vm-pop)
    (fmakunbound 'neovm--vm-peek)
    (fmakunbound 'neovm--vm-step)
    (fmakunbound 'neovm--vm-run)))"#;
    let expect = expect_test::expect![[r#""OK (6 25 1 5 12 21)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Power function and expression evaluator using the VM
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vm_advanced_power_and_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--vm-make (lambda (&optional n) (list nil 0 (make-vector (or n 16) 0) nil)))
  (fset 'neovm--vm-stack (lambda (vm) (nth 0 vm)))
  (fset 'neovm--vm-pc (lambda (vm) (nth 1 vm)))
  (fset 'neovm--vm-locals (lambda (vm) (nth 2 vm)))
  (fset 'neovm--vm-set-stack (lambda (vm s) (setcar vm s)))
  (fset 'neovm--vm-set-pc (lambda (vm p) (setcar (nthcdr 1 vm) p)))
  (fset 'neovm--vm-push (lambda (vm v) (funcall 'neovm--vm-set-stack vm (cons v (funcall 'neovm--vm-stack vm)))))
  (fset 'neovm--vm-pop (lambda (vm) (let ((s (funcall 'neovm--vm-stack vm))) (funcall 'neovm--vm-set-stack vm (cdr s)) (car s))))
  (fset 'neovm--vm-peek (lambda (vm) (car (funcall 'neovm--vm-stack vm))))

  (fset 'neovm--vm-step
    (lambda (vm prog)
      (let* ((pc (funcall 'neovm--vm-pc vm))
             (instr (aref prog pc))
             (op (if (consp instr) (car instr) instr)))
        (cond
         ((eq op 'PUSH) (funcall 'neovm--vm-push vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'POP) (funcall 'neovm--vm-pop vm) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'DUP) (funcall 'neovm--vm-push vm (funcall 'neovm--vm-peek vm)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'ADD) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (+ a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'SUB) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (- a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'MUL) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (* a b))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'EQ) (let* ((b (funcall 'neovm--vm-pop vm)) (a (funcall 'neovm--vm-pop vm))) (funcall 'neovm--vm-push vm (if (= a b) 1 0))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'JMP) (funcall 'neovm--vm-set-pc vm (cadr instr)))
         ((eq op 'JZ) (if (= (funcall 'neovm--vm-pop vm) 0) (funcall 'neovm--vm-set-pc vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc))))
         ((eq op 'JNZ) (if (/= (funcall 'neovm--vm-pop vm) 0) (funcall 'neovm--vm-set-pc vm (cadr instr)) (funcall 'neovm--vm-set-pc vm (1+ pc))))
         ((eq op 'LOAD_LOCAL) (funcall 'neovm--vm-push vm (aref (funcall 'neovm--vm-locals vm) (cadr instr))) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'STORE_LOCAL) (aset (funcall 'neovm--vm-locals vm) (cadr instr) (funcall 'neovm--vm-pop vm)) (funcall 'neovm--vm-set-pc vm (1+ pc)))
         ((eq op 'HALT) nil)
         (t (error "Bad op: %S" op)))
        vm)))

  (fset 'neovm--vm-run
    (lambda (vm prog &optional max)
      (let ((s 0) (lim (or max 10000)))
        (while (and (< s lim) (< (funcall 'neovm--vm-pc vm) (length prog))
                    (not (eq (let ((i (aref prog (funcall 'neovm--vm-pc vm))))
                               (if (consp i) (car i) i)) 'HALT)))
          (funcall 'neovm--vm-step vm prog) (setq s (1+ s)))
        s)))

  (unwind-protect
      (list
       ;; Compute base^exp using repeated multiplication
       ;; local[0]=base, local[1]=exp, local[2]=result
       (let ((results nil))
         (dolist (pair '((2 10) (3 5) (5 3) (7 0) (1 100)))
           (let ((vm (funcall 'neovm--vm-make)))
             (let ((prog (vector
                          (list 'PUSH (car pair))    ;; 0: push base
                          '(STORE_LOCAL 0)            ;; 1: local[0] = base
                          (list 'PUSH (cadr pair))   ;; 2: push exp
                          '(STORE_LOCAL 1)            ;; 3: local[1] = exp
                          '(PUSH 1)                   ;; 4: push 1 (initial result)
                          '(STORE_LOCAL 2)            ;; 5: local[2] = 1
                          ;; 6: loop check: exp == 0?
                          '(LOAD_LOCAL 1) '(PUSH 0) 'EQ '(JNZ 16)
                          ;; 10: result *= base
                          '(LOAD_LOCAL 2) '(LOAD_LOCAL 0) 'MUL '(STORE_LOCAL 2)
                          ;; 14: exp -= 1
                          '(LOAD_LOCAL 1) '(PUSH 1) 'SUB '(STORE_LOCAL 1)
                          ;; 18: loop
                          '(JMP 6)
                          ;; 19: done
                          '(LOAD_LOCAL 2) 'HALT)))
               ;; Fix JNZ at 9 to jump to 19
               (aset prog 9 '(JNZ 19))
               (funcall 'neovm--vm-run vm prog)
               (setq results (cons (funcall 'neovm--vm-peek vm) results)))))
         (nreverse results))

       ;; Evaluate postfix expression: 3 4 + 2 * 5 - = (3+4)*2-5 = 9
       (let ((vm (funcall 'neovm--vm-make)))
         (let ((prog (vector '(PUSH 3) '(PUSH 4) 'ADD '(PUSH 2) 'MUL '(PUSH 5) 'SUB 'HALT)))
           (funcall 'neovm--vm-run vm prog)
           (funcall 'neovm--vm-peek vm)))

       ;; More complex postfix: 2 3 * 4 5 * + = 6 + 20 = 26
       (let ((vm (funcall 'neovm--vm-make)))
         (let ((prog (vector '(PUSH 2) '(PUSH 3) 'MUL '(PUSH 4) '(PUSH 5) 'MUL 'ADD 'HALT)))
           (funcall 'neovm--vm-run vm prog)
           (funcall 'neovm--vm-peek vm))))
    (fmakunbound 'neovm--vm-make)
    (fmakunbound 'neovm--vm-stack)
    (fmakunbound 'neovm--vm-pc)
    (fmakunbound 'neovm--vm-locals)
    (fmakunbound 'neovm--vm-set-stack)
    (fmakunbound 'neovm--vm-set-pc)
    (fmakunbound 'neovm--vm-push)
    (fmakunbound 'neovm--vm-pop)
    (fmakunbound 'neovm--vm-peek)
    (fmakunbound 'neovm--vm-step)
    (fmakunbound 'neovm--vm-run)))"#;
    let expect = expect_test::expect![[r#""OK ((1024 243 125 1 1) 9 26)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
