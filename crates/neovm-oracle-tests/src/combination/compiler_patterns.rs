//! Complex oracle parity tests for compiler/interpreter patterns in Elisp:
//! lexer producing token streams, AST builder, constant folding,
//! variable resolution / scope analysis, code generation to a stack machine,
//! and end-to-end pipeline from source string to evaluated result.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Lexer: source string -> token stream
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_lexer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Lexer for a tiny language with: let, if, fn, return, identifiers,
    // integers, operators, parens, braces, semicolons
    let form = r#"(progn
  (fset 'neovm--test-lex
    (lambda (src)
      (let ((tokens nil) (i 0) (len (length src)))
        (while (< i len)
          (let ((ch (aref src i)))
            (cond
             ;; Whitespace
             ((memq ch '(?\s ?\t ?\n ?\r))
              (setq i (1+ i)))
             ;; Single-char tokens
             ((= ch ?\() (setq tokens (cons '(LPAREN) tokens) i (1+ i)))
             ((= ch ?\)) (setq tokens (cons '(RPAREN) tokens) i (1+ i)))
             ((= ch ?\{) (setq tokens (cons '(LBRACE) tokens) i (1+ i)))
             ((= ch ?\}) (setq tokens (cons '(RBRACE) tokens) i (1+ i)))
             ((= ch ?\;) (setq tokens (cons '(SEMI) tokens) i (1+ i)))
             ((= ch ?,) (setq tokens (cons '(COMMA) tokens) i (1+ i)))
             ;; Two-char operators: ==, !=, <=, >=
             ((and (memq ch '(?= ?! ?< ?>))
                   (< (1+ i) len)
                   (= (aref src (1+ i)) ?=))
              (setq tokens (cons (list 'OP (concat (char-to-string ch) "=")) tokens)
                    i (+ i 2)))
             ;; Single-char operators
             ((memq ch '(?+ ?- ?* ?/ ?< ?> ?=))
              (setq tokens (cons (list 'OP (char-to-string ch)) tokens)
                    i (1+ i)))
             ;; Numbers
             ((and (>= ch ?0) (<= ch ?9))
              (let ((start i))
                (while (and (< i len) (>= (aref src i) ?0) (<= (aref src i) ?9))
                  (setq i (1+ i)))
                (setq tokens (cons (list 'INT (string-to-number (substring src start i)))
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
                               ((string= word "let") '(KW-LET))
                               ((string= word "if") '(KW-IF))
                               ((string= word "else") '(KW-ELSE))
                               ((string= word "fn") '(KW-FN))
                               ((string= word "return") '(KW-RETURN))
                               (t (list 'IDENT word)))
                              tokens)))))
             ;; Skip unknown
             (t (setq i (1+ i))))))
        (nreverse tokens))))

  (unwind-protect
      (list
       (funcall 'neovm--test-lex "let x = 10;")
       (funcall 'neovm--test-lex "fn add(a, b) { return a + b; }")
       (funcall 'neovm--test-lex "if x >= 10 { y = x * 2; }")
       (funcall 'neovm--test-lex "let result = foo(1, 2) + bar(3);")
       (funcall 'neovm--test-lex "a == b != c <= d"))
    (fmakunbound 'neovm--test-lex)))"#;
    let expect = expect_test::expect![[
        r#""OK (((KW-LET) (IDENT \"x\") (OP \"=\") (INT 10) (SEMI)) ((KW-FN) (IDENT \"add\") (LPAREN) (IDENT \"a\") (COMMA) (IDENT \"b\") (RPAREN) (LBRACE) (KW-RETURN) (IDENT \"a\") (OP \"+\") (IDENT \"b\") (SEMI) (RBRACE)) ((KW-IF) (IDENT \"x\") (OP \">=\") (INT 10) (LBRACE) (IDENT \"y\") (OP \"=\") (IDENT \"x\") (OP \"*\") (INT 2) (SEMI) (RBRACE)) ((KW-LET) (IDENT \"result\") (OP \"=\") (IDENT \"foo\") (LPAREN) (INT 1) (COMMA) (INT 2) (RPAREN) (OP \"+\") (IDENT \"bar\") (LPAREN) (INT 3) (RPAREN) (SEMI)) ((IDENT \"a\") (OP \"==\") (IDENT \"b\") (OP \"!=\") (IDENT \"c\") (OP \"<=\") (IDENT \"d\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AST builder from token stream
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_ast_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build AST from a simplified expression grammar:
    // expr     = term (('+' | '-') term)*
    // term     = unary (('*' | '/') unary)*
    // unary    = '-' unary | primary
    // primary  = INT | IDENT | '(' expr ')'
    let form = r#"(progn
  (defvar neovm--test-ast-toks nil)

  (fset 'neovm--test-ast-peek
    (lambda ()
      (car neovm--test-ast-toks)))

  (fset 'neovm--test-ast-eat
    (lambda ()
      (let ((tok (car neovm--test-ast-toks)))
        (setq neovm--test-ast-toks (cdr neovm--test-ast-toks))
        tok)))

  (fset 'neovm--test-ast-primary
    (lambda ()
      (let ((tok (funcall 'neovm--test-ast-peek)))
        (cond
         ((eq (car tok) 'INT)
          (funcall 'neovm--test-ast-eat)
          (list 'lit (cadr tok)))
         ((eq (car tok) 'IDENT)
          (funcall 'neovm--test-ast-eat)
          (list 'ref (cadr tok)))
         ((eq (car tok) 'LPAREN)
          (funcall 'neovm--test-ast-eat)
          (let ((e (funcall 'neovm--test-ast-expr)))
            (funcall 'neovm--test-ast-eat) ;; RPAREN
            e))
         (t (list 'error "unexpected token" tok))))))

  (fset 'neovm--test-ast-unary
    (lambda ()
      (let ((tok (funcall 'neovm--test-ast-peek)))
        (if (and (eq (car tok) 'OP) (string= (cadr tok) "-"))
            (progn
              (funcall 'neovm--test-ast-eat)
              (list 'neg (funcall 'neovm--test-ast-unary)))
          (funcall 'neovm--test-ast-primary)))))

  (fset 'neovm--test-ast-term
    (lambda ()
      (let ((left (funcall 'neovm--test-ast-unary)))
        (while (and neovm--test-ast-toks
                    (eq (car (funcall 'neovm--test-ast-peek)) 'OP)
                    (or (string= (cadr (funcall 'neovm--test-ast-peek)) "*")
                        (string= (cadr (funcall 'neovm--test-ast-peek)) "/")))
          (let ((op-tok (funcall 'neovm--test-ast-eat)))
            (let ((right (funcall 'neovm--test-ast-unary)))
              (setq left (list 'binop (cadr op-tok) left right)))))
        left)))

  (fset 'neovm--test-ast-expr
    (lambda ()
      (let ((left (funcall 'neovm--test-ast-term)))
        (while (and neovm--test-ast-toks
                    (eq (car (funcall 'neovm--test-ast-peek)) 'OP)
                    (or (string= (cadr (funcall 'neovm--test-ast-peek)) "+")
                        (string= (cadr (funcall 'neovm--test-ast-peek)) "-")))
          (let ((op-tok (funcall 'neovm--test-ast-eat)))
            (let ((right (funcall 'neovm--test-ast-term)))
              (setq left (list 'binop (cadr op-tok) left right)))))
        left)))

  (fset 'neovm--test-ast-parse
    (lambda (tokens)
      (setq neovm--test-ast-toks tokens)
      (funcall 'neovm--test-ast-expr)))

  (unwind-protect
      (list
       ;; 3 + 4 -> (binop "+" (lit 3) (lit 4))
       (funcall 'neovm--test-ast-parse '((INT 3) (OP "+") (INT 4)))
       ;; 2 * 3 + 1 -> (binop "+" (binop "*" (lit 2) (lit 3)) (lit 1))
       (funcall 'neovm--test-ast-parse '((INT 2) (OP "*") (INT 3) (OP "+") (INT 1)))
       ;; (1 + 2) * 3
       (funcall 'neovm--test-ast-parse '((LPAREN) (INT 1) (OP "+") (INT 2) (RPAREN) (OP "*") (INT 3)))
       ;; -5 + x
       (funcall 'neovm--test-ast-parse '((OP "-") (INT 5) (OP "+") (IDENT "x")))
       ;; a * b + c * d
       (funcall 'neovm--test-ast-parse '((IDENT "a") (OP "*") (IDENT "b") (OP "+") (IDENT "c") (OP "*") (IDENT "d"))))
    (fmakunbound 'neovm--test-ast-peek)
    (fmakunbound 'neovm--test-ast-eat)
    (fmakunbound 'neovm--test-ast-primary)
    (fmakunbound 'neovm--test-ast-unary)
    (fmakunbound 'neovm--test-ast-term)
    (fmakunbound 'neovm--test-ast-expr)
    (fmakunbound 'neovm--test-ast-parse)
    (makunbound 'neovm--test-ast-toks)))"#;
    let expect = expect_test::expect![[
        r#""OK ((binop \"+\" (lit 3) (lit 4)) (binop \"+\" (binop \"*\" (lit 2) (lit 3)) (lit 1)) (binop \"*\" (binop \"+\" (lit 1) (lit 2)) (lit 3)) (binop \"+\" (neg (lit 5)) (ref \"x\")) (binop \"+\" (binop \"*\" (ref \"a\") (ref \"b\")) (binop \"*\" (ref \"c\") (ref \"d\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constant folding optimization pass
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_constant_folding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk an AST, fold constant sub-expressions
    // AST: (lit N), (ref X), (neg E), (binop OP L R)
    let form = r#"(progn
  (fset 'neovm--test-cf-fold
    (lambda (ast)
      (cond
       ((eq (car ast) 'lit) ast)
       ((eq (car ast) 'ref) ast)
       ((eq (car ast) 'neg)
        (let ((inner (funcall 'neovm--test-cf-fold (cadr ast))))
          (cond
           ;; -lit -> lit(-n)
           ((eq (car inner) 'lit)
            (list 'lit (- (cadr inner))))
           ;; --x -> x
           ((eq (car inner) 'neg)
            (cadr inner))
           (t (list 'neg inner)))))
       ((eq (car ast) 'binop)
        (let ((op (cadr ast))
              (left (funcall 'neovm--test-cf-fold (caddr ast)))
              (right (funcall 'neovm--test-cf-fold (cadddr ast))))
          (cond
           ;; Both constants: evaluate
           ((and (eq (car left) 'lit) (eq (car right) 'lit))
            (let ((lv (cadr left)) (rv (cadr right)))
              (list 'lit
                    (cond
                     ((string= op "+") (+ lv rv))
                     ((string= op "-") (- lv rv))
                     ((string= op "*") (* lv rv))
                     ((string= op "/") (if (= rv 0) 0 (/ lv rv)))
                     (t 0)))))
           ;; x + 0 or 0 + x
           ((and (string= op "+") (eq (car right) 'lit) (= (cadr right) 0))
            left)
           ((and (string= op "+") (eq (car left) 'lit) (= (cadr left) 0))
            right)
           ;; x * 0 or 0 * x
           ((and (string= op "*") (eq (car right) 'lit) (= (cadr right) 0))
            '(lit 0))
           ((and (string= op "*") (eq (car left) 'lit) (= (cadr left) 0))
            '(lit 0))
           ;; x * 1 or 1 * x
           ((and (string= op "*") (eq (car right) 'lit) (= (cadr right) 1))
            left)
           ((and (string= op "*") (eq (car left) 'lit) (= (cadr left) 1))
            right)
           ;; x - 0
           ((and (string= op "-") (eq (car right) 'lit) (= (cadr right) 0))
            left)
           (t (list 'binop op left right)))))
       (t ast))))

  (unwind-protect
      (list
       ;; 3 + 4 -> 7
       (funcall 'neovm--test-cf-fold '(binop "+" (lit 3) (lit 4)))
       ;; x + 0 -> x
       (funcall 'neovm--test-cf-fold '(binop "+" (ref "x") (lit 0)))
       ;; 0 * x -> 0
       (funcall 'neovm--test-cf-fold '(binop "*" (lit 0) (ref "x")))
       ;; (2 + 3) * (4 - 1) -> 15
       (funcall 'neovm--test-cf-fold
                '(binop "*" (binop "+" (lit 2) (lit 3))
                        (binop "-" (lit 4) (lit 1))))
       ;; x * 1 + 0 -> x
       (funcall 'neovm--test-cf-fold
                '(binop "+" (binop "*" (ref "x") (lit 1)) (lit 0)))
       ;; --5 -> 5
       (funcall 'neovm--test-cf-fold '(neg (neg (lit 5))))
       ;; -(3) -> -3
       (funcall 'neovm--test-cf-fold '(neg (lit 3)))
       ;; Nested: (x + 0) * (1 * y) -> x * y
       (funcall 'neovm--test-cf-fold
                '(binop "*" (binop "+" (ref "x") (lit 0))
                        (binop "*" (lit 1) (ref "y")))))
    (fmakunbound 'neovm--test-cf-fold)))"#;
    let expect = expect_test::expect![[
        r#""OK ((lit 7) (ref \"x\") (lit 0) (lit 15) (ref \"x\") (lit 5) (lit -3) (binop \"*\" (ref \"x\") (ref \"y\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Variable resolution / scope analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_scope_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Analyze an AST with let-bindings and report:
    // - free variables (referenced but not bound)
    // - unused bindings (bound but never referenced)
    // - shadowed bindings (same name bound in nested scope)
    // AST nodes: (lit N), (ref X), (binop OP L R),
    //            (let-expr ((name . init) ...) body)
    let form = r#"(progn
  ;; Collect all referenced variable names (set of strings)
  (fset 'neovm--test-sa-refs
    (lambda (ast)
      (cond
       ((eq (car ast) 'lit) nil)
       ((eq (car ast) 'ref) (list (cadr ast)))
       ((eq (car ast) 'neg)
        (funcall 'neovm--test-sa-refs (cadr ast)))
       ((eq (car ast) 'binop)
        (append (funcall 'neovm--test-sa-refs (caddr ast))
                (funcall 'neovm--test-sa-refs (cadddr ast))))
       ((eq (car ast) 'let-expr)
        (let ((bindings (cadr ast))
              (body (caddr ast))
              (init-refs nil))
          ;; Refs from init expressions
          (dolist (b bindings)
            (setq init-refs (append init-refs
                                    (funcall 'neovm--test-sa-refs (cdr b)))))
          ;; Refs from body, minus the bound names
          (let ((bound-names (mapcar #'car bindings))
                (body-refs (funcall 'neovm--test-sa-refs body)))
            (append init-refs
                    (delq nil (mapcar (lambda (r)
                                        (unless (member r bound-names) r))
                                      body-refs))))))
       (t nil))))

  (fset 'neovm--test-sa-analyze
    (lambda (ast scope)
      (let ((refs (funcall 'neovm--test-sa-refs ast))
            (results nil))
        ;; Deduplicate refs
        (let ((unique-refs nil))
          (dolist (r refs)
            (unless (member r unique-refs)
              (setq unique-refs (cons r unique-refs))))
          (setq refs (nreverse unique-refs)))
        ;; Free variables: referenced but not in scope
        (let ((free nil))
          (dolist (r refs)
            (unless (member r scope)
              (setq free (cons r free))))
          (setq results (cons (cons 'free (sort (nreverse free) #'string-lessp))
                              results)))
        ;; For let-expr: find unused bindings
        (when (eq (car ast) 'let-expr)
          (let ((bindings (cadr ast))
                (body (caddr ast))
                (body-refs (funcall 'neovm--test-sa-refs body))
                (unused nil))
            (dolist (b bindings)
              (unless (member (car b) body-refs)
                (setq unused (cons (car b) unused))))
            (setq results (cons (cons 'unused (sort (nreverse unused) #'string-lessp))
                                results)))
          ;; Shadowed bindings
          (let ((bindings (cadr ast))
                (shadowed nil))
            (dolist (b bindings)
              (when (member (car b) scope)
                (setq shadowed (cons (car b) shadowed))))
            (setq results (cons (cons 'shadowed (sort (nreverse shadowed) #'string-lessp))
                                results))))
        (nreverse results))))

  (unwind-protect
      (list
       ;; Simple: x + y, both free
       (funcall 'neovm--test-sa-analyze
                '(binop "+" (ref "x") (ref "y"))
                nil)
       ;; Let binds x, y is free
       (funcall 'neovm--test-sa-analyze
                '(let-expr (("x" . (lit 10)))
                   (binop "+" (ref "x") (ref "y")))
                nil)
       ;; Unused binding
       (funcall 'neovm--test-sa-analyze
                '(let-expr (("x" . (lit 10)) ("unused" . (lit 99)))
                   (ref "x"))
                nil)
       ;; Shadowing: x already in scope
       (funcall 'neovm--test-sa-analyze
                '(let-expr (("x" . (lit 5)))
                   (ref "x"))
                '("x" "y"))
       ;; Complex: nested let with shadow and free
       (funcall 'neovm--test-sa-analyze
                '(let-expr (("a" . (ref "ext")) ("b" . (lit 0)))
                   (binop "+" (ref "a") (ref "c")))
                '("a")))
    (fmakunbound 'neovm--test-sa-refs)
    (fmakunbound 'neovm--test-sa-analyze)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable body)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Code generation: AST -> stack machine instructions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_codegen_stack_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate instructions for a stack-based VM:
    // (push N), (load X), (add), (sub), (mul), (div), (negate)
    // Then simulate execution of the instruction sequence.
    let form = r#"(progn
  (fset 'neovm--test-cg-emit
    (lambda (ast)
      (cond
       ((eq (car ast) 'lit)
        (list (list 'push (cadr ast))))
       ((eq (car ast) 'ref)
        (list (list 'load (cadr ast))))
       ((eq (car ast) 'neg)
        (append (funcall 'neovm--test-cg-emit (cadr ast))
                (list '(negate))))
       ((eq (car ast) 'binop)
        (let ((op (cadr ast))
              (left (caddr ast))
              (right (cadddr ast)))
          (append (funcall 'neovm--test-cg-emit left)
                  (funcall 'neovm--test-cg-emit right)
                  (list (list (cond
                               ((string= op "+") 'add)
                               ((string= op "-") 'sub)
                               ((string= op "*") 'mul)
                               ((string= op "/") 'div)
                               (t 'nop)))))))
       (t nil))))

  ;; Stack machine simulator
  (fset 'neovm--test-cg-run
    (lambda (instrs env)
      (let ((stack nil))
        (dolist (instr instrs)
          (cond
           ((eq (car instr) 'push)
            (setq stack (cons (cadr instr) stack)))
           ((eq (car instr) 'load)
            (let ((binding (assoc (cadr instr) env)))
              (setq stack (cons (if binding (cdr binding) 0) stack))))
           ((eq (car instr) 'negate)
            (setq stack (cons (- (car stack)) (cdr stack))))
           ((memq (car instr) '(add sub mul div))
            (let ((b (car stack))
                  (a (cadr stack)))
              (setq stack (cons (cond
                                 ((eq (car instr) 'add) (+ a b))
                                 ((eq (car instr) 'sub) (- a b))
                                 ((eq (car instr) 'mul) (* a b))
                                 ((eq (car instr) 'div) (if (= b 0) 0 (/ a b)))
                                 (t 0))
                                (cddr stack)))))))
        (car stack))))

  (unwind-protect
      (let ((test-cases
             (list
              ;; 3 + 4
              (cons '(binop "+" (lit 3) (lit 4)) nil)
              ;; x * 2 + y, with x=5, y=3
              (cons '(binop "+" (binop "*" (ref "x") (lit 2)) (ref "y"))
                    '(("x" . 5) ("y" . 3)))
              ;; -(a + b), with a=10, b=7
              (cons '(neg (binop "+" (ref "a") (ref "b")))
                    '(("a" . 10) ("b" . 7)))
              ;; (2 + 3) * (8 - 2)
              (cons '(binop "*" (binop "+" (lit 2) (lit 3))
                            (binop "-" (lit 8) (lit 2)))
                    nil)
              ;; a * b + c * d with env
              (cons '(binop "+" (binop "*" (ref "a") (ref "b"))
                            (binop "*" (ref "c") (ref "d")))
                    '(("a" . 3) ("b" . 4) ("c" . 5) ("d" . 6))))))
        (mapcar (lambda (tc)
                  (let* ((ast (car tc))
                         (env (cdr tc))
                         (code (funcall 'neovm--test-cg-emit ast))
                         (result (funcall 'neovm--test-cg-run code env)))
                    (list 'code code 'result result)))
                test-cases))
    (fmakunbound 'neovm--test-cg-emit)
    (fmakunbound 'neovm--test-cg-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((code ((push 3) (push 4) (add)) result 7) (code ((load \"x\") (push 2) (mul) (load \"y\") (add)) result 13) (code ((load \"a\") (load \"b\") (add) (negate)) result -17) (code ((push 2) (push 3) (add) (push 8) (push 2) (sub) (mul)) result 30) (code ((load \"a\") (load \"b\") (mul) (load \"c\") (load \"d\") (mul) (add)) result 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// End-to-end: source string -> lex -> parse -> optimize -> codegen -> evaluate
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compiler_end_to_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full pipeline combining all phases:
    // 1. Lex source string
    // 2. Parse tokens into AST
    // 3. Constant-fold the AST
    // 4. Generate stack machine code
    // 5. Execute and return result
    let form = r#"(progn
  ;; === LEXER ===
  (fset 'neovm--test-e2e-lex
    (lambda (src)
      (let ((tokens nil) (i 0) (len (length src)))
        (while (< i len)
          (let ((ch (aref src i)))
            (cond
             ((memq ch '(?\s ?\t ?\n)) (setq i (1+ i)))
             ((= ch ?\()
              (setq tokens (cons '(LPAREN) tokens) i (1+ i)))
             ((= ch ?\))
              (setq tokens (cons '(RPAREN) tokens) i (1+ i)))
             ((memq ch '(?+ ?- ?* ?/))
              (setq tokens (cons (list 'OP (char-to-string ch)) tokens)
                    i (1+ i)))
             ((and (>= ch ?0) (<= ch ?9))
              (let ((start i))
                (while (and (< i len) (>= (aref src i) ?0) (<= (aref src i) ?9))
                  (setq i (1+ i)))
                (setq tokens (cons (list 'INT (string-to-number (substring src start i)))
                                   tokens))))
             ((or (and (>= ch ?a) (<= ch ?z)) (= ch ?_))
              (let ((start i))
                (while (and (< i len)
                            (let ((c (aref src i)))
                              (or (and (>= c ?a) (<= c ?z))
                                  (and (>= c ?0) (<= c ?9)) (= c ?_))))
                  (setq i (1+ i)))
                (setq tokens (cons (list 'IDENT (substring src start i)) tokens))))
             (t (setq i (1+ i))))))
        (nreverse tokens))))

  ;; === PARSER ===
  (defvar neovm--test-e2e-toks nil)

  (fset 'neovm--test-e2e-peek (lambda () (car neovm--test-e2e-toks)))
  (fset 'neovm--test-e2e-eat
    (lambda ()
      (let ((t1 (car neovm--test-e2e-toks)))
        (setq neovm--test-e2e-toks (cdr neovm--test-e2e-toks)) t1)))

  (fset 'neovm--test-e2e-primary
    (lambda ()
      (let ((tok (funcall 'neovm--test-e2e-peek)))
        (cond
         ((eq (car tok) 'INT)
          (funcall 'neovm--test-e2e-eat) (list 'lit (cadr tok)))
         ((eq (car tok) 'IDENT)
          (funcall 'neovm--test-e2e-eat) (list 'ref (cadr tok)))
         ((eq (car tok) 'LPAREN)
          (funcall 'neovm--test-e2e-eat)
          (let ((e (funcall 'neovm--test-e2e-expr)))
            (funcall 'neovm--test-e2e-eat) e))
         (t (list 'lit 0))))))

  (fset 'neovm--test-e2e-unary
    (lambda ()
      (if (and neovm--test-e2e-toks
               (eq (car (funcall 'neovm--test-e2e-peek)) 'OP)
               (string= (cadr (funcall 'neovm--test-e2e-peek)) "-"))
          (progn (funcall 'neovm--test-e2e-eat)
                 (list 'neg (funcall 'neovm--test-e2e-unary)))
        (funcall 'neovm--test-e2e-primary))))

  (fset 'neovm--test-e2e-term
    (lambda ()
      (let ((left (funcall 'neovm--test-e2e-unary)))
        (while (and neovm--test-e2e-toks
                    (eq (car (funcall 'neovm--test-e2e-peek)) 'OP)
                    (or (string= (cadr (funcall 'neovm--test-e2e-peek)) "*")
                        (string= (cadr (funcall 'neovm--test-e2e-peek)) "/")))
          (let ((op (cadr (funcall 'neovm--test-e2e-eat))))
            (setq left (list 'binop op left (funcall 'neovm--test-e2e-unary)))))
        left)))

  (fset 'neovm--test-e2e-expr
    (lambda ()
      (let ((left (funcall 'neovm--test-e2e-term)))
        (while (and neovm--test-e2e-toks
                    (eq (car (funcall 'neovm--test-e2e-peek)) 'OP)
                    (or (string= (cadr (funcall 'neovm--test-e2e-peek)) "+")
                        (string= (cadr (funcall 'neovm--test-e2e-peek)) "-")))
          (let ((op (cadr (funcall 'neovm--test-e2e-eat))))
            (setq left (list 'binop op left (funcall 'neovm--test-e2e-term)))))
        left)))

  (fset 'neovm--test-e2e-parse
    (lambda (tokens)
      (setq neovm--test-e2e-toks tokens)
      (funcall 'neovm--test-e2e-expr)))

  ;; === OPTIMIZER (constant folding) ===
  (fset 'neovm--test-e2e-fold
    (lambda (ast)
      (cond
       ((eq (car ast) 'lit) ast)
       ((eq (car ast) 'ref) ast)
       ((eq (car ast) 'neg)
        (let ((inner (funcall 'neovm--test-e2e-fold (cadr ast))))
          (if (eq (car inner) 'lit)
              (list 'lit (- (cadr inner)))
            (list 'neg inner))))
       ((eq (car ast) 'binop)
        (let ((op (cadr ast))
              (l (funcall 'neovm--test-e2e-fold (caddr ast)))
              (r (funcall 'neovm--test-e2e-fold (cadddr ast))))
          (if (and (eq (car l) 'lit) (eq (car r) 'lit))
              (list 'lit
                    (let ((lv (cadr l)) (rv (cadr r)))
                      (cond ((string= op "+") (+ lv rv))
                            ((string= op "-") (- lv rv))
                            ((string= op "*") (* lv rv))
                            ((string= op "/") (if (= rv 0) 0 (/ lv rv)))
                            (t 0))))
            (list 'binop op l r))))
       (t ast))))

  ;; === CODE GENERATOR ===
  (fset 'neovm--test-e2e-codegen
    (lambda (ast)
      (cond
       ((eq (car ast) 'lit) (list (list 'push (cadr ast))))
       ((eq (car ast) 'ref) (list (list 'load (cadr ast))))
       ((eq (car ast) 'neg)
        (append (funcall 'neovm--test-e2e-codegen (cadr ast))
                '((negate))))
       ((eq (car ast) 'binop)
        (append (funcall 'neovm--test-e2e-codegen (caddr ast))
                (funcall 'neovm--test-e2e-codegen (cadddr ast))
                (list (list (cond ((string= (cadr ast) "+") 'add)
                                  ((string= (cadr ast) "-") 'sub)
                                  ((string= (cadr ast) "*") 'mul)
                                  ((string= (cadr ast) "/") 'div)
                                  (t 'nop))))))
       (t nil))))

  ;; === VM ===
  (fset 'neovm--test-e2e-vm-run
    (lambda (code env)
      (let ((stack nil))
        (dolist (instr code)
          (cond
           ((eq (car instr) 'push) (setq stack (cons (cadr instr) stack)))
           ((eq (car instr) 'load)
            (setq stack (cons (or (cdr (assoc (cadr instr) env)) 0) stack)))
           ((eq (car instr) 'negate)
            (setq stack (cons (- (car stack)) (cdr stack))))
           (t (let ((b (car stack)) (a (cadr stack)))
                (setq stack (cons (cond ((eq (car instr) 'add) (+ a b))
                                        ((eq (car instr) 'sub) (- a b))
                                        ((eq (car instr) 'mul) (* a b))
                                        ((eq (car instr) 'div) (if (= b 0) 0 (/ a b)))
                                        (t 0))
                                  (cddr stack)))))))
        (car stack))))

  ;; === FULL PIPELINE ===
  (fset 'neovm--test-e2e-compile-run
    (lambda (source env)
      (let* ((tokens (funcall 'neovm--test-e2e-lex source))
             (ast (funcall 'neovm--test-e2e-parse tokens))
             (opt (funcall 'neovm--test-e2e-fold ast))
             (code (funcall 'neovm--test-e2e-codegen opt))
             (result (funcall 'neovm--test-e2e-vm-run code env)))
        (list 'tokens (length tokens)
              'ast-folded (not (equal ast opt))
              'code-len (length code)
              'result result))))

  (unwind-protect
      (list
       ;; Pure constants: fully foldable
       (funcall 'neovm--test-e2e-compile-run "3 + 4 * 2" nil)
       ;; With variables
       (funcall 'neovm--test-e2e-compile-run "x * 2 + y"
                '(("x" . 5) ("y" . 3)))
       ;; Parenthesized
       (funcall 'neovm--test-e2e-compile-run "(10 + 20) * (3 - 1)" nil)
       ;; Negation
       (funcall 'neovm--test-e2e-compile-run "-5 + 15" nil)
       ;; Complex with vars
       (funcall 'neovm--test-e2e-compile-run "a * b + c * d - e"
                '(("a" . 2) ("b" . 3) ("c" . 4) ("d" . 5) ("e" . 1)))
       ;; Single number
       (funcall 'neovm--test-e2e-compile-run "42" nil))
    (fmakunbound 'neovm--test-e2e-lex)
    (fmakunbound 'neovm--test-e2e-peek)
    (fmakunbound 'neovm--test-e2e-eat)
    (fmakunbound 'neovm--test-e2e-primary)
    (fmakunbound 'neovm--test-e2e-unary)
    (fmakunbound 'neovm--test-e2e-term)
    (fmakunbound 'neovm--test-e2e-expr)
    (fmakunbound 'neovm--test-e2e-parse)
    (fmakunbound 'neovm--test-e2e-fold)
    (fmakunbound 'neovm--test-e2e-codegen)
    (fmakunbound 'neovm--test-e2e-vm-run)
    (fmakunbound 'neovm--test-e2e-compile-run)
    (makunbound 'neovm--test-e2e-toks)))"#;
    let expect = expect_test::expect![[
        r#""OK ((tokens 5 ast-folded t code-len 1 result 11) (tokens 5 ast-folded nil code-len 5 result 13) (tokens 11 ast-folded t code-len 1 result 60) (tokens 4 ast-folded t code-len 1 result 10) (tokens 9 ast-folded nil code-len 9 result 25) (tokens 1 ast-folded nil code-len 1 result 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
