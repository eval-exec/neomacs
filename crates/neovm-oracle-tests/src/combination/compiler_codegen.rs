//! Code generation pattern oracle parity tests:
//! three-address code from AST, register allocation via graph coloring,
//! instruction selection via tree pattern matching, stack frame layout,
//! calling convention implementation, optimization passes,
//! and assembly-like output generation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Three-address code generation from AST
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_codegen_three_address() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; AST nodes: (op left right) for binary, (num val) for literals, (var name) for vars
  ;; Three-address: (op dest src1 src2) or (assign dest src)
  (defvar neovm--tac-counter 0)

  (fset 'neovm--tac-fresh
    (lambda ()
      (setq neovm--tac-counter (1+ neovm--tac-counter))
      (intern (format "t%d" neovm--tac-counter))))

  (fset 'neovm--tac-gen
    (lambda (node)
      ;; Returns (result-temp . instructions-list)
      (cond
       ;; Literal number
       ((eq (car node) 'num)
        (let ((t1 (funcall 'neovm--tac-fresh)))
          (cons t1 (list (list 'assign t1 (nth 1 node))))))
       ;; Variable reference
       ((eq (car node) 'var)
        (let ((t1 (funcall 'neovm--tac-fresh)))
          (cons t1 (list (list 'load t1 (nth 1 node))))))
       ;; Binary operation
       ((memq (car node) '(+ - * /))
        (let* ((left-result (funcall 'neovm--tac-gen (nth 1 node)))
               (right-result (funcall 'neovm--tac-gen (nth 2 node)))
               (t1 (funcall 'neovm--tac-fresh))
               (op (car node))
               (left-instrs (cdr left-result))
               (right-instrs (cdr right-result)))
          (cons t1
                (append left-instrs right-instrs
                        (list (list op t1 (car left-result) (car right-result)))))))
       ;; Comparison: generates compare + conditional
       ((memq (car node) '(< > <= >= == !=))
        (let* ((left-result (funcall 'neovm--tac-gen (nth 1 node)))
               (right-result (funcall 'neovm--tac-gen (nth 2 node)))
               (t1 (funcall 'neovm--tac-fresh)))
          (cons t1
                (append (cdr left-result) (cdr right-result)
                        (list (list 'cmp t1 (car node)
                                    (car left-result) (car right-result)))))))
       ;; If-then-else: (if cond then else)
       ((eq (car node) 'if)
        (let* ((cond-result (funcall 'neovm--tac-gen (nth 1 node)))
               (then-result (funcall 'neovm--tac-gen (nth 2 node)))
               (else-result (funcall 'neovm--tac-gen (nth 3 node)))
               (result-t (funcall 'neovm--tac-fresh))
               (else-label (intern (format "L%d" neovm--tac-counter)))
               (_ (setq neovm--tac-counter (1+ neovm--tac-counter)))
               (end-label (intern (format "L%d" neovm--tac-counter))))
          (cons result-t
                (append (cdr cond-result)
                        (list (list 'if-false (car cond-result) else-label))
                        (cdr then-result)
                        (list (list 'assign result-t (car then-result))
                              (list 'goto end-label)
                              (list 'label else-label))
                        (cdr else-result)
                        (list (list 'assign result-t (car else-result))
                              (list 'label end-label))))))
       (t (cons nil nil)))))

  (unwind-protect
      (progn
        (setq neovm--tac-counter 0)
        (let* (;; (a + b) * (c - 2)
               (ast1 '(* (+ (var a) (var b)) (- (var c) (num 2))))
               (r1 (funcall 'neovm--tac-gen ast1))
               ;; if (x < 10) then (x + 1) else (x * 2)
               (_ (setq neovm--tac-counter 0))
               (ast2 '(if (< (var x) (num 10))
                          (+ (var x) (num 1))
                          (* (var x) (num 2))))
               (r2 (funcall 'neovm--tac-gen ast2)))
          (list (car r1) (length (cdr r1)) (cdr r1)
                (car r2) (length (cdr r2)) (cdr r2))))
    (makunbound 'neovm--tac-counter)
    (fmakunbound 'neovm--tac-fresh)
    (fmakunbound 'neovm--tac-gen)))"#;
    let expect = expect_test::expect![[
        r#""OK (t7 7 ((load t1 a) (load t2 b) (+ t3 t1 t2) (load t4 c) (assign t5 2) (- t6 t4 t5) (* t7 t3 t6)) t10 15 ((load t1 x) (assign t2 10) (cmp t3 < t1 t2) (if-false t3 L10) (load t4 x) (assign t5 1) (+ t6 t4 t5) (assign t10 t6) (goto L11) (label L10) (load t7 x) (assign t8 2) (* t9 t7 t8) (assign t10 t9) (label L11)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Register allocation via graph coloring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_codegen_register_allocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Liveness analysis + graph coloring register allocation
  ;; Interference graph: variable pairs that are live simultaneously
  ;; Greedy coloring with K registers

  (fset 'neovm--ra-build-interference
    (lambda (live-ranges)
      ;; live-ranges: list of (var start end)
      ;; Returns alist of (var . neighbors)
      (let ((graph nil))
        ;; Initialize all vars
        (dolist (lr live-ranges)
          (unless (assq (car lr) graph)
            (push (list (car lr)) graph)))
        ;; Add edges for overlapping ranges
        (let ((remaining live-ranges))
          (while remaining
            (let ((a (car remaining))
                  (rest (cdr remaining)))
              (dolist (b rest)
                ;; Overlap: not (a-end < b-start or b-end < a-start)
                (when (and (<= (nth 1 a) (nth 2 b))
                           (<= (nth 1 b) (nth 2 a)))
                  (let ((a-entry (assq (car a) graph))
                        (b-entry (assq (car b) graph)))
                    (unless (memq (car b) (cdr a-entry))
                      (setcdr a-entry (cons (car b) (cdr a-entry))))
                    (unless (memq (car a) (cdr b-entry))
                      (setcdr b-entry (cons (car a) (cdr b-entry)))))))
              (setq remaining rest))))
        graph)))

  (fset 'neovm--ra-color-graph
    (lambda (graph k)
      ;; Greedy graph coloring with K colors
      ;; Sort by degree (most constrained first)
      (let* ((sorted (sort (copy-sequence graph)
                           (lambda (a b) (> (length (cdr a)) (length (cdr b))))))
             (coloring nil))
        (dolist (node sorted)
          (let* ((var (car node))
                 (neighbors (cdr node))
                 ;; Colors used by neighbors
                 (used (mapcar (lambda (n) (cdr (assq n coloring))) neighbors))
                 ;; Find first available color
                 (color 0))
            (while (memq color used)
              (setq color (1+ color)))
            (if (< color k)
                (push (cons var color) coloring)
              ;; Spill: assign -1
              (push (cons var -1) coloring))))
        (nreverse coloring))))

  (fset 'neovm--ra-assign-registers
    (lambda (coloring reg-names)
      ;; Map colors to register names
      (mapcar (lambda (pair)
                (let ((color (cdr pair)))
                  (cons (car pair)
                        (if (< color 0) 'spill
                          (nth color reg-names)))))
              coloring)))

  (unwind-protect
      (let* (;; Live ranges: (var start end)
             (ranges '((a 0 5) (b 1 3) (c 2 7) (d 4 6) (e 6 8) (f 0 2)))
             (graph (funcall 'neovm--ra-build-interference ranges))
             (coloring3 (funcall 'neovm--ra-color-graph graph 3))
             (coloring4 (funcall 'neovm--ra-color-graph graph 4))
             (regs3 (funcall 'neovm--ra-assign-registers coloring3 '(rax rbx rcx)))
             (regs4 (funcall 'neovm--ra-assign-registers coloring4 '(r0 r1 r2 r3)))
             ;; Check: no two interfering vars share a register
             (valid (let ((ok t))
                      (dolist (node graph)
                        (let ((v (car node))
                              (neighbors (cdr node)))
                          (dolist (n neighbors)
                            (when (and (not (eq (cdr (assq v regs4)) 'spill))
                                       (not (eq (cdr (assq n regs4)) 'spill))
                                       (eq (cdr (assq v regs4)) (cdr (assq n regs4))))
                              (setq ok nil)))))
                      ok)))
        (list graph coloring3 regs3 regs4 valid))
    (fmakunbound 'neovm--ra-build-interference)
    (fmakunbound 'neovm--ra-color-graph)
    (fmakunbound 'neovm--ra-assign-registers)))"#;
    let expect = expect_test::expect![[
        r#""OK (((f c b a) (e d c) (d e c a) (c f e d b a) (b f c a) (a f d c b)) ((c . 0) (a . 1) (f . 2) (d . 2) (b . -1) (e . 1)) ((c . rax) (a . rbx) (f . rcx) (d . rcx) (b . spill) (e . rbx)) ((c . r0) (a . r1) (f . r2) (d . r2) (b . r3) (e . r1)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Instruction selection via tree pattern matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_codegen_instruction_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Pattern-match IR trees to machine instructions
  ;; Patterns: (add reg imm) -> addi, (add reg reg) -> add, (load (add reg imm)) -> ld offset
  ;; (mul reg (num 2)) -> shl 1, (mul reg (num 4)) -> shl 2, etc.

  (fset 'neovm--isel-match
    (lambda (node)
      ;; Returns list of machine instructions
      (cond
       ;; Multiply by power of 2 -> shift
       ((and (eq (car node) 'mul)
             (eq (car (nth 2 node)) 'num)
             (let ((n (nth 1 (nth 2 node))))
               (and (> n 0) (= (logand n (1- n)) 0))))
        (let* ((sub (funcall 'neovm--isel-match (nth 1 node)))
               (src (car (last (car (last sub)))))
               (shift-amt (let ((n (nth 1 (nth 2 node))) (s 0))
                            (while (> n 1) (setq n (/ n 2) s (1+ s))) s))
               (dest (intern (format "v%d" (random 1000)))))
          (append sub (list (list 'SHL dest src shift-amt)))))
       ;; Add with immediate
       ((and (eq (car node) 'add)
             (eq (car (nth 2 node)) 'num))
        (let* ((sub (funcall 'neovm--isel-match (nth 1 node)))
               (src (car (last (car (last sub))))))
          (append sub (list (list 'ADDI (intern (format "v%d" (random 1000)))
                                  src (nth 1 (nth 2 node)))))))
       ;; Add two registers
       ((eq (car node) 'add)
        (let* ((left (funcall 'neovm--isel-match (nth 1 node)))
               (right (funcall 'neovm--isel-match (nth 2 node)))
               (lsrc (car (last (car (last left)))))
               (rsrc (car (last (car (last right))))))
          (append left right
                  (list (list 'ADD (intern (format "v%d" (random 1000))) lsrc rsrc)))))
       ;; Load from (add base offset) -> LD with offset addressing
       ((and (eq (car node) 'load)
             (eq (car (nth 1 node)) 'add)
             (eq (car (nth 2 (nth 1 node))) 'num))
        (let* ((base-instrs (funcall 'neovm--isel-match (nth 1 (nth 1 node))))
               (base-reg (car (last (car (last base-instrs)))))
               (offset (nth 1 (nth 2 (nth 1 node)))))
          (append base-instrs
                  (list (list 'LD (intern (format "v%d" (random 1000)))
                              base-reg offset)))))
       ;; Simple load
       ((eq (car node) 'load)
        (let* ((sub (funcall 'neovm--isel-match (nth 1 node)))
               (addr (car (last (car (last sub))))))
          (append sub (list (list 'LD (intern (format "v%d" (random 1000)))
                                  addr 0)))))
       ;; Register reference
       ((eq (car node) 'reg)
        (list (list 'MOV (nth 1 node) (nth 1 node))))
       ;; Immediate
       ((eq (car node) 'num)
        (let ((dest (intern (format "v%d" (random 1000)))))
          (list (list 'LI dest (nth 1 node)))))
       (t (list (list 'UNKNOWN node))))))

  (unwind-protect
      (progn
        ;; Seed random for deterministic output
        (random "neovm-isel-seed")
        (list
         ;; x * 8 -> SHL by 3
         (funcall 'neovm--isel-match '(mul (reg x) (num 8)))
         ;; a + 42
         (funcall 'neovm--isel-match '(add (reg a) (num 42)))
         ;; load from (base + 16)
         (funcall 'neovm--isel-match '(load (add (reg base) (num 16))))
         ;; (a + b) * 4
         (funcall 'neovm--isel-match '(mul (add (reg a) (reg b)) (num 4)))
         ;; Count instruction types
         (let* ((instrs (funcall 'neovm--isel-match
                                 '(add (mul (reg x) (num 4)) (num 10))))
                (types (mapcar #'car instrs)))
           (list types (length instrs)))))
    (fmakunbound 'neovm--isel-match)))"#;
    let expect = expect_test::expect![[
        r#""OK (((MOV x x) (SHL v734 x 3)) ((MOV a a) (ADDI v588 a 42)) ((MOV base base) (LD v220 base 16)) ((MOV a a) (MOV b b) (ADD v225 a b) (SHL v335 b 2)) ((MOV SHL ADDI) 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stack frame layout computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_codegen_stack_frame_layout() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Compute stack frame layout: locals, spills, saved registers, alignment
  (fset 'neovm--frame-align
    (lambda (offset alignment)
      ;; Round up to next multiple of alignment
      (let ((mask (1- alignment)))
        (logand (+ offset mask) (lognot mask)))))

  (fset 'neovm--frame-layout
    (lambda (params locals spilled-regs callee-saved-regs)
      ;; params: list of (name . size)
      ;; locals: list of (name size alignment)
      ;; spilled-regs: list of register names (each 8 bytes)
      ;; callee-saved-regs: list of register names (each 8 bytes)
      ;; Returns: hash-table with offsets and total size
      (let ((frame (make-hash-table :test 'equal))
            (offset 0))
        ;; Return address (pushed by call)
        (puthash "return-addr" offset frame)
        (setq offset (+ offset 8))
        ;; Saved frame pointer
        (puthash "saved-rbp" offset frame)
        (setq offset (+ offset 8))
        ;; Callee-saved registers
        (let ((saved-start offset))
          (dolist (r callee-saved-regs)
            (puthash (format "saved-%s" (symbol-name r)) offset frame)
            (setq offset (+ offset 8)))
          (puthash "callee-saved-size" (- offset saved-start) frame))
        ;; Spilled registers
        (dolist (r spilled-regs)
          (puthash (format "spill-%s" (symbol-name r)) offset frame)
          (setq offset (+ offset 8)))
        ;; Local variables (respect alignment)
        (dolist (local locals)
          (let* ((name (nth 0 local))
                 (size (nth 1 local))
                 (align (nth 2 local)))
            (setq offset (funcall 'neovm--frame-align offset align))
            (puthash (format "local-%s" (symbol-name name)) offset frame)
            (setq offset (+ offset size))))
        ;; Align total frame to 16 bytes
        (setq offset (funcall 'neovm--frame-align offset 16))
        (puthash "total-size" offset frame)
        frame)))

  (unwind-protect
      (let* ((layout (funcall 'neovm--frame-layout
                               '((x . 8) (y . 8))
                               '((buf 256 16) (counter 4 4) (flag 1 1) (ptr 8 8))
                               '(r12 r13)
                               '(rbx r14 r15)))
             (keys (let ((ks nil))
                     (maphash (lambda (k v) (push (cons k v) ks)) layout)
                     (sort ks (lambda (a b) (< (cdr a) (cdr b)))))))
        (list keys
              (gethash "total-size" layout)
              (gethash "local-buf" layout)
              (gethash "callee-saved-size" layout)))
    (fmakunbound 'neovm--frame-align)
    (fmakunbound 'neovm--frame-layout)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"return-addr\" . 0) (\"saved-rbp\" . 8) (\"saved-rbx\" . 16) (\"callee-saved-size\" . 24) (\"saved-r14\" . 24) (\"saved-r15\" . 32) (\"spill-r12\" . 40) (\"spill-r13\" . 48) (\"local-buf\" . 64) (\"local-counter\" . 320) (\"local-flag\" . 324) (\"local-ptr\" . 328) (\"total-size\" . 336)) 336 64 24)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Calling convention: argument passing and return values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_codegen_calling_convention() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simulate a calling convention:
  ;; First 6 integer args in registers (rdi, rsi, rdx, rcx, r8, r9)
  ;; Remaining args on stack (right-to-left push order)
  ;; Return value in rax (and rdx for 128-bit)

  (defvar neovm--cc-reg-args '(rdi rsi rdx rcx r8 r9))

  (fset 'neovm--cc-classify-args
    (lambda (args)
      ;; Returns (reg-assignments . stack-assignments)
      ;; Each: list of (arg-index . location)
      (let ((reg-assigns nil)
            (stack-assigns nil)
            (reg-idx 0)
            (stack-offset 0))
        (dotimes (i (length args))
          (let ((arg (nth i args)))
            (cond
             ;; Struct > 16 bytes -> pointer on stack
             ((and (listp arg) (eq (car arg) 'struct) (> (nth 1 arg) 16))
              (push (list i 'stack-ptr stack-offset (nth 1 arg)) stack-assigns)
              (setq stack-offset (+ stack-offset 8)))
             ;; Register available
             ((< reg-idx 6)
              (push (list i 'reg (nth reg-idx neovm--cc-reg-args)) reg-assigns)
              (setq reg-idx (1+ reg-idx)))
             ;; Stack
             (t
              (push (list i 'stack stack-offset) stack-assigns)
              (setq stack-offset (+ stack-offset 8))))))
        (cons (nreverse reg-assigns) (nreverse stack-assigns)))))

  (fset 'neovm--cc-gen-prologue
    (lambda (classification)
      ;; Generate instructions to save register args to locals
      (let ((instrs nil))
        (dolist (ra (car classification))
          (push (list 'MOV (format "[rbp-%d]" (* 8 (1+ (nth 0 ra))))
                      (nth 2 ra))
                instrs))
        (nreverse instrs))))

  (fset 'neovm--cc-gen-call
    (lambda (func-name args classification)
      ;; Generate call sequence
      (let ((instrs nil)
            (stack-args (cdr classification)))
        ;; Push stack args right-to-left
        (dolist (sa (nreverse (copy-sequence stack-args)))
          (push (list 'PUSH (format "arg%d" (nth 0 sa))) instrs))
        ;; Move register args
        (dolist (ra (car classification))
          (push (list 'MOV (nth 2 ra) (format "arg%d" (nth 0 ra))) instrs))
        ;; Call
        (push (list 'CALL func-name) instrs)
        ;; Clean up stack
        (when stack-args
          (push (list 'ADD 'rsp (* 8 (length stack-args))) instrs))
        (nreverse instrs))))

  (unwind-protect
      (let* (;; 4 args: all fit in registers
             (args4 '(int int int int))
             (class4 (funcall 'neovm--cc-classify-args args4))
             ;; 8 args: 6 in regs, 2 on stack
             (args8 '(int int int int int int int int))
             (class8 (funcall 'neovm--cc-classify-args args8))
             ;; Mixed with large struct
             (args-mixed '(int int (struct 32) int int int int))
             (class-mixed (funcall 'neovm--cc-classify-args args-mixed))
             ;; Generate call sequence for 8-arg case
             (call-instrs (funcall 'neovm--cc-gen-call "my_func" args8 class8))
             (prologue (funcall 'neovm--cc-gen-prologue class4)))
        (list class4 class8 class-mixed
              (length call-instrs) call-instrs
              prologue))
    (makunbound 'neovm--cc-reg-args)
    (fmakunbound 'neovm--cc-classify-args)
    (fmakunbound 'neovm--cc-gen-prologue)
    (fmakunbound 'neovm--cc-gen-call)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((0 reg rdi) (1 reg rsi) (2 reg rdx) (3 reg rcx))) (((0 reg rdi) (1 reg rsi) (2 reg rdx) (3 reg rcx) (4 reg r8) (5 reg r9)) (6 stack 0) (7 stack 8)) (((0 reg rdi) (1 reg rsi) (3 reg rdx) (4 reg rcx) (5 reg r8) (6 reg r9)) (2 stack-ptr 0 32)) 10 ((PUSH \"arg7\") (PUSH \"arg6\") (MOV rdi \"arg0\") (MOV rsi \"arg1\") (MOV rdx \"arg2\") (MOV rcx \"arg3\") (MOV r8 \"arg4\") (MOV r9 \"arg5\") (CALL \"my_func\") (ADD rsp 16)) ((MOV \"[rbp-8]\" rdi) (MOV \"[rbp-16]\" rsi) (MOV \"[rbp-24]\" rdx) (MOV \"[rbp-32]\" rcx)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Optimization passes: constant folding, dead code elimination, strength reduction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_codegen_optimization_passes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; IR: list of three-address instructions
  ;; (op dest src1 src2) | (assign dest val) | (label L) | (goto L) | (if-false src L)

  (fset 'neovm--opt-constant-fold
    (lambda (instrs)
      ;; If both operands are known constants, compute at compile time
      (let ((constants (make-hash-table :test 'eq))
            (result nil))
        (dolist (instr instrs)
          (cond
           ((eq (car instr) 'assign)
            (puthash (nth 1 instr) (nth 2 instr) constants)
            (push instr result))
           ((memq (car instr) '(+ - * /))
            (let ((s1 (gethash (nth 2 instr) constants))
                  (s2 (gethash (nth 3 instr) constants)))
              (if (and (numberp s1) (numberp s2))
                  (let ((val (cond ((eq (car instr) '+) (+ s1 s2))
                                   ((eq (car instr) '-) (- s1 s2))
                                   ((eq (car instr) '*) (* s1 s2))
                                   ((eq (car instr) '/) (/ s1 s2)))))
                    (puthash (nth 1 instr) val constants)
                    (push (list 'assign (nth 1 instr) val) result))
                (push instr result))))
           (t (push instr result))))
        (nreverse result))))

  (fset 'neovm--opt-dead-code-elim
    (lambda (instrs)
      ;; Remove assignments to variables never used later
      (let ((used (make-hash-table :test 'eq)))
        ;; First pass: collect all used variables
        (dolist (instr instrs)
          (when (> (length instr) 2)
            (dolist (operand (nthcdr 2 instr))
              (when (symbolp operand)
                (puthash operand t used)))))
        ;; Second pass: remove dead assignments
        (let ((result nil))
          (dolist (instr instrs)
            (if (and (or (eq (car instr) 'assign) (memq (car instr) '(+ - * /)))
                     (not (gethash (nth 1 instr) used)))
                nil  ;; dead, skip
              (push instr result)))
          (nreverse result)))))

  (fset 'neovm--opt-strength-reduce
    (lambda (instrs)
      ;; Replace multiply by power of 2 with shift, x*1->x, x+0->x, x*0->0
      (let ((result nil))
        (dolist (instr instrs)
          (cond
           ;; x * 2^n -> x << n
           ((and (eq (car instr) '*)
                 (numberp (nth 3 instr))
                 (> (nth 3 instr) 0)
                 (= (logand (nth 3 instr) (1- (nth 3 instr))) 0))
            (let ((n (nth 3 instr)) (s 0))
              (while (> n 1) (setq n (/ n 2) s (1+ s)))
              (push (list 'shl (nth 1 instr) (nth 2 instr) s) result)))
           ;; x * 1 -> assign
           ((and (eq (car instr) '*) (equal (nth 3 instr) 1))
            (push (list 'assign (nth 1 instr) (nth 2 instr)) result))
           ;; x * 0 -> assign 0
           ((and (eq (car instr) '*) (equal (nth 3 instr) 0))
            (push (list 'assign (nth 1 instr) 0) result))
           ;; x + 0 -> assign
           ((and (eq (car instr) '+) (equal (nth 3 instr) 0))
            (push (list 'assign (nth 1 instr) (nth 2 instr)) result))
           ;; x - 0 -> assign
           ((and (eq (car instr) '-) (equal (nth 3 instr) 0))
            (push (list 'assign (nth 1 instr) (nth 2 instr)) result))
           (t (push instr result))))
        (nreverse result))))

  (unwind-protect
      (let* ((ir '((assign a 10)
                   (assign b 20)
                   (+ c a b)
                   (* d c 2)
                   (assign e 5)
                   (+ f e 0)
                   (* g d 1)
                   (* h d 8)
                   (assign dead 999)
                   (assign result h)))
             (folded (funcall 'neovm--opt-constant-fold ir))
             (reduced (funcall 'neovm--opt-strength-reduce folded))
             (cleaned (funcall 'neovm--opt-dead-code-elim reduced)))
        (list (length ir) ir
              (length folded) folded
              (length reduced) reduced
              (length cleaned) cleaned))
    (fmakunbound 'neovm--opt-constant-fold)
    (fmakunbound 'neovm--opt-dead-code-elim)
    (fmakunbound 'neovm--opt-strength-reduce)))"#;
    let expect = expect_test::expect![[
        r#""OK (10 ((assign a 10) (assign b 20) (+ c a b) (* d c 2) (assign e 5) (+ f e 0) (* g d 1) (* h d 8) (assign dead 999) (assign result h)) 10 ((assign a 10) (assign b 20) (assign c 30) (* d c 2) (assign e 5) (+ f e 0) (* g d 1) (* h d 8) (assign dead 999) (assign result h)) 10 ((assign a 10) (assign b 20) (assign c 30) (shl d c 1) (assign e 5) (assign f e) (shl g d 0) (shl h d 3) (assign dead 999) (assign result h)) 5 ((assign c 30) (shl d c 1) (assign e 5) (shl g d 0) (shl h d 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Assembly-like output generation from IR
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_codegen_assembly_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Convert IR instructions to x86-64-like assembly text
  (fset 'neovm--asm-gen
    (lambda (instrs reg-map)
      ;; reg-map: alist of (var . register-name-string)
      (let ((lines nil))
        (dolist (instr instrs)
          (let ((op (car instr)))
            (cond
             ((eq op 'assign)
              (let ((dest (cdr (assq (nth 1 instr) reg-map)))
                    (val (nth 2 instr)))
                (if (numberp val)
                    (push (format "    mov %s, %d" (or dest "???") val) lines)
                  (let ((src (cdr (assq val reg-map))))
                    (push (format "    mov %s, %s" (or dest "???") (or src "???")) lines)))))
             ((eq op 'load)
              (let ((dest (cdr (assq (nth 1 instr) reg-map)))
                    (src (cdr (assq (nth 2 instr) reg-map))))
                (push (format "    mov %s, [%s]" (or dest "???") (or src "???")) lines)))
             ((memq op '(+ add))
              (let ((dest (cdr (assq (nth 1 instr) reg-map)))
                    (s1 (cdr (assq (nth 2 instr) reg-map)))
                    (s2 (if (numberp (nth 3 instr)) (nth 3 instr)
                          (cdr (assq (nth 3 instr) reg-map)))))
                (when (not (string= (or dest "") (or s1 "")))
                  (push (format "    mov %s, %s" dest s1) lines))
                (push (format "    add %s, %s" (or dest "???")
                              (if (numberp s2) (number-to-string s2) (or s2 "???")))
                      lines)))
             ((memq op '(- sub))
              (let ((dest (cdr (assq (nth 1 instr) reg-map)))
                    (s1 (cdr (assq (nth 2 instr) reg-map)))
                    (s2 (if (numberp (nth 3 instr)) (nth 3 instr)
                          (cdr (assq (nth 3 instr) reg-map)))))
                (when (not (string= (or dest "") (or s1 "")))
                  (push (format "    mov %s, %s" dest s1) lines))
                (push (format "    sub %s, %s" (or dest "???")
                              (if (numberp s2) (number-to-string s2) (or s2 "???")))
                      lines)))
             ((eq op 'shl)
              (let ((dest (cdr (assq (nth 1 instr) reg-map)))
                    (src (cdr (assq (nth 2 instr) reg-map))))
                (when (not (string= (or dest "") (or src "")))
                  (push (format "    mov %s, %s" dest src) lines))
                (push (format "    shl %s, %d" (or dest "???") (nth 3 instr)) lines)))
             ((eq op '*)
              (let ((dest (cdr (assq (nth 1 instr) reg-map)))
                    (s1 (cdr (assq (nth 2 instr) reg-map)))
                    (s2 (if (numberp (nth 3 instr)) (nth 3 instr)
                          (cdr (assq (nth 3 instr) reg-map)))))
                (push (format "    imul %s, %s, %s" (or dest "???") (or s1 "???")
                              (if (numberp s2) (number-to-string s2) (or s2 "???")))
                      lines)))
             ((eq op 'label)
              (push (format "%s:" (symbol-name (nth 1 instr))) lines))
             ((eq op 'goto)
              (push (format "    jmp %s" (symbol-name (nth 1 instr))) lines))
             ((eq op 'ret)
              (push "    ret" lines))
             ((eq op 'call)
              (push (format "    call %s" (nth 1 instr)) lines))
             (t (push (format "    ; unknown: %s" instr) lines)))))
        (mapconcat #'identity (nreverse lines) "\n"))))

  (unwind-protect
      (let* ((ir '((assign a 10)
                   (assign b 20)
                   (+ c a b)
                   (shl d c 3)
                   (- e d 5)
                   (label done)
                   (ret)))
             (reg-map '((a . "rax") (b . "rbx") (c . "rcx")
                        (d . "rdx") (e . "rsi") (result . "rax")))
             (asm-text (funcall 'neovm--asm-gen ir reg-map))
             ;; Also test with function prologue/epilogue
             (full-fn (concat "my_function:\n"
                              "    push rbp\n"
                              "    mov rbp, rsp\n"
                              asm-text "\n"
                              "    pop rbp\n"
                              "    ret")))
        (list asm-text
              (length (split-string asm-text "\n"))
              full-fn))
    (fmakunbound 'neovm--asm-gen)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"    mov rax, 10\\n    mov rbx, 20\\n    mov rcx, rax\\n    add rcx, rbx\\n    mov rdx, rcx\\n    shl rdx, 3\\n    mov rsi, rdx\\n    sub rsi, 5\\ndone:\\n    ret\" 10 \"my_function:\\n    push rbp\\n    mov rbp, rsp\\n    mov rax, 10\\n    mov rbx, 20\\n    mov rcx, rax\\n    add rcx, rbx\\n    mov rdx, rcx\\n    shl rdx, 3\\n    mov rsi, rdx\\n    sub rsi, 5\\ndone:\\n    ret\\n    pop rbp\\n    ret\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
