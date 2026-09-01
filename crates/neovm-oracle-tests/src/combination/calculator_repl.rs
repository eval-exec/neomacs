//! Oracle parity tests for a calculator REPL processor implemented in Elisp.
//!
//! Builds a calculator that tokenizes input, parses expressions, evaluates
//! with variables, handles assignment (x = expr), expression history,
//! last-result reference (ans), multiple statements per line separated by
//! semicolons, and error messages for bad input.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Tokenizer for calculator input
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_calc_repl_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tokenize calculator input into tokens: numbers, identifiers, operators,
    // parentheses, equals, semicolons.
    let form = r#"(progn
  (fset 'neovm--cr-tokenize
    (lambda (input)
      (let ((pos 0) (len (length input)) (tokens nil))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ;; Whitespace
              ((memq ch '(?\s ?\t))
               (setq pos (1+ pos)))
              ;; Digits (including decimals)
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start pos))
                 (while (and (< pos len)
                             (let ((c (aref input pos)))
                               (or (and (>= c ?0) (<= c ?9))
                                   (= c ?.))))
                   (setq pos (1+ pos)))
                 (let ((numstr (substring input start pos)))
                   (setq tokens (cons (cons 'num (string-to-number numstr))
                                      tokens)))))
              ;; Letters/underscore: identifier
              ((or (and (>= ch ?a) (<= ch ?z))
                   (and (>= ch ?A) (<= ch ?Z))
                   (= ch ?_))
               (let ((start pos))
                 (while (and (< pos len)
                             (let ((c (aref input pos)))
                               (or (and (>= c ?a) (<= c ?z))
                                   (and (>= c ?A) (<= c ?Z))
                                   (and (>= c ?0) (<= c ?9))
                                   (= c ?_))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'id (substring input start pos))
                                    tokens))))
              ;; Semicolons
              ((= ch ?\;)
               (setq tokens (cons '(semi) tokens))
               (setq pos (1+ pos)))
              ;; Equals
              ((= ch ?=)
               (setq tokens (cons '(eq) tokens))
               (setq pos (1+ pos)))
              ;; Operators and parens
              ((memq ch '(?+ ?- ?* ?/ ?% ?^ ?\( ?\)))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens))
               (setq pos (1+ pos)))
              ;; Unknown character
              (t (setq tokens (cons (cons 'unknown (char-to-string ch)) tokens))
                 (setq pos (1+ pos))))))
        (nreverse tokens))))

  (unwind-protect
      (list
        (funcall 'neovm--cr-tokenize "2 + 3 * 4")
        (funcall 'neovm--cr-tokenize "x = 10; y = 20; x + y")
        (funcall 'neovm--cr-tokenize "ans + 5")
        (funcall 'neovm--cr-tokenize "(a + b) * (c - d)")
        (funcall 'neovm--cr-tokenize "result = 2 ^ 10")
        (funcall 'neovm--cr-tokenize "")
        (funcall 'neovm--cr-tokenize "123.456 + 789")
        (funcall 'neovm--cr-tokenize "foo_bar = baz_1 + 42"))
    (fmakunbound 'neovm--cr-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK (((num . 2) (op . \"+\") (num . 3) (op . \"*\") (num . 4)) ((id . \"x\") (eq) (num . 10) (semi) (id . \"y\") (eq) (num . 20) (semi) (id . \"x\") (op . \"+\") (id . \"y\")) ((id . \"ans\") (op . \"+\") (num . 5)) ((op . \"(\") (id . \"a\") (op . \"+\") (id . \"b\") (op . \")\") (op . \"*\") (op . \"(\") (id . \"c\") (op . \"-\") (id . \"d\") (op . \")\")) ((id . \"result\") (eq) (num . 2) (op . \"^\") (num . 10)) nil ((num . 123.456) (op . \"+\") (num . 789)) ((id . \"foo_bar\") (eq) (id . \"baz_1\") (op . \"+\") (num . 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parser and evaluator with variables and assignment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_calc_repl_parser_evaluator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full recursive descent parser for: assignment, arithmetic with
    // precedence (^, then */, then +-), unary minus, parentheses, variables.
    let form = r#"(progn
  ;; Tokenizer (same as above, inlined)
  (fset 'neovm--cr2-tokenize
    (lambda (input)
      (let ((pos 0) (len (length input)) (tokens nil))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t)) (setq pos (1+ pos)))
              ((or (and (>= ch ?0) (<= ch ?9)) (= ch ?.))
               (let ((start pos))
                 (while (and (< pos len)
                             (let ((c (aref input pos)))
                               (or (and (>= c ?0) (<= c ?9)) (= c ?.))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ((or (and (>= ch ?a) (<= ch ?z)) (and (>= ch ?A) (<= ch ?Z))
                   (= ch ?_))
               (let ((start pos))
                 (while (and (< pos len)
                             (let ((c (aref input pos)))
                               (or (and (>= c ?a) (<= c ?z))
                                   (and (>= c ?A) (<= c ?Z))
                                   (and (>= c ?0) (<= c ?9))
                                   (= c ?_))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'id (substring input start pos))
                                    tokens))))
              ((= ch ?\;) (setq tokens (cons '(semi) tokens))
               (setq pos (1+ pos)))
              ((= ch ?=) (setq tokens (cons '(eq) tokens))
               (setq pos (1+ pos)))
              ((memq ch '(?+ ?- ?* ?/ ?% ?^ ?\( ?\)))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens))
               (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  ;; Parser state
  (defvar neovm--cr2-tokens nil)
  (defvar neovm--cr2-env nil)
  (defvar neovm--cr2-ans 0)
  (defvar neovm--cr2-history nil)

  (fset 'neovm--cr2-peek (lambda () (car neovm--cr2-tokens)))
  (fset 'neovm--cr2-consume
    (lambda () (prog1 (car neovm--cr2-tokens)
                 (setq neovm--cr2-tokens (cdr neovm--cr2-tokens)))))

  ;; factor = number | identifier | 'ans' | '-' factor | '(' expr ')'
  (fset 'neovm--cr2-parse-factor
    (lambda ()
      (let ((tok (funcall 'neovm--cr2-peek)))
        (cond
          ((and (eq (car tok) 'op) (string= (cdr tok) "-"))
           (funcall 'neovm--cr2-consume)
           (- (funcall 'neovm--cr2-parse-factor)))
          ((eq (car tok) 'num)
           (funcall 'neovm--cr2-consume)
           (cdr tok))
          ((eq (car tok) 'id)
           (funcall 'neovm--cr2-consume)
           (let ((name (cdr tok)))
             (if (string= name "ans")
                 neovm--cr2-ans
               (let ((binding (assoc name neovm--cr2-env)))
                 (if binding (cdr binding)
                   (signal 'error (list (format "undefined variable: %s" name))))))))
          ((and (eq (car tok) 'op) (string= (cdr tok) "("))
           (funcall 'neovm--cr2-consume)
           (let ((val (funcall 'neovm--cr2-parse-expr)))
             (funcall 'neovm--cr2-consume) ;; ')'
             val))
          (t 0)))))

  ;; power = factor ('^' factor)*  (right-associative)
  (fset 'neovm--cr2-parse-power
    (lambda ()
      (let ((base (funcall 'neovm--cr2-parse-factor)))
        (let ((tok (funcall 'neovm--cr2-peek)))
          (if (and (eq (car tok) 'op) (string= (cdr tok) "^"))
              (progn
                (funcall 'neovm--cr2-consume)
                (let ((exp (funcall 'neovm--cr2-parse-power)))
                  (truncate (expt base exp))))
            base)))))

  ;; term = power (('*' | '/' | '%') power)*
  (fset 'neovm--cr2-parse-term
    (lambda ()
      (let ((val (funcall 'neovm--cr2-parse-power)) (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--cr2-peek)))
            (if (and (eq (car tok) 'op)
                     (member (cdr tok) '("*" "/" "%")))
                (let ((op (cdr (funcall 'neovm--cr2-consume)))
                      (right (funcall 'neovm--cr2-parse-power)))
                  (cond ((string= op "*") (setq val (* val right)))
                        ((string= op "/")
                         (if (= right 0)
                             (signal 'error '("division by zero"))
                           (setq val (/ val right))))
                        ((string= op "%") (setq val (% val right)))))
              (setq done t))))
        val)))

  ;; expr = term (('+' | '-') term)*
  (fset 'neovm--cr2-parse-expr
    (lambda ()
      (let ((val (funcall 'neovm--cr2-parse-term)) (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--cr2-peek)))
            (if (and (eq (car tok) 'op)
                     (member (cdr tok) '("+" "-")))
                (let ((op (cdr (funcall 'neovm--cr2-consume)))
                      (right (funcall 'neovm--cr2-parse-term)))
                  (if (string= op "+")
                      (setq val (+ val right))
                    (setq val (- val right))))
              (setq done t))))
        val)))

  ;; statement = id '=' expr | expr
  (fset 'neovm--cr2-parse-statement
    (lambda ()
      (let ((tok (funcall 'neovm--cr2-peek)))
        ;; Check for assignment: id = expr
        (if (and (eq (car tok) 'id)
                 (let ((next (cadr neovm--cr2-tokens)))
                   (eq (car next) 'eq)))
            (let ((name (cdr (funcall 'neovm--cr2-consume))))
              (funcall 'neovm--cr2-consume) ;; eat '='
              (let ((val (funcall 'neovm--cr2-parse-expr)))
                (setq neovm--cr2-env
                      (cons (cons name val)
                            (assoc-delete-all name neovm--cr2-env)))
                val))
          ;; Otherwise, just parse an expression
          (funcall 'neovm--cr2-parse-expr)))))

  ;; Process a line: multiple statements separated by semicolons
  (fset 'neovm--cr2-process-line
    (lambda (input)
      (setq neovm--cr2-tokens (funcall 'neovm--cr2-tokenize input))
      (let ((results nil))
        (while neovm--cr2-tokens
          (let ((val (condition-case err
                         (funcall 'neovm--cr2-parse-statement)
                       (error (list 'error (cadr err))))))
            (unless (listp val)
              (setq neovm--cr2-ans val)
              (setq neovm--cr2-history
                    (cons val neovm--cr2-history)))
            (setq results (cons val results))
            ;; Consume semicolon if present
            (when (and neovm--cr2-tokens
                       (eq (car (funcall 'neovm--cr2-peek)) 'semi))
              (funcall 'neovm--cr2-consume))))
        (nreverse results))))

  (unwind-protect
      (progn
        ;; Reset state
        (setq neovm--cr2-env nil
              neovm--cr2-ans 0
              neovm--cr2-history nil)
        (list
          ;; Simple arithmetic
          (funcall 'neovm--cr2-process-line "2 + 3 * 4")
          ;; Assignment
          (funcall 'neovm--cr2-process-line "x = 10")
          ;; Use variable
          (funcall 'neovm--cr2-process-line "x * 5")
          ;; Multiple statements
          (funcall 'neovm--cr2-process-line "a = 3; b = 4; a * a + b * b")
          ;; ans reference
          (funcall 'neovm--cr2-process-line "ans + 1")
          ;; Power operator
          (funcall 'neovm--cr2-process-line "2 ^ 10")
          ;; Complex expression with all features
          (funcall 'neovm--cr2-process-line "c = (a + b) * 2; c + ans")
          ;; Current environment
          neovm--cr2-env
          ;; History
          neovm--cr2-history))
    (fmakunbound 'neovm--cr2-tokenize)
    (fmakunbound 'neovm--cr2-peek)
    (fmakunbound 'neovm--cr2-consume)
    (fmakunbound 'neovm--cr2-parse-factor)
    (fmakunbound 'neovm--cr2-parse-power)
    (fmakunbound 'neovm--cr2-parse-term)
    (fmakunbound 'neovm--cr2-parse-expr)
    (fmakunbound 'neovm--cr2-parse-statement)
    (fmakunbound 'neovm--cr2-process-line)
    (makunbound 'neovm--cr2-tokens)
    (makunbound 'neovm--cr2-env)
    (makunbound 'neovm--cr2-ans)
    (makunbound 'neovm--cr2-history)))"#;
    let expect = expect_test::expect![[
        r#""OK ((14) (10) (50) (3 4 25) (26) (1024) (14 28) ((\"c\" . 14) (\"b\" . 4) (\"a\" . 3) (\"x\" . 10)) (28 14 1024 26 25 4 3 50 10 14))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error handling: undefined variables, division by zero, bad syntax
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_calc_repl_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that the calculator properly reports errors for various bad inputs.
    let form = r#"(progn
  ;; Minimal inlined tokenizer + evaluator for error testing
  (fset 'neovm--cr3-tokenize
    (lambda (input)
      (let ((pos 0) (len (length input)) (tokens nil))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t)) (setq pos (1+ pos)))
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start pos))
                 (while (and (< pos len) (>= (aref input pos) ?0)
                             (<= (aref input pos) ?9))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ((or (and (>= ch ?a) (<= ch ?z)) (= ch ?_))
               (let ((start pos))
                 (while (and (< pos len)
                             (let ((c (aref input pos)))
                               (or (and (>= c ?a) (<= c ?z))
                                   (and (>= c ?0) (<= c ?9))
                                   (= c ?_))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'id (substring input start pos))
                                    tokens))))
              ((= ch ?=) (setq tokens (cons '(eq) tokens)
                                pos (1+ pos)))
              ((= ch ?\;) (setq tokens (cons '(semi) tokens)
                                 pos (1+ pos)))
              ((memq ch '(?+ ?- ?* ?/ ?\( ?\)))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens)
                     pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (defvar neovm--cr3-toks nil)
  (defvar neovm--cr3-env nil)

  (fset 'neovm--cr3-peek (lambda () (car neovm--cr3-toks)))
  (fset 'neovm--cr3-eat
    (lambda () (prog1 (car neovm--cr3-toks)
                 (setq neovm--cr3-toks (cdr neovm--cr3-toks)))))

  (fset 'neovm--cr3-factor
    (lambda ()
      (let ((t1 (funcall 'neovm--cr3-peek)))
        (cond
          ((and (eq (car t1) 'op) (string= (cdr t1) "-"))
           (funcall 'neovm--cr3-eat)
           (- (funcall 'neovm--cr3-factor)))
          ((eq (car t1) 'num)
           (cdr (funcall 'neovm--cr3-eat)))
          ((eq (car t1) 'id)
           (let ((name (cdr (funcall 'neovm--cr3-eat))))
             (let ((b (assoc name neovm--cr3-env)))
               (if b (cdr b)
                 (signal 'error (list (format "undefined: %s" name)))))))
          ((and (eq (car t1) 'op) (string= (cdr t1) "("))
           (funcall 'neovm--cr3-eat)
           (let ((v (funcall 'neovm--cr3-expr)))
             (funcall 'neovm--cr3-eat) v))
          (t (signal 'error '("unexpected token")))))))

  (fset 'neovm--cr3-term
    (lambda ()
      (let ((v (funcall 'neovm--cr3-factor)) (d nil))
        (while (not d)
          (let ((t1 (funcall 'neovm--cr3-peek)))
            (if (and (eq (car t1) 'op) (member (cdr t1) '("*" "/")))
                (let ((op (cdr (funcall 'neovm--cr3-eat)))
                      (r (funcall 'neovm--cr3-factor)))
                  (if (and (string= op "/") (= r 0))
                      (signal 'error '("division by zero"))
                    (setq v (if (string= op "*") (* v r) (/ v r)))))
              (setq d t))))
        v)))

  (fset 'neovm--cr3-expr
    (lambda ()
      (let ((v (funcall 'neovm--cr3-term)) (d nil))
        (while (not d)
          (let ((t1 (funcall 'neovm--cr3-peek)))
            (if (and (eq (car t1) 'op) (member (cdr t1) '("+" "-")))
                (let ((op (cdr (funcall 'neovm--cr3-eat)))
                      (r (funcall 'neovm--cr3-term)))
                  (setq v (if (string= op "+") (+ v r) (- v r))))
              (setq d t))))
        v)))

  (fset 'neovm--cr3-eval-line
    (lambda (input)
      (setq neovm--cr3-toks (funcall 'neovm--cr3-tokenize input))
      (condition-case err
          (cond
            ;; Empty input
            ((null neovm--cr3-toks) (list 'empty))
            ;; Assignment: id = expr
            ((and (eq (car (car neovm--cr3-toks)) 'id)
                  (eq (car (cadr neovm--cr3-toks)) 'eq))
             (let ((name (cdr (funcall 'neovm--cr3-eat))))
               (funcall 'neovm--cr3-eat)
               (let ((val (funcall 'neovm--cr3-expr)))
                 (setq neovm--cr3-env
                       (cons (cons name val) neovm--cr3-env))
                 (list 'assigned name val))))
            ;; Expression
            (t (list 'result (funcall 'neovm--cr3-expr))))
        (error (list 'error (cadr err))))))

  (unwind-protect
      (progn
        (setq neovm--cr3-env nil)
        (list
          ;; Normal evaluation
          (funcall 'neovm--cr3-eval-line "10 + 20")
          ;; Assignment then use
          (funcall 'neovm--cr3-eval-line "x = 42")
          (funcall 'neovm--cr3-eval-line "x + 8")
          ;; Undefined variable
          (funcall 'neovm--cr3-eval-line "y + 1")
          ;; Division by zero
          (funcall 'neovm--cr3-eval-line "10 / 0")
          ;; Division by zero in sub-expression
          (funcall 'neovm--cr3-eval-line "5 + 3 / (2 - 2)")
          ;; Empty input
          (funcall 'neovm--cr3-eval-line "")
          ;; Nested parens
          (funcall 'neovm--cr3-eval-line "((10 + 5) * 2)")
          ;; Complex
          (funcall 'neovm--cr3-eval-line "a = 7")
          (funcall 'neovm--cr3-eval-line "b = a * 3")
          (funcall 'neovm--cr3-eval-line "a + b")))
    (fmakunbound 'neovm--cr3-tokenize)
    (fmakunbound 'neovm--cr3-peek)
    (fmakunbound 'neovm--cr3-eat)
    (fmakunbound 'neovm--cr3-factor)
    (fmakunbound 'neovm--cr3-term)
    (fmakunbound 'neovm--cr3-expr)
    (fmakunbound 'neovm--cr3-eval-line)
    (makunbound 'neovm--cr3-toks)
    (makunbound 'neovm--cr3-env)))"#;
    let expect = expect_test::expect![[
        r#""OK ((result 30) (assigned \"x\" 42) (result 50) (error \"undefined: y\") (error \"division by zero\") (error \"division by zero\") (empty) (result 30) (assigned \"a\" 7) (assigned \"b\" 21) (result 28))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Expression history and ans reference across multiple lines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_calc_repl_history_and_ans() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track expression history and support 'ans' to reference the last result.
    let form = r#"(progn
  (defvar neovm--cr4-toks nil)
  (defvar neovm--cr4-env nil)
  (defvar neovm--cr4-ans 0)
  (defvar neovm--cr4-hist nil)

  (fset 'neovm--cr4-tokenize
    (lambda (input)
      (let ((pos 0) (len (length input)) (tokens nil))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t)) (setq pos (1+ pos)))
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start pos))
                 (while (and (< pos len) (>= (aref input pos) ?0)
                             (<= (aref input pos) ?9))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ((or (and (>= ch ?a) (<= ch ?z)) (= ch ?_))
               (let ((start pos))
                 (while (and (< pos len)
                             (let ((c (aref input pos)))
                               (or (and (>= c ?a) (<= c ?z))
                                   (and (>= c ?0) (<= c ?9))
                                   (= c ?_))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'id (substring input start pos))
                                    tokens))))
              ((= ch ?=) (setq tokens (cons '(eq) tokens)
                                pos (1+ pos)))
              ((memq ch '(?+ ?- ?* ?/ ?\( ?\)))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens)
                     pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (fset 'neovm--cr4-peek (lambda () (car neovm--cr4-toks)))
  (fset 'neovm--cr4-eat
    (lambda () (prog1 (car neovm--cr4-toks)
                 (setq neovm--cr4-toks (cdr neovm--cr4-toks)))))

  (fset 'neovm--cr4-factor
    (lambda ()
      (let ((t1 (funcall 'neovm--cr4-peek)))
        (cond
          ((and (eq (car t1) 'op) (string= (cdr t1) "-"))
           (funcall 'neovm--cr4-eat) (- (funcall 'neovm--cr4-factor)))
          ((eq (car t1) 'num)
           (cdr (funcall 'neovm--cr4-eat)))
          ((eq (car t1) 'id)
           (let ((name (cdr (funcall 'neovm--cr4-eat))))
             (cond
               ((string= name "ans") neovm--cr4-ans)
               (t (let ((b (assoc name neovm--cr4-env)))
                    (if b (cdr b)
                      (signal 'error (list (format "undefined: %s" name)))))))))
          ((and (eq (car t1) 'op) (string= (cdr t1) "("))
           (funcall 'neovm--cr4-eat)
           (let ((v (funcall 'neovm--cr4-expr)))
             (funcall 'neovm--cr4-eat) v))
          (t 0)))))

  (fset 'neovm--cr4-term
    (lambda ()
      (let ((v (funcall 'neovm--cr4-factor)) (d nil))
        (while (not d)
          (let ((t1 (funcall 'neovm--cr4-peek)))
            (if (and (eq (car t1) 'op) (member (cdr t1) '("*" "/")))
                (let ((op (cdr (funcall 'neovm--cr4-eat)))
                      (r (funcall 'neovm--cr4-factor)))
                  (setq v (if (string= op "*") (* v r) (/ v r))))
              (setq d t))))
        v)))

  (fset 'neovm--cr4-expr
    (lambda ()
      (let ((v (funcall 'neovm--cr4-term)) (d nil))
        (while (not d)
          (let ((t1 (funcall 'neovm--cr4-peek)))
            (if (and (eq (car t1) 'op) (member (cdr t1) '("+" "-")))
                (let ((op (cdr (funcall 'neovm--cr4-eat)))
                      (r (funcall 'neovm--cr4-term)))
                  (setq v (if (string= op "+") (+ v r) (- v r))))
              (setq d t))))
        v)))

  (fset 'neovm--cr4-run
    (lambda (input)
      (setq neovm--cr4-toks (funcall 'neovm--cr4-tokenize input))
      (condition-case err
          (let ((result
                 (if (and (eq (car (car neovm--cr4-toks)) 'id)
                          (eq (car (cadr neovm--cr4-toks)) 'eq))
                     (let ((name (cdr (funcall 'neovm--cr4-eat))))
                       (funcall 'neovm--cr4-eat)
                       (let ((val (funcall 'neovm--cr4-expr)))
                         (setq neovm--cr4-env
                               (cons (cons name val) neovm--cr4-env))
                         val))
                   (funcall 'neovm--cr4-expr))))
            (setq neovm--cr4-ans result)
            (setq neovm--cr4-hist (cons result neovm--cr4-hist))
            result)
        (error (list 'error (cadr err))))))

  (unwind-protect
      (progn
        (setq neovm--cr4-env nil neovm--cr4-ans 0 neovm--cr4-hist nil)
        ;; Session: series of calculator inputs
        (let ((r1 (funcall 'neovm--cr4-run "10 + 20"))       ;; 30
              (r2 (funcall 'neovm--cr4-run "ans * 2"))        ;; 60
              (r3 (funcall 'neovm--cr4-run "x = ans + 5"))    ;; 65
              (r4 (funcall 'neovm--cr4-run "x * 2"))          ;; 130
              (r5 (funcall 'neovm--cr4-run "ans - x"))        ;; 65
              (r6 (funcall 'neovm--cr4-run "y = 100"))        ;; 100
              (r7 (funcall 'neovm--cr4-run "x + y + ans"))    ;; 265
              (r8 (funcall 'neovm--cr4-run "ans"))            ;; 265
              )
          (list
            r1 r2 r3 r4 r5 r6 r7 r8
            ;; Final ans
            neovm--cr4-ans
            ;; Full history (most recent first)
            neovm--cr4-hist
            ;; Environment
            (length neovm--cr4-env))))
    (fmakunbound 'neovm--cr4-tokenize)
    (fmakunbound 'neovm--cr4-peek)
    (fmakunbound 'neovm--cr4-eat)
    (fmakunbound 'neovm--cr4-factor)
    (fmakunbound 'neovm--cr4-term)
    (fmakunbound 'neovm--cr4-expr)
    (fmakunbound 'neovm--cr4-run)
    (makunbound 'neovm--cr4-toks)
    (makunbound 'neovm--cr4-env)
    (makunbound 'neovm--cr4-ans)
    (makunbound 'neovm--cr4-hist)))"#;
    let expect = expect_test::expect![[
        r#""OK (30 60 65 130 65 100 265 265 265 (265 265 100 65 130 65 60 30) 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-statement lines with semicolons and cumulative state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_calc_repl_multi_statement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Process lines with multiple semicolon-separated statements,
    // where each statement can use variables defined earlier in the same line.
    let form = r#"(progn
  (defvar neovm--cr5-toks nil)
  (defvar neovm--cr5-env nil)

  (fset 'neovm--cr5-tokenize
    (lambda (input)
      (let ((pos 0) (len (length input)) (tokens nil))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t)) (setq pos (1+ pos)))
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start pos))
                 (while (and (< pos len) (>= (aref input pos) ?0)
                             (<= (aref input pos) ?9))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ((or (and (>= ch ?a) (<= ch ?z)) (= ch ?_))
               (let ((start pos))
                 (while (and (< pos len)
                             (let ((c (aref input pos)))
                               (or (and (>= c ?a) (<= c ?z))
                                   (and (>= c ?0) (<= c ?9)) (= c ?_))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'id (substring input start pos))
                                    tokens))))
              ((= ch ?=) (setq tokens (cons '(eq) tokens)
                                pos (1+ pos)))
              ((= ch ?\;) (setq tokens (cons '(semi) tokens)
                                 pos (1+ pos)))
              ((memq ch '(?+ ?- ?* ?/ ?\( ?\)))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens)
                     pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (fset 'neovm--cr5-peek (lambda () (car neovm--cr5-toks)))
  (fset 'neovm--cr5-eat
    (lambda () (prog1 (car neovm--cr5-toks)
                 (setq neovm--cr5-toks (cdr neovm--cr5-toks)))))

  (fset 'neovm--cr5-factor
    (lambda ()
      (let ((t1 (funcall 'neovm--cr5-peek)))
        (cond
          ((and (eq (car t1) 'op) (string= (cdr t1) "-"))
           (funcall 'neovm--cr5-eat) (- (funcall 'neovm--cr5-factor)))
          ((eq (car t1) 'num) (cdr (funcall 'neovm--cr5-eat)))
          ((eq (car t1) 'id)
           (let ((name (cdr (funcall 'neovm--cr5-eat))))
             (let ((b (assoc name neovm--cr5-env)))
               (if b (cdr b) (signal 'error (list (format "undef: %s" name)))))))
          ((and (eq (car t1) 'op) (string= (cdr t1) "("))
           (funcall 'neovm--cr5-eat)
           (let ((v (funcall 'neovm--cr5-expr)))
             (funcall 'neovm--cr5-eat) v))
          (t 0)))))

  (fset 'neovm--cr5-term
    (lambda ()
      (let ((v (funcall 'neovm--cr5-factor)) (d nil))
        (while (not d)
          (let ((t1 (funcall 'neovm--cr5-peek)))
            (if (and (eq (car t1) 'op) (member (cdr t1) '("*" "/")))
                (let ((op (cdr (funcall 'neovm--cr5-eat)))
                      (r (funcall 'neovm--cr5-factor)))
                  (setq v (if (string= op "*") (* v r) (/ v r))))
              (setq d t))))
        v)))

  (fset 'neovm--cr5-expr
    (lambda ()
      (let ((v (funcall 'neovm--cr5-term)) (d nil))
        (while (not d)
          (let ((t1 (funcall 'neovm--cr5-peek)))
            (if (and (eq (car t1) 'op) (member (cdr t1) '("+" "-")))
                (let ((op (cdr (funcall 'neovm--cr5-eat)))
                      (r (funcall 'neovm--cr5-term)))
                  (setq v (if (string= op "+") (+ v r) (- v r))))
              (setq d t))))
        v)))

  (fset 'neovm--cr5-stmt
    (lambda ()
      (if (and (eq (car (car neovm--cr5-toks)) 'id)
               (eq (car (cadr neovm--cr5-toks)) 'eq))
          (let ((name (cdr (funcall 'neovm--cr5-eat))))
            (funcall 'neovm--cr5-eat)
            (let ((val (funcall 'neovm--cr5-expr)))
              (setq neovm--cr5-env
                    (cons (cons name val) neovm--cr5-env))
              (cons 'assign (cons name val))))
        (cons 'result (funcall 'neovm--cr5-expr)))))

  (fset 'neovm--cr5-process
    (lambda (input)
      (setq neovm--cr5-toks (funcall 'neovm--cr5-tokenize input))
      (let ((results nil))
        (while neovm--cr5-toks
          (condition-case err
              (progn
                (setq results (cons (funcall 'neovm--cr5-stmt) results))
                ;; Eat semicolons
                (while (and neovm--cr5-toks
                            (eq (car (funcall 'neovm--cr5-peek)) 'semi))
                  (funcall 'neovm--cr5-eat)))
            (error
             (setq results (cons (list 'error (cadr err)) results))
             (setq neovm--cr5-toks nil))))
        (nreverse results))))

  (unwind-protect
      (progn
        (setq neovm--cr5-env nil)
        (list
          ;; Single statement
          (funcall 'neovm--cr5-process "42")
          ;; Multi-statement: assignments then expression using them
          (funcall 'neovm--cr5-process "a = 10; b = 20; a + b")
          ;; Chain assignments
          (funcall 'neovm--cr5-process "x = 5; y = x * 2; z = x + y; z")
          ;; Error in middle of multi-statement
          (funcall 'neovm--cr5-process "p = 3; q = unknown; p + q")
          ;; All expressions (no assignments)
          (funcall 'neovm--cr5-process "1 + 1; 2 * 3; 10 - 4")
          ;; Final environment state
          neovm--cr5-env))
    (fmakunbound 'neovm--cr5-tokenize)
    (fmakunbound 'neovm--cr5-peek)
    (fmakunbound 'neovm--cr5-eat)
    (fmakunbound 'neovm--cr5-factor)
    (fmakunbound 'neovm--cr5-term)
    (fmakunbound 'neovm--cr5-expr)
    (fmakunbound 'neovm--cr5-stmt)
    (fmakunbound 'neovm--cr5-process)
    (makunbound 'neovm--cr5-toks)
    (makunbound 'neovm--cr5-env)))"#;
    let expect = expect_test::expect![[
        r#""OK (((result . 42)) ((assign \"a\" . 10) (assign \"b\" . 20) (result . 30)) ((assign \"x\" . 5) (assign \"y\" . 10) (assign \"z\" . 15) (result . 15)) ((assign \"p\" . 3) (error \"undef: unknown\")) ((result . 2) (result . 6) (result . 6)) ((\"p\" . 3) (\"z\" . 15) (\"y\" . 10) (\"x\" . 5) (\"b\" . 20) (\"a\" . 10)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Built-in functions in the calculator: abs, max, min, sqrt
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_calc_repl_builtin_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extend the calculator with built-in function calls: abs(x), max(a,b),
    // min(a,b). Function calls are id followed by '(' args ')'.
    let form = r#"(progn
  (defvar neovm--cr6-toks nil)
  (defvar neovm--cr6-env nil)

  (fset 'neovm--cr6-tokenize
    (lambda (input)
      (let ((pos 0) (len (length input)) (tokens nil))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ((memq ch '(?\s ?\t)) (setq pos (1+ pos)))
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start pos))
                 (while (and (< pos len) (>= (aref input pos) ?0)
                             (<= (aref input pos) ?9))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'num (string-to-number
                                                 (substring input start pos)))
                                    tokens))))
              ((or (and (>= ch ?a) (<= ch ?z)) (= ch ?_))
               (let ((start pos))
                 (while (and (< pos len)
                             (let ((c (aref input pos)))
                               (or (and (>= c ?a) (<= c ?z))
                                   (and (>= c ?0) (<= c ?9)) (= c ?_))))
                   (setq pos (1+ pos)))
                 (setq tokens (cons (cons 'id (substring input start pos))
                                    tokens))))
              ((= ch ?=) (setq tokens (cons '(eq) tokens) pos (1+ pos)))
              ((= ch ?,) (setq tokens (cons '(comma) tokens) pos (1+ pos)))
              ((memq ch '(?+ ?- ?* ?/ ?\( ?\)))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens)
                     pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (fset 'neovm--cr6-peek (lambda () (car neovm--cr6-toks)))
  (fset 'neovm--cr6-eat
    (lambda () (prog1 (car neovm--cr6-toks)
                 (setq neovm--cr6-toks (cdr neovm--cr6-toks)))))

  (fset 'neovm--cr6-factor
    (lambda ()
      (let ((t1 (funcall 'neovm--cr6-peek)))
        (cond
          ((and (eq (car t1) 'op) (string= (cdr t1) "-"))
           (funcall 'neovm--cr6-eat) (- (funcall 'neovm--cr6-factor)))
          ((eq (car t1) 'num) (cdr (funcall 'neovm--cr6-eat)))
          ((eq (car t1) 'id)
           (let ((name (cdr (funcall 'neovm--cr6-eat))))
             ;; Check if function call: next token is '('
             (if (and (funcall 'neovm--cr6-peek)
                      (eq (car (funcall 'neovm--cr6-peek)) 'op)
                      (string= (cdr (funcall 'neovm--cr6-peek)) "("))
                 ;; Function call
                 (progn
                   (funcall 'neovm--cr6-eat) ;; eat '('
                   (let ((args nil))
                     ;; Parse arguments
                     (unless (and (funcall 'neovm--cr6-peek)
                                  (eq (car (funcall 'neovm--cr6-peek)) 'op)
                                  (string= (cdr (funcall 'neovm--cr6-peek)) ")"))
                       (setq args (list (funcall 'neovm--cr6-expr)))
                       (while (and (funcall 'neovm--cr6-peek)
                                   (eq (car (funcall 'neovm--cr6-peek)) 'comma))
                         (funcall 'neovm--cr6-eat) ;; eat ','
                         (setq args (append args
                                            (list (funcall 'neovm--cr6-expr))))))
                     (funcall 'neovm--cr6-eat) ;; eat ')'
                     ;; Dispatch builtin
                     (cond
                       ((string= name "abs")
                        (abs (car args)))
                       ((string= name "max")
                        (max (car args) (cadr args)))
                       ((string= name "min")
                        (min (car args) (cadr args)))
                       (t (signal 'error
                                  (list (format "unknown function: %s" name)))))))
               ;; Variable reference
               (let ((b (assoc name neovm--cr6-env)))
                 (if b (cdr b)
                   (signal 'error (list (format "undefined: %s" name))))))))
          ((and (eq (car t1) 'op) (string= (cdr t1) "("))
           (funcall 'neovm--cr6-eat)
           (let ((v (funcall 'neovm--cr6-expr)))
             (funcall 'neovm--cr6-eat) v))
          (t 0)))))

  (fset 'neovm--cr6-term
    (lambda ()
      (let ((v (funcall 'neovm--cr6-factor)) (d nil))
        (while (not d)
          (let ((t1 (funcall 'neovm--cr6-peek)))
            (if (and (eq (car t1) 'op) (member (cdr t1) '("*" "/")))
                (let ((op (cdr (funcall 'neovm--cr6-eat)))
                      (r (funcall 'neovm--cr6-factor)))
                  (setq v (if (string= op "*") (* v r) (/ v r))))
              (setq d t))))
        v)))

  (fset 'neovm--cr6-expr
    (lambda ()
      (let ((v (funcall 'neovm--cr6-term)) (d nil))
        (while (not d)
          (let ((t1 (funcall 'neovm--cr6-peek)))
            (if (and (eq (car t1) 'op) (member (cdr t1) '("+" "-")))
                (let ((op (cdr (funcall 'neovm--cr6-eat)))
                      (r (funcall 'neovm--cr6-term)))
                  (setq v (if (string= op "+") (+ v r) (- v r))))
              (setq d t))))
        v)))

  (fset 'neovm--cr6-eval
    (lambda (input)
      (setq neovm--cr6-toks (funcall 'neovm--cr6-tokenize input))
      (condition-case err
          (if (and (eq (car (car neovm--cr6-toks)) 'id)
                   (cadr neovm--cr6-toks)
                   (eq (car (cadr neovm--cr6-toks)) 'eq))
              (let ((name (cdr (funcall 'neovm--cr6-eat))))
                (funcall 'neovm--cr6-eat)
                (let ((val (funcall 'neovm--cr6-expr)))
                  (setq neovm--cr6-env
                        (cons (cons name val) neovm--cr6-env))
                  val))
            (funcall 'neovm--cr6-expr))
        (error (list 'error (cadr err))))))

  (unwind-protect
      (progn
        (setq neovm--cr6-env nil)
        (list
          ;; abs
          (funcall 'neovm--cr6-eval "abs(-7)")
          (funcall 'neovm--cr6-eval "abs(42)")
          ;; max / min
          (funcall 'neovm--cr6-eval "max(10, 20)")
          (funcall 'neovm--cr6-eval "min(10, 20)")
          ;; Nested function calls in expressions
          (funcall 'neovm--cr6-eval "max(abs(-5), abs(3)) + 10")
          ;; Assign with function
          (funcall 'neovm--cr6-eval "biggest = max(100, 200)")
          (funcall 'neovm--cr6-eval "biggest + 1")
          ;; min of expressions
          (funcall 'neovm--cr6-eval "min(3 + 4, 2 * 5)")
          ;; Unknown function
          (funcall 'neovm--cr6-eval "sqrt(4)")
          ;; Complex expression
          (funcall 'neovm--cr6-eval "a = 15")
          (funcall 'neovm--cr6-eval "b = -8")
          (funcall 'neovm--cr6-eval "max(a, b) - min(a, b) + abs(b)")))
    (fmakunbound 'neovm--cr6-tokenize)
    (fmakunbound 'neovm--cr6-peek)
    (fmakunbound 'neovm--cr6-eat)
    (fmakunbound 'neovm--cr6-factor)
    (fmakunbound 'neovm--cr6-term)
    (fmakunbound 'neovm--cr6-expr)
    (fmakunbound 'neovm--cr6-eval)
    (makunbound 'neovm--cr6-toks)
    (makunbound 'neovm--cr6-env)))"#;
    let expect = expect_test::expect![[
        r#""OK (7 42 20 10 15 200 201 7 (error \"unknown function: sqrt\") 15 -8 31)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
