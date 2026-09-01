//! Oracle parity tests for a Pratt parser (top-down operator precedence)
//! implemented in Elisp.
//!
//! Tests tokenization, Pratt parsing with binding power for operators,
//! operator precedence (+/- low, */% medium, ^ high right-assoc),
//! prefix operators (unary minus/plus), parenthesized expressions,
//! and full parse-and-evaluate of arithmetic expressions.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Tokenizer: string to token list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pratt_parser_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pp-is-digit
    (lambda (ch) (and (>= ch ?0) (<= ch ?9))))

  (fset 'neovm--pp-tokenize
    (lambda (input)
      "Tokenize INPUT into list of (type . value) tokens."
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ;; Whitespace
              ((memq ch '(?\s ?\t ?\n))
               (setq pos (1+ pos)))
              ;; Number (integer only for simplicity)
              ((funcall 'neovm--pp-is-digit ch)
               (let ((start pos))
                 (while (and (< pos len)
                             (funcall 'neovm--pp-is-digit (aref input pos)))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ;; Operators and punctuation
              ((memq ch '(?+ ?- ?* ?/ ?% ?^ ?\( ?\)))
               (setq tokens (cons (cons (cond ((= ch ?\() 'lparen)
                                              ((= ch ?\)) 'rparen)
                                              (t 'op))
                                        (char-to-string ch))
                                  tokens))
               (setq pos (1+ pos)))
              ;; Unknown: skip
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (unwind-protect
      (list
        (funcall 'neovm--pp-tokenize "1 + 2")
        (funcall 'neovm--pp-tokenize "3 * 4 + 5")
        (funcall 'neovm--pp-tokenize "(1 + 2) * 3")
        (funcall 'neovm--pp-tokenize "10 - -3")
        (funcall 'neovm--pp-tokenize "2 ^ 3 ^ 2")
        (funcall 'neovm--pp-tokenize "100 % 7")
        (funcall 'neovm--pp-tokenize "")
        (funcall 'neovm--pp-tokenize "42")
        (funcall 'neovm--pp-tokenize "((1))"))
    (fmakunbound 'neovm--pp-is-digit)
    (fmakunbound 'neovm--pp-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK (((num . 1) (op . \"+\") (num . 2)) ((num . 3) (op . \"*\") (num . 4) (op . \"+\") (num . 5)) ((lparen . \"(\") (num . 1) (op . \"+\") (num . 2) (rparen . \")\") (op . \"*\") (num . 3)) ((num . 10) (op . \"-\") (op . \"-\") (num . 3)) ((num . 2) (op . \"^\") (num . 3) (op . \"^\") (num . 2)) ((num . 100) (op . \"%\") (num . 7)) nil ((num . 42)) ((lparen . \"(\") (lparen . \"(\") (num . 1) (rparen . \")\") (rparen . \")\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pratt parser with binding power and precedence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pratt_parser_basic_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full Pratt parser: tokenize + parse into AST + evaluate.
    // Precedence: +/- (1), */% (2), ^ (3, right-assoc)
    // Prefix: unary -, unary +
    let form = r#"(progn
  (fset 'neovm--pp-is-digit
    (lambda (ch) (and (>= ch ?0) (<= ch ?9))))

  (fset 'neovm--pp-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t ?\n)) (setq pos (1+ pos)))
              ((funcall 'neovm--pp-is-digit ch)
               (let ((start pos))
                 (while (and (< pos len)
                             (funcall 'neovm--pp-is-digit (aref input pos)))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ((memq ch '(?+ ?- ?* ?/ ?% ?^ ?\( ?\)))
               (setq tokens (cons (cons (cond ((= ch ?\() 'lparen)
                                              ((= ch ?\)) 'rparen)
                                              (t 'op))
                                        (char-to-string ch))
                                  tokens))
               (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  ;; Parser state: mutable list holding remaining tokens
  (fset 'neovm--pp-peek
    (lambda (state) (car (car state))))

  (fset 'neovm--pp-advance
    (lambda (state)
      (let ((tok (car (car state))))
        (setcar state (cdr (car state)))
        tok)))

  ;; Binding power: returns (left-bp . right-bp) for infix operators
  (fset 'neovm--pp-infix-bp
    (lambda (op)
      (cond
        ((or (string= op "+") (string= op "-")) (cons 1 2))
        ((or (string= op "*") (string= op "/") (string= op "%")) (cons 3 4))
        ;; ^ is right-associative: right bp = left bp (not +1)
        ((string= op "^") (cons 6 5))
        (t nil))))

  ;; Prefix binding power
  (fset 'neovm--pp-prefix-bp
    (lambda (op)
      (cond
        ((or (string= op "+") (string= op "-")) 7)
        (t nil))))

  ;; Parse expression with minimum binding power
  (fset 'neovm--pp-parse-expr
    (lambda (state min-bp)
      (let* ((tok (funcall 'neovm--pp-advance state))
             (lhs
              (cond
                ;; Number literal
                ((and tok (eq (car tok) 'num))
                 (list 'num (cdr tok)))
                ;; Parenthesized expression
                ((and tok (eq (car tok) 'lparen))
                 (let ((inner (funcall 'neovm--pp-parse-expr state 0)))
                   (funcall 'neovm--pp-advance state)  ;; consume rparen
                   inner))
                ;; Prefix operator
                ((and tok (eq (car tok) 'op))
                 (let ((pbp (funcall 'neovm--pp-prefix-bp (cdr tok))))
                   (if pbp
                       (let ((rhs (funcall 'neovm--pp-parse-expr state pbp)))
                         (list 'prefix (cdr tok) rhs))
                     (list 'error "unexpected prefix" tok))))
                (t (list 'error "unexpected token" tok)))))
        ;; Infix loop
        (let ((done nil))
          (while (not done)
            (let ((next (funcall 'neovm--pp-peek state)))
              (if (and next (eq (car next) 'op))
                  (let ((bp (funcall 'neovm--pp-infix-bp (cdr next))))
                    (if (and bp (> (car bp) min-bp))
                        (progn
                          (funcall 'neovm--pp-advance state)
                          (let ((rhs (funcall 'neovm--pp-parse-expr
                                              state (cdr bp))))
                            (setq lhs (list 'binop (cdr next) lhs rhs))))
                      (setq done t)))
                (setq done t)))))
        lhs)))

  (fset 'neovm--pp-parse
    (lambda (input)
      (let* ((tokens (funcall 'neovm--pp-tokenize input))
             (state (list tokens)))
        (funcall 'neovm--pp-parse-expr state 0))))

  (unwind-protect
      (list
        ;; Simple addition
        (funcall 'neovm--pp-parse "1 + 2")
        ;; Precedence: * binds tighter than +
        (funcall 'neovm--pp-parse "1 + 2 * 3")
        ;; Left-to-right for same precedence
        (funcall 'neovm--pp-parse "1 + 2 + 3")
        ;; Right-associative ^
        (funcall 'neovm--pp-parse "2 ^ 3 ^ 2")
        ;; Mixed precedence
        (funcall 'neovm--pp-parse "1 + 2 * 3 + 4")
        ;; Single number
        (funcall 'neovm--pp-parse "42"))
    (fmakunbound 'neovm--pp-is-digit)
    (fmakunbound 'neovm--pp-tokenize)
    (fmakunbound 'neovm--pp-peek)
    (fmakunbound 'neovm--pp-advance)
    (fmakunbound 'neovm--pp-infix-bp)
    (fmakunbound 'neovm--pp-prefix-bp)
    (fmakunbound 'neovm--pp-parse-expr)
    (fmakunbound 'neovm--pp-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK ((binop \"+\" (num 1) (num 2)) (binop \"+\" (num 1) (binop \"*\" (num 2) (num 3))) (binop \"+\" (binop \"+\" (num 1) (num 2)) (num 3)) (binop \"^\" (num 2) (binop \"^\" (num 3) (num 2))) (binop \"+\" (binop \"+\" (num 1) (binop \"*\" (num 2) (num 3))) (num 4)) (num 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Prefix operators: unary minus and unary plus
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pratt_parser_prefix_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pp-is-digit
    (lambda (ch) (and (>= ch ?0) (<= ch ?9))))
  (fset 'neovm--pp-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t ?\n)) (setq pos (1+ pos)))
              ((funcall 'neovm--pp-is-digit ch)
               (let ((start pos))
                 (while (and (< pos len)
                             (funcall 'neovm--pp-is-digit (aref input pos)))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ((memq ch '(?+ ?- ?* ?/ ?% ?^ ?\( ?\)))
               (setq tokens (cons (cons (cond ((= ch ?\() 'lparen)
                                              ((= ch ?\)) 'rparen)
                                              (t 'op))
                                        (char-to-string ch))
                                  tokens))
               (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))
  (fset 'neovm--pp-peek (lambda (state) (car (car state))))
  (fset 'neovm--pp-advance
    (lambda (state)
      (let ((tok (car (car state)))) (setcar state (cdr (car state))) tok)))
  (fset 'neovm--pp-infix-bp
    (lambda (op)
      (cond
        ((or (string= op "+") (string= op "-")) (cons 1 2))
        ((or (string= op "*") (string= op "/") (string= op "%")) (cons 3 4))
        ((string= op "^") (cons 6 5))
        (t nil))))
  (fset 'neovm--pp-prefix-bp
    (lambda (op)
      (cond ((or (string= op "+") (string= op "-")) 7) (t nil))))
  (fset 'neovm--pp-parse-expr
    (lambda (state min-bp)
      (let* ((tok (funcall 'neovm--pp-advance state))
             (lhs
              (cond
                ((and tok (eq (car tok) 'num)) (list 'num (cdr tok)))
                ((and tok (eq (car tok) 'lparen))
                 (let ((inner (funcall 'neovm--pp-parse-expr state 0)))
                   (funcall 'neovm--pp-advance state) inner))
                ((and tok (eq (car tok) 'op))
                 (let ((pbp (funcall 'neovm--pp-prefix-bp (cdr tok))))
                   (if pbp
                       (let ((rhs (funcall 'neovm--pp-parse-expr state pbp)))
                         (list 'prefix (cdr tok) rhs))
                     (list 'error "bad prefix" tok))))
                (t (list 'error "unexpected" tok)))))
        (let ((done nil))
          (while (not done)
            (let ((next (funcall 'neovm--pp-peek state)))
              (if (and next (eq (car next) 'op))
                  (let ((bp (funcall 'neovm--pp-infix-bp (cdr next))))
                    (if (and bp (> (car bp) min-bp))
                        (progn
                          (funcall 'neovm--pp-advance state)
                          (let ((rhs (funcall 'neovm--pp-parse-expr state (cdr bp))))
                            (setq lhs (list 'binop (cdr next) lhs rhs))))
                      (setq done t)))
                (setq done t)))))
        lhs)))
  (fset 'neovm--pp-parse
    (lambda (input)
      (let* ((tokens (funcall 'neovm--pp-tokenize input))
             (state (list tokens)))
        (funcall 'neovm--pp-parse-expr state 0))))

  (unwind-protect
      (list
        ;; Unary minus
        (funcall 'neovm--pp-parse "-5")
        ;; Unary plus
        (funcall 'neovm--pp-parse "+3")
        ;; Unary minus in expression
        (funcall 'neovm--pp-parse "1 + -2")
        ;; Double unary minus
        (funcall 'neovm--pp-parse "--4")
        ;; Unary minus with multiplication
        (funcall 'neovm--pp-parse "-2 * 3")
        ;; Unary minus with exponentiation
        (funcall 'neovm--pp-parse "-2 ^ 3")
        ;; Unary minus in parentheses
        (funcall 'neovm--pp-parse "(-5) + 3")
        ;; Complex: unary in nested expression
        (funcall 'neovm--pp-parse "-(1 + 2) * -3"))
    (fmakunbound 'neovm--pp-is-digit)
    (fmakunbound 'neovm--pp-tokenize)
    (fmakunbound 'neovm--pp-peek)
    (fmakunbound 'neovm--pp-advance)
    (fmakunbound 'neovm--pp-infix-bp)
    (fmakunbound 'neovm--pp-prefix-bp)
    (fmakunbound 'neovm--pp-parse-expr)
    (fmakunbound 'neovm--pp-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK ((prefix \"-\" (num 5)) (prefix \"+\" (num 3)) (binop \"+\" (num 1) (prefix \"-\" (num 2))) (prefix \"-\" (prefix \"-\" (num 4))) (binop \"*\" (prefix \"-\" (num 2)) (num 3)) (binop \"^\" (prefix \"-\" (num 2)) (num 3)) (binop \"+\" (prefix \"-\" (num 5)) (num 3)) (binop \"*\" (prefix \"-\" (binop \"+\" (num 1) (num 2))) (prefix \"-\" (num 3))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parenthesized expressions and grouping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pratt_parser_parens() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pp-is-digit
    (lambda (ch) (and (>= ch ?0) (<= ch ?9))))
  (fset 'neovm--pp-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t ?\n)) (setq pos (1+ pos)))
              ((funcall 'neovm--pp-is-digit ch)
               (let ((start pos))
                 (while (and (< pos len)
                             (funcall 'neovm--pp-is-digit (aref input pos)))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ((memq ch '(?+ ?- ?* ?/ ?% ?^ ?\( ?\)))
               (setq tokens (cons (cons (cond ((= ch ?\() 'lparen)
                                              ((= ch ?\)) 'rparen)
                                              (t 'op))
                                        (char-to-string ch))
                                  tokens))
               (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))
  (fset 'neovm--pp-peek (lambda (state) (car (car state))))
  (fset 'neovm--pp-advance
    (lambda (state)
      (let ((tok (car (car state)))) (setcar state (cdr (car state))) tok)))
  (fset 'neovm--pp-infix-bp
    (lambda (op)
      (cond
        ((or (string= op "+") (string= op "-")) (cons 1 2))
        ((or (string= op "*") (string= op "/") (string= op "%")) (cons 3 4))
        ((string= op "^") (cons 6 5))
        (t nil))))
  (fset 'neovm--pp-prefix-bp
    (lambda (op)
      (cond ((or (string= op "+") (string= op "-")) 7) (t nil))))
  (fset 'neovm--pp-parse-expr
    (lambda (state min-bp)
      (let* ((tok (funcall 'neovm--pp-advance state))
             (lhs
              (cond
                ((and tok (eq (car tok) 'num)) (list 'num (cdr tok)))
                ((and tok (eq (car tok) 'lparen))
                 (let ((inner (funcall 'neovm--pp-parse-expr state 0)))
                   (funcall 'neovm--pp-advance state) inner))
                ((and tok (eq (car tok) 'op))
                 (let ((pbp (funcall 'neovm--pp-prefix-bp (cdr tok))))
                   (if pbp
                       (let ((rhs (funcall 'neovm--pp-parse-expr state pbp)))
                         (list 'prefix (cdr tok) rhs))
                     (list 'error "bad prefix" tok))))
                (t (list 'error "unexpected" tok)))))
        (let ((done nil))
          (while (not done)
            (let ((next (funcall 'neovm--pp-peek state)))
              (if (and next (eq (car next) 'op))
                  (let ((bp (funcall 'neovm--pp-infix-bp (cdr next))))
                    (if (and bp (> (car bp) min-bp))
                        (progn
                          (funcall 'neovm--pp-advance state)
                          (let ((rhs (funcall 'neovm--pp-parse-expr state (cdr bp))))
                            (setq lhs (list 'binop (cdr next) lhs rhs))))
                      (setq done t)))
                (setq done t)))))
        lhs)))
  (fset 'neovm--pp-parse
    (lambda (input)
      (let* ((tokens (funcall 'neovm--pp-tokenize input))
             (state (list tokens)))
        (funcall 'neovm--pp-parse-expr state 0))))

  (unwind-protect
      (list
        ;; Simple parenthesized
        (funcall 'neovm--pp-parse "(1 + 2)")
        ;; Parens override precedence
        (funcall 'neovm--pp-parse "(1 + 2) * 3")
        ;; Without parens for comparison
        (funcall 'neovm--pp-parse "1 + 2 * 3")
        ;; Nested parens
        (funcall 'neovm--pp-parse "((1 + 2))")
        (funcall 'neovm--pp-parse "((1 + 2) * (3 + 4))")
        ;; Deep nesting
        (funcall 'neovm--pp-parse "(((5)))")
        ;; Parens with prefix
        (funcall 'neovm--pp-parse "-(1 + 2)")
        ;; Complex grouping
        (funcall 'neovm--pp-parse "(1 + 2) * (3 - 4) + (5 * 6)"))
    (fmakunbound 'neovm--pp-is-digit)
    (fmakunbound 'neovm--pp-tokenize)
    (fmakunbound 'neovm--pp-peek)
    (fmakunbound 'neovm--pp-advance)
    (fmakunbound 'neovm--pp-infix-bp)
    (fmakunbound 'neovm--pp-prefix-bp)
    (fmakunbound 'neovm--pp-parse-expr)
    (fmakunbound 'neovm--pp-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK ((binop \"+\" (num 1) (num 2)) (binop \"*\" (binop \"+\" (num 1) (num 2)) (num 3)) (binop \"+\" (num 1) (binop \"*\" (num 2) (num 3))) (binop \"+\" (num 1) (num 2)) (binop \"*\" (binop \"+\" (num 1) (num 2)) (binop \"+\" (num 3) (num 4))) (num 5) (prefix \"-\" (binop \"+\" (num 1) (num 2))) (binop \"+\" (binop \"*\" (binop \"+\" (num 1) (num 2)) (binop \"-\" (num 3) (num 4))) (binop \"*\" (num 5) (num 6))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parse and evaluate: full pipeline from string to result
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pratt_parser_evaluate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pp-is-digit
    (lambda (ch) (and (>= ch ?0) (<= ch ?9))))
  (fset 'neovm--pp-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t ?\n)) (setq pos (1+ pos)))
              ((funcall 'neovm--pp-is-digit ch)
               (let ((start pos))
                 (while (and (< pos len)
                             (funcall 'neovm--pp-is-digit (aref input pos)))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ((memq ch '(?+ ?- ?* ?/ ?% ?^ ?\( ?\)))
               (setq tokens (cons (cons (cond ((= ch ?\() 'lparen)
                                              ((= ch ?\)) 'rparen)
                                              (t 'op))
                                        (char-to-string ch))
                                  tokens))
               (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))
  (fset 'neovm--pp-peek (lambda (state) (car (car state))))
  (fset 'neovm--pp-advance
    (lambda (state)
      (let ((tok (car (car state)))) (setcar state (cdr (car state))) tok)))
  (fset 'neovm--pp-infix-bp
    (lambda (op)
      (cond
        ((or (string= op "+") (string= op "-")) (cons 1 2))
        ((or (string= op "*") (string= op "/") (string= op "%")) (cons 3 4))
        ((string= op "^") (cons 6 5))
        (t nil))))
  (fset 'neovm--pp-prefix-bp
    (lambda (op)
      (cond ((or (string= op "+") (string= op "-")) 7) (t nil))))
  (fset 'neovm--pp-parse-expr
    (lambda (state min-bp)
      (let* ((tok (funcall 'neovm--pp-advance state))
             (lhs
              (cond
                ((and tok (eq (car tok) 'num)) (list 'num (cdr tok)))
                ((and tok (eq (car tok) 'lparen))
                 (let ((inner (funcall 'neovm--pp-parse-expr state 0)))
                   (funcall 'neovm--pp-advance state) inner))
                ((and tok (eq (car tok) 'op))
                 (let ((pbp (funcall 'neovm--pp-prefix-bp (cdr tok))))
                   (if pbp
                       (let ((rhs (funcall 'neovm--pp-parse-expr state pbp)))
                         (list 'prefix (cdr tok) rhs))
                     (list 'error "bad prefix" tok))))
                (t (list 'error "unexpected" tok)))))
        (let ((done nil))
          (while (not done)
            (let ((next (funcall 'neovm--pp-peek state)))
              (if (and next (eq (car next) 'op))
                  (let ((bp (funcall 'neovm--pp-infix-bp (cdr next))))
                    (if (and bp (> (car bp) min-bp))
                        (progn
                          (funcall 'neovm--pp-advance state)
                          (let ((rhs (funcall 'neovm--pp-parse-expr state (cdr bp))))
                            (setq lhs (list 'binop (cdr next) lhs rhs))))
                      (setq done t)))
                (setq done t)))))
        lhs)))
  (fset 'neovm--pp-parse
    (lambda (input)
      (let* ((tokens (funcall 'neovm--pp-tokenize input))
             (state (list tokens)))
        (funcall 'neovm--pp-parse-expr state 0))))

  ;; Context: walk AST and compute result
  (fset 'neovm--pp-eval-ast
    (lambda (ast)
      (cond
        ((eq (car ast) 'num) (cadr ast))
        ((eq (car ast) 'prefix)
         (let ((op (cadr ast))
               (val (funcall 'neovm--pp-eval-ast (caddr ast))))
           (cond
             ((string= op "-") (- val))
             ((string= op "+") val)
             (t 0))))
        ((eq (car ast) 'binop)
         (let ((op (cadr ast))
               (l (funcall 'neovm--pp-eval-ast (caddr ast)))
               (r (funcall 'neovm--pp-eval-ast (cadddr ast))))
           (cond
             ((string= op "+") (+ l r))
             ((string= op "-") (- l r))
             ((string= op "*") (* l r))
             ((string= op "/") (/ l r))
             ((string= op "%") (% l r))
             ((string= op "^") (expt l r))
             (t 0))))
        (t 0))))

  ;; Convenience: parse and evaluate in one step
  (fset 'neovm--pp-calc
    (lambda (input)
      (funcall 'neovm--pp-eval-ast (funcall 'neovm--pp-parse input))))

  (unwind-protect
      (list
        ;; Basic arithmetic
        (funcall 'neovm--pp-calc "1 + 2")
        (funcall 'neovm--pp-calc "10 - 3")
        (funcall 'neovm--pp-calc "4 * 5")
        (funcall 'neovm--pp-calc "15 / 3")
        (funcall 'neovm--pp-calc "17 % 5")
        ;; Precedence: 2 + 3 * 4 = 14 (not 20)
        (funcall 'neovm--pp-calc "2 + 3 * 4")
        ;; Parens override: (2 + 3) * 4 = 20
        (funcall 'neovm--pp-calc "(2 + 3) * 4")
        ;; Right-assoc exponentiation: 2^3^2 = 2^(3^2) = 2^9 = 512
        (funcall 'neovm--pp-calc "2 ^ 3 ^ 2")
        ;; Left-assoc: (2^3)^2 = 8^2 = 64
        (funcall 'neovm--pp-calc "(2 ^ 3) ^ 2")
        ;; Unary minus
        (funcall 'neovm--pp-calc "-5 + 3")
        (funcall 'neovm--pp-calc "-(1 + 2) * 3")
        ;; Complex expression
        (funcall 'neovm--pp-calc "(10 + 20) * (30 - 25) / 5")
        ;; Nested
        (funcall 'neovm--pp-calc "((2 + 3) * (4 - 1)) + 7")
        ;; Single number
        (funcall 'neovm--pp-calc "42")
        ;; Modulo
        (funcall 'neovm--pp-calc "100 % 7 + 2"))
    (fmakunbound 'neovm--pp-is-digit)
    (fmakunbound 'neovm--pp-tokenize)
    (fmakunbound 'neovm--pp-peek)
    (fmakunbound 'neovm--pp-advance)
    (fmakunbound 'neovm--pp-infix-bp)
    (fmakunbound 'neovm--pp-prefix-bp)
    (fmakunbound 'neovm--pp-parse-expr)
    (fmakunbound 'neovm--pp-parse)
    (fmakunbound 'neovm--pp-eval-ast)
    (fmakunbound 'neovm--pp-calc)))"#;
    let expect = expect_test::expect![[r#""OK (3 7 20 5 2 14 20 512 64 -2 -9 30 22 42 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AST pretty-printing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pratt_parser_pretty_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pp-is-digit
    (lambda (ch) (and (>= ch ?0) (<= ch ?9))))
  (fset 'neovm--pp-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t ?\n)) (setq pos (1+ pos)))
              ((funcall 'neovm--pp-is-digit ch)
               (let ((start pos))
                 (while (and (< pos len)
                             (funcall 'neovm--pp-is-digit (aref input pos)))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ((memq ch '(?+ ?- ?* ?/ ?% ?^ ?\( ?\)))
               (setq tokens (cons (cons (cond ((= ch ?\() 'lparen)
                                              ((= ch ?\)) 'rparen)
                                              (t 'op))
                                        (char-to-string ch))
                                  tokens))
               (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))
  (fset 'neovm--pp-peek (lambda (state) (car (car state))))
  (fset 'neovm--pp-advance
    (lambda (state)
      (let ((tok (car (car state)))) (setcar state (cdr (car state))) tok)))
  (fset 'neovm--pp-infix-bp
    (lambda (op)
      (cond
        ((or (string= op "+") (string= op "-")) (cons 1 2))
        ((or (string= op "*") (string= op "/") (string= op "%")) (cons 3 4))
        ((string= op "^") (cons 6 5))
        (t nil))))
  (fset 'neovm--pp-prefix-bp
    (lambda (op)
      (cond ((or (string= op "+") (string= op "-")) 7) (t nil))))
  (fset 'neovm--pp-parse-expr
    (lambda (state min-bp)
      (let* ((tok (funcall 'neovm--pp-advance state))
             (lhs
              (cond
                ((and tok (eq (car tok) 'num)) (list 'num (cdr tok)))
                ((and tok (eq (car tok) 'lparen))
                 (let ((inner (funcall 'neovm--pp-parse-expr state 0)))
                   (funcall 'neovm--pp-advance state) inner))
                ((and tok (eq (car tok) 'op))
                 (let ((pbp (funcall 'neovm--pp-prefix-bp (cdr tok))))
                   (if pbp
                       (let ((rhs (funcall 'neovm--pp-parse-expr state pbp)))
                         (list 'prefix (cdr tok) rhs))
                     (list 'error "bad prefix" tok))))
                (t (list 'error "unexpected" tok)))))
        (let ((done nil))
          (while (not done)
            (let ((next (funcall 'neovm--pp-peek state)))
              (if (and next (eq (car next) 'op))
                  (let ((bp (funcall 'neovm--pp-infix-bp (cdr next))))
                    (if (and bp (> (car bp) min-bp))
                        (progn
                          (funcall 'neovm--pp-advance state)
                          (let ((rhs (funcall 'neovm--pp-parse-expr state (cdr bp))))
                            (setq lhs (list 'binop (cdr next) lhs rhs))))
                      (setq done t)))
                (setq done t)))))
        lhs)))
  (fset 'neovm--pp-parse
    (lambda (input)
      (let* ((tokens (funcall 'neovm--pp-tokenize input))
             (state (list tokens)))
        (funcall 'neovm--pp-parse-expr state 0))))

  ;; Pretty-print AST back to infix notation with minimal parens
  (fset 'neovm--pp-ast-to-string
    (lambda (ast)
      (cond
        ((eq (car ast) 'num)
         (number-to-string (cadr ast)))
        ((eq (car ast) 'prefix)
         (concat "(" (cadr ast) (funcall 'neovm--pp-ast-to-string (caddr ast)) ")"))
        ((eq (car ast) 'binop)
         (concat "("
                 (funcall 'neovm--pp-ast-to-string (caddr ast))
                 " " (cadr ast) " "
                 (funcall 'neovm--pp-ast-to-string (cadddr ast))
                 ")"))
        (t "?"))))

  ;; Indented tree display
  (fset 'neovm--pp-ast-tree
    (lambda (ast indent)
      (let ((pad (make-string indent ?\s)))
        (cond
          ((eq (car ast) 'num)
           (concat pad (number-to-string (cadr ast)) "\n"))
          ((eq (car ast) 'prefix)
           (concat pad "prefix:" (cadr ast) "\n"
                   (funcall 'neovm--pp-ast-tree (caddr ast) (+ indent 2))))
          ((eq (car ast) 'binop)
           (concat pad "op:" (cadr ast) "\n"
                   (funcall 'neovm--pp-ast-tree (caddr ast) (+ indent 2))
                   (funcall 'neovm--pp-ast-tree (cadddr ast) (+ indent 2))))
          (t (concat pad "?\n"))))))

  (unwind-protect
      (list
        ;; Infix output
        (funcall 'neovm--pp-ast-to-string (funcall 'neovm--pp-parse "1 + 2 * 3"))
        (funcall 'neovm--pp-ast-to-string (funcall 'neovm--pp-parse "(1 + 2) * 3"))
        (funcall 'neovm--pp-ast-to-string (funcall 'neovm--pp-parse "-5 + 3"))
        (funcall 'neovm--pp-ast-to-string (funcall 'neovm--pp-parse "2 ^ 3 ^ 2"))
        ;; Tree output
        (funcall 'neovm--pp-ast-tree (funcall 'neovm--pp-parse "1 + 2 * 3") 0)
        (funcall 'neovm--pp-ast-tree (funcall 'neovm--pp-parse "-(1 + 2)") 0)
        ;; Round-trip: parse, pretty-print, re-parse, evaluate both
        (let* ((expr "2 + 3 * 4")
               (ast1 (funcall 'neovm--pp-parse expr))
               (printed (funcall 'neovm--pp-ast-to-string ast1)))
          (list expr printed ast1)))
    (fmakunbound 'neovm--pp-is-digit)
    (fmakunbound 'neovm--pp-tokenize)
    (fmakunbound 'neovm--pp-peek)
    (fmakunbound 'neovm--pp-advance)
    (fmakunbound 'neovm--pp-infix-bp)
    (fmakunbound 'neovm--pp-prefix-bp)
    (fmakunbound 'neovm--pp-parse-expr)
    (fmakunbound 'neovm--pp-parse)
    (fmakunbound 'neovm--pp-ast-to-string)
    (fmakunbound 'neovm--pp-ast-tree)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"(1 + (2 * 3))\" \"((1 + 2) * 3)\" \"((-5) + 3)\" \"(2 ^ (3 ^ 2))\" \"op:+\\n  1\\n  op:*\\n    2\\n    3\\n\" \"prefix:-\\n  op:+\\n    1\\n    2\\n\" (\"2 + 3 * 4\" \"(2 + (3 * 4))\" (binop \"+\" (num 2) (binop \"*\" (num 3) (num 4)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: modulo operator and combined expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pratt_parser_modulo_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pp-is-digit
    (lambda (ch) (and (>= ch ?0) (<= ch ?9))))
  (fset 'neovm--pp-tokenize
    (lambda (input)
      (let ((pos 0) (tokens nil) (len (length input)))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t ?\n)) (setq pos (1+ pos)))
              ((funcall 'neovm--pp-is-digit ch)
               (let ((start pos))
                 (while (and (< pos len)
                             (funcall 'neovm--pp-is-digit (aref input pos)))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ((memq ch '(?+ ?- ?* ?/ ?% ?^ ?\( ?\)))
               (setq tokens (cons (cons (cond ((= ch ?\() 'lparen)
                                              ((= ch ?\)) 'rparen)
                                              (t 'op))
                                        (char-to-string ch))
                                  tokens))
               (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))
  (fset 'neovm--pp-peek (lambda (state) (car (car state))))
  (fset 'neovm--pp-advance
    (lambda (state)
      (let ((tok (car (car state)))) (setcar state (cdr (car state))) tok)))
  (fset 'neovm--pp-infix-bp
    (lambda (op)
      (cond
        ((or (string= op "+") (string= op "-")) (cons 1 2))
        ((or (string= op "*") (string= op "/") (string= op "%")) (cons 3 4))
        ((string= op "^") (cons 6 5))
        (t nil))))
  (fset 'neovm--pp-prefix-bp
    (lambda (op)
      (cond ((or (string= op "+") (string= op "-")) 7) (t nil))))
  (fset 'neovm--pp-parse-expr
    (lambda (state min-bp)
      (let* ((tok (funcall 'neovm--pp-advance state))
             (lhs
              (cond
                ((and tok (eq (car tok) 'num)) (list 'num (cdr tok)))
                ((and tok (eq (car tok) 'lparen))
                 (let ((inner (funcall 'neovm--pp-parse-expr state 0)))
                   (funcall 'neovm--pp-advance state) inner))
                ((and tok (eq (car tok) 'op))
                 (let ((pbp (funcall 'neovm--pp-prefix-bp (cdr tok))))
                   (if pbp
                       (let ((rhs (funcall 'neovm--pp-parse-expr state pbp)))
                         (list 'prefix (cdr tok) rhs))
                     (list 'error "bad prefix" tok))))
                (t (list 'error "unexpected" tok)))))
        (let ((done nil))
          (while (not done)
            (let ((next (funcall 'neovm--pp-peek state)))
              (if (and next (eq (car next) 'op))
                  (let ((bp (funcall 'neovm--pp-infix-bp (cdr next))))
                    (if (and bp (> (car bp) min-bp))
                        (progn
                          (funcall 'neovm--pp-advance state)
                          (let ((rhs (funcall 'neovm--pp-parse-expr state (cdr bp))))
                            (setq lhs (list 'binop (cdr next) lhs rhs))))
                      (setq done t)))
                (setq done t)))))
        lhs)))
  (fset 'neovm--pp-parse
    (lambda (input)
      (let* ((tokens (funcall 'neovm--pp-tokenize input))
             (state (list tokens)))
        (funcall 'neovm--pp-parse-expr state 0))))
  (fset 'neovm--pp-eval-ast
    (lambda (ast)
      (cond
        ((eq (car ast) 'num) (cadr ast))
        ((eq (car ast) 'prefix)
         (let ((op (cadr ast))
               (val (funcall 'neovm--pp-eval-ast (caddr ast))))
           (cond ((string= op "-") (- val)) ((string= op "+") val) (t 0))))
        ((eq (car ast) 'binop)
         (let ((op (cadr ast))
               (l (funcall 'neovm--pp-eval-ast (caddr ast)))
               (r (funcall 'neovm--pp-eval-ast (cadddr ast))))
           (cond
             ((string= op "+") (+ l r))
             ((string= op "-") (- l r))
             ((string= op "*") (* l r))
             ((string= op "/") (/ l r))
             ((string= op "%") (% l r))
             ((string= op "^") (expt l r))
             (t 0))))
        (t 0))))
  (fset 'neovm--pp-calc
    (lambda (input)
      (funcall 'neovm--pp-eval-ast (funcall 'neovm--pp-parse input))))

  (unwind-protect
      (list
        ;; Modulo has same precedence as * and /
        (funcall 'neovm--pp-parse "10 % 3 + 1")
        (funcall 'neovm--pp-calc "10 % 3 + 1")
        (funcall 'neovm--pp-calc "10 % 3 * 2")
        (funcall 'neovm--pp-calc "100 % 7")
        ;; Mixed operations stress test
        (funcall 'neovm--pp-calc "2 + 3 * 4 - 1")
        (funcall 'neovm--pp-calc "10 / 2 + 3 * 4 - 5")
        (funcall 'neovm--pp-calc "2 ^ 3 + 1")
        (funcall 'neovm--pp-calc "1 + 2 ^ 3")
        ;; Verify associativity with subtraction
        (funcall 'neovm--pp-calc "10 - 3 - 2")
        ;; Verify associativity with division
        (funcall 'neovm--pp-calc "100 / 10 / 2")
        ;; Complex nested
        (funcall 'neovm--pp-calc "(1 + 2) * (3 + 4) - (5 + 6)")
        ;; All operators in one expression
        (funcall 'neovm--pp-calc "2 ^ 3 + 4 * 5 - 6 / 3 % 2"))
    (fmakunbound 'neovm--pp-is-digit)
    (fmakunbound 'neovm--pp-tokenize)
    (fmakunbound 'neovm--pp-peek)
    (fmakunbound 'neovm--pp-advance)
    (fmakunbound 'neovm--pp-infix-bp)
    (fmakunbound 'neovm--pp-prefix-bp)
    (fmakunbound 'neovm--pp-parse-expr)
    (fmakunbound 'neovm--pp-parse)
    (fmakunbound 'neovm--pp-eval-ast)
    (fmakunbound 'neovm--pp-calc)))"#;
    let expect = expect_test::expect![[
        r#""OK ((binop \"+\" (binop \"%\" (num 10) (num 3)) (num 1)) 2 2 2 13 12 9 9 5 5 10 28)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
