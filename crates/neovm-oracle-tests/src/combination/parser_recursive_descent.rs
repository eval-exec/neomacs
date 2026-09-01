//! Oracle parity tests for a recursive descent parser implemented in Elisp:
//! tokenizer, parser for arithmetic with proper precedence (atom, unary,
//! multiplicative, additive, comparison), AST construction, pretty-printing
//! AST, evaluating AST, and parser error recovery.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Tokenizer: converts input string into a list of tokens
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_parser_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tokenizer that produces (type . value) pairs for: numbers, identifiers,
    // operators (+, -, *, /, <, >, =, <=, >=, ==, !=), parentheses, comma.
    let form = r#"
(progn
  (fset 'neovm--rdp-is-digit
    (lambda (ch) (and (>= ch ?0) (<= ch ?9))))
  (fset 'neovm--rdp-is-alpha
    (lambda (ch) (or (and (>= ch ?a) (<= ch ?z))
                     (and (>= ch ?A) (<= ch ?Z))
                     (= ch ?_))))
  (fset 'neovm--rdp-is-alnum
    (lambda (ch) (or (funcall 'neovm--rdp-is-digit ch)
                     (funcall 'neovm--rdp-is-alpha ch))))

  (fset 'neovm--rdp-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ;; Skip whitespace
              ((memq ch '(?\s ?\t ?\n))
               (setq pos (1+ pos)))
              ;; Number
              ((funcall 'neovm--rdp-is-digit ch)
               (let ((start pos) (has-dot nil))
                 (while (and (< pos len)
                             (or (funcall 'neovm--rdp-is-digit (aref input pos))
                                 (and (= (aref input pos) ?.)
                                      (not has-dot))))
                   (when (= (aref input pos) ?.)
                     (setq has-dot t))
                   (setq pos (1+ pos)))
                 (let ((numstr (substring input start pos)))
                   (setq tokens (cons (cons 'number
                                            (if has-dot
                                                (string-to-number numstr)
                                              (string-to-number numstr)))
                                      tokens)))))
              ;; Identifier
              ((funcall 'neovm--rdp-is-alpha ch)
               (let ((start pos))
                 (while (and (< pos len)
                             (funcall 'neovm--rdp-is-alnum (aref input pos)))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'ident (substring input start pos))
                                    tokens))))
              ;; Two-char operators
              ((and (< (1+ pos) len)
                    (let ((two (substring input pos (+ pos 2))))
                      (member two '("<=" ">=" "==" "!="))))
               (setq tokens (cons (cons 'op (substring input pos (+ pos 2)))
                                  tokens))
               (setq pos (+ pos 2)))
              ;; Single-char operators and punctuation
              ((memq ch '(?+ ?- ?* ?/ ?< ?> ?= ?( ?) ?,))
               (setq tokens (cons (cons (cond ((= ch ?\() 'lparen)
                                              ((= ch ?\)) 'rparen)
                                              ((= ch ?,) 'comma)
                                              (t 'op))
                                        (char-to-string ch))
                                  tokens))
               (setq pos (1+ pos)))
              ;; Unknown character - skip
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (unwind-protect
      (list
        (funcall 'neovm--rdp-tokenize "1 + 2 * 3")
        (funcall 'neovm--rdp-tokenize "x + y * (z - 1)")
        (funcall 'neovm--rdp-tokenize "foo(10, 20)")
        (funcall 'neovm--rdp-tokenize "a <= b && c >= d")
        (funcall 'neovm--rdp-tokenize "3.14 + 2.0 * -1")
        (funcall 'neovm--rdp-tokenize "-x + +y")
        (funcall 'neovm--rdp-tokenize "")
        (funcall 'neovm--rdp-tokenize "123"))
    (fmakunbound 'neovm--rdp-is-digit)
    (fmakunbound 'neovm--rdp-is-alpha)
    (fmakunbound 'neovm--rdp-is-alnum)
    (fmakunbound 'neovm--rdp-tokenize)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((number . 1) (op . \"+\") (number . 2) (op . \"*\") (number . 3)) ((ident . \"x\") (op . \"+\") (ident . \"y\") (op . \"*\") (lparen . \"(\") (ident . \"z\") (op . \"-\") (number . 1) (rparen . \")\")) ((ident . \"foo\") (lparen . \"(\") (number . 10) (comma . \",\") (number . 20) (rparen . \")\")) ((ident . \"a\") (op . \"<=\") (ident . \"b\") (ident . \"c\") (op . \">=\") (ident . \"d\")) ((number . 3.14) (op . \"+\") (number . 2.0) (op . \"*\") (op . \"-\") (number . 1)) ((op . \"-\") (ident . \"x\") (op . \"+\") (op . \"+\") (ident . \"y\")) nil ((number . 123)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full recursive descent parser with precedence climbing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_parser_ast_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse arithmetic expressions into AST nodes:
    //   atom = number | ident | '(' expr ')'
    //   unary = ('-' | '+') unary | atom
    //   mul = unary (('*' | '/') unary)*
    //   add = mul (('+' | '-') mul)*
    //   cmp = add (('<' | '>' | '<=' | '>=' | '==' | '!=') add)?
    //   expr = cmp
    let form = r#"
(progn
  ;; Tokenizer (minimal, inlined)
  (fset 'neovm--rdp2-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t)) (setq pos (1+ pos)))
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start pos))
                 (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'number (string-to-number (substring input start pos))) tokens))))
              ((or (and (>= ch ?a) (<= ch ?z)) (and (>= ch ?A) (<= ch ?Z)) (= ch ?_))
               (let ((start pos))
                 (while (and (< pos len)
                             (let ((c (aref input pos)))
                               (or (and (>= c ?a) (<= c ?z))
                                   (and (>= c ?A) (<= c ?Z))
                                   (and (>= c ?0) (<= c ?9))
                                   (= c ?_))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'ident (substring input start pos)) tokens))))
              ((and (< (1+ pos) len)
                    (member (substring input pos (+ pos 2)) '("<=" ">=" "==" "!=")))
               (setq tokens (cons (cons 'op (substring input pos (+ pos 2))) tokens))
               (setq pos (+ pos 2)))
              ((memq ch '(?+ ?- ?* ?/ ?< ?> ?=))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens))
               (setq pos (1+ pos)))
              ((= ch ?\()
               (setq tokens (cons (cons 'lparen "(") tokens))
               (setq pos (1+ pos)))
              ((= ch ?\))
               (setq tokens (cons (cons 'rparen ")") tokens))
               (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  ;; Parser state: token list stored in dynamic variable
  (defvar neovm--rdp2-tokens nil)

  (fset 'neovm--rdp2-peek
    (lambda ()
      (car neovm--rdp2-tokens)))

  (fset 'neovm--rdp2-advance
    (lambda ()
      (let ((tok (car neovm--rdp2-tokens)))
        (setq neovm--rdp2-tokens (cdr neovm--rdp2-tokens))
        tok)))

  (fset 'neovm--rdp2-expect
    (lambda (type)
      (let ((tok (funcall 'neovm--rdp2-peek)))
        (if (and tok (eq (car tok) type))
            (funcall 'neovm--rdp2-advance)
          nil))))

  ;; atom = number | ident | '(' expr ')'
  (fset 'neovm--rdp2-parse-atom
    (lambda ()
      (let ((tok (funcall 'neovm--rdp2-peek)))
        (cond
          ((null tok) (list 'error "unexpected end of input"))
          ((eq (car tok) 'number)
           (funcall 'neovm--rdp2-advance)
           (list 'num (cdr tok)))
          ((eq (car tok) 'ident)
           (funcall 'neovm--rdp2-advance)
           (list 'var (cdr tok)))
          ((eq (car tok) 'lparen)
           (funcall 'neovm--rdp2-advance)
           (let ((inner (funcall 'neovm--rdp2-parse-expr)))
             (if (funcall 'neovm--rdp2-expect 'rparen)
                 inner
               (list 'error "missing closing paren"))))
          (t (list 'error (format "unexpected token: %S" tok)))))))

  ;; unary = ('-' | '+') unary | atom
  (fset 'neovm--rdp2-parse-unary
    (lambda ()
      (let ((tok (funcall 'neovm--rdp2-peek)))
        (if (and tok (eq (car tok) 'op)
                 (member (cdr tok) '("-" "+")))
            (let ((op (cdr (funcall 'neovm--rdp2-advance))))
              (let ((operand (funcall 'neovm--rdp2-parse-unary)))
                (list 'unary op operand)))
          (funcall 'neovm--rdp2-parse-atom)))))

  ;; mul = unary (('*' | '/') unary)*
  (fset 'neovm--rdp2-parse-mul
    (lambda ()
      (let ((left (funcall 'neovm--rdp2-parse-unary))
            (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--rdp2-peek)))
            (if (and tok (eq (car tok) 'op)
                     (member (cdr tok) '("*" "/")))
                (let ((op (cdr (funcall 'neovm--rdp2-advance)))
                      (right (funcall 'neovm--rdp2-parse-unary)))
                  (setq left (list 'binop op left right)))
              (setq done t))))
        left)))

  ;; add = mul (('+' | '-') mul)*
  (fset 'neovm--rdp2-parse-add
    (lambda ()
      (let ((left (funcall 'neovm--rdp2-parse-mul))
            (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--rdp2-peek)))
            (if (and tok (eq (car tok) 'op)
                     (member (cdr tok) '("+" "-")))
                (let ((op (cdr (funcall 'neovm--rdp2-advance)))
                      (right (funcall 'neovm--rdp2-parse-mul)))
                  (setq left (list 'binop op left right)))
              (setq done t))))
        left)))

  ;; cmp = add (cmp-op add)?
  (fset 'neovm--rdp2-parse-cmp
    (lambda ()
      (let ((left (funcall 'neovm--rdp2-parse-add)))
        (let ((tok (funcall 'neovm--rdp2-peek)))
          (if (and tok (eq (car tok) 'op)
                   (member (cdr tok) '("<" ">" "<=" ">=" "==" "!=")))
              (let ((op (cdr (funcall 'neovm--rdp2-advance)))
                    (right (funcall 'neovm--rdp2-parse-add)))
                (list 'cmp op left right))
            left)))))

  ;; expr = cmp
  (fset 'neovm--rdp2-parse-expr
    (lambda () (funcall 'neovm--rdp2-parse-cmp)))

  (fset 'neovm--rdp2-parse
    (lambda (input)
      (setq neovm--rdp2-tokens (funcall 'neovm--rdp2-tokenize input))
      (let ((ast (funcall 'neovm--rdp2-parse-expr)))
        (if neovm--rdp2-tokens
            (list 'partial ast 'remaining neovm--rdp2-tokens)
          ast))))

  (unwind-protect
      (list
        ;; Simple number
        (funcall 'neovm--rdp2-parse "42")
        ;; Simple variable
        (funcall 'neovm--rdp2-parse "x")
        ;; Binary operations with correct precedence
        (funcall 'neovm--rdp2-parse "1 + 2")
        (funcall 'neovm--rdp2-parse "1 + 2 * 3")
        (funcall 'neovm--rdp2-parse "1 * 2 + 3")
        ;; Parenthesized override
        (funcall 'neovm--rdp2-parse "(1 + 2) * 3")
        ;; Unary operators
        (funcall 'neovm--rdp2-parse "-x")
        (funcall 'neovm--rdp2-parse "-1 + 2")
        (funcall 'neovm--rdp2-parse "-(1 + 2)")
        ;; Comparison
        (funcall 'neovm--rdp2-parse "a + b < c * d")
        (funcall 'neovm--rdp2-parse "x >= 0")
        ;; Complex nested
        (funcall 'neovm--rdp2-parse "((a + b) * (c - d)) / (e + f)")
        ;; Chain of additions
        (funcall 'neovm--rdp2-parse "a + b + c + d"))
    (fmakunbound 'neovm--rdp2-tokenize)
    (fmakunbound 'neovm--rdp2-peek)
    (fmakunbound 'neovm--rdp2-advance)
    (fmakunbound 'neovm--rdp2-expect)
    (fmakunbound 'neovm--rdp2-parse-atom)
    (fmakunbound 'neovm--rdp2-parse-unary)
    (fmakunbound 'neovm--rdp2-parse-mul)
    (fmakunbound 'neovm--rdp2-parse-add)
    (fmakunbound 'neovm--rdp2-parse-cmp)
    (fmakunbound 'neovm--rdp2-parse-expr)
    (fmakunbound 'neovm--rdp2-parse)
    (makunbound 'neovm--rdp2-tokens)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((num 42) (var \"x\") (binop \"+\" (num 1) (num 2)) (binop \"+\" (num 1) (binop \"*\" (num 2) (num 3))) (binop \"+\" (binop \"*\" (num 1) (num 2)) (num 3)) (binop \"*\" (binop \"+\" (num 1) (num 2)) (num 3)) (unary \"-\" (var \"x\")) (binop \"+\" (unary \"-\" (num 1)) (num 2)) (unary \"-\" (binop \"+\" (num 1) (num 2))) (cmp \"<\" (binop \"+\" (var \"a\") (var \"b\")) (binop \"*\" (var \"c\") (var \"d\"))) (cmp \">=\" (var \"x\") (num 0)) (binop \"/\" (binop \"*\" (binop \"+\" (var \"a\") (var \"b\")) (binop \"-\" (var \"c\") (var \"d\"))) (binop \"+\" (var \"e\") (var \"f\"))) (binop \"+\" (binop \"+\" (binop \"+\" (var \"a\") (var \"b\")) (var \"c\")) (var \"d\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AST pretty-printer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_parser_ast_pretty_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pretty-print AST nodes back into human-readable infix notation,
    // with minimal parentheses based on precedence. Also convert to
    // fully-parenthesized and prefix (Lisp) notation.
    let form = r#"
(progn
  ;; Reuse tokenizer and parser (minimal inline)
  (fset 'neovm--rdp3-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t)) (setq pos (1+ pos)))
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start pos))
                 (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'number (string-to-number (substring input start pos))) tokens))))
              ((or (and (>= ch ?a) (<= ch ?z)) (= ch ?_))
               (let ((start pos))
                 (while (and (< pos len) (let ((c (aref input pos))) (or (and (>= c ?a) (<= c ?z)) (= c ?_))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'ident (substring input start pos)) tokens))))
              ((memq ch '(?+ ?- ?* ?/))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens))
               (setq pos (1+ pos)))
              ((= ch ?\()
               (setq tokens (cons (cons 'lparen "(") tokens)) (setq pos (1+ pos)))
              ((= ch ?\))
               (setq tokens (cons (cons 'rparen ")") tokens)) (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (defvar neovm--rdp3-tokens nil)
  (fset 'neovm--rdp3-peek (lambda () (car neovm--rdp3-tokens)))
  (fset 'neovm--rdp3-advance
    (lambda () (let ((t1 (car neovm--rdp3-tokens)))
                 (setq neovm--rdp3-tokens (cdr neovm--rdp3-tokens)) t1)))
  (fset 'neovm--rdp3-expect
    (lambda (type) (let ((t1 (funcall 'neovm--rdp3-peek)))
                     (if (and t1 (eq (car t1) type)) (funcall 'neovm--rdp3-advance) nil))))

  (fset 'neovm--rdp3-parse-atom
    (lambda ()
      (let ((tok (funcall 'neovm--rdp3-peek)))
        (cond
          ((and tok (eq (car tok) 'number)) (funcall 'neovm--rdp3-advance) (list 'num (cdr tok)))
          ((and tok (eq (car tok) 'ident)) (funcall 'neovm--rdp3-advance) (list 'var (cdr tok)))
          ((and tok (eq (car tok) 'lparen))
           (funcall 'neovm--rdp3-advance)
           (let ((inner (funcall 'neovm--rdp3-parse-expr)))
             (funcall 'neovm--rdp3-expect 'rparen) inner))
          (t (list 'error "unexpected"))))))

  (fset 'neovm--rdp3-parse-unary
    (lambda ()
      (let ((tok (funcall 'neovm--rdp3-peek)))
        (if (and tok (eq (car tok) 'op) (string= (cdr tok) "-"))
            (progn (funcall 'neovm--rdp3-advance)
                   (list 'unary "-" (funcall 'neovm--rdp3-parse-unary)))
          (funcall 'neovm--rdp3-parse-atom)))))

  (fset 'neovm--rdp3-parse-mul
    (lambda ()
      (let ((left (funcall 'neovm--rdp3-parse-unary)) (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--rdp3-peek)))
            (if (and tok (eq (car tok) 'op) (member (cdr tok) '("*" "/")))
                (let ((op (cdr (funcall 'neovm--rdp3-advance))))
                  (setq left (list 'binop op left (funcall 'neovm--rdp3-parse-unary))))
              (setq done t)))) left)))

  (fset 'neovm--rdp3-parse-add
    (lambda ()
      (let ((left (funcall 'neovm--rdp3-parse-mul)) (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--rdp3-peek)))
            (if (and tok (eq (car tok) 'op) (member (cdr tok) '("+" "-")))
                (let ((op (cdr (funcall 'neovm--rdp3-advance))))
                  (setq left (list 'binop op left (funcall 'neovm--rdp3-parse-mul))))
              (setq done t)))) left)))

  (fset 'neovm--rdp3-parse-expr (lambda () (funcall 'neovm--rdp3-parse-add)))

  (fset 'neovm--rdp3-parse
    (lambda (input)
      (setq neovm--rdp3-tokens (funcall 'neovm--rdp3-tokenize input))
      (funcall 'neovm--rdp3-parse-expr)))

  ;; Pretty-printer: fully parenthesized
  (fset 'neovm--rdp3-to-full-parens
    (lambda (ast)
      (cond
        ((eq (car ast) 'num) (format "%d" (cadr ast)))
        ((eq (car ast) 'var) (cadr ast))
        ((eq (car ast) 'unary)
         (format "(-%s)" (funcall 'neovm--rdp3-to-full-parens (nth 2 ast))))
        ((eq (car ast) 'binop)
         (format "(%s %s %s)"
                 (funcall 'neovm--rdp3-to-full-parens (nth 2 ast))
                 (nth 1 ast)
                 (funcall 'neovm--rdp3-to-full-parens (nth 3 ast))))
        (t (format "%S" ast)))))

  ;; Pretty-printer: prefix (Lisp) notation
  (fset 'neovm--rdp3-to-prefix
    (lambda (ast)
      (cond
        ((eq (car ast) 'num) (format "%d" (cadr ast)))
        ((eq (car ast) 'var) (cadr ast))
        ((eq (car ast) 'unary)
         (format "(- %s)" (funcall 'neovm--rdp3-to-prefix (nth 2 ast))))
        ((eq (car ast) 'binop)
         (format "(%s %s %s)"
                 (nth 1 ast)
                 (funcall 'neovm--rdp3-to-prefix (nth 2 ast))
                 (funcall 'neovm--rdp3-to-prefix (nth 3 ast))))
        (t (format "%S" ast)))))

  (unwind-protect
      (let ((exprs '("1 + 2" "1 + 2 * 3" "(1 + 2) * 3"
                     "a + b * c - d / e" "-x + y"
                     "a * b + c * d" "((a + b))")))
        (mapcar (lambda (expr)
                  (let ((ast (funcall 'neovm--rdp3-parse expr)))
                    (list 'input expr
                          'ast ast
                          'full-parens (funcall 'neovm--rdp3-to-full-parens ast)
                          'prefix (funcall 'neovm--rdp3-to-prefix ast))))
                exprs))
    (fmakunbound 'neovm--rdp3-tokenize)
    (fmakunbound 'neovm--rdp3-peek)
    (fmakunbound 'neovm--rdp3-advance)
    (fmakunbound 'neovm--rdp3-expect)
    (fmakunbound 'neovm--rdp3-parse-atom)
    (fmakunbound 'neovm--rdp3-parse-unary)
    (fmakunbound 'neovm--rdp3-parse-mul)
    (fmakunbound 'neovm--rdp3-parse-add)
    (fmakunbound 'neovm--rdp3-parse-expr)
    (fmakunbound 'neovm--rdp3-parse)
    (fmakunbound 'neovm--rdp3-to-full-parens)
    (fmakunbound 'neovm--rdp3-to-prefix)
    (makunbound 'neovm--rdp3-tokens)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((input \"1 + 2\" ast (binop \"+\" (num 1) (num 2)) full-parens \"(1 + 2)\" prefix \"(+ 1 2)\") (input \"1 + 2 * 3\" ast (binop \"+\" (num 1) (binop \"*\" (num 2) (num 3))) full-parens \"(1 + (2 * 3))\" prefix \"(+ 1 (* 2 3))\") (input \"(1 + 2) * 3\" ast (binop \"*\" (binop \"+\" (num 1) (num 2)) (num 3)) full-parens \"((1 + 2) * 3)\" prefix \"(* (+ 1 2) 3)\") (input \"a + b * c - d / e\" ast (binop \"-\" (binop \"+\" (var \"a\") (binop \"*\" (var \"b\") (var \"c\"))) (binop \"/\" (var \"d\") (var \"e\"))) full-parens \"((a + (b * c)) - (d / e))\" prefix \"(- (+ a (* b c)) (/ d e))\") (input \"-x + y\" ast (binop \"+\" (unary \"-\" (var \"x\")) (var \"y\")) full-parens \"((-x) + y)\" prefix \"(+ (- x) y)\") (input \"a * b + c * d\" ast (binop \"+\" (binop \"*\" (var \"a\") (var \"b\")) (binop \"*\" (var \"c\") (var \"d\"))) full-parens \"((a * b) + (c * d))\" prefix \"(+ (* a b) (* c d))\") (input \"((a + b))\" ast (binop \"+\" (var \"a\") (var \"b\")) full-parens \"(a + b)\" prefix \"(+ a b)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AST evaluator with variable environment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_parser_ast_evaluator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse an expression, then evaluate the resulting AST in an environment
    // that maps variable names to values. Test arithmetic with variables.
    let form = r#"
(progn
  ;; Minimal tokenizer/parser (same structure, inlined)
  (fset 'neovm--rdp4-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t)) (setq pos (1+ pos)))
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start pos))
                 (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'number (string-to-number (substring input start pos))) tokens))))
              ((or (and (>= ch ?a) (<= ch ?z)) (= ch ?_))
               (let ((start pos))
                 (while (and (< pos len) (let ((c (aref input pos)))
                                            (or (and (>= c ?a) (<= c ?z)) (= c ?_)
                                                (and (>= c ?0) (<= c ?9)))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'ident (substring input start pos)) tokens))))
              ((and (< (1+ pos) len)
                    (member (substring input pos (+ pos 2)) '("<=" ">=" "!=")))
               (setq tokens (cons (cons 'op (substring input pos (+ pos 2))) tokens))
               (setq pos (+ pos 2)))
              ((memq ch '(?+ ?- ?* ?/ ?< ?> ?=))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens))
               (setq pos (1+ pos)))
              ((= ch ?\()
               (setq tokens (cons (cons 'lparen "(") tokens)) (setq pos (1+ pos)))
              ((= ch ?\))
               (setq tokens (cons (cons 'rparen ")") tokens)) (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (defvar neovm--rdp4-tokens nil)
  (fset 'neovm--rdp4-peek (lambda () (car neovm--rdp4-tokens)))
  (fset 'neovm--rdp4-advance
    (lambda () (let ((t1 (car neovm--rdp4-tokens)))
                 (setq neovm--rdp4-tokens (cdr neovm--rdp4-tokens)) t1)))
  (fset 'neovm--rdp4-expect
    (lambda (type) (let ((t1 (funcall 'neovm--rdp4-peek)))
                     (if (and t1 (eq (car t1) type)) (funcall 'neovm--rdp4-advance) nil))))

  (fset 'neovm--rdp4-parse-atom
    (lambda ()
      (let ((tok (funcall 'neovm--rdp4-peek)))
        (cond
          ((and tok (eq (car tok) 'number)) (funcall 'neovm--rdp4-advance) (list 'num (cdr tok)))
          ((and tok (eq (car tok) 'ident)) (funcall 'neovm--rdp4-advance) (list 'var (cdr tok)))
          ((and tok (eq (car tok) 'lparen))
           (funcall 'neovm--rdp4-advance)
           (let ((inner (funcall 'neovm--rdp4-parse-expr)))
             (funcall 'neovm--rdp4-expect 'rparen) inner))
          (t (list 'error "unexpected"))))))

  (fset 'neovm--rdp4-parse-unary
    (lambda ()
      (let ((tok (funcall 'neovm--rdp4-peek)))
        (if (and tok (eq (car tok) 'op) (string= (cdr tok) "-"))
            (progn (funcall 'neovm--rdp4-advance)
                   (list 'unary "-" (funcall 'neovm--rdp4-parse-unary)))
          (funcall 'neovm--rdp4-parse-atom)))))

  (fset 'neovm--rdp4-parse-mul
    (lambda ()
      (let ((left (funcall 'neovm--rdp4-parse-unary)) (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--rdp4-peek)))
            (if (and tok (eq (car tok) 'op) (member (cdr tok) '("*" "/")))
                (let ((op (cdr (funcall 'neovm--rdp4-advance))))
                  (setq left (list 'binop op left (funcall 'neovm--rdp4-parse-unary))))
              (setq done t)))) left)))

  (fset 'neovm--rdp4-parse-add
    (lambda ()
      (let ((left (funcall 'neovm--rdp4-parse-mul)) (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--rdp4-peek)))
            (if (and tok (eq (car tok) 'op) (member (cdr tok) '("+" "-")))
                (let ((op (cdr (funcall 'neovm--rdp4-advance))))
                  (setq left (list 'binop op left (funcall 'neovm--rdp4-parse-mul))))
              (setq done t)))) left)))

  (fset 'neovm--rdp4-parse-cmp
    (lambda ()
      (let ((left (funcall 'neovm--rdp4-parse-add)))
        (let ((tok (funcall 'neovm--rdp4-peek)))
          (if (and tok (eq (car tok) 'op) (member (cdr tok) '("<" ">" "<=" ">=")))
              (let ((op (cdr (funcall 'neovm--rdp4-advance))))
                (list 'cmp op left (funcall 'neovm--rdp4-parse-add)))
            left)))))

  (fset 'neovm--rdp4-parse-expr (lambda () (funcall 'neovm--rdp4-parse-cmp)))

  (fset 'neovm--rdp4-parse
    (lambda (input)
      (setq neovm--rdp4-tokens (funcall 'neovm--rdp4-tokenize input))
      (funcall 'neovm--rdp4-parse-expr)))

  ;; Context
  (fset 'neovm--rdp4-eval-ast
    (lambda (ast env)
      (cond
        ((eq (car ast) 'num) (cadr ast))
        ((eq (car ast) 'var)
         (let ((binding (assoc (cadr ast) env)))
           (if binding (cdr binding)
             (list 'undefined-var (cadr ast)))))
        ((eq (car ast) 'unary)
         (let ((val (funcall 'neovm--rdp4-eval-ast (nth 2 ast) env)))
           (if (numberp val) (- val) (list 'error "cannot negate" val))))
        ((eq (car ast) 'binop)
         (let ((op (nth 1 ast))
               (left (funcall 'neovm--rdp4-eval-ast (nth 2 ast) env))
               (right (funcall 'neovm--rdp4-eval-ast (nth 3 ast) env)))
           (if (and (numberp left) (numberp right))
               (cond
                 ((string= op "+") (+ left right))
                 ((string= op "-") (- left right))
                 ((string= op "*") (* left right))
                 ((string= op "/") (if (= right 0) (list 'error "division by zero")
                                     (/ left right)))
                 (t (list 'error "unknown op" op)))
             (list 'error "non-numeric operands" left right))))
        ((eq (car ast) 'cmp)
         (let ((op (nth 1 ast))
               (left (funcall 'neovm--rdp4-eval-ast (nth 2 ast) env))
               (right (funcall 'neovm--rdp4-eval-ast (nth 3 ast) env)))
           (if (and (numberp left) (numberp right))
               (cond
                 ((string= op "<") (if (< left right) 1 0))
                 ((string= op ">") (if (> left right) 1 0))
                 ((string= op "<=") (if (<= left right) 1 0))
                 ((string= op ">=") (if (>= left right) 1 0))
                 (t (list 'error "unknown cmp" op)))
             (list 'error "non-numeric comparison" left right))))
        (t (list 'error "bad ast node" ast)))))

  (fset 'neovm--rdp4-eval-expr
    (lambda (input env)
      (funcall 'neovm--rdp4-eval-ast
               (funcall 'neovm--rdp4-parse input) env)))

  (unwind-protect
      (let ((env '(("x" . 10) ("y" . 3) ("z" . 7) ("a" . 100) ("b" . 5))))
        (list
          ;; Simple arithmetic
          (funcall 'neovm--rdp4-eval-expr "2 + 3" nil)
          (funcall 'neovm--rdp4-eval-expr "10 - 4 * 2" nil)
          (funcall 'neovm--rdp4-eval-expr "(10 - 4) * 2" nil)
          ;; With variables
          (funcall 'neovm--rdp4-eval-expr "x + y" env)
          (funcall 'neovm--rdp4-eval-expr "x * y + z" env)
          (funcall 'neovm--rdp4-eval-expr "(x + y) * (z - y)" env)
          (funcall 'neovm--rdp4-eval-expr "a / b" env)
          ;; Unary
          (funcall 'neovm--rdp4-eval-expr "-x + y" env)
          (funcall 'neovm--rdp4-eval-expr "-(x + y)" env)
          ;; Comparison
          (funcall 'neovm--rdp4-eval-expr "x > y" env)
          (funcall 'neovm--rdp4-eval-expr "y >= z" env)
          (funcall 'neovm--rdp4-eval-expr "x + y <= z + b" env)
          ;; Division by zero
          (funcall 'neovm--rdp4-eval-expr "x / 0" env)
          ;; Undefined variable
          (funcall 'neovm--rdp4-eval-expr "w + 1" env)))
    (fmakunbound 'neovm--rdp4-tokenize)
    (fmakunbound 'neovm--rdp4-peek)
    (fmakunbound 'neovm--rdp4-advance)
    (fmakunbound 'neovm--rdp4-expect)
    (fmakunbound 'neovm--rdp4-parse-atom)
    (fmakunbound 'neovm--rdp4-parse-unary)
    (fmakunbound 'neovm--rdp4-parse-mul)
    (fmakunbound 'neovm--rdp4-parse-add)
    (fmakunbound 'neovm--rdp4-parse-cmp)
    (fmakunbound 'neovm--rdp4-parse-expr)
    (fmakunbound 'neovm--rdp4-parse)
    (fmakunbound 'neovm--rdp4-eval-ast)
    (fmakunbound 'neovm--rdp4-eval-expr)
    (makunbound 'neovm--rdp4-tokens)))
"#;
    let expect = expect_test::expect![[
        r#""OK (5 2 12 13 37 52 20 -7 -13 1 0 0 (error \"division by zero\") (error \"non-numeric operands\" (undefined-var \"w\") 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parse-evaluate round-trip: verify algebraic identities
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_parser_algebraic_identities() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use the parser+evaluator to verify algebraic identities hold:
    // commutativity, associativity, distributivity, identity elements.
    let form = r#"
(progn
  ;; Inline minimal parser+evaluator
  (fset 'neovm--rdp5-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t)) (setq pos (1+ pos)))
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start pos))
                 (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'number (string-to-number (substring input start pos))) tokens))))
              ((or (and (>= ch ?a) (<= ch ?z)) (= ch ?_))
               (let ((start pos))
                 (while (and (< pos len) (let ((c (aref input pos)))
                                            (or (and (>= c ?a) (<= c ?z)) (= c ?_)
                                                (and (>= c ?0) (<= c ?9)))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'ident (substring input start pos)) tokens))))
              ((memq ch '(?+ ?- ?* ?/))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens))
               (setq pos (1+ pos)))
              ((= ch ?\() (setq tokens (cons (cons 'lparen "(") tokens)) (setq pos (1+ pos)))
              ((= ch ?\)) (setq tokens (cons (cons 'rparen ")") tokens)) (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (defvar neovm--rdp5-tokens nil)
  (fset 'neovm--rdp5-peek (lambda () (car neovm--rdp5-tokens)))
  (fset 'neovm--rdp5-advance
    (lambda () (let ((t1 (car neovm--rdp5-tokens)))
                 (setq neovm--rdp5-tokens (cdr neovm--rdp5-tokens)) t1)))
  (fset 'neovm--rdp5-expect
    (lambda (tp) (if (and (car neovm--rdp5-tokens) (eq (car (car neovm--rdp5-tokens)) tp))
                     (funcall 'neovm--rdp5-advance) nil)))

  (fset 'neovm--rdp5-parse-atom
    (lambda ()
      (let ((tok (funcall 'neovm--rdp5-peek)))
        (cond
          ((and tok (eq (car tok) 'number)) (funcall 'neovm--rdp5-advance) (list 'num (cdr tok)))
          ((and tok (eq (car tok) 'ident)) (funcall 'neovm--rdp5-advance) (list 'var (cdr tok)))
          ((and tok (eq (car tok) 'lparen))
           (funcall 'neovm--rdp5-advance)
           (let ((inner (funcall 'neovm--rdp5-parse-add)))
             (funcall 'neovm--rdp5-expect 'rparen) inner))
          (t (list 'num 0))))))

  (fset 'neovm--rdp5-parse-unary
    (lambda ()
      (let ((tok (funcall 'neovm--rdp5-peek)))
        (if (and tok (eq (car tok) 'op) (string= (cdr tok) "-"))
            (progn (funcall 'neovm--rdp5-advance) (list 'unary "-" (funcall 'neovm--rdp5-parse-unary)))
          (funcall 'neovm--rdp5-parse-atom)))))

  (fset 'neovm--rdp5-parse-mul
    (lambda ()
      (let ((left (funcall 'neovm--rdp5-parse-unary)) (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--rdp5-peek)))
            (if (and tok (eq (car tok) 'op) (member (cdr tok) '("*" "/")))
                (let ((op (cdr (funcall 'neovm--rdp5-advance))))
                  (setq left (list 'binop op left (funcall 'neovm--rdp5-parse-unary))))
              (setq done t)))) left)))

  (fset 'neovm--rdp5-parse-add
    (lambda ()
      (let ((left (funcall 'neovm--rdp5-parse-mul)) (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--rdp5-peek)))
            (if (and tok (eq (car tok) 'op) (member (cdr tok) '("+" "-")))
                (let ((op (cdr (funcall 'neovm--rdp5-advance))))
                  (setq left (list 'binop op left (funcall 'neovm--rdp5-parse-mul))))
              (setq done t)))) left)))

  (fset 'neovm--rdp5-parse
    (lambda (input) (setq neovm--rdp5-tokens (funcall 'neovm--rdp5-tokenize input))
      (funcall 'neovm--rdp5-parse-add)))

  (fset 'neovm--rdp5-eval
    (lambda (ast env)
      (cond
        ((eq (car ast) 'num) (cadr ast))
        ((eq (car ast) 'var) (cdr (assoc (cadr ast) env)))
        ((eq (car ast) 'unary) (- (funcall 'neovm--rdp5-eval (nth 2 ast) env)))
        ((eq (car ast) 'binop)
         (let ((op (nth 1 ast))
               (l (funcall 'neovm--rdp5-eval (nth 2 ast) env))
               (r (funcall 'neovm--rdp5-eval (nth 3 ast) env)))
           (cond ((string= op "+") (+ l r))
                 ((string= op "-") (- l r))
                 ((string= op "*") (* l r))
                 ((string= op "/") (/ l r))))))))

  (fset 'neovm--rdp5-calc
    (lambda (input env)
      (funcall 'neovm--rdp5-eval (funcall 'neovm--rdp5-parse input) env)))

  (unwind-protect
      (let ((env '(("a" . 7) ("b" . 3) ("c" . 5))))
        (list
          ;; Commutativity of addition: a+b = b+a
          (= (funcall 'neovm--rdp5-calc "a + b" env)
             (funcall 'neovm--rdp5-calc "b + a" env))
          ;; Commutativity of multiplication: a*b = b*a
          (= (funcall 'neovm--rdp5-calc "a * b" env)
             (funcall 'neovm--rdp5-calc "b * a" env))
          ;; Associativity of addition: (a+b)+c = a+(b+c)
          (= (funcall 'neovm--rdp5-calc "(a + b) + c" env)
             (funcall 'neovm--rdp5-calc "a + (b + c)" env))
          ;; Associativity of multiplication: (a*b)*c = a*(b*c)
          (= (funcall 'neovm--rdp5-calc "(a * b) * c" env)
             (funcall 'neovm--rdp5-calc "a * (b * c)" env))
          ;; Distributivity: a*(b+c) = a*b + a*c
          (= (funcall 'neovm--rdp5-calc "a * (b + c)" env)
             (funcall 'neovm--rdp5-calc "a * b + a * c" env))
          ;; Identity: a+0 = a, a*1 = a
          (= (funcall 'neovm--rdp5-calc "a + 0" env)
             (funcall 'neovm--rdp5-calc "a" env))
          (= (funcall 'neovm--rdp5-calc "a * 1" env)
             (funcall 'neovm--rdp5-calc "a" env))
          ;; Negation: a + (-a) = 0
          (= (funcall 'neovm--rdp5-calc "a + -a" env) 0)
          ;; Complex: (a+b)*(a-b) = a*a - b*b
          (= (funcall 'neovm--rdp5-calc "(a + b) * (a - b)" env)
             (funcall 'neovm--rdp5-calc "a * a - b * b" env))
          ;; Actual values for sanity check
          (funcall 'neovm--rdp5-calc "a * b + c" env)
          (funcall 'neovm--rdp5-calc "(a + b) * c - a" env)))
    (fmakunbound 'neovm--rdp5-tokenize)
    (fmakunbound 'neovm--rdp5-peek)
    (fmakunbound 'neovm--rdp5-advance)
    (fmakunbound 'neovm--rdp5-expect)
    (fmakunbound 'neovm--rdp5-parse-atom)
    (fmakunbound 'neovm--rdp5-parse-unary)
    (fmakunbound 'neovm--rdp5-parse-mul)
    (fmakunbound 'neovm--rdp5-parse-add)
    (fmakunbound 'neovm--rdp5-parse)
    (fmakunbound 'neovm--rdp5-eval)
    (fmakunbound 'neovm--rdp5-calc)
    (makunbound 'neovm--rdp5-tokens)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t 26 43)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parser error recovery: skip bad tokens and continue parsing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rd_parser_error_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parser that attempts error recovery: when an unexpected token is found,
    // record an error node and skip to the next reasonable point (e.g.,
    // closing paren or operator). Parse multiple semicolon-separated
    // expressions, recovering from errors in individual ones.
    let form = r#"
(progn
  (fset 'neovm--rdp6-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t)) (setq pos (1+ pos)))
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start pos))
                 (while (and (< pos len) (>= (aref input pos) ?0) (<= (aref input pos) ?9))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'number (string-to-number (substring input start pos))) tokens))))
              ((or (and (>= ch ?a) (<= ch ?z)) (= ch ?_))
               (let ((start pos))
                 (while (and (< pos len) (let ((c (aref input pos)))
                                            (or (and (>= c ?a) (<= c ?z)) (= c ?_))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'ident (substring input start pos)) tokens))))
              ((memq ch '(?+ ?- ?* ?/))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens)) (setq pos (1+ pos)))
              ((= ch ?\()
               (setq tokens (cons (cons 'lparen "(") tokens)) (setq pos (1+ pos)))
              ((= ch ?\))
               (setq tokens (cons (cons 'rparen ")") tokens)) (setq pos (1+ pos)))
              ((= ch ?\;)
               (setq tokens (cons (cons 'semi ";") tokens)) (setq pos (1+ pos)))
              (t (setq tokens (cons (cons 'unknown (char-to-string ch)) tokens))
                 (setq pos (1+ pos))))))
        (nreverse tokens))))

  (defvar neovm--rdp6-tokens nil)
  (defvar neovm--rdp6-errors nil)

  (fset 'neovm--rdp6-peek (lambda () (car neovm--rdp6-tokens)))
  (fset 'neovm--rdp6-advance
    (lambda () (let ((t1 (car neovm--rdp6-tokens)))
                 (setq neovm--rdp6-tokens (cdr neovm--rdp6-tokens)) t1)))

  ;; Skip tokens until we find a semicolon, rparen, or end
  (fset 'neovm--rdp6-recover
    (lambda ()
      (let ((skipped nil))
        (while (and neovm--rdp6-tokens
                    (not (memq (car (car neovm--rdp6-tokens)) '(semi rparen))))
          (setq skipped (cons (funcall 'neovm--rdp6-advance) skipped)))
        (nreverse skipped))))

  (fset 'neovm--rdp6-parse-atom
    (lambda ()
      (let ((tok (funcall 'neovm--rdp6-peek)))
        (cond
          ((null tok) (list 'error "unexpected end"))
          ((eq (car tok) 'number) (funcall 'neovm--rdp6-advance) (list 'num (cdr tok)))
          ((eq (car tok) 'ident) (funcall 'neovm--rdp6-advance) (list 'var (cdr tok)))
          ((eq (car tok) 'lparen)
           (funcall 'neovm--rdp6-advance)
           (let ((inner (funcall 'neovm--rdp6-parse-add)))
             (let ((close (funcall 'neovm--rdp6-peek)))
               (if (and close (eq (car close) 'rparen))
                   (progn (funcall 'neovm--rdp6-advance) inner)
                 (setq neovm--rdp6-errors
                       (cons "missing closing paren" neovm--rdp6-errors))
                 inner))))
          (t
           (setq neovm--rdp6-errors
                 (cons (format "unexpected token: %S" tok) neovm--rdp6-errors))
           (funcall 'neovm--rdp6-recover)
           (list 'error "recovered"))))))

  (fset 'neovm--rdp6-parse-mul
    (lambda ()
      (let ((left (funcall 'neovm--rdp6-parse-atom)) (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--rdp6-peek)))
            (if (and tok (eq (car tok) 'op) (member (cdr tok) '("*" "/")))
                (let ((op (cdr (funcall 'neovm--rdp6-advance))))
                  (setq left (list 'binop op left (funcall 'neovm--rdp6-parse-atom))))
              (setq done t)))) left)))

  (fset 'neovm--rdp6-parse-add
    (lambda ()
      (let ((left (funcall 'neovm--rdp6-parse-mul)) (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--rdp6-peek)))
            (if (and tok (eq (car tok) 'op) (member (cdr tok) '("+" "-")))
                (let ((op (cdr (funcall 'neovm--rdp6-advance))))
                  (setq left (list 'binop op left (funcall 'neovm--rdp6-parse-mul))))
              (setq done t)))) left)))

  ;; Parse multiple semicolon-separated expressions with recovery
  (fset 'neovm--rdp6-parse-program
    (lambda (input)
      (setq neovm--rdp6-tokens (funcall 'neovm--rdp6-tokenize input))
      (setq neovm--rdp6-errors nil)
      (let ((exprs nil))
        (while neovm--rdp6-tokens
          (let ((expr (funcall 'neovm--rdp6-parse-add)))
            (setq exprs (cons expr exprs))
            ;; Consume semicolon if present
            (let ((tok (funcall 'neovm--rdp6-peek)))
              (when (and tok (eq (car tok) 'semi))
                (funcall 'neovm--rdp6-advance)))))
        (list 'program (nreverse exprs)
              'errors (nreverse neovm--rdp6-errors)))))

  (unwind-protect
      (list
        ;; All valid expressions
        (funcall 'neovm--rdp6-parse-program "1 + 2; 3 * 4; 5 - 1")
        ;; Expression with missing close paren
        (funcall 'neovm--rdp6-parse-program "(1 + 2; 3 * 4")
        ;; Unknown token triggers recovery
        (funcall 'neovm--rdp6-parse-program "1 + 2; @ + 3; 4 * 5")
        ;; Empty program
        (funcall 'neovm--rdp6-parse-program "")
        ;; Single valid expression
        (funcall 'neovm--rdp6-parse-program "42")
        ;; Multiple errors
        (funcall 'neovm--rdp6-parse-program "1 +; * 2; (3")
        ;; Valid complex expression
        (funcall 'neovm--rdp6-parse-program "(1 + 2) * (3 - 4); 5 / (6 + 7)"))
    (fmakunbound 'neovm--rdp6-tokenize)
    (fmakunbound 'neovm--rdp6-peek)
    (fmakunbound 'neovm--rdp6-advance)
    (fmakunbound 'neovm--rdp6-recover)
    (fmakunbound 'neovm--rdp6-parse-atom)
    (fmakunbound 'neovm--rdp6-parse-mul)
    (fmakunbound 'neovm--rdp6-parse-add)
    (fmakunbound 'neovm--rdp6-parse-program)
    (makunbound 'neovm--rdp6-tokens)
    (makunbound 'neovm--rdp6-errors)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((program ((binop \"+\" (num 1) (num 2)) (binop \"*\" (num 3) (num 4)) (binop \"-\" (num 5) (num 1))) errors nil) (program ((binop \"+\" (num 1) (num 2)) (binop \"*\" (num 3) (num 4))) errors (\"missing closing paren\")) (program ((binop \"+\" (num 1) (num 2)) (error \"recovered\") (binop \"*\" (num 4) (num 5))) errors (\"unexpected token: (unknown . \\\"@\\\")\")) (program nil errors nil) (program ((num 42)) errors nil) (program ((binop \"+\" (num 1) (error \"recovered\")) (error \"recovered\") (num 3)) errors (\"unexpected token: (semi . \\\";\\\")\" \"unexpected token: (op . \\\"*\\\")\" \"missing closing paren\")) (program ((binop \"*\" (binop \"+\" (num 1) (num 2)) (binop \"-\" (num 3) (num 4))) (binop \"/\" (num 5) (binop \"+\" (num 6) (num 7)))) errors nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
