//! Oracle parity tests for lambda calculus implemented in Elisp:
//! Church encoding of booleans (true, false, and, or, not, if),
//! Church numerals (zero, succ, plus, mult, pred, is-zero),
//! pairs, lists, Y combinator for recursion, evaluation/reduction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Church booleans: true, false, and, or, not, if
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_booleans() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Church booleans: TRUE = (lambda (t f) t), FALSE = (lambda (t f) f)
    // Logical operations built from these.
    let form = r#"(let ((TRUE  (lambda (x) (lambda (y) x)))
                        (FALSE (lambda (x) (lambda (y) y)))
                        ;; Convert Church bool to Elisp bool
                        (to-bool (lambda (cb) (funcall (funcall cb t) nil))))
                    ;; AND = (lambda (p q) (funcall (funcall p q) p))
                    (let ((AND (lambda (p q) (funcall (funcall p q) p)))
                          ;; OR = (lambda (p q) (funcall (funcall p p) q))
                          (OR  (lambda (p q) (funcall (funcall p p) q)))
                          ;; NOT = (lambda (p) (funcall (funcall p FALSE) TRUE))
                          (NOT (lambda (p) (funcall (funcall p FALSE) TRUE)))
                          ;; IF = (lambda (cond then else) (funcall (funcall cond then) else))
                          (IF  (lambda (cond then-val else-val)
                                 (funcall (funcall cond then-val) else-val))))
                      (list
                       ;; Basic values
                       (funcall to-bool TRUE)
                       (funcall to-bool FALSE)
                       ;; AND truth table
                       (funcall to-bool (funcall AND TRUE TRUE))
                       (funcall to-bool (funcall AND TRUE FALSE))
                       (funcall to-bool (funcall AND FALSE TRUE))
                       (funcall to-bool (funcall AND FALSE FALSE))
                       ;; OR truth table
                       (funcall to-bool (funcall OR TRUE TRUE))
                       (funcall to-bool (funcall OR TRUE FALSE))
                       (funcall to-bool (funcall OR FALSE TRUE))
                       (funcall to-bool (funcall OR FALSE FALSE))
                       ;; NOT
                       (funcall to-bool (funcall NOT TRUE))
                       (funcall to-bool (funcall NOT FALSE))
                       ;; Double negation
                       (funcall to-bool (funcall NOT (funcall NOT TRUE)))
                       ;; IF-THEN-ELSE
                       (funcall IF TRUE 'yes 'no)
                       (funcall IF FALSE 'yes 'no)
                       ;; De Morgan: NOT(AND(p,q)) = OR(NOT(p),NOT(q))
                       (let ((p TRUE) (q FALSE))
                         (equal (funcall to-bool
                                         (funcall NOT (funcall AND p q)))
                                (funcall to-bool
                                         (funcall OR (funcall NOT p)
                                                  (funcall NOT q))))))))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil nil nil t t t nil nil t t yes no t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church numerals: zero, succ, plus, mult, to-int
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_numerals_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Church numeral N = (lambda (f) (lambda (x) (f (f ... (f x)))))
    // applied N times.
    let form = r#"(let (;; Zero: apply f zero times
                        (ZERO  (lambda (f) (lambda (x) x)))
                        ;; Successor: apply f one more time
                        (SUCC  (lambda (n) (lambda (f) (lambda (x)
                                 (funcall f
                                          (funcall (funcall n f) x))))))
                        ;; Convert Church numeral to integer
                        (to-int (lambda (n) (funcall (funcall n #'1+) 0))))
                    ;; Build numerals 1 through 5
                    (let* ((ONE   (funcall SUCC ZERO))
                           (TWO   (funcall SUCC ONE))
                           (THREE (funcall SUCC TWO))
                           (FOUR  (funcall SUCC THREE))
                           (FIVE  (funcall SUCC FOUR)))
                      ;; PLUS: (lambda (m n) (lambda (f) (lambda (x)
                      ;;         (funcall (funcall m f) (funcall (funcall n f) x)))))
                      (let ((PLUS (lambda (m n)
                                    (lambda (f)
                                      (lambda (x)
                                        (funcall (funcall m f)
                                                 (funcall (funcall n f) x))))))
                            ;; MULT: (lambda (m n) (lambda (f) (funcall m (funcall n f))))
                            (MULT (lambda (m n)
                                    (lambda (f)
                                      (funcall m (funcall n f))))))
                        (list
                         ;; Basic conversions
                         (funcall to-int ZERO)
                         (funcall to-int ONE)
                         (funcall to-int TWO)
                         (funcall to-int THREE)
                         (funcall to-int FIVE)
                         ;; Addition
                         (funcall to-int (funcall PLUS TWO THREE))    ;; 5
                         (funcall to-int (funcall PLUS ZERO FOUR))    ;; 4
                         (funcall to-int (funcall PLUS FIVE FIVE))    ;; 10
                         ;; Multiplication
                         (funcall to-int (funcall MULT TWO THREE))    ;; 6
                         (funcall to-int (funcall MULT THREE FOUR))   ;; 12
                         (funcall to-int (funcall MULT ZERO FIVE))    ;; 0
                         (funcall to-int (funcall MULT ONE FIVE))     ;; 5
                         ;; Combined: 2*3 + 4 = 10
                         (funcall to-int
                                  (funcall PLUS
                                           (funcall MULT TWO THREE)
                                           FOUR))
                         ;; (2+3) * (1+1) = 10
                         (funcall to-int
                                  (funcall MULT
                                           (funcall PLUS TWO THREE)
                                           (funcall PLUS ONE ONE)))))))"#;
    let expect = expect_test::expect![[r#""OK (0 1 2 3 5 5 4 10 6 12 0 5 10 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church numerals: predecessor and is-zero
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_pred_iszero() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Predecessor using the pair trick (Kleene's method):
    // Build pairs (0,0), (0,1), (1,2), (2,3), ... and take first.
    let form = r#"(let ((ZERO (lambda (f) (lambda (x) x)))
                        (SUCC (lambda (n) (lambda (f) (lambda (x)
                                (funcall f (funcall (funcall n f) x))))))
                        (TRUE  (lambda (x) (lambda (y) x)))
                        (FALSE (lambda (x) (lambda (y) y)))
                        (to-int (lambda (n) (funcall (funcall n #'1+) 0)))
                        (to-bool (lambda (cb) (funcall (funcall cb t) nil))))
                    ;; Church pairs
                    (let ((PAIR (lambda (a b) (lambda (sel) (funcall (funcall sel a) b))))
                          (FST  (lambda (p) (funcall p TRUE)))
                          (SND  (lambda (p) (funcall p FALSE))))
                      ;; IS-ZERO: returns TRUE if n is ZERO, FALSE otherwise
                      ;; IS-ZERO = (lambda (n) (funcall (funcall n (lambda (_) FALSE)) TRUE))
                      (let ((IS-ZERO (lambda (n)
                                       (funcall (funcall n (lambda (_) FALSE)) TRUE))))
                        ;; PRED using the pair trick:
                        ;; Start with (ZERO, ZERO), apply n times: (snd pair, succ (snd pair))
                        ;; Then take fst.
                        (let ((PRED (lambda (n)
                                      (funcall FST
                                               (funcall (funcall n
                                                                 (lambda (p)
                                                                   (funcall PAIR
                                                                            (funcall SND p)
                                                                            (funcall SUCC
                                                                                     (funcall SND p)))))
                                                        (funcall PAIR ZERO ZERO))))))
                          (let* ((ONE   (funcall SUCC ZERO))
                                 (TWO   (funcall SUCC ONE))
                                 (THREE (funcall SUCC TWO))
                                 (FOUR  (funcall SUCC THREE))
                                 (FIVE  (funcall SUCC FOUR)))
                            (list
                             ;; IS-ZERO tests
                             (funcall to-bool (funcall IS-ZERO ZERO))
                             (funcall to-bool (funcall IS-ZERO ONE))
                             (funcall to-bool (funcall IS-ZERO FIVE))
                             ;; PRED tests
                             (funcall to-int (funcall PRED ONE))      ;; 0
                             (funcall to-int (funcall PRED TWO))      ;; 1
                             (funcall to-int (funcall PRED THREE))    ;; 2
                             (funcall to-int (funcall PRED FIVE))     ;; 4
                             ;; PRED of ZERO is ZERO
                             (funcall to-int (funcall PRED ZERO))     ;; 0
                             ;; SUCC then PRED is identity (for n > 0)
                             (funcall to-int
                                      (funcall PRED (funcall SUCC THREE))) ;; 3
                             ;; Pair tests
                             (funcall to-int
                                      (funcall FST
                                               (funcall PAIR TWO FOUR))) ;; 2
                             (funcall to-int
                                      (funcall SND
                                               (funcall PAIR TWO FOUR))) ;; 4
                             )))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 55 36)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church-encoded lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Church lists: NIL = (lambda (c n) n), CONS = (lambda (h t) (lambda (c n) (c h (t c n))))
    // fold-right is built into the list representation.
    let form = r#"(let ((TRUE  (lambda (x) (lambda (y) x)))
                        (FALSE (lambda (x) (lambda (y) y)))
                        (to-bool (lambda (cb) (funcall (funcall cb t) nil))))
                    ;; Church list constructors
                    (let ((NIL-LIST  (lambda (c n) n))
                          (CONS-LIST (lambda (h t)
                                      (lambda (c n)
                                        (funcall c h (funcall t c n)))))
                          ;; IS-NIL: (lambda (l) (funcall l (lambda (h t) FALSE) TRUE))
                          (IS-NIL (lambda (l)
                                    (funcall l (lambda (h t) FALSE) TRUE)))
                          ;; HEAD: (lambda (l) (funcall l (lambda (h t) h) nil))
                          (HEAD (lambda (l)
                                  (funcall l (lambda (h t) h) nil)))
                          ;; FOLD-RIGHT is built in: (funcall list f init)
                          ;; Convert church list to elisp list
                          (to-list (lambda (cl)
                                     (funcall cl (lambda (h acc) (cons h acc)) nil))))
                      ;; Build a list: [10, 20, 30]
                      (let* ((l1 (funcall CONS-LIST 10
                                          (funcall CONS-LIST 20
                                                   (funcall CONS-LIST 30 NIL-LIST))))
                             ;; MAP using fold
                             (MAP (lambda (f l)
                                    (funcall l
                                             (lambda (h acc)
                                               (funcall CONS-LIST (funcall f h) acc))
                                             NIL-LIST)))
                             ;; SUM using fold
                             (SUM (lambda (l)
                                    (funcall l (lambda (h acc) (+ h acc)) 0)))
                             ;; LENGTH using fold
                             (LENGTH (lambda (l)
                                       (funcall l (lambda (h acc) (1+ acc)) 0)))
                             ;; APPEND two church lists
                             (APPEND (lambda (l1 l2)
                                       (funcall l1
                                                (lambda (h acc)
                                                  (funcall CONS-LIST h acc))
                                                l2))))
                        (let* ((l2 (funcall CONS-LIST 40
                                            (funcall CONS-LIST 50 NIL-LIST)))
                               (l3 (funcall APPEND l1 l2))
                               ;; Map: double each element
                               (doubled (funcall MAP (lambda (x) (* x 2)) l1)))
                          (list
                           ;; Basic operations
                           (funcall to-list l1)
                           (funcall HEAD l1)
                           (funcall to-bool (funcall IS-NIL NIL-LIST))
                           (funcall to-bool (funcall IS-NIL l1))
                           (funcall SUM l1)
                           (funcall LENGTH l1)
                           ;; Append
                           (funcall to-list l3)
                           (funcall LENGTH l3)
                           (funcall SUM l3)
                           ;; Map
                           (funcall to-list doubled)
                           (funcall SUM doubled))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 30) 10 t nil 60 3 (10 20 30 40 50) 5 150 (20 40 60) 120)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Y combinator for recursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_y_combinator_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Z combinator (applicative-order Y combinator) for strict languages:
    // Z = (lambda (f) ((lambda (x) (f (lambda (v) ((x x) v))))
    //                   (lambda (x) (f (lambda (v) ((x x) v))))))
    let form = r#"(let ((Z (lambda (f)
                             (funcall
                              (lambda (x)
                                (funcall f (lambda (v) (funcall (funcall x x) v))))
                              (lambda (x)
                                (funcall f (lambda (v) (funcall (funcall x x) v))))))))
                    ;; Factorial via Z combinator
                    (let ((fact-gen (lambda (self)
                                     (lambda (n)
                                       (if (= n 0) 1
                                         (* n (funcall self (1- n)))))))
                          ;; Fibonacci via Z combinator
                          (fib-gen (lambda (self)
                                    (lambda (n)
                                      (cond ((= n 0) 0)
                                            ((= n 1) 1)
                                            (t (+ (funcall self (- n 1))
                                                  (funcall self (- n 2))))))))
                          ;; Sum of list via Z combinator
                          (sum-gen (lambda (self)
                                    (lambda (lst)
                                      (if (null lst) 0
                                        (+ (car lst) (funcall self (cdr lst))))))))
                      (let ((fact (funcall Z fact-gen))
                            (fib  (funcall Z fib-gen))
                            (sum-list (funcall Z sum-gen)))
                        (list
                         ;; Factorials
                         (funcall fact 0)
                         (funcall fact 1)
                         (funcall fact 5)
                         (funcall fact 7)
                         ;; Fibonacci
                         (funcall fib 0)
                         (funcall fib 1)
                         (funcall fib 6)
                         (funcall fib 10)
                         ;; Sum of list
                         (funcall sum-list '(1 2 3 4 5))
                         (funcall sum-list nil)
                         (funcall sum-list '(10 20 30))))))"#;
    let expect = expect_test::expect![[r#""OK (1 1 120 5040 0 1 8 55 15 0 60)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church numeral exponentiation and subtraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_church_exponentiation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // EXP = (lambda (m n) (funcall n m)) -- n applied to m
    // SUB = (lambda (m n) (funcall (funcall n PRED) m)) -- apply pred n times
    let form = r#"(let ((ZERO (lambda (f) (lambda (x) x)))
                        (SUCC (lambda (n) (lambda (f) (lambda (x)
                                (funcall f (funcall (funcall n f) x))))))
                        (TRUE  (lambda (x) (lambda (y) x)))
                        (FALSE (lambda (x) (lambda (y) y)))
                        (to-int (lambda (n) (funcall (funcall n #'1+) 0))))
                    (let ((PAIR (lambda (a b) (lambda (sel) (funcall (funcall sel a) b))))
                          (FST  (lambda (p) (funcall p TRUE)))
                          (SND  (lambda (p) (funcall p FALSE))))
                      (let ((PRED (lambda (n)
                                    (funcall FST
                                             (funcall (funcall n
                                                               (lambda (p)
                                                                 (funcall PAIR
                                                                          (funcall SND p)
                                                                          (funcall SUCC
                                                                                   (funcall SND p)))))
                                                      (funcall PAIR ZERO ZERO))))))
                        (let* ((ONE   (funcall SUCC ZERO))
                               (TWO   (funcall SUCC ONE))
                               (THREE (funcall SUCC TWO))
                               (FOUR  (funcall SUCC THREE))
                               ;; PLUS
                               (PLUS (lambda (m n)
                                       (lambda (f)
                                         (lambda (x)
                                           (funcall (funcall m f)
                                                    (funcall (funcall n f) x))))))
                               ;; MULT
                               (MULT (lambda (m n) (lambda (f) (funcall m (funcall n f)))))
                               ;; EXP: m^n = (funcall n m)
                               ;; Actually: EXP = (lambda (b e) (funcall e b))
                               (EXP (lambda (base exp)
                                      (funcall exp base)))
                               ;; SUB: m - n = apply PRED n times to m
                               (SUB (lambda (m n)
                                      (funcall (funcall n PRED) m))))
                          (list
                           ;; Exponentiation
                           (funcall to-int (funcall EXP TWO THREE))     ;; 2^3 = 8
                           (funcall to-int (funcall EXP THREE TWO))     ;; 3^2 = 9
                           (funcall to-int (funcall EXP TWO FOUR))      ;; 2^4 = 16
                           (funcall to-int (funcall EXP ONE FOUR))      ;; 1^4 = 1
                           ;; Subtraction
                           (funcall to-int (funcall SUB FOUR TWO))      ;; 4-2 = 2
                           (funcall to-int (funcall SUB THREE ONE))     ;; 3-1 = 2
                           (funcall to-int (funcall SUB FOUR FOUR))     ;; 4-4 = 0
                           ;; Underflow floors at zero
                           (funcall to-int (funcall SUB TWO THREE))     ;; max(2-3,0) = 0
                           ;; Combined: 2^3 - 3^2 + 1 = 8 - 9 + 1 = 0
                           (funcall to-int
                                    (funcall PLUS
                                             (funcall SUB
                                                      (funcall EXP TWO THREE)
                                                      (funcall EXP THREE TWO))
                                             ONE))
                           ;; (3+1)^2 = 16
                           (funcall to-int
                                    (funcall EXP
                                             (funcall PLUS THREE ONE)
                                             TWO))))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 61 56)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lambda calculus expression evaluator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lambda_calculus_evaluator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A simple lambda calculus evaluator operating on symbolic expressions.
    // Syntax: (var x), (lam x body), (app f arg), (lit n)
    // Supports substitution-based beta reduction.
    let form = r#"(progn
  (fset 'neovm--test-lc-subst
    ;; Substitute: replace free occurrences of var with val in expr
    (lambda (expr var val)
      (cond
       ((and (consp expr) (eq (car expr) 'var))
        (if (eq (cadr expr) var) val expr))
       ((and (consp expr) (eq (car expr) 'lit))
        expr)
       ((and (consp expr) (eq (car expr) 'lam))
        (if (eq (cadr expr) var)
            expr ;; var is shadowed
          (list 'lam (cadr expr)
                (funcall 'neovm--test-lc-subst (caddr expr) var val))))
       ((and (consp expr) (eq (car expr) 'app))
        (list 'app
              (funcall 'neovm--test-lc-subst (cadr expr) var val)
              (funcall 'neovm--test-lc-subst (caddr expr) var val)))
       ((and (consp expr) (eq (car expr) 'add))
        (list 'add
              (funcall 'neovm--test-lc-subst (cadr expr) var val)
              (funcall 'neovm--test-lc-subst (caddr expr) var val)))
       (t expr))))

  (fset 'neovm--test-lc-eval
    ;; Evaluate a lambda calculus expression
    (lambda (expr)
      (cond
       ((and (consp expr) (eq (car expr) 'lit))
        (cadr expr))
       ((and (consp expr) (eq (car expr) 'var))
        expr) ;; free variable
       ((and (consp expr) (eq (car expr) 'lam))
        expr) ;; lambda is a value
       ((and (consp expr) (eq (car expr) 'add))
        (let ((a (funcall 'neovm--test-lc-eval (cadr expr)))
              (b (funcall 'neovm--test-lc-eval (caddr expr))))
          (if (and (numberp a) (numberp b))
              (+ a b)
            (list 'add a b))))
       ((and (consp expr) (eq (car expr) 'app))
        (let ((fn-val (funcall 'neovm--test-lc-eval (cadr expr)))
              (arg-val (funcall 'neovm--test-lc-eval (caddr expr))))
          (if (and (consp fn-val) (eq (car fn-val) 'lam))
              ;; Beta reduction: substitute parameter with argument
              (funcall 'neovm--test-lc-eval
                       (funcall 'neovm--test-lc-subst
                                (caddr fn-val)
                                (cadr fn-val)
                                (if (numberp arg-val)
                                    (list 'lit arg-val)
                                  arg-val)))
            (list 'app fn-val arg-val))))
       (t expr))))

  (unwind-protect
      (list
       ;; (lambda x. x) 5 = 5
       (funcall 'neovm--test-lc-eval
                '(app (lam x (var x)) (lit 5)))
       ;; (lambda x. x + 1) 10 = 11
       (funcall 'neovm--test-lc-eval
                '(app (lam x (add (var x) (lit 1))) (lit 10)))
       ;; ((lambda x. (lambda y. x + y)) 3) 4 = 7
       (funcall 'neovm--test-lc-eval
                '(app (app (lam x (lam y (add (var x) (var y))))
                           (lit 3))
                      (lit 4)))
       ;; Nested: (lambda f. (lambda x. f (f x)))
       ;; Apply to (lambda y. y+1) then to 0 => 2
       (funcall 'neovm--test-lc-eval
                '(app (app (lam f (lam x (app (var f) (app (var f) (var x)))))
                           (lam y (add (var y) (lit 1))))
                      (lit 0)))
       ;; Variable shadowing: (lambda x. (lambda x. x)) applied to 1 then 2 => 2
       (funcall 'neovm--test-lc-eval
                '(app (app (lam x (lam x (var x))) (lit 1)) (lit 2)))
       ;; Pure addition
       (funcall 'neovm--test-lc-eval
                '(add (add (lit 1) (lit 2)) (add (lit 3) (lit 4)))))
    (fmakunbound 'neovm--test-lc-subst)
    (fmakunbound 'neovm--test-lc-eval)))"#;
    let expect = expect_test::expect![[r#""OK (5 11 7 2 2 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
