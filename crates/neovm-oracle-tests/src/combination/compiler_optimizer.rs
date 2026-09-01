//! Oracle parity tests for compiler optimization patterns in Elisp.
//!
//! Covers: constant folding, dead code elimination, common subexpression
//! elimination (CSE), strength reduction, peephole optimization, basic
//! block construction, and data flow analysis for live variables.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Constant folding: evaluate compile-time-known expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_opt_constant_folding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An AST is represented as nested lists:
    //   (const N), (var NAME), (binop OP L R), (if COND THEN ELSE)
    // Constant folding rewrites (binop OP (const A) (const B)) -> (const result).
    let form = r#"
(progn
  (fset 'neovm--copt-cf-fold
    (lambda (ast)
      "Constant-fold an AST node recursively."
      (cond
       ((eq (car ast) 'const) ast)
       ((eq (car ast) 'var) ast)
       ((eq (car ast) 'binop)
        (let* ((op (nth 1 ast))
               (left (funcall 'neovm--copt-cf-fold (nth 2 ast)))
               (right (funcall 'neovm--copt-cf-fold (nth 3 ast))))
          (if (and (eq (car left) 'const)
                   (eq (car right) 'const))
              (let ((lv (cadr left))
                    (rv (cadr right)))
                (list 'const
                      (cond
                       ((eq op '+) (+ lv rv))
                       ((eq op '-) (- lv rv))
                       ((eq op '*) (* lv rv))
                       ((and (eq op '/) (/= rv 0)) (/ lv rv))
                       (t (list 'binop op left right)))))
            (list 'binop op left right))))
       ((eq (car ast) 'if)
        (let ((cond-ast (funcall 'neovm--copt-cf-fold (nth 1 ast)))
              (then-ast (funcall 'neovm--copt-cf-fold (nth 2 ast)))
              (else-ast (funcall 'neovm--copt-cf-fold (nth 3 ast))))
          ;; If condition is constant, eliminate branch
          (if (eq (car cond-ast) 'const)
              (if (/= (cadr cond-ast) 0) then-ast else-ast)
            (list 'if cond-ast then-ast else-ast))))
       (t ast))))

  (unwind-protect
      (let* (;; (3 + 4) * 2 => 14
             (ast1 '(binop * (binop + (const 3) (const 4)) (const 2)))
             (folded1 (funcall 'neovm--copt-cf-fold ast1))
             ;; (x + (2 * 3)) => (binop + (var x) (const 6))
             (ast2 '(binop + (var x) (binop * (const 2) (const 3))))
             (folded2 (funcall 'neovm--copt-cf-fold ast2))
             ;; if (1) then (const 10) else (const 20) => (const 10)
             (ast3 '(if (const 1) (const 10) (const 20)))
             (folded3 (funcall 'neovm--copt-cf-fold ast3))
             ;; if (0) then (const 10) else (const 20) => (const 20)
             (ast4 '(if (const 0) (const 10) (const 20)))
             (folded4 (funcall 'neovm--copt-cf-fold ast4))
             ;; Nested: ((5 - 3) + (2 * (1 + 1))) => (const 6)
             (ast5 '(binop + (binop - (const 5) (const 3))
                             (binop * (const 2) (binop + (const 1) (const 1)))))
             (folded5 (funcall 'neovm--copt-cf-fold ast5))
             ;; Mixed: (x * (3 + 0)) => (binop * (var x) (const 3))
             (ast6 '(binop * (var x) (binop + (const 3) (const 0))))
             (folded6 (funcall 'neovm--copt-cf-fold ast6)))
        (list folded1 folded2 folded3 folded4 folded5 folded6))
    (fmakunbound 'neovm--copt-cf-fold)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((const 14) (binop + (var x) (const 6)) (const 10) (const 20) (const 6) (binop * (var x) (const 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dead code elimination: remove assignments to unused variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_opt_dead_code_elimination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Instruction representation: (assign VAR EXPR) | (return EXPR) | (print EXPR)
    // An assignment is dead if VAR is never read in any subsequent instruction.
    let form = r#"
(progn
  (fset 'neovm--copt-dce-used-vars
    (lambda (expr)
      "Collect all variable names used in EXPR (an AST node)."
      (cond
       ((eq (car expr) 'var) (list (cadr expr)))
       ((eq (car expr) 'const) nil)
       ((eq (car expr) 'binop)
        (append (funcall 'neovm--copt-dce-used-vars (nth 2 expr))
                (funcall 'neovm--copt-dce-used-vars (nth 3 expr))))
       ((eq (car expr) 'call)
        (let ((vars nil))
          (dolist (arg (cddr expr))
            (setq vars (append vars (funcall 'neovm--copt-dce-used-vars arg))))
          vars))
       (t nil))))

  (fset 'neovm--copt-dce-eliminate
    (lambda (instrs)
      "Remove dead assignments. Iterate until no more can be removed."
      (let ((changed t))
        (while changed
          (setq changed nil)
          ;; Collect all used variables from non-assign instructions
          ;; and right-hand sides of assigns
          (let ((used nil))
            ;; First pass: collect used vars from return/print exprs
            ;; and from the RHS of all assigns
            (dolist (instr instrs)
              (cond
               ((eq (car instr) 'return)
                (setq used (append used (funcall 'neovm--copt-dce-used-vars (cadr instr)))))
               ((eq (car instr) 'print)
                (setq used (append used (funcall 'neovm--copt-dce-used-vars (cadr instr)))))
               ((eq (car instr) 'assign)
                (setq used (append used (funcall 'neovm--copt-dce-used-vars (nth 2 instr)))))))
            ;; Now remove assigns to vars not in used set
            (let ((new-instrs nil))
              (dolist (instr instrs)
                (if (and (eq (car instr) 'assign)
                         (not (memq (cadr instr) used)))
                    (setq changed t)
                  (push instr new-instrs)))
              (setq instrs (nreverse new-instrs))))))
      instrs))

  (unwind-protect
      (let* (;; x = 5; y = x + 1; z = 10; return y
             ;; z is dead (never used), should be removed
             (prog1-instrs '((assign x (const 5))
                             (assign y (binop + (var x) (const 1)))
                             (assign z (const 10))
                             (return (var y))))
             (opt1 (funcall 'neovm--copt-dce-eliminate (copy-sequence prog1-instrs)))
             ;; a = 1; b = 2; c = a + b; d = 99; e = d * 2; return c
             ;; d and e are dead
             (prog2-instrs '((assign a (const 1))
                             (assign b (const 2))
                             (assign c (binop + (var a) (var b)))
                             (assign d (const 99))
                             (assign e (binop * (var d) (const 2)))
                             (return (var c))))
             (opt2 (funcall 'neovm--copt-dce-eliminate (copy-sequence prog2-instrs)))
             ;; All dead except print: x = 1; y = 2; print 42
             (prog3-instrs '((assign x (const 1))
                             (assign y (const 2))
                             (print (const 42))))
             (opt3 (funcall 'neovm--copt-dce-eliminate (copy-sequence prog3-instrs))))
        (list
         ;; prog1: z removed, 3 instructions remain
         (length opt1)
         (not (assq 'z (mapcar (lambda (i) (when (eq (car i) 'assign) (cons (cadr i) t))) opt1)))
         opt1
         ;; prog2: d and e removed, 4 instructions remain
         (length opt2)
         opt2
         ;; prog3: x and y removed, only print remains
         (length opt3)
         opt3))
    (fmakunbound 'neovm--copt-dce-used-vars)
    (fmakunbound 'neovm--copt-dce-eliminate)))
"#;
    let expect = expect_test::expect![[
        r#""OK (3 t ((assign x (const 5)) (assign y (binop + (var x) (const 1))) (return (var y))) 4 ((assign a (const 1)) (assign b (const 2)) (assign c (binop + (var a) (var b))) (return (var c))) 1 ((print (const 42))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Common subexpression elimination (CSE)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_opt_cse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find instructions that compute the same expression and reuse the result.
    let form = r#"
(progn
  (fset 'neovm--copt-cse-expr-key
    (lambda (expr)
      "Generate a canonical key for an expression for comparison."
      (cond
       ((eq (car expr) 'const) (format "const:%s" (cadr expr)))
       ((eq (car expr) 'var) (format "var:%s" (cadr expr)))
       ((eq (car expr) 'binop)
        (let ((op (nth 1 expr))
              (lk (funcall 'neovm--copt-cse-expr-key (nth 2 expr)))
              (rk (funcall 'neovm--copt-cse-expr-key (nth 3 expr))))
          ;; For commutative ops, sort operands
          (if (memq op '(+ *))
              (if (string< lk rk)
                  (format "(%s %s %s)" op lk rk)
                (format "(%s %s %s)" op rk lk))
            (format "(%s %s %s)" op lk rk))))
       (t (format "%S" expr)))))

  (fset 'neovm--copt-cse-optimize
    (lambda (instrs)
      "Replace duplicate expressions with references to first computation."
      (let ((expr-map (make-hash-table :test 'equal))
            (result nil))
        (dolist (instr instrs)
          (if (eq (car instr) 'assign)
              (let* ((var (cadr instr))
                     (expr (nth 2 instr))
                     (key (funcall 'neovm--copt-cse-expr-key expr))
                     (existing (gethash key expr-map)))
                (if existing
                    ;; Reuse: assign var = existing-var
                    (push (list 'assign var (list 'var existing)) result)
                  ;; First time: record and keep
                  (puthash key var expr-map)
                  (push instr result)))
            (push instr result)))
        (nreverse result))))

  (unwind-protect
      (let* (;; a = x + y; b = x + y; c = a * b => b should reuse a
             (prog1-instrs '((assign a (binop + (var x) (var y)))
                             (assign b (binop + (var x) (var y)))
                             (assign c (binop * (var a) (var b)))
                             (return (var c))))
             (opt1 (funcall 'neovm--copt-cse-optimize prog1-instrs))
             ;; Commutativity: a = x * y; b = y * x => b should reuse a
             (prog2-instrs '((assign a (binop * (var x) (var y)))
                             (assign b (binop * (var y) (var x)))
                             (return (binop + (var a) (var b)))))
             (opt2 (funcall 'neovm--copt-cse-optimize prog2-instrs))
             ;; No CSE opportunity: a = x + y; b = x - y
             (prog3-instrs '((assign a (binop + (var x) (var y)))
                             (assign b (binop - (var x) (var y)))
                             (return (binop + (var a) (var b)))))
             (opt3 (funcall 'neovm--copt-cse-optimize prog3-instrs)))
        (list
         ;; opt1: b = (var a) instead of recomputing
         opt1
         (equal (nth 2 (nth 1 opt1)) '(var a))
         ;; opt2: b = (var a) due to commutativity
         opt2
         (equal (nth 2 (nth 1 opt2)) '(var a))
         ;; opt3: no change, all different
         (equal opt3 prog3-instrs)))
    (fmakunbound 'neovm--copt-cse-expr-key)
    (fmakunbound 'neovm--copt-cse-optimize)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((assign a (binop + (var x) (var y))) (assign b (var a)) (assign c (binop * (var a) (var b))) (return (var c))) t ((assign a (binop * (var x) (var y))) (assign b (var a)) (return (binop + (var a) (var b)))) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Strength reduction: replace expensive ops with cheaper equivalents
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_opt_strength_reduction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Replace patterns:
    //   x * 2 => x + x (or x << 1)
    //   x * power-of-2 => x << log2(n)
    //   x / power-of-2 => x >> log2(n)
    //   x % power-of-2 => x & (n-1)
    //   x * 0 => 0, x * 1 => x, x + 0 => x
    let form = r#"
(progn
  (fset 'neovm--copt-sr-power-of-2-p
    (lambda (n)
      "Check if N is a power of 2 and return log2(N), or nil."
      (if (and (integerp n) (> n 0))
          (let ((log 0) (v n))
            (if (= (logand v (1- v)) 0)
                (progn
                  (while (> v 1)
                    (setq v (ash v -1) log (1+ log)))
                  log)
              nil))
        nil)))

  (fset 'neovm--copt-sr-reduce
    (lambda (ast)
      "Apply strength reduction to AST."
      (cond
       ((or (eq (car ast) 'const) (eq (car ast) 'var)) ast)
       ((eq (car ast) 'binop)
        (let* ((op (nth 1 ast))
               (left (funcall 'neovm--copt-sr-reduce (nth 2 ast)))
               (right (funcall 'neovm--copt-sr-reduce (nth 3 ast))))
          (cond
           ;; x * 0 => 0
           ((and (eq op '*) (eq (car right) 'const) (= (cadr right) 0))
            '(const 0))
           ;; 0 * x => 0
           ((and (eq op '*) (eq (car left) 'const) (= (cadr left) 0))
            '(const 0))
           ;; x * 1 => x
           ((and (eq op '*) (eq (car right) 'const) (= (cadr right) 1))
            left)
           ;; 1 * x => x
           ((and (eq op '*) (eq (car left) 'const) (= (cadr left) 1))
            right)
           ;; x + 0 => x
           ((and (eq op '+) (eq (car right) 'const) (= (cadr right) 0))
            left)
           ;; 0 + x => x
           ((and (eq op '+) (eq (car left) 'const) (= (cadr left) 0))
            right)
           ;; x * power-of-2 => x << log2(n)
           ((and (eq op '*) (eq (car right) 'const)
                 (funcall 'neovm--copt-sr-power-of-2-p (cadr right)))
            (let ((shift (funcall 'neovm--copt-sr-power-of-2-p (cadr right))))
              (if (= shift 1)
                  (list 'binop '+ left left)  ;; x * 2 => x + x
                (list 'binop 'lshift left (list 'const shift)))))
           ;; x / power-of-2 => x >> log2(n)
           ((and (eq op '/) (eq (car right) 'const)
                 (funcall 'neovm--copt-sr-power-of-2-p (cadr right)))
            (list 'binop 'rshift left
                  (list 'const (funcall 'neovm--copt-sr-power-of-2-p (cadr right)))))
           ;; x % power-of-2 => x & (n-1)
           ((and (eq op '%) (eq (car right) 'const)
                 (funcall 'neovm--copt-sr-power-of-2-p (cadr right)))
            (list 'binop 'bitand left (list 'const (1- (cadr right)))))
           ;; No reduction
           (t (list 'binop op left right)))))
       (t ast))))

  (unwind-protect
      (let* (;; x * 2 => x + x
             (r1 (funcall 'neovm--copt-sr-reduce '(binop * (var x) (const 2))))
             ;; x * 8 => x << 3
             (r2 (funcall 'neovm--copt-sr-reduce '(binop * (var x) (const 8))))
             ;; x / 4 => x >> 2
             (r3 (funcall 'neovm--copt-sr-reduce '(binop / (var x) (const 4))))
             ;; x % 16 => x & 15
             (r4 (funcall 'neovm--copt-sr-reduce '(binop % (var x) (const 16))))
             ;; x * 0 => 0
             (r5 (funcall 'neovm--copt-sr-reduce '(binop * (var x) (const 0))))
             ;; x * 1 => x
             (r6 (funcall 'neovm--copt-sr-reduce '(binop * (var x) (const 1))))
             ;; x + 0 => x
             (r7 (funcall 'neovm--copt-sr-reduce '(binop + (var x) (const 0))))
             ;; Nested: (y * 4) + (z * 0) => (y << 2) + 0 => y << 2
             (r8 (funcall 'neovm--copt-sr-reduce
                   '(binop + (binop * (var y) (const 4))
                             (binop * (var z) (const 0)))))
             ;; x * 3 => no reduction (3 is not power of 2)
             (r9 (funcall 'neovm--copt-sr-reduce '(binop * (var x) (const 3)))))
        (list r1 r2 r3 r4 r5 r6 r7 r8 r9
              ;; Verify specific reductions
              (equal r1 '(binop + (var x) (var x)))
              (equal r5 '(const 0))
              (equal r6 '(var x))
              (equal r7 '(var x))
              ;; r9 should be unchanged (no reduction for *3)
              (equal r9 '(binop * (var x) (const 3)))))
    (fmakunbound 'neovm--copt-sr-power-of-2-p)
    (fmakunbound 'neovm--copt-sr-reduce)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((binop + (var x) (var x)) (binop lshift (var x) (const 3)) (binop rshift (var x) (const 2)) (binop bitand (var x) (const 15)) (const 0) (var x) (var x) (binop lshift (var y) (const 2)) (binop * (var x) (const 3)) t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Peephole optimization on instruction sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_opt_peephole() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Peephole patterns on a stack-machine instruction set:
    //   (push X) (pop) => eliminated (push-pop pair)
    //   (push X) (push Y) (add) => (push (+ X Y)) if both constant
    //   (load X) (store X) => eliminated (no-op store to same)
    //   (jump L) where L is next instruction => eliminated
    //   (dup) (pop) => eliminated
    let form = r#"
(progn
  (fset 'neovm--copt-ph-optimize
    (lambda (instrs)
      "Apply peephole optimization rules in one pass."
      (let ((result nil)
            (i 0)
            (len (length instrs)))
        (while (< i len)
          (let ((curr (nth i instrs))
                (next (when (< (1+ i) len) (nth (1+ i) instrs))))
            (cond
             ;; push X, pop => skip both
             ((and (eq (car curr) 'push)
                   next (eq (car next) 'pop))
              (setq i (+ i 2)))
             ;; dup, pop => skip both
             ((and (eq (car curr) 'dup)
                   next (eq (car next) 'pop))
              (setq i (+ i 2)))
             ;; push X, push Y, add => push (X+Y) if both constant
             ((and (eq (car curr) 'push)
                   (numberp (cadr curr))
                   next
                   (eq (car next) 'push)
                   (numberp (cadr next))
                   (< (+ i 2) len)
                   (eq (car (nth (+ i 2) instrs)) 'add))
              (push (list 'push (+ (cadr curr) (cadr next))) result)
              (setq i (+ i 3)))
             ;; load X, store X => skip both
             ((and (eq (car curr) 'load)
                   next (eq (car next) 'store)
                   (eq (cadr curr) (cadr next)))
              (setq i (+ i 2)))
             ;; jump L where L is label at i+1 => skip jump
             ((and (eq (car curr) 'jump)
                   next (eq (car next) 'label)
                   (eq (cadr curr) (cadr next)))
              (setq i (1+ i)))
             ;; Default: keep instruction
             (t (push curr result)
                (setq i (1+ i))))))
        (nreverse result))))

  (fset 'neovm--copt-ph-multi-pass
    (lambda (instrs max-passes)
      "Apply peephole optimizer repeatedly until stable or max-passes."
      (let ((current instrs)
            (pass 0))
        (while (< pass max-passes)
          (let ((optimized (funcall 'neovm--copt-ph-optimize current)))
            (if (equal optimized current)
                (setq pass max-passes)  ;; Stable
              (setq current optimized
                    pass (1+ pass)))))
        current)))

  (unwind-protect
      (let* (;; push-pop elimination
             (p1 '((push 5) (pop) (push 10) (ret)))
             (o1 (funcall 'neovm--copt-ph-optimize p1))
             ;; Constant folding via push-push-add
             (p2 '((push 3) (push 7) (add) (push 2) (mul) (ret)))
             (o2 (funcall 'neovm--copt-ph-optimize p2))
             ;; load-store elimination
             (p3 '((load x) (store x) (load y) (push 1) (add) (store y) (ret)))
             (o3 (funcall 'neovm--copt-ph-optimize p3))
             ;; Jump to next label elimination
             (p4 '((push 1) (jump L1) (label L1) (push 2) (add) (ret)))
             (o4 (funcall 'neovm--copt-ph-optimize p4))
             ;; dup-pop elimination
             (p5 '((load x) (dup) (pop) (push 1) (add) (ret)))
             (o5 (funcall 'neovm--copt-ph-optimize p5))
             ;; Multi-pass: push 1, push 2, add => push 3, then push 3, push 4, add => push 7
             (p6 '((push 1) (push 2) (add) (push 4) (add) (ret)))
             (o6 (funcall 'neovm--copt-ph-multi-pass p6 5)))
        (list
         o1 (length o1)   ;; push-pop removed
         o2               ;; 3+7 folded to 10
         o3 (length o3)   ;; load x, store x removed
         o4               ;; jump L1 removed
         o5 (length o5)   ;; dup-pop removed
         o6))             ;; Multi-pass should fold all constants
    (fmakunbound 'neovm--copt-ph-optimize)
    (fmakunbound 'neovm--copt-ph-multi-pass)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((push 10) (ret)) 2 ((push 10) (push 2) (mul) (ret)) ((load y) (push 1) (add) (store y) (ret)) 5 ((push 1) (label L1) (push 2) (add) (ret)) ((load x) (push 1) (add) (ret)) 4 ((push 7) (ret)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Basic block construction from linear instruction stream
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_opt_basic_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build basic blocks: maximal sequences of straight-line code.
    // Block starts: program entry, jump target labels, instruction after branch.
    // Block ends: jump/branch instructions, instruction before a label.
    let form = r#"
(progn
  (fset 'neovm--copt-bb-build
    (lambda (instrs)
      "Build basic blocks from instruction list.
       Returns list of (block-id start-idx end-idx instructions successors)."
      ;; First pass: find leaders (block start indices)
      (let ((leaders (make-hash-table))
            (label-idx (make-hash-table)))
        (puthash 0 t leaders)  ;; First instruction is always a leader
        (let ((i 0))
          (dolist (instr instrs)
            (cond
             ((eq (car instr) 'label)
              (puthash i t leaders)
              (puthash (cadr instr) i label-idx))
             ((memq (car instr) '(jump branch))
              ;; Instruction after branch is a leader
              (when (< (1+ i) (length instrs))
                (puthash (1+ i) t leaders))))
            (setq i (1+ i))))
        ;; Second pass: build blocks
        (let ((blocks nil)
              (block-id 0)
              (block-start 0)
              (block-instrs nil)
              (i 0))
          (dolist (instr instrs)
            (when (and (> i 0) (gethash i leaders))
              ;; End current block, start new one
              (push (list block-id block-start (1- i) (nreverse block-instrs))
                    blocks)
              (setq block-id (1+ block-id)
                    block-start i
                    block-instrs nil))
            (push instr block-instrs)
            (setq i (1+ i)))
          ;; Finish last block
          (when block-instrs
            (push (list block-id block-start (1- i) (nreverse block-instrs))
                  blocks))
          (nreverse blocks)))))

  (fset 'neovm--copt-bb-count-instrs
    (lambda (blocks)
      "Count total instructions across all blocks."
      (let ((total 0))
        (dolist (b blocks)
          (setq total (+ total (length (nth 3 b)))))
        total)))

  (unwind-protect
      (let* ((prog1 '((push 1)
                       (push 2)
                       (add)
                       (branch done)
                       (push 3)
                       (push 4)
                       (add)
                       (label done)
                       (ret)))
             (blocks1 (funcall 'neovm--copt-bb-build prog1))
             ;; More complex: if-else
             (prog2 '((load x)
                       (push 0)
                       (cmp)
                       (branch else)
                       (push 1)
                       (jump end)
                       (label else)
                       (push 0)
                       (label end)
                       (store result)
                       (ret)))
             (blocks2 (funcall 'neovm--copt-bb-build prog2))
             ;; Loop
             (prog3 '((push 0)
                       (store sum)
                       (push 10)
                       (store i)
                       (label loop)
                       (load i)
                       (push 0)
                       (cmp)
                       (branch done)
                       (load sum)
                       (load i)
                       (add)
                       (store sum)
                       (load i)
                       (push 1)
                       (sub)
                       (store i)
                       (jump loop)
                       (label done)
                       (load sum)
                       (ret)))
             (blocks3 (funcall 'neovm--copt-bb-build prog3)))
        (list
         ;; prog1: should have 3 blocks
         (length blocks1)
         (funcall 'neovm--copt-bb-count-instrs blocks1)
         (mapcar (lambda (b) (length (nth 3 b))) blocks1)
         ;; prog2: should have 4 blocks
         (length blocks2)
         (mapcar (lambda (b) (length (nth 3 b))) blocks2)
         ;; prog3: loop has 4 blocks
         (length blocks3)
         (mapcar (lambda (b) (list (nth 0 b) (length (nth 3 b)))) blocks3)
         ;; Total instructions preserved
         (= (funcall 'neovm--copt-bb-count-instrs blocks1) (length prog1))
         (= (funcall 'neovm--copt-bb-count-instrs blocks2) (length prog2))
         (= (funcall 'neovm--copt-bb-count-instrs blocks3) (length prog3))))
    (fmakunbound 'neovm--copt-bb-build)
    (fmakunbound 'neovm--copt-bb-count-instrs)))
"#;
    let expect = expect_test::expect![[
        r#""OK (3 9 (4 3 2) 4 (4 2 2 3) 4 ((0 4) (1 5) (2 9) (3 3)) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Data flow analysis: live variable analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_opt_liveness_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute live variables at each program point using backward analysis.
    // A variable is live at a point if it may be used before being redefined.
    // live_in = (live_out - defs) | uses
    let form = r#"
(progn
  (fset 'neovm--copt-lv-uses
    (lambda (instr)
      "Variables used (read) by INSTR."
      (cond
       ((eq (car instr) 'assign)
        ;; Uses are in the RHS expression
        (let ((expr (nth 2 instr)))
          (funcall 'neovm--copt-lv-expr-vars expr)))
       ((memq (car instr) '(return print))
        (funcall 'neovm--copt-lv-expr-vars (cadr instr)))
       (t nil))))

  (fset 'neovm--copt-lv-defs
    (lambda (instr)
      "Variables defined (written) by INSTR."
      (if (eq (car instr) 'assign)
          (list (cadr instr))
        nil)))

  (fset 'neovm--copt-lv-expr-vars
    (lambda (expr)
      "Extract all variable references from an expression."
      (cond
       ((eq (car expr) 'var) (list (cadr expr)))
       ((eq (car expr) 'const) nil)
       ((eq (car expr) 'binop)
        (append (funcall 'neovm--copt-lv-expr-vars (nth 2 expr))
                (funcall 'neovm--copt-lv-expr-vars (nth 3 expr))))
       (t nil))))

  (fset 'neovm--copt-lv-set-union
    (lambda (s1 s2)
      "Union of two sets (lists with no duplicates)."
      (let ((result (copy-sequence s1)))
        (dolist (x s2)
          (unless (memq x result)
            (push x result)))
        (sort result (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  (fset 'neovm--copt-lv-set-diff
    (lambda (s1 s2)
      "Set difference s1 - s2."
      (let ((result nil))
        (dolist (x s1)
          (unless (memq x s2)
            (push x result)))
        (sort result (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  (fset 'neovm--copt-lv-analyze
    (lambda (instrs)
      "Compute live variables at each point (before each instruction).
       Returns list of (instruction live-before live-after)."
      (let* ((n (length instrs))
             (live-out (make-vector n nil))
             (live-in (make-vector n nil))
             (changed t))
        ;; Iterate until fixed point
        (while changed
          (setq changed nil)
          ;; Process instructions backward
          (let ((i (1- n)))
            (while (>= i 0)
              (let ((instr (nth i instrs)))
                ;; live_out[i] = live_in[i+1] (for straight-line code)
                (let ((new-out (if (< (1+ i) n) (aref live-in (1+ i)) nil)))
                  (unless (equal new-out (aref live-out i))
                    (aset live-out i new-out)
                    (setq changed t))
                  ;; live_in[i] = (live_out[i] - defs[i]) | uses[i]
                  (let* ((uses (funcall 'neovm--copt-lv-uses instr))
                         (defs (funcall 'neovm--copt-lv-defs instr))
                         (new-in (funcall 'neovm--copt-lv-set-union
                                          uses
                                          (funcall 'neovm--copt-lv-set-diff
                                                   new-out defs))))
                    (unless (equal new-in (aref live-in i))
                      (aset live-in i new-in)
                      (setq changed t)))))
              (setq i (1- i)))))
        ;; Build result
        (let ((result nil) (i 0))
          (dolist (instr instrs)
            (push (list instr (aref live-in i) (aref live-out i)) result)
            (setq i (1+ i)))
          (nreverse result)))))

  (unwind-protect
      (let* (;; x = a + b; y = x * 2; z = a + 1; return y
             ;; At return: y is live
             ;; Before return: {y}
             ;; Before z=a+1: {y, a} (z is dead)
             ;; Before y=x*2: {x, a} -> wait, a is needed by z=a+1
             ;; Before x=a+b: {a, b}
             (prog1 '((assign x (binop + (var a) (var b)))
                       (assign y (binop * (var x) (const 2)))
                       (assign z (binop + (var a) (const 1)))
                       (return (var y))))
             (analysis1 (funcall 'neovm--copt-lv-analyze prog1))
             ;; Extract live-before for each instruction
             (live-befores (mapcar #'cadr analysis1))
             ;; Detect dead assignment: z is never in any live-out after its def
             (z-live-anywhere (let ((found nil))
                                (dolist (entry analysis1)
                                  (when (memq 'z (nth 2 entry))
                                    (setq found t)))
                                found)))
        (list
         ;; Live vars analysis result
         (length analysis1)
         live-befores
         ;; z should not be live anywhere after assignment
         (not z-live-anywhere)
         ;; At program start, a and b should be live (needed by first instruction)
         (let ((first-live (car live-befores)))
           (list (memq 'a first-live) (memq 'b first-live)))))
    (fmakunbound 'neovm--copt-lv-uses)
    (fmakunbound 'neovm--copt-lv-defs)
    (fmakunbound 'neovm--copt-lv-expr-vars)
    (fmakunbound 'neovm--copt-lv-set-union)
    (fmakunbound 'neovm--copt-lv-set-diff)
    (fmakunbound 'neovm--copt-lv-analyze)))
"#;
    let expect = expect_test::expect![[r#""OK (4 ((a b) (a x) (a y) (y)) t ((a b) (b)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
