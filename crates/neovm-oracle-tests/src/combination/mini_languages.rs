//! Complex oracle tests for mini-language interpreters implemented in Elisp.
//!
//! Tests Forth-like stack machines, Brainfuck interpreters, Logo-like
//! turtle graphics, metacircular evaluators, calculators with variables,
//! and pattern-matching rule engines.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Forth-like stack-based interpreter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mini_forth_interpreter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-forth
    (lambda (program)
      (let ((stack nil)
            (tokens (split-string program " " t)))
        (dolist (tok tokens)
          (cond
           ;; Arithmetic
           ((string= tok "+")
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (+ a b) (cddr stack)))))
           ((string= tok "-")
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (- a b) (cddr stack)))))
           ((string= tok "*")
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (* a b) (cddr stack)))))
           ((string= tok "/")
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (/ a b) (cddr stack)))))
           ;; Stack manipulation
           ((string= tok "DUP")
            (setq stack (cons (car stack) stack)))
           ((string= tok "DROP")
            (setq stack (cdr stack)))
           ((string= tok "SWAP")
            (let ((a (car stack)) (b (cadr stack)))
              (setq stack (cons b (cons a (cddr stack))))))
           ((string= tok "OVER")
            (setq stack (cons (cadr stack) stack)))
           ((string= tok "ROT")
            (let ((a (car stack))
                  (b (cadr stack))
                  (c (caddr stack)))
              (setq stack (cons b (cons c (cons a (cdddr stack)))))))
           ((string= tok "DEPTH")
            (setq stack (cons (length stack) stack)))
           ;; Comparison
           ((string= tok "=")
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (if (= a b) -1 0) (cddr stack)))))
           ((string= tok "<")
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (if (< a b) -1 0) (cddr stack)))))
           ((string= tok ">")
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (if (> a b) -1 0) (cddr stack)))))
           ((string= tok "ABS")
            (setq stack (cons (abs (car stack)) (cdr stack))))
           ((string= tok "NEGATE")
            (setq stack (cons (- (car stack)) (cdr stack))))
           ((string= tok "MAX")
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (max a b) (cddr stack)))))
           ((string= tok "MIN")
            (let ((b (car stack)) (a (cadr stack)))
              (setq stack (cons (min a b) (cddr stack)))))
           ;; Number literal
           (t
            (setq stack (cons (string-to-number tok) stack)))))
        stack)))
  (unwind-protect
      (list
       ;; Basic arithmetic: (3 + 4) * 2 = 14
       (funcall 'neovm--test-forth "3 4 + 2 *")
       ;; Pythagorean: sqrt(3^2 + 4^2) — just compute 3^2 + 4^2 = 25
       (funcall 'neovm--test-forth "3 DUP * 4 DUP * +")
       ;; Stack manipulation
       (funcall 'neovm--test-forth "10 20 SWAP")
       (funcall 'neovm--test-forth "1 2 3 ROT")
       ;; Comparison
       (funcall 'neovm--test-forth "5 5 = 5 3 = 3 5 <")
       ;; Complex: compute max of three numbers
       (funcall 'neovm--test-forth "7 3 MAX 12 MAX")
       ;; Depth tracking
       (funcall 'neovm--test-forth "1 2 3 DEPTH")
       ;; Factorial of 5 via repeated multiply: 1*2*3*4*5
       (funcall 'neovm--test-forth "1 2 * 3 * 4 * 5 *"))
    (fmakunbound 'neovm--test-forth)))"#;
    let expect = expect_test::expect![[
        r#""OK ((14) (25) (10 20) (2 1 3) (-1 0 -1) (12) (3 3 2 1) (120))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Brainfuck interpreter (simplified, no input)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mini_brainfuck() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-bf
    (lambda (code)
      (let ((tape (make-vector 256 0))
            (ptr 0)
            (pc 0)
            (output nil)
            (len (length code))
            (max-steps 10000)
            (steps 0))
        (while (and (< pc len) (< steps max-steps))
          (setq steps (1+ steps))
          (let ((ch (aref code pc)))
            (cond
             ((= ch ?>)
              (setq ptr (1+ ptr)))
             ((= ch ?<)
              (setq ptr (1- ptr)))
             ((= ch ?+)
              (aset tape ptr (% (1+ (aref tape ptr)) 256)))
             ((= ch ?-)
              (aset tape ptr (% (+ (1- (aref tape ptr)) 256) 256)))
             ((= ch ?.)
              (setq output (cons (aref tape ptr) output)))
             ((= ch ?\[)
              (when (= (aref tape ptr) 0)
                ;; Jump to matching ]
                (let ((depth 1))
                  (while (> depth 0)
                    (setq pc (1+ pc))
                    (cond
                     ((= (aref code pc) ?\[) (setq depth (1+ depth)))
                     ((= (aref code pc) ?\]) (setq depth (1- depth))))))))
             ((= ch ?\])
              (unless (= (aref tape ptr) 0)
                ;; Jump back to matching [
                (let ((depth 1))
                  (while (> depth 0)
                    (setq pc (1- pc))
                    (cond
                     ((= (aref code pc) ?\]) (setq depth (1+ depth)))
                     ((= (aref code pc) ?\[) (setq depth (1- depth))))))))
             ))
          (setq pc (1+ pc)))
        ;; Return list of output char codes
        (nreverse output))))
  (unwind-protect
      (list
       ;; Output 65 (A): set cell to 65 then output
       ;; 65 = 8*8 + 1: ++++++++ [>++++++++<-] >+.
       (funcall 'neovm--test-bf "++++++++[>++++++++<-]>+.")
       ;; Output 72 73 (HI): 72=8*9, 73=72+1
       (funcall 'neovm--test-bf "++++++++[>+++++++++<-]>.+.")
       ;; Simple increment and output: output 1,2,3
       (funcall 'neovm--test-bf "+.+.+.")
       ;; Loop counting to 5: output 5
       (funcall 'neovm--test-bf "+++++."))
    (fmakunbound 'neovm--test-bf)))"#;
    let expect = expect_test::expect![[r#""OK ((65) (72 73) (1 2 3) (5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Logo-like turtle graphics (track coordinates)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mini_logo_turtle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-turtle
    (lambda (commands)
      (let ((x 0) (y 0) (angle 0)
            (pen-down t)
            (path nil))
        (when pen-down
          (setq path (list (cons x y))))
        (dolist (cmd commands)
          (let ((op (car cmd))
                (arg (cdr cmd)))
            (cond
             ((eq op 'fd)
              ;; Move forward; use simplified trig for 0/90/180/270
              (let ((a (% (+ (% angle 360) 360) 360)))
                (cond
                 ((= a 0)   (setq y (+ y arg)))
                 ((= a 90)  (setq x (+ x arg)))
                 ((= a 180) (setq y (- y arg)))
                 ((= a 270) (setq x (- x arg)))))
              (when pen-down
                (setq path (cons (cons x y) path))))
             ((eq op 'bk)
              (let ((a (% (+ (% angle 360) 360) 360)))
                (cond
                 ((= a 0)   (setq y (- y arg)))
                 ((= a 90)  (setq x (- x arg)))
                 ((= a 180) (setq y (+ y arg)))
                 ((= a 270) (setq x (+ x arg)))))
              (when pen-down
                (setq path (cons (cons x y) path))))
             ((eq op 'rt) (setq angle (+ angle arg)))
             ((eq op 'lt) (setq angle (- angle arg)))
             ((eq op 'pu) (setq pen-down nil))
             ((eq op 'pd) (setq pen-down t)
              (setq path (cons (cons x y) path))))))
        (list (cons 'pos (cons x y))
              (cons 'angle (% (+ (% angle 360) 360) 360))
              (cons 'path (nreverse path))))))
  (unwind-protect
      (list
       ;; Draw a square: fd 100, rt 90, repeat 4 times
       (funcall 'neovm--test-turtle
                '((fd . 100) (rt . 90) (fd . 100) (rt . 90)
                  (fd . 100) (rt . 90) (fd . 100) (rt . 90)))
       ;; Draw a triangle
       (funcall 'neovm--test-turtle
                '((fd . 50) (rt . 120) (fd . 50) (rt . 120)
                  (fd . 50) (rt . 120)))
       ;; Pen up/down: move without drawing, then draw
       (funcall 'neovm--test-turtle
                '((pu . nil) (fd . 30) (pd . nil) (fd . 20) (rt . 90) (fd . 10)))
       ;; Back and forth
       (funcall 'neovm--test-turtle
                '((fd . 50) (bk . 25) (rt . 90) (fd . 30))))
    (fmakunbound 'neovm--test-turtle)))"#;
    let expect = expect_test::expect![[
        r#""OK (((pos 0 . 0) (angle . 0) (path (0 . 0) (0 . 100) (100 . 100) (100 . 0) (0 . 0))) ((pos 0 . 50) (angle . 0) (path (0 . 0) (0 . 50) (0 . 50) (0 . 50))) ((pos 10 . 50) (angle . 90) (path (0 . 0) (0 . 30) (0 . 50) (10 . 50))) ((pos 30 . 25) (angle . 90) (path (0 . 0) (0 . 50) (0 . 25) (30 . 25))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple Lisp metacircular evaluator subset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mini_lisp_metacircular() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-meval
    (lambda (expr env)
      (cond
       ;; Self-evaluating: numbers, strings
       ((numberp expr) expr)
       ((stringp expr) expr)
       ((eq expr t) t)
       ((null expr) nil)
       ;; Variable lookup
       ((symbolp expr)
        (let ((binding (assq expr env)))
          (if binding (cdr binding)
            (error "Unbound: %s" expr))))
       ;; Special forms and calls
       ((consp expr)
        (let ((head (car expr)))
          (cond
           ;; quote
           ((eq head 'quote) (cadr expr))
           ;; if
           ((eq head 'if)
            (if (funcall 'neovm--test-meval (nth 1 expr) env)
                (funcall 'neovm--test-meval (nth 2 expr) env)
              (funcall 'neovm--test-meval (nth 3 expr) env)))
           ;; lambda — return a closure representation
           ((eq head 'lambda)
            (list 'closure env (nth 1 expr) (nth 2 expr)))
           ;; let
           ((eq head 'let)
            (let ((bindings (nth 1 expr))
                  (body (nth 2 expr))
                  (new-env env))
              (dolist (b bindings)
                (setq new-env
                      (cons (cons (car b)
                                  (funcall 'neovm--test-meval (cadr b) env))
                            new-env)))
              (funcall 'neovm--test-meval body new-env)))
           ;; Function call
           (t
            (let ((fn (funcall 'neovm--test-meval head env))
                  (args (mapcar (lambda (a)
                                  (funcall 'neovm--test-meval a env))
                                (cdr expr))))
              (cond
               ;; Built-in operations
               ((eq fn '+) (apply #'+ args))
               ((eq fn '-) (apply #'- args))
               ((eq fn '*) (apply #'* args))
               ((eq fn '=) (= (nth 0 args) (nth 1 args)))
               ((eq fn '<) (< (nth 0 args) (nth 1 args)))
               ((eq fn 'list) args)
               ;; Closure application
               ((and (consp fn) (eq (car fn) 'closure))
                (let ((cenv (nth 1 fn))
                      (params (nth 2 fn))
                      (body (nth 3 fn)))
                  (let ((call-env cenv)
                        (ps params)
                        (as args))
                    (while ps
                      (setq call-env
                            (cons (cons (car ps) (car as))
                                  call-env))
                      (setq ps (cdr ps) as (cdr as)))
                    (funcall 'neovm--test-meval body call-env))))
               (t (error "Not callable: %S" fn))))))))
       (t (error "Cannot eval: %S" expr)))))
  (unwind-protect
      (let ((builtins '((+ . +) (- . -) (* . *) (= . =) (< . <) (list . list))))
        (list
         ;; Basic arithmetic
         (funcall 'neovm--test-meval '(+ 1 2 3) builtins)
         ;; Let binding
         (funcall 'neovm--test-meval
                  '(let ((x 10) (y 20)) (+ x y))
                  builtins)
         ;; Lambda and application
         (funcall 'neovm--test-meval
                  '(let ((double (lambda (n) (* n 2))))
                     (double 21))
                  builtins)
         ;; If expression
         (funcall 'neovm--test-meval
                  '(if (< 3 5) 100 200)
                  builtins)
         ;; Nested let + lambda
         (funcall 'neovm--test-meval
                  '(let ((add (lambda (a b) (+ a b))))
                     (let ((x 7) (y 8))
                       (add x y)))
                  builtins)
         ;; Quote
         (funcall 'neovm--test-meval '(quote (a b c)) builtins)))
    (fmakunbound 'neovm--test-meval)))"#;
    let expect = expect_test::expect![[r#""OK (6 30 42 100 15 (a b c))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Calculator with variables and assignment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mini_calculator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A calculator that processes statements: (set var expr), (expr), (print var)
    let form = r#"(progn
  (fset 'neovm--test-calc-eval
    (lambda (expr vars)
      (cond
       ((numberp expr) expr)
       ((symbolp expr)
        (let ((v (assq expr vars)))
          (if v (cdr v) 0)))
       ((consp expr)
        (let ((op (car expr)))
          (cond
           ((eq op '+)
            (+ (funcall 'neovm--test-calc-eval (nth 1 expr) vars)
               (funcall 'neovm--test-calc-eval (nth 2 expr) vars)))
           ((eq op '-)
            (- (funcall 'neovm--test-calc-eval (nth 1 expr) vars)
               (funcall 'neovm--test-calc-eval (nth 2 expr) vars)))
           ((eq op '*)
            (* (funcall 'neovm--test-calc-eval (nth 1 expr) vars)
               (funcall 'neovm--test-calc-eval (nth 2 expr) vars)))
           ((eq op '/)
            (/ (funcall 'neovm--test-calc-eval (nth 1 expr) vars)
               (funcall 'neovm--test-calc-eval (nth 2 expr) vars)))
           ((eq op 'mod)
            (% (funcall 'neovm--test-calc-eval (nth 1 expr) vars)
               (funcall 'neovm--test-calc-eval (nth 2 expr) vars)))
           ((eq op 'abs)
            (abs (funcall 'neovm--test-calc-eval (nth 1 expr) vars)))
           ((eq op 'max)
            (max (funcall 'neovm--test-calc-eval (nth 1 expr) vars)
                 (funcall 'neovm--test-calc-eval (nth 2 expr) vars)))
           ((eq op 'min)
            (min (funcall 'neovm--test-calc-eval (nth 1 expr) vars)
                 (funcall 'neovm--test-calc-eval (nth 2 expr) vars)))
           (t 0))))
       (t 0))))
  (fset 'neovm--test-calc-run
    (lambda (stmts)
      (let ((vars nil) (output nil))
        (dolist (stmt stmts)
          (cond
           ((and (consp stmt) (eq (car stmt) 'set))
            (let ((var (nth 1 stmt))
                  (val (funcall 'neovm--test-calc-eval (nth 2 stmt) vars)))
              (let ((existing (assq var vars)))
                (if existing
                    (setcdr existing val)
                  (setq vars (cons (cons var val) vars))))))
           ((and (consp stmt) (eq (car stmt) 'print))
            (let ((var (nth 1 stmt)))
              (setq output
                    (cons (cons var
                                (funcall 'neovm--test-calc-eval var vars))
                          output))))
           (t
            (setq output
                  (cons (funcall 'neovm--test-calc-eval stmt vars)
                        output)))))
        (list (nreverse output) vars))))
  (unwind-protect
      (list
       ;; Simple variable assignment and retrieval
       (funcall 'neovm--test-calc-run
                '((set x 10) (set y 20) (set z (+ x y))
                  (print x) (print y) (print z)))
       ;; Complex expressions with variables
       (funcall 'neovm--test-calc-run
                '((set a 100) (set b 7)
                  (set q (/ a b)) (set r (mod a b))
                  (print q) (print r)
                  ;; Verify: a = q*b + r
                  (+ (* q b) r)))
       ;; Chained updates
       (funcall 'neovm--test-calc-run
                '((set counter 0)
                  (set counter (+ counter 1))
                  (set counter (+ counter 1))
                  (set counter (+ counter 1))
                  (print counter)
                  (set result (* counter counter))
                  (print result)))
       ;; Nested max/min/abs
       (funcall 'neovm--test-calc-run
                '((set a -5) (set b 3) (set c -8)
                  (set m (max (abs a) (max (abs b) (abs c))))
                  (set n (min a (min b c)))
                  (print m) (print n))))
    (fmakunbound 'neovm--test-calc-eval)
    (fmakunbound 'neovm--test-calc-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((x . 10) (y . 20) (z . 30)) ((z . 30) (y . 20) (x . 10))) (((q . 14) (r . 2) 100) ((r . 2) (q . 14) (b . 7) (a . 100))) (((counter . 3) (result . 9)) ((result . 9) (counter . 3))) (((m . 8) (n . -8)) ((n . -8) (m . 8) (c . -8) (b . 3) (a . -5))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pattern matching / rule engine with guards
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mini_pattern_rule_engine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pattern language: match expressions against patterns with guards
    // Patterns: 'any, (lit val), (var name), (cons p1 p2), (guard pat pred)
    let form = r#"(progn
  (fset 'neovm--test-pmatch2
    (lambda (pattern value bindings)
      (cond
       ;; Wildcard
       ((eq pattern 'any) bindings)
       ;; Literal match
       ((and (consp pattern) (eq (car pattern) 'lit))
        (if (equal (cadr pattern) value) bindings nil))
       ;; Variable binding
       ((and (consp pattern) (eq (car pattern) 'var))
        (let ((name (cadr pattern))
              (existing (assq (cadr pattern) bindings)))
          (if existing
              (if (equal (cdr existing) value) bindings nil)
            (cons (cons name value) bindings))))
       ;; Cons pattern
       ((and (consp pattern) (eq (car pattern) 'cons))
        (if (consp value)
            (let ((r1 (funcall 'neovm--test-pmatch2
                                (nth 1 pattern) (car value) bindings)))
              (if r1
                  (funcall 'neovm--test-pmatch2
                            (nth 2 pattern) (cdr value) r1)
                nil))
          nil))
       ;; Guard: match inner pattern, then check predicate on bindings
       ((and (consp pattern) (eq (car pattern) 'guard))
        (let ((inner (nth 1 pattern))
              (pred (nth 2 pattern)))
          (let ((r (funcall 'neovm--test-pmatch2 inner value bindings)))
            (if (and r (funcall pred r)) r nil))))
       ;; Or pattern
       ((and (consp pattern) (eq (car pattern) 'or))
        (let ((alts (cdr pattern)) (result nil))
          (while (and alts (not result))
            (setq result
                  (funcall 'neovm--test-pmatch2 (car alts) value bindings))
            (setq alts (cdr alts)))
          result))
       (t nil))))
  ;; Rule engine: list of (pattern . action-fn), applies first matching rule
  (fset 'neovm--test-rule-apply
    (lambda (rules value)
      (let ((remaining rules) (result nil) (found nil))
        (while (and remaining (not found))
          (let ((rule (car remaining)))
            (let ((bindings (funcall 'neovm--test-pmatch2
                                      (car rule) value nil)))
              (when bindings
                (setq result (funcall (cdr rule) bindings))
                (setq found t))))
          (setq remaining (cdr remaining)))
        (if found result 'no-match))))
  (unwind-protect
      (let ((rules
             (list
              ;; Rule 1: pair of equal numbers
              (cons '(cons (var x) (var x))
                    (lambda (bindings)
                      (list 'pair (cdr (assq 'x bindings)))))
              ;; Rule 2: pair where first > 10
              (cons '(guard (cons (var a) (var b))
                            ,(lambda (bindings)
                               (> (cdr (assq 'a bindings)) 10)))
                    (lambda (bindings)
                      (list 'big-first (cdr (assq 'a bindings))
                            (cdr (assq 'b bindings)))))
              ;; Rule 3: any pair
              (cons '(cons any any)
                    (lambda (_b) 'generic-pair))
              ;; Rule 4: literal nil
              (cons '(lit nil)
                    (lambda (_b) 'is-nil))
              ;; Rule 5: anything
              (cons 'any
                    (lambda (_b) 'fallback)))))
        (list
         ;; Equal pair matches rule 1
         (funcall 'neovm--test-rule-apply rules '(5 . 5))
         ;; Big first matches rule 2 (not rule 1 since 20 != 3)
         (funcall 'neovm--test-rule-apply rules '(20 . 3))
         ;; Generic pair (neither equal nor big first)
         (funcall 'neovm--test-rule-apply rules '(2 . 7))
         ;; nil matches rule 4
         (funcall 'neovm--test-rule-apply rules nil)
         ;; number matches rule 5 (fallback)
         (funcall 'neovm--test-rule-apply rules 42)
         ;; Or pattern test
         (funcall 'neovm--test-pmatch2
                  '(or (lit 1) (lit 2) (lit 3)) 2 nil)
         (funcall 'neovm--test-pmatch2
                  '(or (lit 1) (lit 2) (lit 3)) 5 nil)))
    (fmakunbound 'neovm--test-pmatch2)
    (fmakunbound 'neovm--test-rule-apply)))"#;
    let expect = expect_test::expect![[
        r#""ERR (invalid-function (\\, (lambda (bindings) (> (cdr (assq 'a bindings)) 10))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
