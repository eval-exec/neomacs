//! Oracle parity tests for abstract machine implementations in Elisp.
//!
//! Tests a register machine, SECD machine, abstract stack machine with
//! local variables, simple Turing machine simulation, and CEK machine.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Register machine: load, store, add, sub, mul, jump, branch-if-zero
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_machine_register() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Register machine with 8 registers (vector), a program (list of instructions),
  ;; and a program counter. Instructions are lists: (op . args)
  ;; Ops: (load reg val) (store reg-dst reg-src) (add dst a b) (sub dst a b)
  ;;       (mul dst a b) (jump addr) (bz reg addr) (halt)
  (fset 'neovm--test-rm-run
    (lambda (program)
      (let ((regs (make-vector 8 0))
            (pc 0)
            (halted nil)
            (steps 0)
            (max-steps 500))
        (while (and (not halted) (< pc (length program)) (< steps max-steps))
          (let* ((instr (nth pc program))
                 (op (car instr)))
            (setq steps (1+ steps))
            (cond
              ((eq op 'load)
               (aset regs (nth 1 instr) (nth 2 instr))
               (setq pc (1+ pc)))
              ((eq op 'store)
               (aset regs (nth 1 instr) (aref regs (nth 2 instr)))
               (setq pc (1+ pc)))
              ((eq op 'add)
               (aset regs (nth 1 instr)
                     (+ (aref regs (nth 2 instr))
                        (aref regs (nth 3 instr))))
               (setq pc (1+ pc)))
              ((eq op 'sub)
               (aset regs (nth 1 instr)
                     (- (aref regs (nth 2 instr))
                        (aref regs (nth 3 instr))))
               (setq pc (1+ pc)))
              ((eq op 'mul)
               (aset regs (nth 1 instr)
                     (* (aref regs (nth 2 instr))
                        (aref regs (nth 3 instr))))
               (setq pc (1+ pc)))
              ((eq op 'jump)
               (setq pc (nth 1 instr)))
              ((eq op 'bz)
               (if (= (aref regs (nth 1 instr)) 0)
                   (setq pc (nth 2 instr))
                 (setq pc (1+ pc))))
              ((eq op 'halt)
               (setq halted t))
              (t (setq halted t)))))
        (list (append regs nil) steps halted))))

  (unwind-protect
      (let* (;; Program 1: compute factorial of 5 iteratively
             ;; R0 = n (input), R1 = accumulator, R2 = 1 (constant), R3 = 0 (constant)
             (factorial-prog
              '((load 0 5)       ; R0 = 5 (n)
                (load 1 1)       ; R1 = 1 (acc)
                (load 2 1)       ; R2 = 1
                (load 3 0)       ; R3 = 0
                (bz 0 8)         ; if R0 == 0, jump to halt (addr 8)
                (mul 1 1 0)      ; R1 = R1 * R0
                (sub 0 0 2)      ; R0 = R0 - 1
                (jump 4)         ; jump back to check
                (halt)))
             (r1 (funcall 'neovm--test-rm-run factorial-prog))
             ;; Program 2: compute fibonacci(8) using R0=a, R1=b, R2=counter, R3=temp
             (fib-prog
              '((load 0 0)       ; R0 = 0 (a)
                (load 1 1)       ; R1 = 1 (b)
                (load 2 8)       ; R2 = 8 (counter)
                (load 3 0)       ; R3 = temp
                (load 4 1)       ; R4 = 1 (decrement constant)
                (load 5 0)       ; R5 = 0 (zero constant)
                (bz 2 12)        ; if counter == 0, halt
                (store 3 1)      ; temp = b
                (add 1 0 1)      ; b = a + b
                (store 0 3)      ; a = temp
                (sub 2 2 4)      ; counter--
                (jump 6)         ; loop
                (halt)))
             (r2 (funcall 'neovm--test-rm-run fib-prog))
             ;; Program 3: sum 1..10 using R0=counter(10), R1=sum, R2=1, R3=0
             (sum-prog
              '((load 0 10)      ; R0 = 10
                (load 1 0)       ; R1 = 0 (sum)
                (load 2 1)       ; R2 = 1
                (load 3 0)       ; R3 = 0
                (bz 0 8)         ; if counter == 0, halt
                (add 1 1 0)      ; sum += counter
                (sub 0 0 2)      ; counter--
                (jump 4)         ; loop
                (halt)))
             (r3 (funcall 'neovm--test-rm-run sum-prog)))
        (list r1 r2 r3))
    (fmakunbound 'neovm--test-rm-run)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 120 1 0 0 0 0 0) 26 t) ((21 34 0 21 1 0 0 0) 56 t) ((0 55 1 0 0 0 0 0) 46 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SECD machine (Stack, Environment, Control, Dump)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_machine_secd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Minimal SECD machine for arithmetic expressions.
  ;; Control instructions: (ldc val) (ld idx) (add) (sub) (mul)
  ;;   (sel then-code else-code) (join) (ldf code) (ap) (rtn) (stop)
  ;; Environment is a list of frames (each frame a list of values).
  (fset 'neovm--test-secd-run
    (lambda (code initial-env)
      (let ((stack nil)
            (env initial-env)
            (ctrl code)
            (dump nil)
            (halted nil)
            (steps 0))
        (while (and (not halted) ctrl (< steps 300))
          (let* ((instr (car ctrl))
                 (op (car instr)))
            (setq ctrl (cdr ctrl))
            (setq steps (1+ steps))
            (cond
              ;; Load constant
              ((eq op 'ldc)
               (setq stack (cons (cadr instr) stack)))
              ;; Load from environment: (ld frame-idx . val-idx)
              ((eq op 'ld)
               (let* ((fi (cadr instr))
                      (vi (cddr instr))
                      (frame (nth fi env))
                      (val (nth vi frame)))
                 (setq stack (cons val stack))))
              ;; Arithmetic: pop two, push result
              ((eq op 'add)
               (let ((b (car stack)) (a (cadr stack)))
                 (setq stack (cons (+ a b) (cddr stack)))))
              ((eq op 'sub)
               (let ((b (car stack)) (a (cadr stack)))
                 (setq stack (cons (- a b) (cddr stack)))))
              ((eq op 'mul)
               (let ((b (car stack)) (a (cadr stack)))
                 (setq stack (cons (* a b) (cddr stack)))))
              ;; Conditional: pop stack, select then/else branch
              ((eq op 'sel)
               (let ((val (car stack))
                     (then-code (cadr instr))
                     (else-code (caddr instr)))
                 (setq stack (cdr stack))
                 (setq dump (cons ctrl dump))
                 (if (not (eq val 0))
                     (setq ctrl then-code)
                   (setq ctrl else-code))))
              ;; Join: restore ctrl from dump
              ((eq op 'join)
               (setq ctrl (car dump))
               (setq dump (cdr dump)))
              ;; Make closure: (ldf code) -> push (closure code env)
              ((eq op 'ldf)
               (setq stack (cons (list 'closure (cadr instr) env) stack)))
              ;; Apply: pop closure and arg-list, extend env
              ((eq op 'ap)
               (let* ((closure (car stack))
                      (args (cadr stack))
                      (c-code (cadr closure))
                      (c-env (caddr closure)))
                 (setq dump (cons (list (cddr stack) env ctrl) dump))
                 (setq stack nil)
                 (setq env (cons args c-env))
                 (setq ctrl c-code)))
              ;; Return
              ((eq op 'rtn)
               (let ((saved (car dump)))
                 (setq dump (cdr dump))
                 (setq stack (cons (car stack) (nth 0 saved)))
                 (setq env (nth 1 saved))
                 (setq ctrl (nth 2 saved))))
              ;; Stop
              ((eq op 'stop)
               (setq halted t))
              (t (setq halted t)))))
        (list (car stack) steps))))

  (unwind-protect
      (let* (;; Program 1: (3 + 4) * (10 - 2)
             (prog1 '((ldc 3) (ldc 4) (add) (ldc 10) (ldc 2) (sub) (mul) (stop)))
             (r1 (funcall 'neovm--test-secd-run prog1 nil))
             ;; Program 2: conditional — if 1 then (5+5) else (2*3)
             (prog2 '((ldc 1)
                       (sel ((ldc 5) (ldc 5) (add) (join))
                            ((ldc 2) (ldc 3) (mul) (join)))
                       (stop)))
             (r2 (funcall 'neovm--test-secd-run prog2 nil))
             ;; Program 3: conditional with false — if 0 then (5+5) else (2*3)
             (prog3 '((ldc 0)
                       (sel ((ldc 5) (ldc 5) (add) (join))
                            ((ldc 2) (ldc 3) (mul) (join)))
                       (stop)))
             (r3 (funcall 'neovm--test-secd-run prog3 nil))
             ;; Program 4: load from environment, compute x*x+y where env=((3 7))
             (prog4 '((ld 0 . 0) (ld 0 . 0) (mul) (ld 0 . 1) (add) (stop)))
             (r4 (funcall 'neovm--test-secd-run prog4 '((3 7)))))
        (list r1 r2 r3 r4))
    (fmakunbound 'neovm--test-secd-run)))"#;
    let expect = expect_test::expect![[r#""OK ((56 8) (10 7) (6 7) (16 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Abstract stack machine with local variables and subroutines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_machine_stack_with_locals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Stack machine with local variable slots and call/ret for subroutines.
  ;; Instructions: (push val) (pop) (dup) (swap) (add) (sub) (mul) (mod)
  ;;   (neg) (load-local idx) (store-local idx) (call addr) (ret)
  ;;   (jz addr) (jmp addr) (cmp-lt) (halt)
  ;; cmp-lt: pop b, a; push 1 if a<b else 0
  (fset 'neovm--test-sm-run
    (lambda (program num-locals)
      (let ((stack nil)
            (locals (make-vector num-locals 0))
            (call-stack nil)
            (pc 0)
            (halted nil)
            (steps 0))
        (while (and (not halted) (< pc (length program)) (< steps 500))
          (let* ((instr (nth pc program))
                 (op (car instr)))
            (setq steps (1+ steps))
            (cond
              ((eq op 'push)
               (setq stack (cons (cadr instr) stack))
               (setq pc (1+ pc)))
              ((eq op 'pop)
               (setq stack (cdr stack))
               (setq pc (1+ pc)))
              ((eq op 'dup)
               (setq stack (cons (car stack) stack))
               (setq pc (1+ pc)))
              ((eq op 'swap)
               (let ((a (car stack)) (b (cadr stack)))
                 (setq stack (cons b (cons a (cddr stack)))))
               (setq pc (1+ pc)))
              ((eq op 'add)
               (let ((b (car stack)) (a (cadr stack)))
                 (setq stack (cons (+ a b) (cddr stack))))
               (setq pc (1+ pc)))
              ((eq op 'sub)
               (let ((b (car stack)) (a (cadr stack)))
                 (setq stack (cons (- a b) (cddr stack))))
               (setq pc (1+ pc)))
              ((eq op 'mul)
               (let ((b (car stack)) (a (cadr stack)))
                 (setq stack (cons (* a b) (cddr stack))))
               (setq pc (1+ pc)))
              ((eq op 'mod)
               (let ((b (car stack)) (a (cadr stack)))
                 (setq stack (cons (% a b) (cddr stack))))
               (setq pc (1+ pc)))
              ((eq op 'neg)
               (setq stack (cons (- (car stack)) (cdr stack)))
               (setq pc (1+ pc)))
              ((eq op 'load-local)
               (setq stack (cons (aref locals (cadr instr)) stack))
               (setq pc (1+ pc)))
              ((eq op 'store-local)
               (aset locals (cadr instr) (car stack))
               (setq stack (cdr stack))
               (setq pc (1+ pc)))
              ((eq op 'call)
               (setq call-stack (cons (1+ pc) call-stack))
               (setq pc (cadr instr)))
              ((eq op 'ret)
               (setq pc (car call-stack))
               (setq call-stack (cdr call-stack)))
              ((eq op 'jz)
               (let ((v (car stack)))
                 (setq stack (cdr stack))
                 (if (= v 0)
                     (setq pc (cadr instr))
                   (setq pc (1+ pc)))))
              ((eq op 'jmp)
               (setq pc (cadr instr)))
              ((eq op 'cmp-lt)
               (let ((b (car stack)) (a (cadr stack)))
                 (setq stack (cons (if (< a b) 1 0) (cddr stack))))
               (setq pc (1+ pc)))
              ((eq op 'halt)
               (setq halted t))
              (t (setq halted t)))))
        (list stack (append locals nil) steps))))

  (unwind-protect
      (let* (;; Program 1: compute GCD(48, 18) using Euclidean algorithm
             ;; local0 = a, local1 = b
             (gcd-prog
              '((push 48) (store-local 0)    ; a = 48
                (push 18) (store-local 1)    ; b = 18
                ;; loop: if b == 0, done
                (load-local 1) (jz 14)       ; if b==0 goto end
                ;; a, b = b, a % b
                (load-local 0)               ; push a
                (load-local 1)               ; push b
                (mod)                         ; a % b
                (load-local 1)               ; push old b
                (store-local 0)              ; a = old b
                (swap) (pop)                 ; clean stack
                (store-local 1)              ; b = a%b
                (jmp 4)                      ; loop
                (load-local 0) (halt)))      ; result in local0
             (r1 (funcall 'neovm--test-sm-run gcd-prog 4))
             ;; Program 2: compute sum of squares 1^2+2^2+...+5^2
             ;; local0=counter, local1=limit, local2=sum
             (sumsq-prog
              '((push 1) (store-local 0)     ; counter = 1
                (push 5) (store-local 1)     ; limit = 5
                (push 0) (store-local 2)     ; sum = 0
                ;; loop:
                (load-local 0) (load-local 1) (cmp-lt) ; counter < limit?
                (push 1) (sub) (neg)         ; negate (0->0, -1->1 but we need <=)
                ;; Simpler: check counter > limit
                (load-local 0) (load-local 1) (sub) ; counter - limit
                (push 1) (cmp-lt)            ; (counter-limit) < 1, i.e., counter<=limit
                (jz 27)                      ; if not, done
                ;; sum += counter * counter
                (load-local 0) (dup) (mul)   ; counter^2
                (load-local 2) (add)         ; sum + counter^2
                (store-local 2)              ; save sum
                ;; counter++
                (load-local 0) (push 1) (add)
                (store-local 0)
                (jmp 12)                     ; loop back to comparison
                (load-local 2) (halt)))
             (r2 (funcall 'neovm--test-sm-run sumsq-prog 4)))
        (list r1 r2))
    (fmakunbound 'neovm--test-sm-run)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple Turing machine simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_machine_turing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Turing machine: tape (alist of pos->symbol), head position, state,
  ;; transition table (alist of (state . symbol) -> (new-symbol new-dir new-state))
  ;; Direction: L or R. Halt when no transition found.
  (fset 'neovm--test-tm-read
    (lambda (tape pos blank)
      (let ((cell (assq pos tape)))
        (if cell (cdr cell) blank))))

  (fset 'neovm--test-tm-write
    (lambda (tape pos sym)
      (let ((cell (assq pos tape)))
        (if cell
            (progn (setcdr cell sym) tape)
          (cons (cons pos sym) tape)))))

  (fset 'neovm--test-tm-lookup
    (lambda (table state sym)
      (let ((key (cons state sym))
            (result nil))
        (dolist (entry table)
          (when (and (equal (car (car entry)) state)
                     (equal (cdr (car entry)) sym))
            (setq result (cdr entry))))
        result)))

  (fset 'neovm--test-tm-run
    (lambda (table initial-tape initial-state blank)
      (let ((tape initial-tape)
            (pos 0)
            (state initial-state)
            (steps 0)
            (max-steps 200)
            (halted nil))
        (while (and (not halted) (< steps max-steps))
          (let* ((sym (funcall 'neovm--test-tm-read tape pos blank))
                 (trans (funcall 'neovm--test-tm-lookup table state sym)))
            (if (not trans)
                (setq halted t)
              (let ((new-sym (nth 0 trans))
                    (dir     (nth 1 trans))
                    (new-st  (nth 2 trans)))
                (setq tape (funcall 'neovm--test-tm-write tape pos new-sym))
                (setq pos (if (eq dir 'R) (1+ pos) (1- pos)))
                (setq state new-st)
                (setq steps (1+ steps))))))
        ;; Extract tape contents from min to max position
        (let ((min-p 0) (max-p 0))
          (dolist (cell tape)
            (when (< (car cell) min-p) (setq min-p (car cell)))
            (when (> (car cell) max-p) (setq max-p (car cell))))
          (let ((contents nil) (p max-p))
            (while (>= p min-p)
              (setq contents
                    (cons (funcall 'neovm--test-tm-read tape p blank) contents))
              (setq p (1- p)))
            (list state pos steps contents))))))

  (unwind-protect
      (let* (;; TM 1: Unary increment — add 1 to a unary number (string of 1s)
             ;; States: q0 (scan right), q1 (write and halt)
             ;; Input: tape has 1s at positions 0,1,2 (representing 3)
             ;; Output: 1s at positions 0,1,2,3 (representing 4)
             (inc-table
              '((((q0 . 1) . (1 R q0))      ; scanning 1, move right
                ((q0 . 0) . (1 R q-halt))))) ; found blank, write 1, halt
             (inc-tape '((0 . 1) (1 . 1) (2 . 1)))
             (r1 (funcall 'neovm--test-tm-run inc-table inc-tape 'q0 0))

             ;; TM 2: Binary NOT — flip 0s and 1s, stop at blank (2)
             ;; Input: binary 1011 at positions 0-3
             (not-table
              '((((q0 . 0) . (1 R q0))
                ((q0 . 1) . (0 R q0))
                ((q0 . 2) . (2 R q-halt)))))
             (not-tape '((0 . 1) (1 . 0) (2 . 1) (3 . 1)))
             (r2 (funcall 'neovm--test-tm-run not-table not-tape 'q0 2))

             ;; TM 3: Move to leftmost 1 — scans right then comes back left
             ;; States: q-right (move right until blank), q-left (move left until blank)
             (bounce-table
              '((((q-right . 1) . (1 R q-right))
                ((q-right . 0) . (0 L q-left))
                ((q-left . 1) . (1 L q-left))
                ((q-left . 0) . (0 R q-done)))))
             (bounce-tape '((0 . 1) (1 . 1) (2 . 1) (3 . 1) (4 . 1)))
             (r3 (funcall 'neovm--test-tm-run bounce-table bounce-tape 'q-right 0)))
        (list r1 r2 r3))
    (fmakunbound 'neovm--test-tm-read)
    (fmakunbound 'neovm--test-tm-write)
    (fmakunbound 'neovm--test-tm-lookup)
    (fmakunbound 'neovm--test-tm-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((q0 0 0 (1 1 1)) (q0 0 0 (1 0 1 1)) (q-right 0 0 (1 1 1 1 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CEK machine (Control, Environment, Continuation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_machine_cek() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; CEK machine for a tiny lambda calculus with integers and arithmetic.
  ;; Expressions: integer | symbol (var) | (lam var body) | (app fn arg)
  ;;              | (plus e1 e2) | (times e1 e2) | (ifz cond then else)
  ;; Environment: alist of (var . value)
  ;; Continuations: (mt) | (arg-k expr env cont) | (fn-k closure cont)
  ;;               | (plus-r-k expr env cont) | (plus-l-k val cont)
  ;;               | (times-r-k expr env cont) | (times-l-k val cont)
  ;;               | (ifz-k then else env cont)

  (fset 'neovm--test-cek-lookup
    (lambda (env var)
      (let ((pair (assq var env)))
        (if pair (cdr pair)
          (error "Unbound variable: %s" var)))))

  (fset 'neovm--test-cek-step
    (lambda (state)
      (let ((ctrl (nth 0 state))
            (env  (nth 1 state))
            (cont (nth 2 state)))
        (cond
          ;; Integer literal — apply continuation
          ((integerp ctrl)
           (list 'apply-cont cont ctrl))
          ;; Variable lookup
          ((symbolp ctrl)
           (list 'apply-cont cont (funcall 'neovm--test-cek-lookup env ctrl)))
          ;; Lambda abstraction — create closure
          ((eq (car ctrl) 'lam)
           (let ((var (cadr ctrl))
                 (body (caddr ctrl)))
             (list 'apply-cont cont (list 'closure var body env))))
          ;; Application
          ((eq (car ctrl) 'app)
           (let ((fn-expr (cadr ctrl))
                 (arg-expr (caddr ctrl)))
             (list 'eval fn-expr env (list 'arg-k arg-expr env cont))))
          ;; Plus
          ((eq (car ctrl) 'plus)
           (list 'eval (cadr ctrl) env
                 (list 'plus-r-k (caddr ctrl) env cont)))
          ;; Times
          ((eq (car ctrl) 'times)
           (list 'eval (cadr ctrl) env
                 (list 'times-r-k (caddr ctrl) env cont)))
          ;; If-zero
          ((eq (car ctrl) 'ifz)
           (list 'eval (cadr ctrl) env
                 (list 'ifz-k (caddr ctrl) (cadddr ctrl) env cont)))
          (t (list 'error "Unknown expression" ctrl))))))

  (fset 'neovm--test-cek-apply-cont
    (lambda (cont val)
      (cond
        ((eq (car cont) 'mt) (list 'done val))
        ((eq (car cont) 'arg-k)
         (let ((arg-expr (cadr cont))
               (env (caddr cont))
               (k (cadddr cont)))
           (list 'eval arg-expr env (list 'fn-k val k))))
        ((eq (car cont) 'fn-k)
         (let* ((closure (cadr cont))
                (k (caddr cont))
                (var (cadr closure))
                (body (caddr closure))
                (c-env (cadddr closure)))
           (list 'eval body (cons (cons var val) c-env) k)))
        ((eq (car cont) 'plus-r-k)
         (let ((r-expr (cadr cont))
               (env (caddr cont))
               (k (cadddr cont)))
           (list 'eval r-expr env (list 'plus-l-k val k))))
        ((eq (car cont) 'plus-l-k)
         (list 'apply-cont (caddr cont) (+ (cadr cont) val)))
        ((eq (car cont) 'times-r-k)
         (let ((r-expr (cadr cont))
               (env (caddr cont))
               (k (cadddr cont)))
           (list 'eval r-expr env (list 'times-l-k val k))))
        ((eq (car cont) 'times-l-k)
         (list 'apply-cont (caddr cont) (* (cadr cont) val)))
        ((eq (car cont) 'ifz-k)
         (let ((then-e (cadr cont))
               (else-e (caddr cont))
               (env (cadddr cont))
               (k (car (cddddr cont))))
           (if (= val 0)
               (list 'eval then-e env k)
             (list 'eval else-e env k))))
        (t (list 'error "Unknown continuation" cont)))))

  (fset 'neovm--test-cek-eval
    (lambda (expr)
      (let ((state (list 'eval expr nil '(mt)))
            (steps 0)
            (max-steps 500)
            (done nil)
            (result nil))
        (while (and (not done) (< steps max-steps))
          (setq steps (1+ steps))
          (let ((kind (car state)))
            (cond
              ((eq kind 'eval)
               (setq state (funcall 'neovm--test-cek-step
                                    (list (cadr state)
                                          (caddr state)
                                          (cadddr state)))))
              ((eq kind 'apply-cont)
               (setq state (funcall 'neovm--test-cek-apply-cont
                                    (cadr state) (caddr state))))
              ((eq kind 'done)
               (setq done t)
               (setq result (cadr state)))
              ((eq kind 'error)
               (setq done t)
               (setq result (list 'error (cdr state))))
              (t (setq done t)
                 (setq result (list 'stuck state))))))
        (list result steps))))

  (unwind-protect
      (let* (;; 1: Simple constant
             (r1 (funcall 'neovm--test-cek-eval 42))
             ;; 2: (plus 3 4) = 7
             (r2 (funcall 'neovm--test-cek-eval '(plus 3 4)))
             ;; 3: (times (plus 2 3) (plus 4 1)) = 25
             (r3 (funcall 'neovm--test-cek-eval
                          '(times (plus 2 3) (plus 4 1))))
             ;; 4: ((lam x (plus x x)) 5) = 10
             (r4 (funcall 'neovm--test-cek-eval
                          '(app (lam x (plus x x)) 5)))
             ;; 5: ((lam x (times x (plus x 1))) 7) = 56
             (r5 (funcall 'neovm--test-cek-eval
                          '(app (lam x (times x (plus x 1))) 7)))
             ;; 6: ifz — (ifz 0 42 99) = 42
             (r6 (funcall 'neovm--test-cek-eval '(ifz 0 42 99)))
             ;; 7: ifz — (ifz 1 42 99) = 99
             (r7 (funcall 'neovm--test-cek-eval '(ifz 1 42 99)))
             ;; 8: Nested lambdas (currying): ((lam x (lam y (plus x y))) 3) applied to 4
             (r8 (funcall 'neovm--test-cek-eval
                          '(app (app (lam x (lam y (plus x y))) 3) 4))))
        (list r1 r2 r3 r4 r5 r6 r7 r8))
    (fmakunbound 'neovm--test-cek-lookup)
    (fmakunbound 'neovm--test-cek-step)
    (fmakunbound 'neovm--test-cek-apply-cont)
    (fmakunbound 'neovm--test-cek-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK ((42 3) (7 7) (25 15) (10 12) (56 16) (42 6) (99 6) (7 17))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combined: compile arithmetic expression to register machine, then run it
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_machine_compiler_to_register() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Compile simple arithmetic expressions to register machine instructions.
  ;; Expressions: integer | (+ e1 e2) | (* e1 e2) | (- e1 e2)
  ;; Compilation target: register machine with registers R0..R7
  ;; Strategy: allocate registers linearly, compile left to reg, right to reg+1,
  ;; then emit op writing result into target reg.

  (fset 'neovm--test-comp-compile
    (lambda (expr target-reg)
      (cond
        ((integerp expr)
         (list (list 'load target-reg expr)))
        ((memq (car expr) '(+ * -))
         (let* ((op (car expr))
                (left (cadr expr))
                (right (caddr expr))
                (left-reg target-reg)
                (right-reg (1+ target-reg))
                (left-code (funcall 'neovm--test-comp-compile left left-reg))
                (right-code (funcall 'neovm--test-comp-compile right right-reg))
                (rm-op (cond ((eq op '+) 'add)
                             ((eq op '*) 'mul)
                             ((eq op '-) 'sub))))
           (append left-code right-code
                   (list (list rm-op target-reg left-reg right-reg)))))
        (t (list (list 'load target-reg 0))))))

  ;; Reuse register machine runner
  (fset 'neovm--test-comp-rm-run
    (lambda (program)
      (let ((regs (make-vector 8 0))
            (pc 0)
            (halted nil)
            (steps 0))
        (while (and (not halted) (< pc (length program)) (< steps 300))
          (let* ((instr (nth pc program))
                 (op (car instr)))
            (setq steps (1+ steps))
            (cond
              ((eq op 'load)
               (aset regs (nth 1 instr) (nth 2 instr))
               (setq pc (1+ pc)))
              ((eq op 'add)
               (aset regs (nth 1 instr)
                     (+ (aref regs (nth 2 instr))
                        (aref regs (nth 3 instr))))
               (setq pc (1+ pc)))
              ((eq op 'sub)
               (aset regs (nth 1 instr)
                     (- (aref regs (nth 2 instr))
                        (aref regs (nth 3 instr))))
               (setq pc (1+ pc)))
              ((eq op 'mul)
               (aset regs (nth 1 instr)
                     (* (aref regs (nth 2 instr))
                        (aref regs (nth 3 instr))))
               (setq pc (1+ pc)))
              ((eq op 'halt) (setq halted t))
              (t (setq halted t)))))
        (aref regs 0))))

  (fset 'neovm--test-comp-eval
    (lambda (expr)
      (let* ((code (funcall 'neovm--test-comp-compile expr 0))
             (prog (append code (list '(halt)))))
        (funcall 'neovm--test-comp-rm-run prog))))

  (unwind-protect
      (let* (;; Direct evaluation for verification
             (exprs '(42
                      (+ 3 4)
                      (* 5 6)
                      (- 10 3)
                      (+ (* 2 3) (* 4 5))
                      (* (+ 1 2) (- 10 4))
                      (+ (+ 1 2) (+ 3 (+ 4 5)))))
             ;; Compile and run each, also compute expected via eval
             (results
              (mapcar (lambda (e)
                        (let ((compiled-result (funcall 'neovm--test-comp-eval e))
                              (direct-result
                               (cond ((integerp e) e)
                                     (t (eval e)))))
                          (list e compiled-result direct-result
                                (= compiled-result direct-result))))
                      exprs)))
        results)
    (fmakunbound 'neovm--test-comp-compile)
    (fmakunbound 'neovm--test-comp-rm-run)
    (fmakunbound 'neovm--test-comp-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK ((42 42 42 t) ((+ 3 4) 7 7 t) ((* 5 6) 30 30 t) ((- 10 3) 7 7 t) ((+ (* 2 3) (* 4 5)) 26 26 t) ((* (+ 1 2) (- 10 4)) 18 18 t) ((+ (+ 1 2) (+ 3 (+ 4 5))) 15 15 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
