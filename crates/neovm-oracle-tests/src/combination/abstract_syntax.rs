//! Oracle parity tests for an AST transformation framework in Elisp.
//!
//! AST node types:
//!   (literal val)
//!   (binop op left right)
//!   (unary op expr)
//!   (if-expr cond then else)
//!   (let-expr bindings body)
//!
//! Implements: AST pretty printer, constant folding, dead code elimination,
//! common subexpression elimination, and AST-to-stack-machine compilation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 1. AST construction and pretty printing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ast_construction_and_pretty_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build various AST nodes and pretty-print them to human-readable
    // infix expressions.
    let form = r#"(unwind-protect
      (progn
        ;; AST constructors
        (defun test-ast--literal (val) (list 'literal val))
        (defun test-ast--binop (op left right) (list 'binop op left right))
        (defun test-ast--unary (op expr) (list 'unary op expr))
        (defun test-ast--if-expr (cond then else-branch)
          (list 'if-expr cond then else-branch))
        (defun test-ast--let-expr (bindings body)
          (list 'let-expr bindings body))

        ;; Type predicates
        (defun test-ast--node-type (node) (car node))

        ;; Pretty printer: converts AST to human-readable string
        (defun test-ast--pretty (node)
          (let ((type (test-ast--node-type node)))
            (cond
              ((eq type 'literal)
               (format "%s" (cadr node)))
              ((eq type 'binop)
               (format "(%s %s %s)"
                       (test-ast--pretty (caddr node))
                       (cadr node)
                       (test-ast--pretty (cadddr node))))
              ((eq type 'unary)
               (format "%s(%s)" (cadr node) (test-ast--pretty (caddr node))))
              ((eq type 'if-expr)
               (format "if %s then %s else %s"
                       (test-ast--pretty (cadr (cdr node)))
                       (test-ast--pretty (caddr (cdr node)))
                       (test-ast--pretty (cadddr (cdr node)))))
              ((eq type 'let-expr)
               (let ((bindings-str
                      (mapconcat
                        (lambda (b)
                          (format "%s = %s" (car b) (test-ast--pretty (cdr b))))
                        (cadr (cdr node))
                        ", ")))
                 (format "let %s in %s" bindings-str
                         (test-ast--pretty (caddr (cdr node))))))
              (t (format "<?%s>" type)))))

        ;; Test cases
        (list
          ;; Simple literal
          (test-ast--pretty (test-ast--literal 42))
          ;; Binary operation: 3 + 5
          (test-ast--pretty
            (test-ast--binop '+ (test-ast--literal 3) (test-ast--literal 5)))
          ;; Nested: (2 * 3) + (4 * 5)
          (test-ast--pretty
            (test-ast--binop '+
              (test-ast--binop '* (test-ast--literal 2) (test-ast--literal 3))
              (test-ast--binop '* (test-ast--literal 4) (test-ast--literal 5))))
          ;; Unary: neg(7)
          (test-ast--pretty
            (test-ast--unary 'neg (test-ast--literal 7)))
          ;; If expression
          (test-ast--pretty
            (test-ast--if-expr
              (test-ast--literal t)
              (test-ast--literal 1)
              (test-ast--literal 0)))
          ;; Let expression
          (test-ast--pretty
            (test-ast--let-expr
              (list (cons 'x (test-ast--literal 10))
                    (cons 'y (test-ast--literal 20)))
              (test-ast--binop '+ (test-ast--literal 'x) (test-ast--literal 'y))))
          ;; Complex nested: neg(if t then (1 + 2) else (3 * 4))
          (test-ast--pretty
            (test-ast--unary 'neg
              (test-ast--if-expr
                (test-ast--literal t)
                (test-ast--binop '+ (test-ast--literal 1) (test-ast--literal 2))
                (test-ast--binop '* (test-ast--literal 3) (test-ast--literal 4)))))))
      ;; Cleanup
      (fmakunbound 'test-ast--literal)
      (fmakunbound 'test-ast--binop)
      (fmakunbound 'test-ast--unary)
      (fmakunbound 'test-ast--if-expr)
      (fmakunbound 'test-ast--let-expr)
      (fmakunbound 'test-ast--node-type)
      (fmakunbound 'test-ast--pretty))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp binop)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2. Constant folding pass
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ast_constant_folding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Constant folding: when both operands of a binop are literals,
    // evaluate at compile time and replace with a literal.
    let form = r#"(unwind-protect
      (progn
        (defun test-cf--literal (val) (list 'literal val))
        (defun test-cf--binop (op left right) (list 'binop op left right))
        (defun test-cf--unary (op expr) (list 'unary op expr))
        (defun test-cf--if-expr (cond then else-branch)
          (list 'if-expr cond then else-branch))
        (defun test-cf--node-type (node) (car node))

        ;; Apply binary operation
        (defun test-cf--apply-binop (op a b)
          (cond
            ((eq op '+) (+ a b))
            ((eq op '-) (- a b))
            ((eq op '*) (* a b))
            ((eq op '/) (if (= b 0) nil (/ a b)))
            (t nil)))

        ;; Apply unary operation
        (defun test-cf--apply-unary (op a)
          (cond
            ((eq op 'neg) (- a))
            ((eq op 'not) (not a))
            (t nil)))

        ;; Constant folding pass
        (defun test-cf--fold (node)
          (let ((type (test-cf--node-type node)))
            (cond
              ((eq type 'literal) node)
              ((eq type 'binop)
               (let* ((op (cadr node))
                      (left (test-cf--fold (caddr node)))
                      (right (test-cf--fold (cadddr node))))
                 (if (and (eq (test-cf--node-type left) 'literal)
                          (eq (test-cf--node-type right) 'literal)
                          (numberp (cadr left))
                          (numberp (cadr right)))
                     (let ((result (test-cf--apply-binop op (cadr left) (cadr right))))
                       (if result
                           (test-cf--literal result)
                         (test-cf--binop op left right)))
                   (test-cf--binop op left right))))
              ((eq type 'unary)
               (let* ((op (cadr node))
                      (expr (test-cf--fold (caddr node))))
                 (if (and (eq (test-cf--node-type expr) 'literal)
                          (numberp (cadr expr)))
                     (let ((result (test-cf--apply-unary op (cadr expr))))
                       (if result
                           (test-cf--literal result)
                         (test-cf--unary op expr)))
                   (test-cf--unary op expr))))
              ((eq type 'if-expr)
               (let ((cond-folded (test-cf--fold (cadr (cdr node))))
                     (then-folded (test-cf--fold (caddr (cdr node))))
                     (else-folded (test-cf--fold (cadddr (cdr node)))))
                 (list 'if-expr cond-folded then-folded else-folded)))
              (t node))))

        (list
          ;; 3 + 5 -> 8
          (test-cf--fold (test-cf--binop '+ (test-cf--literal 3) (test-cf--literal 5)))
          ;; (2 * 3) + (4 * 5) -> 6 + 20 -> 26
          (test-cf--fold
            (test-cf--binop '+
              (test-cf--binop '* (test-cf--literal 2) (test-cf--literal 3))
              (test-cf--binop '* (test-cf--literal 4) (test-cf--literal 5))))
          ;; neg(7) -> -7
          (test-cf--fold (test-cf--unary 'neg (test-cf--literal 7)))
          ;; (x + 3) cannot be folded (x is not a number literal)
          (test-cf--fold
            (test-cf--binop '+ (test-cf--literal 'x) (test-cf--literal 3)))
          ;; Nested: (1 + 2) * (3 + 4) -> 3 * 7 -> 21
          (test-cf--fold
            (test-cf--binop '*
              (test-cf--binop '+ (test-cf--literal 1) (test-cf--literal 2))
              (test-cf--binop '+ (test-cf--literal 3) (test-cf--literal 4))))
          ;; Division by zero: don't fold
          (test-cf--fold
            (test-cf--binop '/ (test-cf--literal 10) (test-cf--literal 0)))
          ;; Mixed: (2 + 3) * x -> 5 * x
          (test-cf--fold
            (test-cf--binop '*
              (test-cf--binop '+ (test-cf--literal 2) (test-cf--literal 3))
              (test-cf--literal 'x)))
          ;; Deep nesting: ((1+1) + (2+2)) + ((3+3) + (4+4)) -> 20
          (test-cf--fold
            (test-cf--binop '+
              (test-cf--binop '+
                (test-cf--binop '+ (test-cf--literal 1) (test-cf--literal 1))
                (test-cf--binop '+ (test-cf--literal 2) (test-cf--literal 2)))
              (test-cf--binop '+
                (test-cf--binop '+ (test-cf--literal 3) (test-cf--literal 3))
                (test-cf--binop '+ (test-cf--literal 4) (test-cf--literal 4)))))))
      ;; Cleanup
      (fmakunbound 'test-cf--literal)
      (fmakunbound 'test-cf--binop)
      (fmakunbound 'test-cf--unary)
      (fmakunbound 'test-cf--if-expr)
      (fmakunbound 'test-cf--node-type)
      (fmakunbound 'test-cf--apply-binop)
      (fmakunbound 'test-cf--apply-unary)
      (fmakunbound 'test-cf--fold))"#;
    let expect = expect_test::expect![[
        r#""OK ((literal 8) (literal 26) (literal -7) (binop + (literal x) (literal 3)) (literal 21) (binop / (literal 10) (literal 0)) (binop * (literal 5) (literal x)) (literal 20))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 3. Dead code elimination (if with constant condition)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ast_dead_code_elimination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When an if-expr has a constant-known condition, replace the entire
    // if-expr with the appropriate branch.
    let form = r#"(unwind-protect
      (progn
        (defun test-dce--literal (val) (list 'literal val))
        (defun test-dce--binop (op left right) (list 'binop op left right))
        (defun test-dce--unary (op expr) (list 'unary op expr))
        (defun test-dce--if-expr (cond then else-branch)
          (list 'if-expr cond then else-branch))
        (defun test-dce--let-expr (bindings body)
          (list 'let-expr bindings body))
        (defun test-dce--node-type (node) (car node))

        ;; Dead code elimination pass
        (defun test-dce--eliminate (node)
          (let ((type (test-dce--node-type node)))
            (cond
              ((eq type 'literal) node)
              ((eq type 'binop)
               (test-dce--binop (cadr node)
                                (test-dce--eliminate (caddr node))
                                (test-dce--eliminate (cadddr node))))
              ((eq type 'unary)
               (test-dce--unary (cadr node)
                                (test-dce--eliminate (caddr node))))
              ((eq type 'if-expr)
               (let ((cond-node (test-dce--eliminate (cadr (cdr node))))
                     (then-node (test-dce--eliminate (caddr (cdr node))))
                     (else-node (test-dce--eliminate (cadddr (cdr node)))))
                 ;; If condition is a literal, we know which branch to take
                 (if (eq (test-dce--node-type cond-node) 'literal)
                     (if (cadr cond-node) then-node else-node)
                   (test-dce--if-expr cond-node then-node else-node))))
              ((eq type 'let-expr)
               (test-dce--let-expr
                 (mapcar (lambda (b)
                           (cons (car b) (test-dce--eliminate (cdr b))))
                         (cadr (cdr node)))
                 (test-dce--eliminate (caddr (cdr node)))))
              (t node))))

        (list
          ;; if true then A else B -> A
          (test-dce--eliminate
            (test-dce--if-expr
              (test-dce--literal t)
              (test-dce--literal 42)
              (test-dce--literal 0)))
          ;; if nil then A else B -> B
          (test-dce--eliminate
            (test-dce--if-expr
              (test-dce--literal nil)
              (test-dce--literal 42)
              (test-dce--literal 0)))
          ;; if x then A else B -> unchanged (x is not a known constant)
          (test-dce--eliminate
            (test-dce--if-expr
              (test-dce--literal 'x)
              (test-dce--literal 1)
              (test-dce--literal 2)))
          ;; Nested: if true then (if nil then X else Y) else Z -> Y
          (test-dce--eliminate
            (test-dce--if-expr
              (test-dce--literal t)
              (test-dce--if-expr
                (test-dce--literal nil)
                (test-dce--literal 'X)
                (test-dce--literal 'Y))
              (test-dce--literal 'Z)))
          ;; Dead code in binop: (if true then 3 else 5) + 7 -> 3 + 7
          (test-dce--eliminate
            (test-dce--binop '+
              (test-dce--if-expr
                (test-dce--literal t)
                (test-dce--literal 3)
                (test-dce--literal 5))
              (test-dce--literal 7)))
          ;; Let with dead code in body
          (test-dce--eliminate
            (test-dce--let-expr
              (list (cons 'a (test-dce--literal 10)))
              (test-dce--if-expr
                (test-dce--literal nil)
                (test-dce--literal 'dead)
                (test-dce--binop '+ (test-dce--literal 'a) (test-dce--literal 1)))))))
      ;; Cleanup
      (fmakunbound 'test-dce--literal)
      (fmakunbound 'test-dce--binop)
      (fmakunbound 'test-dce--unary)
      (fmakunbound 'test-dce--if-expr)
      (fmakunbound 'test-dce--let-expr)
      (fmakunbound 'test-dce--node-type)
      (fmakunbound 'test-dce--eliminate))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp if-expr)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 4. Common subexpression elimination (CSE)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ast_common_subexpression_elimination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find common subexpressions in the AST and replace duplicates
    // with references to a shared let-binding.
    let form = r#"(unwind-protect
      (progn
        (defun test-cse--literal (val) (list 'literal val))
        (defun test-cse--binop (op left right) (list 'binop op left right))
        (defun test-cse--unary (op expr) (list 'unary op expr))
        (defun test-cse--let-expr (bindings body) (list 'let-expr bindings body))
        (defun test-cse--node-type (node) (car node))
        (defun test-cse--var (name) (list 'var name))

        ;; Serialize AST node to a canonical string for comparison
        (defun test-cse--serialize (node)
          (let ((type (test-cse--node-type node)))
            (cond
              ((eq type 'literal) (format "L:%s" (cadr node)))
              ((eq type 'binop)
               (format "B:%s:%s:%s" (cadr node)
                       (test-cse--serialize (caddr node))
                       (test-cse--serialize (cadddr node))))
              ((eq type 'unary)
               (format "U:%s:%s" (cadr node)
                       (test-cse--serialize (caddr node))))
              ((eq type 'var) (format "V:%s" (cadr node)))
              (t (format "?:%s" type)))))

        ;; Count occurrences of each subexpression
        (defun test-cse--count-subexprs (node table)
          (let ((key (test-cse--serialize node)))
            (puthash key
                     (cons node (1+ (or (cdr (gethash key table)) 0)))
                     table))
          (let ((type (test-cse--node-type node)))
            (cond
              ((eq type 'binop)
               (test-cse--count-subexprs (caddr node) table)
               (test-cse--count-subexprs (cadddr node) table))
              ((eq type 'unary)
               (test-cse--count-subexprs (caddr node) table)))))

        ;; Replace duplicate subexpressions with variable references
        (defun test-cse--replace (node table counter)
          (let* ((key (test-cse--serialize node))
                 (entry (gethash key table)))
            (if (and entry (> (cdr entry) 1)
                     (not (eq (test-cse--node-type node) 'literal)))
                ;; This subexpression appears multiple times
                (let ((var-name (intern (format "_cse%d" (car counter)))))
                  (setcar counter (1+ (car counter)))
                  (puthash key (cons (car entry) 1) table)  ;; prevent re-replace
                  (test-cse--var var-name))
              ;; Recurse into children
              (let ((type (test-cse--node-type node)))
                (cond
                  ((eq type 'binop)
                   (test-cse--binop (cadr node)
                     (test-cse--replace (caddr node) table counter)
                     (test-cse--replace (cadddr node) table counter)))
                  ((eq type 'unary)
                   (test-cse--unary (cadr node)
                     (test-cse--replace (caddr node) table counter)))
                  (t node))))))

        ;; Full CSE pass: returns (eliminated-ast . list-of-common-subexprs)
        (defun test-cse--run (node)
          (let ((table (make-hash-table :test 'equal)))
            (test-cse--count-subexprs node table)
            ;; Collect subexpressions with count > 1
            (let ((common nil))
              (maphash (lambda (k v)
                         (when (and (> (cdr v) 1)
                                    (not (eq (test-cse--node-type (car v)) 'literal)))
                           (push (list k (cdr v) (test-cse--serialize (car v))) common)))
                       table)
              (let* ((counter (list 0))
                     (new-ast (test-cse--replace node table counter)))
                (list new-ast (length common))))))

        ;; Test: (a + b) * (a + b) has common subexpr (a + b)
        (let* ((a-plus-b (test-cse--binop '+ (test-cse--literal 'a) (test-cse--literal 'b)))
               (expr (test-cse--binop '* a-plus-b a-plus-b)))
          (list
            ;; Serialization
            (test-cse--serialize (test-cse--literal 42))
            (test-cse--serialize a-plus-b)
            (test-cse--serialize expr)
            ;; CSE result
            (test-cse--run expr)
            ;; No common subexprs
            (test-cse--run (test-cse--binop '+ (test-cse--literal 1) (test-cse--literal 2)))
            ;; Triple common subexpr: (a+b) + (a+b) + (a+b)
            (test-cse--run
              (test-cse--binop '+
                a-plus-b
                (test-cse--binop '+ a-plus-b a-plus-b))))))
      ;; Cleanup
      (fmakunbound 'test-cse--literal)
      (fmakunbound 'test-cse--binop)
      (fmakunbound 'test-cse--unary)
      (fmakunbound 'test-cse--let-expr)
      (fmakunbound 'test-cse--node-type)
      (fmakunbound 'test-cse--var)
      (fmakunbound 'test-cse--serialize)
      (fmakunbound 'test-cse--count-subexprs)
      (fmakunbound 'test-cse--replace)
      (fmakunbound 'test-cse--run))"#;
    let expect = expect_test::expect![[
        r#""OK (\"L:42\" \"B:+:L:a:L:b\" \"B:*:B:+:L:a:L:b:B:+:L:a:L:b\" ((binop * (var _cse0) (binop + (literal a) (literal b))) 1) ((binop + (literal 1) (literal 2)) 0) ((binop + (var _cse0) (binop + (binop + (literal a) (literal b)) (binop + (literal a) (literal b)))) 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 5. AST to stack machine compilation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ast_to_stack_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compile AST to a list of stack machine instructions:
    //   (push val), (add), (sub), (mul), (div), (neg),
    //   (jmpf label), (jmp label), (label name)
    // Then execute the stack machine to verify correctness.
    let form = r#"(unwind-protect
      (progn
        (defun test-sm--literal (val) (list 'literal val))
        (defun test-sm--binop (op left right) (list 'binop op left right))
        (defun test-sm--unary (op expr) (list 'unary op expr))
        (defun test-sm--if-expr (cond then else-branch)
          (list 'if-expr cond then else-branch))
        (defun test-sm--node-type (node) (car node))

        ;; Compile AST to stack machine instructions
        (defun test-sm--compile (node label-counter)
          (let ((type (test-sm--node-type node)))
            (cond
              ((eq type 'literal)
               (list (list 'push (cadr node))))
              ((eq type 'binop)
               (let ((op (cadr node))
                     (left-code (test-sm--compile (caddr node) label-counter))
                     (right-code (test-sm--compile (cadddr node) label-counter)))
                 (let ((op-instr (cond
                                   ((eq op '+) '(add))
                                   ((eq op '-) '(sub))
                                   ((eq op '*) '(mul))
                                   ((eq op '/) '(div))
                                   (t (list 'binop op)))))
                   (append left-code right-code (list op-instr)))))
              ((eq type 'unary)
               (let ((op (cadr node))
                     (expr-code (test-sm--compile (caddr node) label-counter)))
                 (let ((op-instr (cond
                                   ((eq op 'neg) '(neg))
                                   (t (list 'unary op)))))
                   (append expr-code (list op-instr)))))
              ((eq type 'if-expr)
               (let* ((lbl-else (format "L%d" (car label-counter)))
                      (_ (setcar label-counter (1+ (car label-counter))))
                      (lbl-end (format "L%d" (car label-counter)))
                      (_ (setcar label-counter (1+ (car label-counter))))
                      (cond-code (test-sm--compile (cadr (cdr node)) label-counter))
                      (then-code (test-sm--compile (caddr (cdr node)) label-counter))
                      (else-code (test-sm--compile (cadddr (cdr node)) label-counter)))
                 (append cond-code
                         (list (list 'jmpf lbl-else))
                         then-code
                         (list (list 'jmp lbl-end))
                         (list (list 'label lbl-else))
                         else-code
                         (list (list 'label lbl-end)))))
              (t (list (list 'error "unknown node"))))))

        ;; Execute stack machine program and return final stack top
        (defun test-sm--execute (program)
          (let ((stack nil)
                (pc 0)
                (len (length program))
                ;; Build label->index map
                (labels (make-hash-table :test 'equal)))
            ;; First pass: find labels
            (let ((i 0))
              (while (< i len)
                (let ((instr (nth i program)))
                  (when (eq (car instr) 'label)
                    (puthash (cadr instr) i labels)))
                (setq i (1+ i))))
            ;; Execute
            (while (< pc len)
              (let ((instr (nth pc program)))
                (cond
                  ((eq (car instr) 'push)
                   (push (cadr instr) stack)
                   (setq pc (1+ pc)))
                  ((eq (car instr) 'add)
                   (let ((b (pop stack)) (a (pop stack)))
                     (push (+ a b) stack))
                   (setq pc (1+ pc)))
                  ((eq (car instr) 'sub)
                   (let ((b (pop stack)) (a (pop stack)))
                     (push (- a b) stack))
                   (setq pc (1+ pc)))
                  ((eq (car instr) 'mul)
                   (let ((b (pop stack)) (a (pop stack)))
                     (push (* a b) stack))
                   (setq pc (1+ pc)))
                  ((eq (car instr) 'div)
                   (let ((b (pop stack)) (a (pop stack)))
                     (push (/ a b) stack))
                   (setq pc (1+ pc)))
                  ((eq (car instr) 'neg)
                   (push (- (pop stack)) stack)
                   (setq pc (1+ pc)))
                  ((eq (car instr) 'jmpf)
                   (let ((val (pop stack)))
                     (if (not val)
                         (setq pc (gethash (cadr instr) labels))
                       (setq pc (1+ pc)))))
                  ((eq (car instr) 'jmp)
                   (setq pc (gethash (cadr instr) labels)))
                  ((eq (car instr) 'label)
                   (setq pc (1+ pc)))
                  (t (setq pc (1+ pc))))))
            (car stack)))

        (list
          ;; Compile and execute: 3 + 5 = 8
          (let* ((ast (test-sm--binop '+ (test-sm--literal 3) (test-sm--literal 5)))
                 (code (test-sm--compile ast (list 0))))
            (list code (test-sm--execute code)))
          ;; (2 * 3) + (4 * 5) = 26
          (let* ((ast (test-sm--binop '+
                        (test-sm--binop '* (test-sm--literal 2) (test-sm--literal 3))
                        (test-sm--binop '* (test-sm--literal 4) (test-sm--literal 5))))
                 (code (test-sm--compile ast (list 0))))
            (list (test-sm--execute code)))
          ;; neg(7) = -7
          (let* ((ast (test-sm--unary 'neg (test-sm--literal 7)))
                 (code (test-sm--compile ast (list 0))))
            (list code (test-sm--execute code)))
          ;; if t then 42 else 0 = 42
          (let* ((ast (test-sm--if-expr
                        (test-sm--literal t)
                        (test-sm--literal 42)
                        (test-sm--literal 0)))
                 (code (test-sm--compile ast (list 0))))
            (list (test-sm--execute code)))
          ;; if nil then 42 else 99 = 99
          (let* ((ast (test-sm--if-expr
                        (test-sm--literal nil)
                        (test-sm--literal 42)
                        (test-sm--literal 99)))
                 (code (test-sm--compile ast (list 0))))
            (list (test-sm--execute code)))
          ;; Complex: if t then (3+4) else (10-2) = 7
          (let* ((ast (test-sm--if-expr
                        (test-sm--literal t)
                        (test-sm--binop '+ (test-sm--literal 3) (test-sm--literal 4))
                        (test-sm--binop '- (test-sm--literal 10) (test-sm--literal 2))))
                 (code (test-sm--compile ast (list 0))))
            (list (test-sm--execute code)))))
      ;; Cleanup
      (fmakunbound 'test-sm--literal)
      (fmakunbound 'test-sm--binop)
      (fmakunbound 'test-sm--unary)
      (fmakunbound 'test-sm--if-expr)
      (fmakunbound 'test-sm--node-type)
      (fmakunbound 'test-sm--compile)
      (fmakunbound 'test-sm--execute))"#;
    let expect = expect_test::expect![[
        r#""OK ((((push 3) (push 5) (add)) 8) (26) (((push 7) (neg)) -7) (0) (99) (8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 6. Combined: fold + DCE + compile + execute pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ast_full_optimization_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Apply constant folding, then dead code elimination, then compile
    // to stack machine and execute. Verify each stage.
    let form = r#"(unwind-protect
      (progn
        ;; Node constructors
        (defun test-pipe--literal (val) (list 'literal val))
        (defun test-pipe--binop (op left right) (list 'binop op left right))
        (defun test-pipe--unary (op expr) (list 'unary op expr))
        (defun test-pipe--if-expr (cond then else-branch)
          (list 'if-expr cond then else-branch))
        (defun test-pipe--node-type (node) (car node))

        ;; Constant folding
        (defun test-pipe--apply-binop (op a b)
          (cond ((eq op '+) (+ a b)) ((eq op '-) (- a b))
                ((eq op '*) (* a b))
                ((eq op '/) (if (= b 0) nil (/ a b)))
                (t nil)))
        (defun test-pipe--fold (node)
          (let ((type (test-pipe--node-type node)))
            (cond
              ((eq type 'literal) node)
              ((eq type 'binop)
               (let ((left (test-pipe--fold (caddr node)))
                     (right (test-pipe--fold (cadddr node))))
                 (if (and (eq (test-pipe--node-type left) 'literal)
                          (eq (test-pipe--node-type right) 'literal)
                          (numberp (cadr left)) (numberp (cadr right)))
                     (let ((r (test-pipe--apply-binop (cadr node) (cadr left) (cadr right))))
                       (if r (test-pipe--literal r) (test-pipe--binop (cadr node) left right)))
                   (test-pipe--binop (cadr node) left right))))
              ((eq type 'unary)
               (let ((expr (test-pipe--fold (caddr node))))
                 (if (and (eq (test-pipe--node-type expr) 'literal)
                          (numberp (cadr expr))
                          (eq (cadr node) 'neg))
                     (test-pipe--literal (- (cadr expr)))
                   (test-pipe--unary (cadr node) expr))))
              ((eq type 'if-expr)
               (list 'if-expr
                     (test-pipe--fold (cadr (cdr node)))
                     (test-pipe--fold (caddr (cdr node)))
                     (test-pipe--fold (cadddr (cdr node)))))
              (t node))))

        ;; Dead code elimination
        (defun test-pipe--dce (node)
          (let ((type (test-pipe--node-type node)))
            (cond
              ((eq type 'literal) node)
              ((eq type 'binop)
               (test-pipe--binop (cadr node)
                 (test-pipe--dce (caddr node))
                 (test-pipe--dce (cadddr node))))
              ((eq type 'unary)
               (test-pipe--unary (cadr node) (test-pipe--dce (caddr node))))
              ((eq type 'if-expr)
               (let ((c (test-pipe--dce (cadr (cdr node)))))
                 (if (eq (test-pipe--node-type c) 'literal)
                     (if (cadr c)
                         (test-pipe--dce (caddr (cdr node)))
                       (test-pipe--dce (cadddr (cdr node))))
                   (list 'if-expr c
                         (test-pipe--dce (caddr (cdr node)))
                         (test-pipe--dce (cadddr (cdr node)))))))
              (t node))))

        ;; Compile to stack machine
        (defun test-pipe--compile (node lc)
          (let ((type (test-pipe--node-type node)))
            (cond
              ((eq type 'literal) (list (list 'push (cadr node))))
              ((eq type 'binop)
               (append (test-pipe--compile (caddr node) lc)
                       (test-pipe--compile (cadddr node) lc)
                       (list (list (cond ((eq (cadr node) '+) 'add)
                                         ((eq (cadr node) '-) 'sub)
                                         ((eq (cadr node) '*) 'mul)
                                         ((eq (cadr node) '/) 'div))))))
              ((eq type 'unary)
               (append (test-pipe--compile (caddr node) lc) (list '(neg))))
              (t (list (list 'push 0))))))

        ;; Execute stack machine
        (defun test-pipe--exec (prog)
          (let ((stack nil) (i 0) (len (length prog)))
            (while (< i len)
              (let ((instr (nth i prog)))
                (cond
                  ((eq (car instr) 'push) (push (cadr instr) stack))
                  ((eq (car instr) 'add) (let ((b (pop stack)) (a (pop stack))) (push (+ a b) stack)))
                  ((eq (car instr) 'sub) (let ((b (pop stack)) (a (pop stack))) (push (- a b) stack)))
                  ((eq (car instr) 'mul) (let ((b (pop stack)) (a (pop stack))) (push (* a b) stack)))
                  ((eq (car instr) 'div) (let ((b (pop stack)) (a (pop stack))) (push (/ a b) stack)))
                  ((eq (car instr) 'neg) (push (- (pop stack)) stack))))
              (setq i (1+ i)))
            (car stack)))

        ;; Full pipeline helper
        (defun test-pipe--run (ast)
          (let* ((folded (test-pipe--fold ast))
                 (dced (test-pipe--dce folded))
                 (code (test-pipe--compile dced (list 0)))
                 (result (test-pipe--exec code)))
            (list folded dced code result)))

        ;; Test cases
        (list
          ;; (1 + 2) + (3 + 4) -> fold to 10 -> push 10 -> 10
          (test-pipe--run
            (test-pipe--binop '+
              (test-pipe--binop '+ (test-pipe--literal 1) (test-pipe--literal 2))
              (test-pipe--binop '+ (test-pipe--literal 3) (test-pipe--literal 4))))
          ;; if true then (2*3) else (4*5) -> fold to if true then 6 else 20 -> dce to 6
          (test-pipe--run
            (test-pipe--if-expr
              (test-pipe--literal t)
              (test-pipe--binop '* (test-pipe--literal 2) (test-pipe--literal 3))
              (test-pipe--binop '* (test-pipe--literal 4) (test-pipe--literal 5))))
          ;; neg(neg(42)) -> fold to 42
          (test-pipe--run
            (test-pipe--unary 'neg (test-pipe--unary 'neg (test-pipe--literal 42))))
          ;; (10 - 3) * 2 -> fold to 14
          (test-pipe--run
            (test-pipe--binop '*
              (test-pipe--binop '- (test-pipe--literal 10) (test-pipe--literal 3))
              (test-pipe--literal 2)))))
      ;; Cleanup
      (fmakunbound 'test-pipe--literal)
      (fmakunbound 'test-pipe--binop)
      (fmakunbound 'test-pipe--unary)
      (fmakunbound 'test-pipe--if-expr)
      (fmakunbound 'test-pipe--node-type)
      (fmakunbound 'test-pipe--apply-binop)
      (fmakunbound 'test-pipe--fold)
      (fmakunbound 'test-pipe--dce)
      (fmakunbound 'test-pipe--compile)
      (fmakunbound 'test-pipe--exec)
      (fmakunbound 'test-pipe--run))"#;
    let expect = expect_test::expect![[
        r#""OK (((literal 10) (literal 10) ((push 10)) 10) ((if-expr (literal 6) (literal 20) nil) nil ((push 0)) 0) ((literal 42) (literal 42) ((push 42)) 42) ((literal 14) (literal 14) ((push 14)) 14))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
