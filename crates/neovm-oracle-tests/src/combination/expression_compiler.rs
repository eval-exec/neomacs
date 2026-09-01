//! Oracle parity tests for an expression compiler implemented in Elisp:
//! parse arithmetic expressions to AST, AST optimization (constant folding),
//! code generation to stack machine instructions, stack machine interpreter,
//! variable binding and lookup, multi-expression programs.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Helper: shared compiler infrastructure
// ---------------------------------------------------------------------------

/// Returns the Elisp source that defines the expression compiler functions.
fn compiler_prelude() -> &'static str {
    r#"
  ;; ====== TOKENIZER ======
  ;; Produces tokens: (num . N), (ident . "name"), (op . "+"),
  ;; (lparen), (rparen), (assign), (semi), (kw-let), (kw-print), (comma)
  (fset 'neovm--exc-tokenize
    (lambda (src)
      (let ((tokens nil) (i 0) (len (length src)))
        (while (< i len)
          (let ((ch (aref src i)))
            (cond
             ;; Whitespace
             ((memq ch '(?\s ?\t ?\n)) (setq i (1+ i)))
             ;; Parentheses
             ((= ch ?\() (setq tokens (cons '(lparen) tokens) i (1+ i)))
             ((= ch ?\)) (setq tokens (cons '(rparen) tokens) i (1+ i)))
             ;; Semicolon
             ((= ch ?\;) (setq tokens (cons '(semi) tokens) i (1+ i)))
             ;; Comma
             ((= ch ?,) (setq tokens (cons '(comma) tokens) i (1+ i)))
             ;; Assignment =
             ((= ch ?=) (setq tokens (cons '(assign) tokens) i (1+ i)))
             ;; Operators: + - * / %
             ((memq ch '(?+ ?- ?* ?/ ?%))
              (setq tokens (cons (cons 'op (char-to-string ch)) tokens)
                    i (1+ i)))
             ;; Numbers (multi-digit)
             ((and (>= ch ?0) (<= ch ?9))
              (let ((start i))
                (while (and (< i len) (>= (aref src i) ?0) (<= (aref src i) ?9))
                  (setq i (1+ i)))
                (setq tokens (cons (cons 'num (string-to-number (substring src start i)))
                                   tokens))))
             ;; Identifiers and keywords
             ((or (and (>= ch ?a) (<= ch ?z))
                  (and (>= ch ?A) (<= ch ?Z))
                  (= ch ?_))
              (let ((start i))
                (while (and (< i len)
                            (let ((c (aref src i)))
                              (or (and (>= c ?a) (<= c ?z))
                                  (and (>= c ?A) (<= c ?Z))
                                  (and (>= c ?0) (<= c ?9))
                                  (= c ?_))))
                  (setq i (1+ i)))
                (let ((word (substring src start i)))
                  (setq tokens
                        (cons (cond
                               ((string= word "let") '(kw-let))
                               ((string= word "print") '(kw-print))
                               (t (cons 'ident word)))
                              tokens)))))
             ;; Skip unknown
             (t (setq i (1+ i))))))
        (nreverse tokens))))

  ;; ====== PARSER ======
  ;; Grammar:
  ;;   program  = statement*
  ;;   statement = let-stmt | print-stmt | expr-stmt
  ;;   let-stmt  = "let" IDENT "=" expr ";"
  ;;   print-stmt = "print" expr ";"
  ;;   expr-stmt  = expr ";"
  ;;   expr     = term (('+' | '-') term)*
  ;;   term     = unary (('*' | '/' | '%') unary)*
  ;;   unary    = '-' unary | primary
  ;;   primary  = NUM | IDENT | '(' expr ')'

  (defvar neovm--exc-toks nil)

  (fset 'neovm--exc-peek (lambda () (car neovm--exc-toks)))
  (fset 'neovm--exc-eat
    (lambda ()
      (let ((tok (car neovm--exc-toks)))
        (setq neovm--exc-toks (cdr neovm--exc-toks))
        tok)))
  (fset 'neovm--exc-expect
    (lambda (type)
      (let ((tok (funcall 'neovm--exc-eat)))
        (if (eq (car tok) type) tok
          (signal 'error (list "expected" type "got" tok))))))

  (fset 'neovm--exc-parse-primary
    (lambda ()
      (let ((tok (funcall 'neovm--exc-peek)))
        (cond
         ((eq (car tok) 'num)
          (funcall 'neovm--exc-eat) (list 'lit (cdr tok)))
         ((eq (car tok) 'ident)
          (funcall 'neovm--exc-eat) (list 'var (cdr tok)))
         ((eq (car tok) 'lparen)
          (funcall 'neovm--exc-eat)
          (let ((e (funcall 'neovm--exc-parse-expr)))
            (funcall 'neovm--exc-expect 'rparen)
            e))
         (t (list 'lit 0))))))

  (fset 'neovm--exc-parse-unary
    (lambda ()
      (if (and neovm--exc-toks
               (eq (car (funcall 'neovm--exc-peek)) 'op)
               (string= (cdr (funcall 'neovm--exc-peek)) "-"))
          (progn (funcall 'neovm--exc-eat)
                 (list 'neg (funcall 'neovm--exc-parse-unary)))
        (funcall 'neovm--exc-parse-primary))))

  (fset 'neovm--exc-parse-term
    (lambda ()
      (let ((left (funcall 'neovm--exc-parse-unary)))
        (while (and neovm--exc-toks
                    (eq (car (funcall 'neovm--exc-peek)) 'op)
                    (member (cdr (funcall 'neovm--exc-peek)) '("*" "/" "%")))
          (let ((op (cdr (funcall 'neovm--exc-eat))))
            (setq left (list 'binop op left (funcall 'neovm--exc-parse-unary)))))
        left)))

  (fset 'neovm--exc-parse-expr
    (lambda ()
      (let ((left (funcall 'neovm--exc-parse-term)))
        (while (and neovm--exc-toks
                    (eq (car (funcall 'neovm--exc-peek)) 'op)
                    (member (cdr (funcall 'neovm--exc-peek)) '("+" "-")))
          (let ((op (cdr (funcall 'neovm--exc-eat))))
            (setq left (list 'binop op left (funcall 'neovm--exc-parse-term)))))
        left)))

  (fset 'neovm--exc-parse-statement
    (lambda ()
      (let ((tok (funcall 'neovm--exc-peek)))
        (cond
         ;; let x = expr;
         ((eq (car tok) 'kw-let)
          (funcall 'neovm--exc-eat)
          (let ((name (cdr (funcall 'neovm--exc-expect 'ident))))
            (funcall 'neovm--exc-expect 'assign)
            (let ((expr (funcall 'neovm--exc-parse-expr)))
              (funcall 'neovm--exc-expect 'semi)
              (list 'let-stmt name expr))))
         ;; print expr;
         ((eq (car tok) 'kw-print)
          (funcall 'neovm--exc-eat)
          (let ((expr (funcall 'neovm--exc-parse-expr)))
            (funcall 'neovm--exc-expect 'semi)
            (list 'print-stmt expr)))
         ;; expr;
         (t
          (let ((expr (funcall 'neovm--exc-parse-expr)))
            (when (and neovm--exc-toks (eq (car (funcall 'neovm--exc-peek)) 'semi))
              (funcall 'neovm--exc-eat))
            (list 'expr-stmt expr)))))))

  (fset 'neovm--exc-parse-program
    (lambda (tokens)
      (setq neovm--exc-toks tokens)
      (let ((stmts nil))
        (while neovm--exc-toks
          (setq stmts (cons (funcall 'neovm--exc-parse-statement) stmts)))
        (nreverse stmts))))

  ;; ====== AST OPTIMIZER (constant folding) ======
  (fset 'neovm--exc-optimize-expr
    (lambda (ast)
      (cond
       ((eq (car ast) 'lit) ast)
       ((eq (car ast) 'var) ast)
       ((eq (car ast) 'neg)
        (let ((inner (funcall 'neovm--exc-optimize-expr (cadr ast))))
          (if (eq (car inner) 'lit)
              (list 'lit (- (cadr inner)))
            (list 'neg inner))))
       ((eq (car ast) 'binop)
        (let ((op (cadr ast))
              (l (funcall 'neovm--exc-optimize-expr (caddr ast)))
              (r (funcall 'neovm--exc-optimize-expr (cadddr ast))))
          (cond
           ;; Both constants: fold
           ((and (eq (car l) 'lit) (eq (car r) 'lit))
            (let ((lv (cadr l)) (rv (cadr r)))
              (list 'lit
                    (cond ((string= op "+") (+ lv rv))
                          ((string= op "-") (- lv rv))
                          ((string= op "*") (* lv rv))
                          ((string= op "/") (if (= rv 0) 0 (/ lv rv)))
                          ((string= op "%") (if (= rv 0) 0 (% lv rv)))
                          (t 0)))))
           ;; Identity: x + 0 -> x, x * 1 -> x, x - 0 -> x
           ((and (string= op "+") (eq (car r) 'lit) (= (cadr r) 0)) l)
           ((and (string= op "+") (eq (car l) 'lit) (= (cadr l) 0)) r)
           ((and (string= op "-") (eq (car r) 'lit) (= (cadr r) 0)) l)
           ((and (string= op "*") (eq (car r) 'lit) (= (cadr r) 1)) l)
           ((and (string= op "*") (eq (car l) 'lit) (= (cadr l) 1)) r)
           ;; Zero: x * 0 -> 0
           ((and (string= op "*") (or (and (eq (car r) 'lit) (= (cadr r) 0))
                                      (and (eq (car l) 'lit) (= (cadr l) 0))))
            '(lit 0))
           (t (list 'binop op l r)))))
       (t ast))))

  (fset 'neovm--exc-optimize-stmt
    (lambda (stmt)
      (cond
       ((eq (car stmt) 'let-stmt)
        (list 'let-stmt (cadr stmt) (funcall 'neovm--exc-optimize-expr (caddr stmt))))
       ((eq (car stmt) 'print-stmt)
        (list 'print-stmt (funcall 'neovm--exc-optimize-expr (cadr stmt))))
       ((eq (car stmt) 'expr-stmt)
        (list 'expr-stmt (funcall 'neovm--exc-optimize-expr (cadr stmt))))
       (t stmt))))

  (fset 'neovm--exc-optimize
    (lambda (stmts)
      (mapcar 'neovm--exc-optimize-stmt stmts)))

  ;; ====== CODE GENERATOR ======
  ;; Generates instructions for a stack machine:
  ;; (push N), (load "var"), (store "var"), (add), (sub), (mul), (div),
  ;; (mod), (neg), (print), (pop)
  (fset 'neovm--exc-codegen-expr
    (lambda (ast)
      (cond
       ((eq (car ast) 'lit)
        (list (list 'push (cadr ast))))
       ((eq (car ast) 'var)
        (list (list 'load (cadr ast))))
       ((eq (car ast) 'neg)
        (append (funcall 'neovm--exc-codegen-expr (cadr ast))
                '((neg))))
       ((eq (car ast) 'binop)
        (append (funcall 'neovm--exc-codegen-expr (caddr ast))
                (funcall 'neovm--exc-codegen-expr (cadddr ast))
                (list (list (cond ((string= (cadr ast) "+") 'add)
                                  ((string= (cadr ast) "-") 'sub)
                                  ((string= (cadr ast) "*") 'mul)
                                  ((string= (cadr ast) "/") 'div)
                                  ((string= (cadr ast) "%") 'mod)
                                  (t 'nop))))))
       (t nil))))

  (fset 'neovm--exc-codegen-stmt
    (lambda (stmt)
      (cond
       ((eq (car stmt) 'let-stmt)
        (append (funcall 'neovm--exc-codegen-expr (caddr stmt))
                (list (list 'store (cadr stmt)))))
       ((eq (car stmt) 'print-stmt)
        (append (funcall 'neovm--exc-codegen-expr (cadr stmt))
                '((print))))
       ((eq (car stmt) 'expr-stmt)
        (append (funcall 'neovm--exc-codegen-expr (cadr stmt))
                '((pop))))
       (t nil))))

  (fset 'neovm--exc-codegen
    (lambda (stmts)
      (let ((code nil))
        (dolist (stmt stmts)
          (setq code (append code (funcall 'neovm--exc-codegen-stmt stmt))))
        code)))

  ;; ====== STACK MACHINE INTERPRETER ======
  (fset 'neovm--exc-vm-run
    (lambda (code env)
      (let ((stack nil)
            (output nil)
            (vars (copy-sequence env)))
        (dolist (instr code)
          (cond
           ((eq (car instr) 'push)
            (setq stack (cons (cadr instr) stack)))
           ((eq (car instr) 'load)
            (let ((binding (assoc (cadr instr) vars)))
              (setq stack (cons (if binding (cdr binding) 0) stack))))
           ((eq (car instr) 'store)
            (let ((val (car stack))
                  (name (cadr instr)))
              (setq stack (cdr stack))
              (let ((binding (assoc name vars)))
                (if binding (setcdr binding val)
                  (setq vars (cons (cons name val) vars))))))
           ((eq (car instr) 'neg)
            (setq stack (cons (- (car stack)) (cdr stack))))
           ((eq (car instr) 'print)
            (setq output (cons (car stack) output))
            (setq stack (cdr stack)))
           ((eq (car instr) 'pop)
            (setq stack (cdr stack)))
           ((memq (car instr) '(add sub mul div mod))
            (let ((b (car stack))
                  (a (cadr stack)))
              (setq stack
                    (cons (cond
                           ((eq (car instr) 'add) (+ a b))
                           ((eq (car instr) 'sub) (- a b))
                           ((eq (car instr) 'mul) (* a b))
                           ((eq (car instr) 'div) (if (= b 0) 0 (/ a b)))
                           ((eq (car instr) 'mod) (if (= b 0) 0 (% a b)))
                           (t 0))
                          (cddr stack)))))))
        (list :output (nreverse output) :vars vars :stack stack))))

  ;; ====== FULL PIPELINE ======
  (fset 'neovm--exc-compile-run
    (lambda (source &optional env)
      (let* ((tokens (funcall 'neovm--exc-tokenize source))
             (ast (funcall 'neovm--exc-parse-program tokens))
             (optimized (funcall 'neovm--exc-optimize ast))
             (code (funcall 'neovm--exc-codegen optimized))
             (result (funcall 'neovm--exc-vm-run code (or env nil))))
        (list :ast ast :optimized optimized :code code :result result))))
"#
}

fn compiler_cleanup() -> &'static str {
    r#"
    (fmakunbound 'neovm--exc-tokenize)
    (fmakunbound 'neovm--exc-peek)
    (fmakunbound 'neovm--exc-eat)
    (fmakunbound 'neovm--exc-expect)
    (fmakunbound 'neovm--exc-parse-primary)
    (fmakunbound 'neovm--exc-parse-unary)
    (fmakunbound 'neovm--exc-parse-term)
    (fmakunbound 'neovm--exc-parse-expr)
    (fmakunbound 'neovm--exc-parse-statement)
    (fmakunbound 'neovm--exc-parse-program)
    (fmakunbound 'neovm--exc-optimize-expr)
    (fmakunbound 'neovm--exc-optimize-stmt)
    (fmakunbound 'neovm--exc-optimize)
    (fmakunbound 'neovm--exc-codegen-expr)
    (fmakunbound 'neovm--exc-codegen-stmt)
    (fmakunbound 'neovm--exc-codegen)
    (fmakunbound 'neovm--exc-vm-run)
    (fmakunbound 'neovm--exc-compile-run)
    (makunbound 'neovm--exc-toks)
"#
}

// ---------------------------------------------------------------------------
// Test: tokenizer and parser for arithmetic expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_compiler_parse_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}
  (unwind-protect
      (list
       ;; Tokenize various expressions
       (funcall 'neovm--exc-tokenize "3 + 4 * 2")
       (funcall 'neovm--exc-tokenize "let x = 10;")
       (funcall 'neovm--exc-tokenize "(a + b) * c")
       (funcall 'neovm--exc-tokenize "print x + 1;")
       ;; Parse arithmetic expressions
       (let ((tokens (funcall 'neovm--exc-tokenize "3 + 4 * 2")))
         (setq neovm--exc-toks tokens)
         (funcall 'neovm--exc-parse-expr))
       ;; Parse with parentheses
       (let ((tokens (funcall 'neovm--exc-tokenize "(3 + 4) * 2")))
         (setq neovm--exc-toks tokens)
         (funcall 'neovm--exc-parse-expr))
       ;; Parse negation
       (let ((tokens (funcall 'neovm--exc-tokenize "-5 + 3")))
         (setq neovm--exc-toks tokens)
         (funcall 'neovm--exc-parse-expr))
       ;; Parse variable reference
       (let ((tokens (funcall 'neovm--exc-tokenize "x * y + z")))
         (setq neovm--exc-toks tokens)
         (funcall 'neovm--exc-parse-expr))
       ;; Parse modulo
       (let ((tokens (funcall 'neovm--exc-tokenize "17 % 5")))
         (setq neovm--exc-toks tokens)
         (funcall 'neovm--exc-parse-expr)))
    {cleanup}))"#,
        prelude = compiler_prelude(),
        cleanup = compiler_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test: AST optimization (constant folding)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_compiler_constant_folding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}
  (unwind-protect
      (list
       ;; Pure constants: fully foldable
       (funcall 'neovm--exc-optimize-expr '(binop "+" (lit 3) (lit 4)))
       ;; Nested constants
       (funcall 'neovm--exc-optimize-expr
                '(binop "*" (binop "+" (lit 2) (lit 3))
                        (binop "-" (lit 10) (lit 4))))
       ;; Identity: x + 0 -> x
       (funcall 'neovm--exc-optimize-expr '(binop "+" (var "x") (lit 0)))
       ;; Identity: 0 + x -> x
       (funcall 'neovm--exc-optimize-expr '(binop "+" (lit 0) (var "x")))
       ;; Identity: x * 1 -> x
       (funcall 'neovm--exc-optimize-expr '(binop "*" (var "x") (lit 1)))
       ;; Zero: x * 0 -> 0
       (funcall 'neovm--exc-optimize-expr '(binop "*" (var "x") (lit 0)))
       ;; Negate constant
       (funcall 'neovm--exc-optimize-expr '(neg (lit 5)))
       ;; Mixed: (x + 0) * (1 * y) -> x * y
       (funcall 'neovm--exc-optimize-expr
                '(binop "*" (binop "+" (var "x") (lit 0))
                        (binop "*" (lit 1) (var "y"))))
       ;; Program-level optimization
       (funcall 'neovm--exc-optimize
                (list '(let-stmt "x" (binop "+" (lit 3) (lit 4)))
                      '(print-stmt (binop "*" (var "x") (lit 1)))
                      '(expr-stmt (binop "+" (lit 0) (var "y"))))))
    {cleanup}))"#,
        prelude = compiler_prelude(),
        cleanup = compiler_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test: code generation to stack machine instructions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_compiler_codegen() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}
  (unwind-protect
      (list
       ;; Simple literal
       (funcall 'neovm--exc-codegen-expr '(lit 42))
       ;; Variable reference
       (funcall 'neovm--exc-codegen-expr '(var "x"))
       ;; Addition
       (funcall 'neovm--exc-codegen-expr '(binop "+" (lit 3) (lit 4)))
       ;; Complex: (a + b) * c
       (funcall 'neovm--exc-codegen-expr
                '(binop "*" (binop "+" (var "a") (var "b")) (var "c")))
       ;; Negation
       (funcall 'neovm--exc-codegen-expr '(neg (var "x")))
       ;; Statement codegen: let x = 5;
       (funcall 'neovm--exc-codegen-stmt '(let-stmt "x" (lit 5)))
       ;; Statement codegen: print x + 1;
       (funcall 'neovm--exc-codegen-stmt
                '(print-stmt (binop "+" (var "x") (lit 1))))
       ;; Full program codegen
       (funcall 'neovm--exc-codegen
                (list '(let-stmt "x" (lit 10))
                      '(let-stmt "y" (binop "*" (var "x") (lit 2)))
                      '(print-stmt (binop "+" (var "x") (var "y"))))))
    {cleanup}))"#,
        prelude = compiler_prelude(),
        cleanup = compiler_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test: stack machine interpreter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_compiler_vm_execution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}
  (unwind-protect
      (list
       ;; Simple push and print
       (funcall 'neovm--exc-vm-run '((push 42) (print)) nil)
       ;; Arithmetic: 3 + 4
       (funcall 'neovm--exc-vm-run '((push 3) (push 4) (add) (print)) nil)
       ;; Load from env
       (funcall 'neovm--exc-vm-run '((load "x") (push 10) (mul) (print))
                '(("x" . 5)))
       ;; Store and load
       (funcall 'neovm--exc-vm-run
                '((push 99) (store "y") (load "y") (print)) nil)
       ;; Negation
       (funcall 'neovm--exc-vm-run '((push 7) (neg) (print)) nil)
       ;; Complex: (a+b) * (a-b) with a=10, b=3
       (funcall 'neovm--exc-vm-run
                '((load "a") (load "b") (add)
                  (load "a") (load "b") (sub)
                  (mul) (print))
                '(("a" . 10) ("b" . 3)))
       ;; Modulo
       (funcall 'neovm--exc-vm-run '((push 17) (push 5) (mod) (print)) nil)
       ;; Division by zero safety
       (funcall 'neovm--exc-vm-run '((push 10) (push 0) (div) (print)) nil)
       ;; Multiple prints
       (funcall 'neovm--exc-vm-run
                '((push 1) (print) (push 2) (print) (push 3) (print)) nil))
    {cleanup}))"#,
        prelude = compiler_prelude(),
        cleanup = compiler_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test: variable binding and lookup in multi-statement programs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_compiler_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}
  (unwind-protect
      (list
       ;; Compile and run: let x = 10; print x;
       (let ((r (funcall 'neovm--exc-compile-run "let x = 10; print x;")))
         (plist-get (plist-get r :result) :output))

       ;; Multiple variables: let a = 3; let b = 7; print a + b;
       (let ((r (funcall 'neovm--exc-compile-run
                          "let a = 3; let b = 7; print a + b;")))
         (plist-get (plist-get r :result) :output))

       ;; Variable reuse: let x = 5; let y = x * 2; print y;
       (let ((r (funcall 'neovm--exc-compile-run
                          "let x = 5; let y = x * 2; print y;")))
         (plist-get (plist-get r :result) :output))

       ;; Variable with initial env
       (let ((r (funcall 'neovm--exc-compile-run
                          "print x + y;" '(("x" . 10) ("y" . 20)))))
         (plist-get (plist-get r :result) :output))

       ;; Variable overwrite: let x = 1; let x = 2; print x;
       (let ((r (funcall 'neovm--exc-compile-run
                          "let x = 1; let x = 2; print x;")))
         (list (plist-get (plist-get r :result) :output)
               (plist-get (plist-get r :result) :vars)))

       ;; Complex program
       (let ((r (funcall 'neovm--exc-compile-run
                          "let a = 10;
                           let b = 3;
                           let sum = a + b;
                           let diff = a - b;
                           let prod = a * b;
                           print sum;
                           print diff;
                           print prod;")))
         (plist-get (plist-get r :result) :output)))
    {cleanup}))"#,
        prelude = compiler_prelude(),
        cleanup = compiler_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test: full pipeline end-to-end with optimization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_compiler_end_to_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}
  (unwind-protect
      (list
       ;; Simple constant expression: fully foldable
       (let ((r (funcall 'neovm--exc-compile-run "print 3 + 4 * 2;")))
         (list :output (plist-get (plist-get r :result) :output)
               :code-len (length (plist-get r :code))
               ;; Optimized AST should be a single lit
               :optimized (plist-get r :optimized)))

       ;; Expression with variables: partial folding
       (let ((r (funcall 'neovm--exc-compile-run
                          "let x = 2 + 3; print x * 0 + 1;")))
         (list :output (plist-get (plist-get r :result) :output)
               :optimized (plist-get r :optimized)))

       ;; Multi-statement with complex math
       (let ((r (funcall 'neovm--exc-compile-run
                          "let base = 100;
                           let tax = base * 8 / 100;
                           let total = base + tax;
                           print total;")))
         (plist-get (plist-get r :result) :output))

       ;; Negative numbers and unary minus
       (let ((r (funcall 'neovm--exc-compile-run
                          "let x = -5; let y = -x; print y; print x + y;")))
         (plist-get (plist-get r :result) :output))

       ;; Modulo arithmetic
       (let ((r (funcall 'neovm--exc-compile-run
                          "let n = 123;
                           let d2 = n % 10;
                           let d1 = n / 10 % 10;
                           let d0 = n / 100;
                           print d0;
                           print d1;
                           print d2;")))
         (plist-get (plist-get r :result) :output))

       ;; Chained computation: compute fibonacci-like manually
       (let ((r (funcall 'neovm--exc-compile-run
                          "let a = 1;
                           let b = 1;
                           let c = a + b;
                           let d = b + c;
                           let e = c + d;
                           let f = d + e;
                           let g = e + f;
                           print a;
                           print b;
                           print c;
                           print d;
                           print e;
                           print f;
                           print g;")))
         (plist-get (plist-get r :result) :output))

       ;; Verify optimization reduces code
       (let* ((source "print 1 + 2 + 3 + 4 + 5;")
              (tokens (funcall 'neovm--exc-tokenize source))
              (ast (funcall 'neovm--exc-parse-program tokens))
              (optimized (funcall 'neovm--exc-optimize ast))
              (code-orig (funcall 'neovm--exc-codegen ast))
              (code-opt (funcall 'neovm--exc-codegen optimized)))
         (list :unoptimized-len (length code-orig)
               :optimized-len (length code-opt)
               :reduced (< (length code-opt) (length code-orig)))))
    {cleanup}))"#,
        prelude = compiler_prelude(),
        cleanup = compiler_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}
