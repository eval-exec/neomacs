//! Oracle parity tests for expression tree operations implemented in Elisp:
//! building expression trees from infix token lists, evaluating trees,
//! converting to prefix and postfix notation, symbolic differentiation
//! of expression trees, and expression simplification.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Build expression tree from infix token list and evaluate
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_tree_build_and_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse infix token lists into expression trees (S-expressions) and evaluate them.
    // Token format: numbers are bare, operators are symbols (+, -, *, /), parens are lparen/rparen.
    // Recursive descent parser with proper precedence: *, / before +, -.
    let form = r#"(progn
  (defvar neovm--et-tokens nil)

  (fset 'neovm--et-peek (lambda () (car neovm--et-tokens)))
  (fset 'neovm--et-consume
    (lambda () (prog1 (car neovm--et-tokens)
                 (setq neovm--et-tokens (cdr neovm--et-tokens)))))

  ;; factor = number | symbol | '(' expr ')'
  (fset 'neovm--et-parse-factor
    (lambda ()
      (let ((tok (funcall 'neovm--et-peek)))
        (cond
          ((numberp tok)
           (funcall 'neovm--et-consume))
          ((and (symbolp tok) (not (memq tok '(+ - * / lparen rparen))))
           (funcall 'neovm--et-consume))
          ((eq tok 'lparen)
           (funcall 'neovm--et-consume)
           (let ((expr (funcall 'neovm--et-parse-expr)))
             (funcall 'neovm--et-consume)  ;; consume rparen
             expr))
          (t 0)))))

  ;; unary = ['-'] factor
  (fset 'neovm--et-parse-unary
    (lambda ()
      (if (eq (funcall 'neovm--et-peek) '-)
          (progn (funcall 'neovm--et-consume)
                 (list 'neg (funcall 'neovm--et-parse-factor)))
        (funcall 'neovm--et-parse-factor))))

  ;; term = unary (('*' | '/') unary)*
  (fset 'neovm--et-parse-term
    (lambda ()
      (let ((left (funcall 'neovm--et-parse-unary))
            (done nil))
        (while (not done)
          (let ((op (funcall 'neovm--et-peek)))
            (if (memq op '(* /))
                (progn
                  (funcall 'neovm--et-consume)
                  (let ((right (funcall 'neovm--et-parse-unary)))
                    (setq left (list op left right))))
              (setq done t))))
        left)))

  ;; expr = term (('+' | '-') term)*
  (fset 'neovm--et-parse-expr
    (lambda ()
      (let ((left (funcall 'neovm--et-parse-term))
            (done nil))
        (while (not done)
          (let ((op (funcall 'neovm--et-peek)))
            (if (memq op '(+ -))
                (progn
                  (funcall 'neovm--et-consume)
                  (let ((right (funcall 'neovm--et-parse-term)))
                    (setq left (list op left right))))
              (setq done t))))
        left)))

  (fset 'neovm--et-parse
    (lambda (tokens)
      (setq neovm--et-tokens tokens)
      (funcall 'neovm--et-parse-expr)))

  ;; Evaluate expression tree
  (fset 'neovm--et-eval
    (lambda (tree env)
      (cond
        ((numberp tree) tree)
        ((symbolp tree)
         (let ((b (assq tree env)))
           (if b (cdr b) (signal 'error (list "unbound" tree)))))
        ((eq (car tree) 'neg)
         (- (funcall 'neovm--et-eval (nth 1 tree) env)))
        ((eq (car tree) '+)
         (+ (funcall 'neovm--et-eval (nth 1 tree) env)
            (funcall 'neovm--et-eval (nth 2 tree) env)))
        ((eq (car tree) '-)
         (- (funcall 'neovm--et-eval (nth 1 tree) env)
            (funcall 'neovm--et-eval (nth 2 tree) env)))
        ((eq (car tree) '*)
         (* (funcall 'neovm--et-eval (nth 1 tree) env)
            (funcall 'neovm--et-eval (nth 2 tree) env)))
        ((eq (car tree) '/)
         (/ (funcall 'neovm--et-eval (nth 1 tree) env)
            (funcall 'neovm--et-eval (nth 2 tree) env)))
        (t (signal 'error (list "unknown node" tree))))))

  (unwind-protect
      (list
        ;; Parse and show tree: 2 + 3 * 4 -> (+ 2 (* 3 4))
        (funcall 'neovm--et-parse '(2 + 3 * 4))
        ;; Parse: (2 + 3) * 4 -> (* (+ 2 3) 4)
        (funcall 'neovm--et-parse '(lparen 2 + 3 rparen * 4))
        ;; Parse: 10 - 3 - 2 -> (- (- 10 3) 2)  left-associative
        (funcall 'neovm--et-parse '(10 - 3 - 2))
        ;; Parse with variable: x * x + 2 * x + 1
        (funcall 'neovm--et-parse '(x * x + 2 * x + 1))
        ;; Evaluate: 2 + 3 * 4 = 14
        (funcall 'neovm--et-eval
                 (funcall 'neovm--et-parse '(2 + 3 * 4)) nil)
        ;; Evaluate: (2 + 3) * 4 = 20
        (funcall 'neovm--et-eval
                 (funcall 'neovm--et-parse '(lparen 2 + 3 rparen * 4)) nil)
        ;; Evaluate: 10 - 3 - 2 = 5
        (funcall 'neovm--et-eval
                 (funcall 'neovm--et-parse '(10 - 3 - 2)) nil)
        ;; Evaluate with variable binding: x=3 in x*x + 2*x + 1 = 16
        (funcall 'neovm--et-eval
                 (funcall 'neovm--et-parse '(x * x + 2 * x + 1))
                 '((x . 3))))
    (fmakunbound 'neovm--et-peek)
    (fmakunbound 'neovm--et-consume)
    (fmakunbound 'neovm--et-parse-factor)
    (fmakunbound 'neovm--et-parse-unary)
    (fmakunbound 'neovm--et-parse-term)
    (fmakunbound 'neovm--et-parse-expr)
    (fmakunbound 'neovm--et-parse)
    (fmakunbound 'neovm--et-eval)
    (makunbound 'neovm--et-tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ 2 (* 3 4)) (* (+ 2 3) 4) (- (- 10 3) 2) (+ (+ (* x x) (* 2 x)) 1) 14 20 5 16)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Convert expression tree to prefix notation (Polish notation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_tree_to_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert expression trees to prefix notation strings.
    let form = r#"(progn
  (fset 'neovm--et-to-prefix
    (lambda (tree)
      "Convert expression tree to prefix notation string."
      (cond
        ((numberp tree) (number-to-string tree))
        ((symbolp tree) (symbol-name tree))
        ((eq (car tree) 'neg)
         (concat "neg " (funcall 'neovm--et-to-prefix (nth 1 tree))))
        (t (concat (symbol-name (car tree)) " "
                   (funcall 'neovm--et-to-prefix (nth 1 tree)) " "
                   (funcall 'neovm--et-to-prefix (nth 2 tree)))))))

  ;; Also return as a flat list for structural comparison
  (fset 'neovm--et-to-prefix-list
    (lambda (tree)
      "Convert expression tree to prefix notation as a flat list."
      (cond
        ((numberp tree) (list tree))
        ((symbolp tree) (list tree))
        ((eq (car tree) 'neg)
         (cons 'neg (funcall 'neovm--et-to-prefix-list (nth 1 tree))))
        (t (append (list (car tree))
                   (funcall 'neovm--et-to-prefix-list (nth 1 tree))
                   (funcall 'neovm--et-to-prefix-list (nth 2 tree)))))))

  (unwind-protect
      (list
        ;; (+ 2 (* 3 4)) -> "+ 2 * 3 4"
        (funcall 'neovm--et-to-prefix '(+ 2 (* 3 4)))
        ;; (* (+ 2 3) 4) -> "* + 2 3 4"
        (funcall 'neovm--et-to-prefix '(* (+ 2 3) 4))
        ;; (- (- 10 3) 2) -> "- - 10 3 2"
        (funcall 'neovm--et-to-prefix '(- (- 10 3) 2))
        ;; (neg 5) -> "neg 5"
        (funcall 'neovm--et-to-prefix '(neg 5))
        ;; Complex: (+ (* x x) (+ (* 2 x) 1))
        (funcall 'neovm--et-to-prefix '(+ (* x x) (+ (* 2 x) 1)))
        ;; Flat list versions for structural comparison
        (funcall 'neovm--et-to-prefix-list '(+ 2 (* 3 4)))
        (funcall 'neovm--et-to-prefix-list '(* (+ 2 3) 4))
        (funcall 'neovm--et-to-prefix-list '(+ (* x x) (+ (* 2 x) 1)))
        ;; Simple leaf nodes
        (funcall 'neovm--et-to-prefix 42)
        (funcall 'neovm--et-to-prefix 'x))
    (fmakunbound 'neovm--et-to-prefix)
    (fmakunbound 'neovm--et-to-prefix-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"+ 2 * 3 4\" \"* + 2 3 4\" \"- - 10 3 2\" \"neg 5\" \"+ * x x + * 2 x 1\" (+ 2 * 3 4) (* + 2 3 4) (+ * x x + * 2 x 1) \"42\" \"x\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Convert expression tree to postfix notation (Reverse Polish notation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_tree_to_postfix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert expression trees to postfix notation (RPN).
    let form = r#"(progn
  (fset 'neovm--et-to-postfix
    (lambda (tree)
      "Convert expression tree to postfix notation string."
      (cond
        ((numberp tree) (number-to-string tree))
        ((symbolp tree) (symbol-name tree))
        ((eq (car tree) 'neg)
         (concat (funcall 'neovm--et-to-postfix (nth 1 tree)) " neg"))
        (t (concat (funcall 'neovm--et-to-postfix (nth 1 tree)) " "
                   (funcall 'neovm--et-to-postfix (nth 2 tree)) " "
                   (symbol-name (car tree)))))))

  (fset 'neovm--et-to-postfix-list
    (lambda (tree)
      "Convert expression tree to postfix notation as a flat list."
      (cond
        ((numberp tree) (list tree))
        ((symbolp tree) (list tree))
        ((eq (car tree) 'neg)
         (append (funcall 'neovm--et-to-postfix-list (nth 1 tree))
                 (list 'neg)))
        (t (append (funcall 'neovm--et-to-postfix-list (nth 1 tree))
                   (funcall 'neovm--et-to-postfix-list (nth 2 tree))
                   (list (car tree)))))))

  ;; Also: evaluate a postfix token list using a stack
  (fset 'neovm--et-eval-postfix
    (lambda (tokens)
      "Evaluate a postfix (RPN) token list using a stack."
      (let ((stack nil))
        (dolist (tok tokens)
          (cond
            ((numberp tok) (setq stack (cons tok stack)))
            ((eq tok 'neg)
             (let ((a (car stack)))
               (setq stack (cons (- a) (cdr stack)))))
            ((memq tok '(+ - * /))
             (let ((b (car stack))
                   (a (cadr stack)))
               (setq stack
                     (cons (cond ((eq tok '+) (+ a b))
                                 ((eq tok '-) (- a b))
                                 ((eq tok '*) (* a b))
                                 ((eq tok '/) (/ a b)))
                           (cddr stack)))))))
        (car stack))))

  (unwind-protect
      (list
        ;; Postfix strings
        ;; (+ 2 (* 3 4)) -> "2 3 4 * +"
        (funcall 'neovm--et-to-postfix '(+ 2 (* 3 4)))
        ;; (* (+ 2 3) 4) -> "2 3 + 4 *"
        (funcall 'neovm--et-to-postfix '(* (+ 2 3) 4))
        ;; (- (- 10 3) 2) -> "10 3 - 2 -"
        (funcall 'neovm--et-to-postfix '(- (- 10 3) 2))
        ;; Postfix lists
        (funcall 'neovm--et-to-postfix-list '(+ 2 (* 3 4)))
        (funcall 'neovm--et-to-postfix-list '(* (+ 2 3) 4))
        (funcall 'neovm--et-to-postfix-list '(+ (* x x) (+ (* 2 x) 1)))
        ;; Evaluate postfix: 2 3 4 * + = 14
        (funcall 'neovm--et-eval-postfix '(2 3 4 * +))
        ;; 2 3 + 4 * = 20
        (funcall 'neovm--et-eval-postfix '(2 3 + 4 *))
        ;; 10 3 - 2 - = 5
        (funcall 'neovm--et-eval-postfix '(10 3 - 2 -))
        ;; 5 neg = -5
        (funcall 'neovm--et-eval-postfix '(5 neg))
        ;; Complex: (3 + 4) * (5 - 2) = 21
        (funcall 'neovm--et-eval-postfix '(3 4 + 5 2 - *))
        ;; Verify roundtrip: tree -> postfix-list -> eval == tree -> direct-eval
        (let ((tree '(+ (* 3 4) (- 10 (* 2 3)))))
          (= (funcall 'neovm--et-eval-postfix
                      (funcall 'neovm--et-to-postfix-list tree))
             16)))  ;; 3*4 + (10 - 2*3) = 12 + 4 = 16
    (fmakunbound 'neovm--et-to-postfix)
    (fmakunbound 'neovm--et-to-postfix-list)
    (fmakunbound 'neovm--et-eval-postfix)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"2 3 4 * +\" \"2 3 + 4 *\" \"10 3 - 2 -\" (2 3 4 * +) (2 3 + 4 *) (x x * 2 x * 1 + +) 14 20 5 -5 21 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbolic differentiation of expression trees
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_tree_differentiation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Symbolic differentiation operating on expression tree nodes.
    // Supports: constants, variables, +, -, *, /, neg, and power (expt).
    let form = r#"(progn
  (fset 'neovm--et-deriv
    (lambda (tree var)
      "Differentiate expression tree TREE with respect to VAR."
      (cond
        ;; d/dx(c) = 0
        ((numberp tree) 0)
        ;; d/dx(x) = 1, d/dx(y) = 0
        ((symbolp tree) (if (eq tree var) 1 0))
        ;; d/dx(-a) = -(d/dx a)
        ((eq (car tree) 'neg)
         (list 'neg (funcall 'neovm--et-deriv (nth 1 tree) var)))
        ;; d/dx(a + b) = d/dx(a) + d/dx(b)
        ((eq (car tree) '+)
         (list '+ (funcall 'neovm--et-deriv (nth 1 tree) var)
               (funcall 'neovm--et-deriv (nth 2 tree) var)))
        ;; d/dx(a - b) = d/dx(a) - d/dx(b)
        ((eq (car tree) '-)
         (list '- (funcall 'neovm--et-deriv (nth 1 tree) var)
               (funcall 'neovm--et-deriv (nth 2 tree) var)))
        ;; Product rule: d/dx(a*b) = a'*b + a*b'
        ((eq (car tree) '*)
         (let ((a (nth 1 tree)) (b (nth 2 tree)))
           (list '+ (list '* (funcall 'neovm--et-deriv a var) b)
                 (list '* a (funcall 'neovm--et-deriv b var)))))
        ;; Quotient rule: d/dx(a/b) = (a'*b - a*b') / (b*b)
        ((eq (car tree) '/)
         (let ((a (nth 1 tree)) (b (nth 2 tree)))
           (list '/ (list '- (list '* (funcall 'neovm--et-deriv a var) b)
                         (list '* a (funcall 'neovm--et-deriv b var)))
                 (list '* b b))))
        ;; Power rule: d/dx(x^n) = n * x^(n-1) * d/dx(x)
        ((eq (car tree) 'expt)
         (let ((base (nth 1 tree)) (power (nth 2 tree)))
           (list '* (list '* power (list 'expt base (list '- power 1)))
                 (funcall 'neovm--et-deriv base var))))
        (t (list 'unknown-deriv tree)))))

  (unwind-protect
      (list
        ;; d/dx(5) = 0
        (funcall 'neovm--et-deriv 5 'x)
        ;; d/dx(x) = 1
        (funcall 'neovm--et-deriv 'x 'x)
        ;; d/dx(y) = 0
        (funcall 'neovm--et-deriv 'y 'x)
        ;; d/dx(x + 3) = (+ 1 0) = 1 (unsimplified)
        (funcall 'neovm--et-deriv '(+ x 3) 'x)
        ;; d/dx(x * x) = (+ (* 1 x) (* x 1))  (product rule)
        (funcall 'neovm--et-deriv '(* x x) 'x)
        ;; d/dx(3 * x) = (+ (* 0 x) (* 3 1))
        (funcall 'neovm--et-deriv '(* 3 x) 'x)
        ;; d/dx(-x) = (neg 1)
        (funcall 'neovm--et-deriv '(neg x) 'x)
        ;; d/dx(x^3) with power rule
        (funcall 'neovm--et-deriv '(expt x 3) 'x)
        ;; d/dx(x / y) = quotient rule
        (funcall 'neovm--et-deriv '(/ x y) 'x)
        ;; d/dx(x^2 + 3*x + 5)
        (funcall 'neovm--et-deriv '(+ (+ (expt x 2) (* 3 x)) 5) 'x))
    (fmakunbound 'neovm--et-deriv)))"#;
    let expect = expect_test::expect![[
        r#""OK (0 1 0 (+ 1 0) (+ (* 1 x) (* x 1)) (+ (* 0 x) (* 3 1)) (neg 1) (* (* 3 (expt x (- 3 1))) 1) (/ (- (* 1 y) (* x 0)) (* y y)) (+ (+ (* (* 2 (expt x (- 2 1))) 1) (+ (* 0 x) (* 3 1))) 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Expression simplification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_tree_simplification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simplify expression trees by applying algebraic rewrite rules.
    // Rules: x+0=x, 0+x=x, x*0=0, x*1=x, 1*x=x, x-0=x, 0-x=neg(x),
    //        x/1=x, neg(neg(x))=x, constant folding, x-x=0.
    let form = r#"(progn
  (fset 'neovm--et-simplify
    (lambda (tree)
      "Simplify expression tree by applying algebraic rules."
      (if (or (numberp tree) (symbolp tree))
          tree
        (let* ((op (car tree))
               (args (mapcar (lambda (e) (funcall 'neovm--et-simplify e))
                             (cdr tree)))
               (a (nth 0 args))
               (b (nth 1 args)))
          (cond
            ;; neg rules
            ((eq op 'neg)
             (cond
               ((numberp a) (- a))
               ;; neg(neg(x)) = x
               ((and (consp a) (eq (car a) 'neg)) (nth 1 a))
               ;; neg(0) = 0
               ((equal a 0) 0)
               (t (list 'neg a))))
            ;; Addition rules
            ((eq op '+)
             (cond
               ((and (numberp a) (numberp b)) (+ a b))
               ((equal a 0) b)
               ((equal b 0) a)
               ;; x + x = 2*x
               ((equal a b) (list '* 2 a))
               (t (list '+ a b))))
            ;; Subtraction rules
            ((eq op '-)
             (cond
               ((and (numberp a) (numberp b)) (- a b))
               ((equal b 0) a)
               ((equal a 0) (list 'neg b))
               ;; x - x = 0
               ((equal a b) 0)
               (t (list '- a b))))
            ;; Multiplication rules
            ((eq op '*)
             (cond
               ((and (numberp a) (numberp b)) (* a b))
               ((or (equal a 0) (equal b 0)) 0)
               ((equal a 1) b)
               ((equal b 1) a)
               ;; -1 * x = neg(x)
               ((equal a -1) (list 'neg b))
               ((equal b -1) (list 'neg a))
               (t (list '* a b))))
            ;; Division rules
            ((eq op '/)
             (cond
               ((and (numberp a) (numberp b) (not (= b 0))) (/ a b))
               ((equal a 0) 0)
               ((equal b 1) a)
               ;; x / x = 1
               ((equal a b) 1)
               (t (list '/ a b))))
            ;; Power rules
            ((eq op 'expt)
             (cond
               ((equal b 0) 1)
               ((equal b 1) a)
               ((and (numberp a) (numberp b)) (expt a b))
               (t (list 'expt a b))))
            (t (cons op args)))))))

  ;; Apply simplification repeatedly until stable
  (fset 'neovm--et-simplify-fix
    (lambda (tree)
      (let ((prev nil) (current tree) (n 0))
        (while (and (not (equal prev current)) (< n 20))
          (setq prev current)
          (setq current (funcall 'neovm--et-simplify current))
          (setq n (1+ n)))
        current)))

  (unwind-protect
      (list
        ;; Basic simplifications
        (funcall 'neovm--et-simplify-fix '(+ x 0))          ;; x
        (funcall 'neovm--et-simplify-fix '(+ 0 x))          ;; x
        (funcall 'neovm--et-simplify-fix '(* x 0))          ;; 0
        (funcall 'neovm--et-simplify-fix '(* x 1))          ;; x
        (funcall 'neovm--et-simplify-fix '(* 1 x))          ;; x
        (funcall 'neovm--et-simplify-fix '(- x 0))          ;; x
        (funcall 'neovm--et-simplify-fix '(- x x))          ;; 0
        (funcall 'neovm--et-simplify-fix '(/ x 1))          ;; x
        (funcall 'neovm--et-simplify-fix '(/ x x))          ;; 1
        (funcall 'neovm--et-simplify-fix '(neg (neg x)))    ;; x
        (funcall 'neovm--et-simplify-fix '(expt x 0))       ;; 1
        (funcall 'neovm--et-simplify-fix '(expt x 1))       ;; x
        ;; Constant folding
        (funcall 'neovm--et-simplify-fix '(+ 3 4))          ;; 7
        (funcall 'neovm--et-simplify-fix '(* 3 4))          ;; 12
        ;; Nested simplification
        (funcall 'neovm--et-simplify-fix '(+ (* 0 x) (* 3 1)))  ;; 3
        (funcall 'neovm--et-simplify-fix '(+ (+ 0 (* 1 x)) (* x 0)))  ;; x
        ;; Derivative-then-simplify: d/dx(3*x) simplified
        ;; Raw derivative: (+ (* 0 x) (* 3 1)) -> simplify -> 3
        (funcall 'neovm--et-simplify-fix '(+ (* 0 x) (* 3 1)))
        ;; x + x = 2*x
        (funcall 'neovm--et-simplify-fix '(+ x x)))
    (fmakunbound 'neovm--et-simplify)
    (fmakunbound 'neovm--et-simplify-fix)))"#;
    let expect = expect_test::expect![[r#""OK (x x 0 x x x 0 x 1 x 1 x 7 12 3 x 3 (* 2 x))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full pipeline: differentiate then simplify expression trees
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_tree_deriv_simplify_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Combine differentiation and simplification to produce clean derivative trees.
    let form = r#"(progn
  (fset 'neovm--et2-deriv
    (lambda (tree var)
      (cond
        ((numberp tree) 0)
        ((symbolp tree) (if (eq tree var) 1 0))
        ((eq (car tree) 'neg)
         (list 'neg (funcall 'neovm--et2-deriv (nth 1 tree) var)))
        ((eq (car tree) '+)
         (list '+ (funcall 'neovm--et2-deriv (nth 1 tree) var)
               (funcall 'neovm--et2-deriv (nth 2 tree) var)))
        ((eq (car tree) '-)
         (list '- (funcall 'neovm--et2-deriv (nth 1 tree) var)
               (funcall 'neovm--et2-deriv (nth 2 tree) var)))
        ((eq (car tree) '*)
         (let ((a (nth 1 tree)) (b (nth 2 tree)))
           (list '+ (list '* (funcall 'neovm--et2-deriv a var) b)
                 (list '* a (funcall 'neovm--et2-deriv b var)))))
        ((eq (car tree) '/)
         (let ((a (nth 1 tree)) (b (nth 2 tree)))
           (list '/ (list '- (list '* (funcall 'neovm--et2-deriv a var) b)
                         (list '* a (funcall 'neovm--et2-deriv b var)))
                 (list '* b b))))
        ((eq (car tree) 'expt)
         (let ((base (nth 1 tree)) (n (nth 2 tree)))
           (list '* (list '* n (list 'expt base (list '- n 1)))
                 (funcall 'neovm--et2-deriv base var))))
        (t tree))))

  (fset 'neovm--et2-simp
    (lambda (tree)
      (if (or (numberp tree) (symbolp tree)) tree
        (let* ((op (car tree))
               (args (mapcar (lambda (e) (funcall 'neovm--et2-simp e)) (cdr tree)))
               (a (nth 0 args)) (b (nth 1 args)))
          (cond
            ((eq op 'neg)
             (cond ((numberp a) (- a))
                   ((and (consp a) (eq (car a) 'neg)) (nth 1 a))
                   ((equal a 0) 0)
                   (t (list 'neg a))))
            ((eq op '+)
             (cond ((and (numberp a) (numberp b)) (+ a b))
                   ((equal a 0) b) ((equal b 0) a)
                   (t (list '+ a b))))
            ((eq op '-)
             (cond ((and (numberp a) (numberp b)) (- a b))
                   ((equal b 0) a) ((equal a b) 0)
                   ((equal a 0) (funcall 'neovm--et2-simp (list 'neg b)))
                   (t (list '- a b))))
            ((eq op '*)
             (cond ((and (numberp a) (numberp b)) (* a b))
                   ((or (equal a 0) (equal b 0)) 0)
                   ((equal a 1) b) ((equal b 1) a)
                   (t (list '* a b))))
            ((eq op '/)
             (cond ((equal a 0) 0) ((equal b 1) a)
                   ((and (numberp a) (numberp b) (not (= b 0))) (/ a b))
                   (t (list '/ a b))))
            ((eq op 'expt)
             (cond ((equal b 0) 1) ((equal b 1) a)
                   ((and (numberp a) (numberp b)) (expt a b))
                   (t (list 'expt a b))))
            (t (cons op args)))))))

  (fset 'neovm--et2-fix
    (lambda (tree)
      (let ((prev nil) (cur tree) (n 0))
        (while (and (not (equal prev cur)) (< n 20))
          (setq prev cur cur (funcall 'neovm--et2-simp cur) n (1+ n)))
        cur)))

  (fset 'neovm--et2-d-simp
    (lambda (tree var)
      (funcall 'neovm--et2-fix (funcall 'neovm--et2-deriv tree var))))

  (unwind-protect
      (list
        ;; d/dx(x + 3) -> 1
        (funcall 'neovm--et2-d-simp '(+ x 3) 'x)
        ;; d/dx(3*x) -> 3
        (funcall 'neovm--et2-d-simp '(* 3 x) 'x)
        ;; d/dx(x*x) -> (+ x x) or 2x after simplification
        (funcall 'neovm--et2-d-simp '(* x x) 'x)
        ;; d/dx(5) -> 0
        (funcall 'neovm--et2-d-simp 5 'x)
        ;; d/dx(x^2 + x) simplified
        (funcall 'neovm--et2-d-simp '(+ (expt x 2) x) 'x)
        ;; d/dx(2*x + 3*x) -> 5
        (funcall 'neovm--et2-d-simp '(+ (* 2 x) (* 3 x)) 'x)
        ;; d/dy(x^2 + y) = 1
        (funcall 'neovm--et2-d-simp '(+ (expt x 2) y) 'y)
        ;; d/dx(-x) -> -1
        (funcall 'neovm--et2-d-simp '(neg x) 'x))
    (fmakunbound 'neovm--et2-deriv)
    (fmakunbound 'neovm--et2-simp)
    (fmakunbound 'neovm--et2-fix)
    (fmakunbound 'neovm--et2-d-simp)))"#;
    let expect = expect_test::expect![[r#""OK (1 3 (+ x x) 0 (+ (* 2 x) 1) 5 1 -1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Expression tree depth, size, and infix pretty-print
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expr_tree_properties_and_infix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute expression tree properties (depth, size, leaf count) and
    // convert back to fully-parenthesized infix notation.
    let form = r#"(progn
  (fset 'neovm--et-depth
    (lambda (tree)
      (cond
        ((or (numberp tree) (symbolp tree)) 0)
        ((eq (car tree) 'neg)
         (1+ (funcall 'neovm--et-depth (nth 1 tree))))
        (t (1+ (max (funcall 'neovm--et-depth (nth 1 tree))
                    (funcall 'neovm--et-depth (nth 2 tree))))))))

  (fset 'neovm--et-size
    (lambda (tree)
      (cond
        ((or (numberp tree) (symbolp tree)) 1)
        ((eq (car tree) 'neg)
         (1+ (funcall 'neovm--et-size (nth 1 tree))))
        (t (+ 1 (funcall 'neovm--et-size (nth 1 tree))
               (funcall 'neovm--et-size (nth 2 tree)))))))

  (fset 'neovm--et-leaf-count
    (lambda (tree)
      (cond
        ((or (numberp tree) (symbolp tree)) 1)
        ((eq (car tree) 'neg)
         (funcall 'neovm--et-leaf-count (nth 1 tree)))
        (t (+ (funcall 'neovm--et-leaf-count (nth 1 tree))
              (funcall 'neovm--et-leaf-count (nth 2 tree)))))))

  (fset 'neovm--et-to-infix
    (lambda (tree)
      "Convert expression tree to fully parenthesized infix string."
      (cond
        ((numberp tree) (number-to-string tree))
        ((symbolp tree) (symbol-name tree))
        ((eq (car tree) 'neg)
         (concat "(-" (funcall 'neovm--et-to-infix (nth 1 tree)) ")"))
        (t (concat "(" (funcall 'neovm--et-to-infix (nth 1 tree))
                   " " (symbol-name (car tree)) " "
                   (funcall 'neovm--et-to-infix (nth 2 tree)) ")")))))

  (fset 'neovm--et-vars
    (lambda (tree)
      "Extract sorted unique variable names from tree."
      (cond
        ((numberp tree) nil)
        ((symbolp tree) (list tree))
        ((eq (car tree) 'neg)
         (funcall 'neovm--et-vars (nth 1 tree)))
        (t (let ((all nil))
             (dolist (v (funcall 'neovm--et-vars (nth 1 tree)))
               (unless (memq v all) (setq all (cons v all))))
             (when (nth 2 tree)
               (dolist (v (funcall 'neovm--et-vars (nth 2 tree)))
                 (unless (memq v all) (setq all (cons v all)))))
             (sort all (lambda (a b)
                         (string< (symbol-name a) (symbol-name b)))))))))

  (unwind-protect
      (let ((e1 '(+ 2 (* 3 4)))
            (e2 '(* (+ x y) (- x y)))
            (e3 '(+ (expt x 2) (+ (* 2 x) 1)))
            (e4 42)
            (e5 'x)
            (e6 '(neg (+ a b))))
        (list
          ;; Depths
          (list (funcall 'neovm--et-depth e1)
                (funcall 'neovm--et-depth e2)
                (funcall 'neovm--et-depth e3)
                (funcall 'neovm--et-depth e4)
                (funcall 'neovm--et-depth e5)
                (funcall 'neovm--et-depth e6))
          ;; Sizes
          (list (funcall 'neovm--et-size e1)
                (funcall 'neovm--et-size e2)
                (funcall 'neovm--et-size e3)
                (funcall 'neovm--et-size e4)
                (funcall 'neovm--et-size e5)
                (funcall 'neovm--et-size e6))
          ;; Leaf counts
          (list (funcall 'neovm--et-leaf-count e1)
                (funcall 'neovm--et-leaf-count e2)
                (funcall 'neovm--et-leaf-count e3)
                (funcall 'neovm--et-leaf-count e4))
          ;; Infix strings
          (list (funcall 'neovm--et-to-infix e1)
                (funcall 'neovm--et-to-infix e2)
                (funcall 'neovm--et-to-infix e3)
                (funcall 'neovm--et-to-infix e4)
                (funcall 'neovm--et-to-infix e5)
                (funcall 'neovm--et-to-infix e6))
          ;; Variables
          (list (funcall 'neovm--et-vars e1)
                (funcall 'neovm--et-vars e2)
                (funcall 'neovm--et-vars e3)
                (funcall 'neovm--et-vars e6))))
    (fmakunbound 'neovm--et-depth)
    (fmakunbound 'neovm--et-size)
    (fmakunbound 'neovm--et-leaf-count)
    (fmakunbound 'neovm--et-to-infix)
    (fmakunbound 'neovm--et-vars)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 2 3 0 0 2) (5 7 9 1 1 4) (3 4 5 1) (\"(2 + (3 * 4))\" \"((x + y) * (x - y))\" \"((x expt 2) + ((2 * x) + 1))\" \"42\" \"x\" \"(-(a + b))\") (nil (x y) (x) (a b)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
