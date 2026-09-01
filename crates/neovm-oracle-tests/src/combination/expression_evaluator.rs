//! Oracle parity tests for a mini expression evaluator built in Elisp.
//!
//! Implements: infix arithmetic with proper precedence, unary operators,
//! variables, let bindings, comparison operators, boolean operations,
//! conditional expressions, function definitions and calls -- a mini
//! calculator language evaluated entirely in Elisp.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Tokenizer: string -> token list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_eval_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tokenize an expression string into a list of tokens.
    // Tokens are: (num . N), (id . "name"), (op . "+"), (lparen), (rparen), etc.
    let form = r#"(progn
  (fset 'neovm--ee-tokenize
    (lambda (input)
      (let ((pos 0) (len (length input)) (tokens nil))
        (while (< pos len)
          (let ((ch (aref input pos)))
            (cond
              ;; Whitespace: skip
              ((memq ch '(?\s ?\t))
               (setq pos (1+ pos)))
              ;; Digits: read number
              ((and (>= ch ?0) (<= ch ?9))
               (let ((start pos))
                 (while (and (< pos len)
                             (>= (aref input pos) ?0)
                             (<= (aref input pos) ?9))
                   (setq pos (1+ pos)))
                 (setq tokens
                       (cons (cons 'num (string-to-number
                                          (substring input start pos)))
                             tokens))))
              ;; Letters/underscore: read identifier
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
                 (setq tokens
                       (cons (cons 'id (substring input start pos))
                             tokens))))
              ;; Two-char operators: <=, >=, ==, !=, &&, ||
              ((and (< (1+ pos) len)
                    (let ((two (substring input pos (+ pos 2))))
                      (member two '("<=" ">=" "==" "!=" "&&" "||"))))
               (setq tokens
                     (cons (cons 'op (substring input pos (+ pos 2)))
                           tokens))
               (setq pos (+ pos 2)))
              ;; Single-char operators and punctuation
              ((memq ch '(?+ ?- ?* ?/ ?% ?< ?> ?! ?= ?, ?\( ?\)))
               (setq tokens
                     (cons (cons 'op (char-to-string ch)) tokens))
               (setq pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  (unwind-protect
      (list
        (funcall 'neovm--ee-tokenize "2 + 3 * 4")
        (funcall 'neovm--ee-tokenize "x = 10")
        (funcall 'neovm--ee-tokenize "foo(1, 2)")
        (funcall 'neovm--ee-tokenize "a <= b && c != 0")
        (funcall 'neovm--ee-tokenize "(x + y) * (x - y)")
        (funcall 'neovm--ee-tokenize "!true")
        (funcall 'neovm--ee-tokenize "let x = 5")
        (funcall 'neovm--ee-tokenize ""))
    (fmakunbound 'neovm--ee-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK (((num . 2) (op . \"+\") (num . 3) (op . \"*\") (num . 4)) ((id . \"x\") (op . \"=\") (num . 10)) ((id . \"foo\") (op . \"(\") (num . 1) (op . \",\") (num . 2) (op . \")\")) ((id . \"a\") (op . \"<=\") (id . \"b\") (op . \"&&\") (id . \"c\") (op . \"!=\") (num . 0)) ((op . \"(\") (id . \"x\") (op . \"+\") (id . \"y\") (op . \")\") (op . \"*\") (op . \"(\") (id . \"x\") (op . \"-\") (id . \"y\") (op . \")\")) ((op . \"!\") (id . \"true\")) ((id . \"let\") (id . \"x\") (op . \"=\") (num . 5)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Expression evaluator: AST-based with proper precedence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_eval_arithmetic_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Recursive descent parser + evaluator for arithmetic with proper
    // operator precedence: *, / bind tighter than +, -. Parentheses override.
    let form = r#"(progn
  ;; Tokenizer
  (fset 'neovm--ee2-tokenize
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
              ((= ch ?\() (setq tokens (cons '(lparen) tokens)
                                 pos (1+ pos)))
              ((= ch ?\)) (setq tokens (cons '(rparen) tokens)
                                 pos (1+ pos)))
              ((memq ch '(?+ ?- ?* ?/ ?%))
               (setq tokens (cons (cons 'op (char-to-string ch)) tokens)
                     pos (1+ pos)))
              (t (setq pos (1+ pos))))))
        (nreverse tokens))))

  ;; Parser state: mutable token list
  (defvar neovm--ee2-tokens nil)

  (fset 'neovm--ee2-peek
    (lambda () (car neovm--ee2-tokens)))

  (fset 'neovm--ee2-consume
    (lambda () (prog1 (car neovm--ee2-tokens)
                 (setq neovm--ee2-tokens (cdr neovm--ee2-tokens)))))

  ;; factor = number | '-' factor | '(' expr ')'
  (fset 'neovm--ee2-parse-factor
    (lambda ()
      (let ((tok (funcall 'neovm--ee2-peek)))
        (cond
          ((and (eq (car tok) 'op) (string= (cdr tok) "-"))
           (funcall 'neovm--ee2-consume)
           (- (funcall 'neovm--ee2-parse-factor)))
          ((eq (car tok) 'num)
           (funcall 'neovm--ee2-consume)
           (cdr tok))
          ((eq (car tok) 'lparen)
           (funcall 'neovm--ee2-consume)
           (let ((val (funcall 'neovm--ee2-parse-expr)))
             (funcall 'neovm--ee2-consume) ;; rparen
             val))
          (t 0)))))

  ;; term = factor (('*' | '/' | '%') factor)*
  (fset 'neovm--ee2-parse-term
    (lambda ()
      (let ((val (funcall 'neovm--ee2-parse-factor))
            (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--ee2-peek)))
            (if (and (eq (car tok) 'op)
                     (member (cdr tok) '("*" "/" "%")))
                (let ((op (cdr (funcall 'neovm--ee2-consume)))
                      (right (funcall 'neovm--ee2-parse-factor)))
                  (cond ((string= op "*") (setq val (* val right)))
                        ((string= op "/") (setq val (/ val right)))
                        ((string= op "%") (setq val (% val right)))))
              (setq done t))))
        val)))

  ;; expr = term (('+' | '-') term)*
  (fset 'neovm--ee2-parse-expr
    (lambda ()
      (let ((val (funcall 'neovm--ee2-parse-term))
            (done nil))
        (while (not done)
          (let ((tok (funcall 'neovm--ee2-peek)))
            (if (and (eq (car tok) 'op)
                     (member (cdr tok) '("+" "-")))
                (let ((op (cdr (funcall 'neovm--ee2-consume)))
                      (right (funcall 'neovm--ee2-parse-term)))
                  (if (string= op "+")
                      (setq val (+ val right))
                    (setq val (- val right))))
              (setq done t))))
        val)))

  (fset 'neovm--ee2-eval
    (lambda (input)
      (setq neovm--ee2-tokens (funcall 'neovm--ee2-tokenize input))
      (funcall 'neovm--ee2-parse-expr)))

  (unwind-protect
      (list
        (funcall 'neovm--ee2-eval "2 + 3")
        (funcall 'neovm--ee2-eval "2 + 3 * 4")
        (funcall 'neovm--ee2-eval "(2 + 3) * 4")
        (funcall 'neovm--ee2-eval "10 - 3 - 2")
        (funcall 'neovm--ee2-eval "100 / 10 / 2")
        (funcall 'neovm--ee2-eval "7 % 3")
        (funcall 'neovm--ee2-eval "2 * 3 + 4 * 5")
        (funcall 'neovm--ee2-eval "((2 + 3) * (4 - 1))")
        (funcall 'neovm--ee2-eval "-5 + 3")
        (funcall 'neovm--ee2-eval "-(3 + 4)")
        (funcall 'neovm--ee2-eval "42")
        (funcall 'neovm--ee2-eval "1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9 + 10"))
    (fmakunbound 'neovm--ee2-tokenize)
    (fmakunbound 'neovm--ee2-peek)
    (fmakunbound 'neovm--ee2-consume)
    (fmakunbound 'neovm--ee2-parse-factor)
    (fmakunbound 'neovm--ee2-parse-term)
    (fmakunbound 'neovm--ee2-parse-expr)
    (fmakunbound 'neovm--ee2-eval)
    (makunbound 'neovm--ee2-tokens)))"#;
    let expect = expect_test::expect![[r#""OK (5 14 20 5 5 1 26 15 -2 -7 42 55)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Variables and let bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_eval_variables_and_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Evaluate expressions with variable references and let bindings.
    // Uses an environment (alist) to store variable bindings.
    let form = r#"(progn
  ;; Simple evaluator that handles: numbers, +, -, *, /, variables, let
  ;; Expressions are S-expression based (already parsed).
  (fset 'neovm--ee3-eval
    (lambda (expr env)
      (cond
        ;; Number literal
        ((numberp expr) expr)
        ;; Variable reference
        ((symbolp expr)
         (let ((binding (assq expr env)))
           (if binding
               (cdr binding)
             (signal 'error (list "unbound variable" expr)))))
        ;; Compound expression
        ((consp expr)
         (let ((head (car expr)))
           (cond
             ;; (let ((var1 val1) (var2 val2) ...) body)
             ((eq head 'let)
              (let ((bindings (cadr expr))
                    (body (caddr expr))
                    (new-env env))
                (dolist (b bindings)
                  (let ((val (funcall 'neovm--ee3-eval (cadr b) new-env)))
                    (setq new-env (cons (cons (car b) val) new-env))))
                (funcall 'neovm--ee3-eval body new-env)))
             ;; (let* ((var1 val1) ...) body) - sequential binding
             ((eq head 'let*)
              (let ((bindings (cadr expr))
                    (body (caddr expr))
                    (new-env env))
                (dolist (b bindings)
                  (let ((val (funcall 'neovm--ee3-eval (cadr b) new-env)))
                    (setq new-env (cons (cons (car b) val) new-env))))
                (funcall 'neovm--ee3-eval body new-env)))
             ;; (begin expr1 expr2 ...) - evaluate in sequence, return last
             ((eq head 'begin)
              (let ((forms (cdr expr)) (result nil))
                (dolist (f forms)
                  (setq result (funcall 'neovm--ee3-eval f env)))
                result))
             ;; Arithmetic: (+ a b), (- a b), (* a b), (/ a b)
             ((memq head '(+ - * /))
              (let ((args (mapcar (lambda (a)
                                    (funcall 'neovm--ee3-eval a env))
                                  (cdr expr))))
                (cond
                  ((eq head '+) (apply #'+ args))
                  ((eq head '-) (apply #'- args))
                  ((eq head '*) (apply #'* args))
                  ((eq head '/) (apply #'/ args)))))
             (t (signal 'error (list "unknown form" head))))))
        (t (signal 'error (list "invalid expression" expr))))))

  (unwind-protect
      (list
        ;; Simple arithmetic in environment
        (funcall 'neovm--ee3-eval '(+ x y) '((x . 10) (y . 20)))
        ;; Let binding
        (funcall 'neovm--ee3-eval
                 '(let ((x 5) (y 3)) (+ x y)) nil)
        ;; Nested let
        (funcall 'neovm--ee3-eval
                 '(let ((x 10))
                    (let ((y (* x 2)))
                      (+ x y)))
                 nil)
        ;; Let* with sequential dependencies
        (funcall 'neovm--ee3-eval
                 '(let* ((a 3) (b (* a a)) (c (+ a b)))
                    c)
                 nil)
        ;; Shadowing
        (funcall 'neovm--ee3-eval
                 '(let ((x 1))
                    (+ x (let ((x 10)) x)))
                 nil)
        ;; Begin block
        (funcall 'neovm--ee3-eval
                 '(let ((x 5))
                    (begin
                      (+ x 1)
                      (+ x 2)
                      (* x 3)))
                 nil)
        ;; Complex nested expression
        (funcall 'neovm--ee3-eval
                 '(let ((a 2) (b 3))
                    (let* ((c (+ a b)) (d (* c c)))
                      (- d (+ a b))))
                 nil))
    (fmakunbound 'neovm--ee3-eval)))"#;
    let expect = expect_test::expect![[r#""OK (30 8 30 12 11 15 20)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Comparison operators and boolean logic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_eval_comparisons_and_booleans() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extend the evaluator with comparison (<, >, <=, >=, ==, !=),
    // boolean operations (and, or, not), and if-then-else.
    let form = r#"(progn
  (fset 'neovm--ee4-eval
    (lambda (expr env)
      (cond
        ((numberp expr) expr)
        ((eq expr 'true) t)
        ((eq expr 'false) nil)
        ((symbolp expr)
         (let ((b (assq expr env)))
           (if b (cdr b) (signal 'error (list "unbound" expr)))))
        ((consp expr)
         (let ((h (car expr)))
           (cond
             ;; Arithmetic
             ((memq h '(+ - * /))
              (let ((args (mapcar (lambda (a) (funcall 'neovm--ee4-eval a env))
                                  (cdr expr))))
                (cond ((eq h '+) (apply #'+ args))
                      ((eq h '-) (apply #'- args))
                      ((eq h '*) (apply #'* args))
                      ((eq h '/) (apply #'/ args)))))
             ;; Comparisons: (<  a b), (> a b), (<= a b), (>= a b), (== a b), (!= a b)
             ((eq h '<)
              (< (funcall 'neovm--ee4-eval (cadr expr) env)
                 (funcall 'neovm--ee4-eval (caddr expr) env)))
             ((eq h '>)
              (> (funcall 'neovm--ee4-eval (cadr expr) env)
                 (funcall 'neovm--ee4-eval (caddr expr) env)))
             ((eq h '<=)
              (<= (funcall 'neovm--ee4-eval (cadr expr) env)
                  (funcall 'neovm--ee4-eval (caddr expr) env)))
             ((eq h '>=)
              (>= (funcall 'neovm--ee4-eval (cadr expr) env)
                  (funcall 'neovm--ee4-eval (caddr expr) env)))
             ((eq h '==)
              (= (funcall 'neovm--ee4-eval (cadr expr) env)
                 (funcall 'neovm--ee4-eval (caddr expr) env)))
             ((eq h '!=)
              (not (= (funcall 'neovm--ee4-eval (cadr expr) env)
                      (funcall 'neovm--ee4-eval (caddr expr) env))))
             ;; Boolean operations
             ((eq h 'and)
              (and (funcall 'neovm--ee4-eval (cadr expr) env)
                   (funcall 'neovm--ee4-eval (caddr expr) env)))
             ((eq h 'or)
              (or (funcall 'neovm--ee4-eval (cadr expr) env)
                  (funcall 'neovm--ee4-eval (caddr expr) env)))
             ((eq h 'not)
              (not (funcall 'neovm--ee4-eval (cadr expr) env)))
             ;; Conditional: (if cond then else)
             ((eq h 'if)
              (if (funcall 'neovm--ee4-eval (cadr expr) env)
                  (funcall 'neovm--ee4-eval (caddr expr) env)
                (funcall 'neovm--ee4-eval (cadddr expr) env)))
             ;; Let
             ((eq h 'let)
              (let ((bindings (cadr expr))
                    (body (caddr expr))
                    (new-env env))
                (dolist (b bindings)
                  (setq new-env
                        (cons (cons (car b)
                                    (funcall 'neovm--ee4-eval (cadr b) new-env))
                              new-env)))
                (funcall 'neovm--ee4-eval body new-env)))
             (t (signal 'error (list "unknown" h))))))
        (t expr))))

  (unwind-protect
      (list
        ;; Comparisons
        (funcall 'neovm--ee4-eval '(< 3 5) nil)
        (funcall 'neovm--ee4-eval '(> 3 5) nil)
        (funcall 'neovm--ee4-eval '(<= 5 5) nil)
        (funcall 'neovm--ee4-eval '(!= 3 4) nil)
        (funcall 'neovm--ee4-eval '(== 7 7) nil)
        ;; Boolean logic
        (funcall 'neovm--ee4-eval '(and (< 1 2) (< 2 3)) nil)
        (funcall 'neovm--ee4-eval '(or (> 1 2) (< 2 3)) nil)
        (funcall 'neovm--ee4-eval '(not (< 5 3)) nil)
        ;; If-then-else
        (funcall 'neovm--ee4-eval '(if (> 10 5) 1 0) nil)
        (funcall 'neovm--ee4-eval '(if (< 10 5) 1 0) nil)
        ;; Complex: absolute value
        (funcall 'neovm--ee4-eval
                 '(let ((x -7))
                    (if (< x 0) (- 0 x) x))
                 nil)
        ;; Complex: max of three
        (funcall 'neovm--ee4-eval
                 '(let ((a 5) (b 12) (c 8))
                    (if (and (>= a b) (>= a c)) a
                      (if (>= b c) b c)))
                 nil)
        ;; Nested conditionals: classify a number
        (funcall 'neovm--ee4-eval
                 '(let ((n 42))
                    (if (< n 0) -1
                      (if (== n 0) 0 1)))
                 nil))
    (fmakunbound 'neovm--ee4-eval)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t t t t t t 1 0 7 12 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Function definitions and calls
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_eval_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Add user-defined functions to the evaluator:
    // (defun name (params...) body) stores in environment.
    // (call name args...) invokes the function.
    let form = r#"(progn
  (fset 'neovm--ee5-eval
    (lambda (expr env)
      (cond
        ((numberp expr) expr)
        ((eq expr 'true) t)
        ((eq expr 'false) nil)
        ((symbolp expr)
         (let ((b (assq expr env)))
           (if b (cdr b) (signal 'error (list "unbound" expr)))))
        ((consp expr)
         (let ((h (car expr)))
           (cond
             ((memq h '(+ - * / %))
              (let ((args (mapcar (lambda (a) (funcall 'neovm--ee5-eval a env))
                                  (cdr expr))))
                (cond ((eq h '+) (apply #'+ args))
                      ((eq h '-) (apply #'- args))
                      ((eq h '*) (apply #'* args))
                      ((eq h '/) (apply #'/ args))
                      ((eq h '%) (% (car args) (cadr args))))))
             ((memq h '(< > <= >= == !=))
              (let ((a (funcall 'neovm--ee5-eval (cadr expr) env))
                    (b (funcall 'neovm--ee5-eval (caddr expr) env)))
                (cond ((eq h '<) (< a b)) ((eq h '>) (> a b))
                      ((eq h '<=) (<= a b)) ((eq h '>=) (>= a b))
                      ((eq h '==) (= a b)) ((eq h '!=) (not (= a b))))))
             ((eq h 'if)
              (if (funcall 'neovm--ee5-eval (cadr expr) env)
                  (funcall 'neovm--ee5-eval (caddr expr) env)
                (if (cdddr expr)
                    (funcall 'neovm--ee5-eval (cadddr expr) env)
                  nil)))
             ((eq h 'let)
              (let ((bindings (cadr expr)) (body (caddr expr)) (e env))
                (dolist (b bindings) (setq e (cons (cons (car b)
                   (funcall 'neovm--ee5-eval (cadr b) e)) e)))
                (funcall 'neovm--ee5-eval body e)))
             ;; (defun name (params...) body) -> store closure in env
             ((eq h 'defun)
              (let ((name (cadr expr))
                    (params (caddr expr))
                    (body (cadddr expr)))
                (cons (cons name (list 'closure params body env)) env)))
             ;; (call name args...)
             ((eq h 'call)
              (let* ((name (cadr expr))
                     (arg-exprs (cddr expr))
                     (fn-entry (assq name env)))
                (if (and fn-entry (eq (car (cdr fn-entry)) 'closure))
                    (let* ((closure (cdr fn-entry))
                           (params (nth 1 closure))
                           (body (nth 2 closure))
                           (closure-env (nth 3 closure))
                           (args (mapcar (lambda (a)
                                           (funcall 'neovm--ee5-eval a env))
                                         arg-exprs))
                           (call-env closure-env))
                      ;; Bind params to args
                      (let ((ps params) (as args))
                        (while (and ps as)
                          (setq call-env (cons (cons (car ps) (car as)) call-env))
                          (setq ps (cdr ps) as (cdr as))))
                      ;; Also add the function itself for recursion
                      (setq call-env (cons (cons name (cdr fn-entry)) call-env))
                      (funcall 'neovm--ee5-eval body call-env))
                  (signal 'error (list "undefined function" name)))))
             ;; (progn expr1 expr2 ...) - evaluate in sequence
             ((eq h 'progn)
              (let ((forms (cdr expr)) (result nil) (e env))
                (dolist (f forms)
                  (if (and (consp f) (eq (car f) 'defun))
                      (setq e (funcall 'neovm--ee5-eval f e)
                            result e)
                    (setq result (funcall 'neovm--ee5-eval f e))))
                result))
             (t (signal 'error (list "unknown" h))))))
        (t expr))))

  (unwind-protect
      (list
        ;; Define and call a simple function
        (let ((env (funcall 'neovm--ee5-eval
                            '(defun square (x) (* x x)) nil)))
          (funcall 'neovm--ee5-eval '(call square 7) env))
        ;; Define and call with multiple args
        (let ((env (funcall 'neovm--ee5-eval
                            '(defun add3 (a b c) (+ a (+ b c))) nil)))
          (funcall 'neovm--ee5-eval '(call add3 10 20 30) env))
        ;; Function using if
        (let ((env (funcall 'neovm--ee5-eval
                            '(defun myabs (x) (if (< x 0) (- 0 x) x)) nil)))
          (list (funcall 'neovm--ee5-eval '(call myabs 5) env)
                (funcall 'neovm--ee5-eval '(call myabs -3) env)
                (funcall 'neovm--ee5-eval '(call myabs 0) env)))
        ;; Recursive function: factorial
        (let ((env (funcall 'neovm--ee5-eval
                            '(defun fact (n)
                               (if (<= n 1) 1
                                 (* n (call fact (- n 1)))))
                            nil)))
          (list (funcall 'neovm--ee5-eval '(call fact 1) env)
                (funcall 'neovm--ee5-eval '(call fact 5) env)
                (funcall 'neovm--ee5-eval '(call fact 10) env)))
        ;; Recursive: fibonacci
        (let ((env (funcall 'neovm--ee5-eval
                            '(defun fib (n)
                               (if (<= n 1) n
                                 (+ (call fib (- n 1))
                                    (call fib (- n 2)))))
                            nil)))
          (list (funcall 'neovm--ee5-eval '(call fib 0) env)
                (funcall 'neovm--ee5-eval '(call fib 1) env)
                (funcall 'neovm--ee5-eval '(call fib 7) env)
                (funcall 'neovm--ee5-eval '(call fib 10) env))))
    (fmakunbound 'neovm--ee5-eval)))"#;
    let expect = expect_test::expect![[r#""OK (49 60 (5 3 0) (1 120 3628800) (0 1 13 55))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: full calculator language with all features combined
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_eval_full_calculator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full test of the calculator language: define multiple functions,
    // use them together, test edge cases.
    let form = r#"(progn
  (fset 'neovm--ee6-eval
    (lambda (expr env)
      (cond
        ((numberp expr) expr)
        ((eq expr 'true) t)
        ((eq expr 'false) nil)
        ((symbolp expr)
         (let ((b (assq expr env)))
           (if b (cdr b) (signal 'error (list "unbound" expr)))))
        ((consp expr)
         (let ((h (car expr)))
           (cond
             ((memq h '(+ - * / %))
              (let ((args (mapcar (lambda (a) (funcall 'neovm--ee6-eval a env))
                                  (cdr expr))))
                (cond ((eq h '+) (apply #'+ args))
                      ((eq h '-) (apply #'- args))
                      ((eq h '*) (apply #'* args))
                      ((eq h '/) (apply #'/ args))
                      ((eq h '%) (% (car args) (cadr args))))))
             ((memq h '(< > <= >= == !=))
              (let ((a (funcall 'neovm--ee6-eval (cadr expr) env))
                    (b (funcall 'neovm--ee6-eval (caddr expr) env)))
                (cond ((eq h '<) (< a b)) ((eq h '>) (> a b))
                      ((eq h '<=) (<= a b)) ((eq h '>=) (>= a b))
                      ((eq h '==) (= a b)) ((eq h '!=) (not (= a b))))))
             ((eq h 'and)
              (and (funcall 'neovm--ee6-eval (cadr expr) env)
                   (funcall 'neovm--ee6-eval (caddr expr) env)))
             ((eq h 'or)
              (or (funcall 'neovm--ee6-eval (cadr expr) env)
                  (funcall 'neovm--ee6-eval (caddr expr) env)))
             ((eq h 'not) (not (funcall 'neovm--ee6-eval (cadr expr) env)))
             ((eq h 'if)
              (if (funcall 'neovm--ee6-eval (cadr expr) env)
                  (funcall 'neovm--ee6-eval (caddr expr) env)
                (if (cdddr expr)
                    (funcall 'neovm--ee6-eval (cadddr expr) env) nil)))
             ((eq h 'let)
              (let ((bindings (cadr expr)) (body (caddr expr)) (e env))
                (dolist (b bindings) (setq e (cons (cons (car b)
                   (funcall 'neovm--ee6-eval (cadr b) e)) e)))
                (funcall 'neovm--ee6-eval body e)))
             ((eq h 'defun)
              (let ((name (cadr expr)) (params (caddr expr))
                    (body (cadddr expr)))
                (cons (cons name (list 'closure params body env)) env)))
             ((eq h 'call)
              (let* ((name (cadr expr)) (arg-exprs (cddr expr))
                     (fn-entry (assq name env)))
                (if (and fn-entry (eq (cadr fn-entry) 'closure))
                    (let* ((closure (cdr fn-entry))
                           (params (nth 1 closure))
                           (body (nth 2 closure))
                           (closure-env (nth 3 closure))
                           (args (mapcar (lambda (a)
                                           (funcall 'neovm--ee6-eval a env))
                                         arg-exprs))
                           (call-env closure-env))
                      (let ((ps params) (as args))
                        (while (and ps as)
                          (setq call-env (cons (cons (car ps) (car as)) call-env))
                          (setq ps (cdr ps) as (cdr as))))
                      (setq call-env (cons (cons name (cdr fn-entry)) call-env))
                      (funcall 'neovm--ee6-eval body call-env))
                  (signal 'error (list "undefined function" name)))))
             ((eq h 'progn)
              (let ((forms (cdr expr)) (result nil) (e env))
                (dolist (f forms)
                  (if (and (consp f) (eq (car f) 'defun))
                      (setq e (funcall 'neovm--ee6-eval f e)
                            result nil)
                    (setq result (funcall 'neovm--ee6-eval f e))))
                result))
             (t (signal 'error (list "unknown" h))))))
        (t expr))))

  (unwind-protect
      ;; Define a mini standard library and compute with it
      (let* ((env nil)
             ;; Define: abs, max2, min2, clamp, sum-range, is-prime
             (env (funcall 'neovm--ee6-eval
                           '(defun abs (x) (if (< x 0) (- 0 x) x)) env))
             (env (funcall 'neovm--ee6-eval
                           '(defun max2 (a b) (if (>= a b) a b)) env))
             (env (funcall 'neovm--ee6-eval
                           '(defun min2 (a b) (if (<= a b) a b)) env))
             (env (funcall 'neovm--ee6-eval
                           '(defun clamp (x lo hi)
                              (call max2 lo (call min2 x hi))) env))
             ;; sum-range: sum from 1 to n
             (env (funcall 'neovm--ee6-eval
                           '(defun sum-range (n)
                              (if (<= n 0) 0
                                (+ n (call sum-range (- n 1))))) env))
             ;; power: x^n for non-negative n
             (env (funcall 'neovm--ee6-eval
                           '(defun power (x n)
                              (if (== n 0) 1
                                (* x (call power x (- n 1))))) env)))
        (list
          ;; abs
          (list (funcall 'neovm--ee6-eval '(call abs -5) env)
                (funcall 'neovm--ee6-eval '(call abs 5) env)
                (funcall 'neovm--ee6-eval '(call abs 0) env))
          ;; max2 / min2
          (list (funcall 'neovm--ee6-eval '(call max2 3 7) env)
                (funcall 'neovm--ee6-eval '(call min2 3 7) env))
          ;; clamp
          (list (funcall 'neovm--ee6-eval '(call clamp 5 0 10) env)
                (funcall 'neovm--ee6-eval '(call clamp -3 0 10) env)
                (funcall 'neovm--ee6-eval '(call clamp 15 0 10) env))
          ;; sum-range
          (list (funcall 'neovm--ee6-eval '(call sum-range 10) env)
                (funcall 'neovm--ee6-eval '(call sum-range 100) env))
          ;; power
          (list (funcall 'neovm--ee6-eval '(call power 2 10) env)
                (funcall 'neovm--ee6-eval '(call power 3 5) env)
                (funcall 'neovm--ee6-eval '(call power 5 0) env))
          ;; Composed: sum of squares from 1 to 5
          ;; Using let + manual loop via recursion
          (let ((env2 (funcall 'neovm--ee6-eval
                               '(defun sum-squares (n)
                                  (if (<= n 0) 0
                                    (+ (call power n 2)
                                       (call sum-squares (- n 1)))))
                               env)))
            (funcall 'neovm--ee6-eval '(call sum-squares 5) env2))
          ;; Complex expression with multiple features
          (funcall 'neovm--ee6-eval
                   '(let ((x 7) (y 3))
                      (if (and (> x 0) (> y 0))
                          (+ (call power x 2) (call power y 2))
                        0))
                   env)))
    (fmakunbound 'neovm--ee6-eval)))"#;
    let expect =
        expect_test::expect![[r#""ERR (error \"Lisp nesting exceeds ‘max-lisp-eval-depth’\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: evaluator with error handling and type checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_eval_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extend evaluator with runtime type checking and error results.
    // Operations that could fail return (ok . value) or (err . message).
    let form = r#"(progn
  (fset 'neovm--ee7-ok (lambda (v) (cons 'ok v)))
  (fset 'neovm--ee7-err (lambda (msg) (cons 'err msg)))
  (fset 'neovm--ee7-ok-p (lambda (r) (eq (car r) 'ok)))
  (fset 'neovm--ee7-val (lambda (r) (cdr r)))

  (fset 'neovm--ee7-eval
    (lambda (expr env)
      (cond
        ((numberp expr) (funcall 'neovm--ee7-ok expr))
        ((symbolp expr)
         (let ((b (assq expr env)))
           (if b (funcall 'neovm--ee7-ok (cdr b))
             (funcall 'neovm--ee7-err
                      (format "unbound: %s" (symbol-name expr))))))
        ((consp expr)
         (let ((h (car expr)))
           (cond
             ;; Safe division: checks for zero
             ((eq h '/)
              (let ((left (funcall 'neovm--ee7-eval (cadr expr) env)))
                (if (not (funcall 'neovm--ee7-ok-p left)) left
                  (let ((right (funcall 'neovm--ee7-eval (caddr expr) env)))
                    (if (not (funcall 'neovm--ee7-ok-p right)) right
                      (if (= (funcall 'neovm--ee7-val right) 0)
                          (funcall 'neovm--ee7-err "division by zero")
                        (funcall 'neovm--ee7-ok
                                 (/ (funcall 'neovm--ee7-val left)
                                    (funcall 'neovm--ee7-val right)))))))))
             ;; Other arithmetic: propagate errors
             ((memq h '(+ - *))
              (let ((left (funcall 'neovm--ee7-eval (cadr expr) env)))
                (if (not (funcall 'neovm--ee7-ok-p left)) left
                  (let ((right (funcall 'neovm--ee7-eval (caddr expr) env)))
                    (if (not (funcall 'neovm--ee7-ok-p right)) right
                      (funcall 'neovm--ee7-ok
                               (cond ((eq h '+) (+ (funcall 'neovm--ee7-val left)
                                                   (funcall 'neovm--ee7-val right)))
                                     ((eq h '-) (- (funcall 'neovm--ee7-val left)
                                                   (funcall 'neovm--ee7-val right)))
                                     ((eq h '*) (* (funcall 'neovm--ee7-val left)
                                                   (funcall 'neovm--ee7-val right))))))))))
             ;; Let with error propagation
             ((eq h 'let)
              (let ((bindings (cadr expr)) (body (caddr expr)) (e env)
                    (failed nil))
                (dolist (b bindings)
                  (unless failed
                    (let ((val (funcall 'neovm--ee7-eval (cadr b) e)))
                      (if (funcall 'neovm--ee7-ok-p val)
                          (setq e (cons (cons (car b)
                                              (funcall 'neovm--ee7-val val)) e))
                        (setq failed val)))))
                (if failed failed
                  (funcall 'neovm--ee7-eval body e))))
             (t (funcall 'neovm--ee7-err (format "unknown op: %s" h))))))
        (t (funcall 'neovm--ee7-err "invalid expression")))))

  (unwind-protect
      (list
        ;; Normal evaluation
        (funcall 'neovm--ee7-eval '(+ 10 20) nil)
        (funcall 'neovm--ee7-eval '(* 3 (+ 4 5)) nil)
        ;; Division by zero error
        (funcall 'neovm--ee7-eval '(/ 10 0) nil)
        ;; Error propagation through arithmetic
        (funcall 'neovm--ee7-eval '(+ 1 (/ 10 0)) nil)
        ;; Unbound variable error
        (funcall 'neovm--ee7-eval 'x nil)
        ;; Error in let binding propagates
        (funcall 'neovm--ee7-eval '(let ((x (/ 1 0))) (+ x 1)) nil)
        ;; Successful let
        (funcall 'neovm--ee7-eval '(let ((x 10) (y 3)) (/ x y)) nil)
        ;; Nested: error deep in expression
        (funcall 'neovm--ee7-eval
                 '(let ((a 5))
                    (* a (+ 1 (/ 10 (- a 5)))))
                 nil))
    (fmakunbound 'neovm--ee7-ok)
    (fmakunbound 'neovm--ee7-err)
    (fmakunbound 'neovm--ee7-ok-p)
    (fmakunbound 'neovm--ee7-val)
    (fmakunbound 'neovm--ee7-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok . 30) (ok . 27) (err . \"division by zero\") (err . \"division by zero\") (err . \"unbound: x\") (err . \"division by zero\") (ok . 3) (err . \"division by zero\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
