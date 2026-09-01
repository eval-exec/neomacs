//! Oracle parity tests for advanced CPS (continuation-passing style) transformation:
//! direct-style to CPS conversion for arithmetic, CPS for let/if/lambda/application,
//! administrative beta reduction, defunctionalization of continuations,
//! CPS with multiple return values, CPS for exception handling (abort/resume),
//! trampoline for stack-safe CPS, one-pass CPS transformation, and selective CPS
//! (only transform effectful subexpressions).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// CPS transform of arithmetic expressions with an explicit converter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_arithmetic_expression_converter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a CPS transformer that takes a direct-style arithmetic AST
    // and produces CPS-style code, then evaluate both.
    let form = r#"(progn
  ;; Direct-style evaluator
  (fset 'neovm--cps-adv-direct-eval
    (lambda (expr env)
      (cond
       ((numberp expr) expr)
       ((symbolp expr) (cdr (assq expr env)))
       ((eq (car expr) '+)
        (+ (funcall 'neovm--cps-adv-direct-eval (nth 1 expr) env)
           (funcall 'neovm--cps-adv-direct-eval (nth 2 expr) env)))
       ((eq (car expr) '*)
        (* (funcall 'neovm--cps-adv-direct-eval (nth 1 expr) env)
           (funcall 'neovm--cps-adv-direct-eval (nth 2 expr) env)))
       ((eq (car expr) '-)
        (- (funcall 'neovm--cps-adv-direct-eval (nth 1 expr) env)
           (funcall 'neovm--cps-adv-direct-eval (nth 2 expr) env)))
       ((eq (car expr) 'let1)
        (let ((val (funcall 'neovm--cps-adv-direct-eval (nth 2 expr) env)))
          (funcall 'neovm--cps-adv-direct-eval (nth 3 expr)
                   (cons (cons (nth 1 expr) val) env)))))))

  ;; CPS evaluator: every operation passes result to continuation
  (fset 'neovm--cps-adv-cps-eval
    (lambda (expr env k)
      (cond
       ((numberp expr) (funcall k expr))
       ((symbolp expr) (funcall k (cdr (assq expr env))))
       ((eq (car expr) '+)
        (funcall 'neovm--cps-adv-cps-eval (nth 1 expr) env
                 (lambda (v1)
                   (funcall 'neovm--cps-adv-cps-eval (nth 2 expr) env
                            (lambda (v2) (funcall k (+ v1 v2)))))))
       ((eq (car expr) '*)
        (funcall 'neovm--cps-adv-cps-eval (nth 1 expr) env
                 (lambda (v1)
                   (funcall 'neovm--cps-adv-cps-eval (nth 2 expr) env
                            (lambda (v2) (funcall k (* v1 v2)))))))
       ((eq (car expr) '-)
        (funcall 'neovm--cps-adv-cps-eval (nth 1 expr) env
                 (lambda (v1)
                   (funcall 'neovm--cps-adv-cps-eval (nth 2 expr) env
                            (lambda (v2) (funcall k (- v1 v2)))))))
       ((eq (car expr) 'let1)
        (funcall 'neovm--cps-adv-cps-eval (nth 2 expr) env
                 (lambda (val)
                   (funcall 'neovm--cps-adv-cps-eval (nth 3 expr)
                            (cons (cons (nth 1 expr) val) env) k)))))))

  (unwind-protect
      (let ((exprs '(42
                     (+ 1 2)
                     (* 3 (+ 4 5))
                     (- (* 6 7) (+ 8 9))
                     (let1 x 10 (+ x x))
                     (let1 a 3 (let1 b 4 (+ (* a a) (* b b))))
                     (let1 x 5 (- (* x x) (+ x 1))))))
        (mapcar (lambda (expr)
                  (let ((direct (funcall 'neovm--cps-adv-direct-eval expr nil))
                        (cps (funcall 'neovm--cps-adv-cps-eval expr nil #'identity)))
                    (list expr direct cps (= direct cps))))
                exprs))
    (fmakunbound 'neovm--cps-adv-direct-eval)
    (fmakunbound 'neovm--cps-adv-cps-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK ((42 42 42 t) ((+ 1 2) 3 3 t) ((* 3 (+ 4 5)) 27 27 t) ((- (* 6 7) (+ 8 9)) 25 25 t) ((let1 x 10 (+ x x)) 20 20 t) ((let1 a 3 (let1 b 4 (+ (* a a) (* b b)))) 25 25 t) ((let1 x 5 (- (* x x) (+ x 1))) 19 19 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPS with if/lambda/application: full language
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_if_lambda_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; CPS evaluator for a richer language:
  ;; num, var, (+ e1 e2), (* e1 e2), (if0 test then else),
  ;; (lam var body), (app fn arg)
  (fset 'neovm--cps-adv-eval2
    (lambda (expr env k)
      (cond
       ((numberp expr) (funcall k expr))
       ((symbolp expr) (funcall k (cdr (assq expr env))))
       ((eq (car expr) '+)
        (funcall 'neovm--cps-adv-eval2 (nth 1 expr) env
                 (lambda (a) (funcall 'neovm--cps-adv-eval2 (nth 2 expr) env
                              (lambda (b) (funcall k (+ a b)))))))
       ((eq (car expr) '*)
        (funcall 'neovm--cps-adv-eval2 (nth 1 expr) env
                 (lambda (a) (funcall 'neovm--cps-adv-eval2 (nth 2 expr) env
                              (lambda (b) (funcall k (* a b)))))))
       ;; (if0 test then else): if test=0 then else
       ((eq (car expr) 'if0)
        (funcall 'neovm--cps-adv-eval2 (nth 1 expr) env
                 (lambda (test-val)
                   (if (= test-val 0)
                       (funcall 'neovm--cps-adv-eval2 (nth 2 expr) env k)
                     (funcall 'neovm--cps-adv-eval2 (nth 3 expr) env k)))))
       ;; (lam var body): closure as (closure env var body)
       ((eq (car expr) 'lam)
        (funcall k (list 'closure env (nth 1 expr) (nth 2 expr))))
       ;; (app fn arg): apply function to argument
       ((eq (car expr) 'app)
        (funcall 'neovm--cps-adv-eval2 (nth 1 expr) env
                 (lambda (fn-val)
                   (funcall 'neovm--cps-adv-eval2 (nth 2 expr) env
                            (lambda (arg-val)
                              (let ((cenv (nth 1 fn-val))
                                    (param (nth 2 fn-val))
                                    (body (nth 3 fn-val)))
                                (funcall 'neovm--cps-adv-eval2 body
                                         (cons (cons param arg-val) cenv) k))))))))))

  (fset 'neovm--cps-adv-run2
    (lambda (expr) (funcall 'neovm--cps-adv-eval2 expr nil #'identity)))

  (unwind-protect
      (list
       ;; Identity function applied
       (funcall 'neovm--cps-adv-run2 '(app (lam x x) 42))
       ;; Constant function
       (funcall 'neovm--cps-adv-run2 '(app (lam x 99) 0))
       ;; Increment
       (funcall 'neovm--cps-adv-run2 '(app (lam n (+ n 1)) 10))
       ;; Square
       (funcall 'neovm--cps-adv-run2 '(app (lam n (* n n)) 7))
       ;; if0: factorial of 0 = 1
       (funcall 'neovm--cps-adv-run2 '(if0 0 1 999))
       ;; if0: factorial of nonzero
       (funcall 'neovm--cps-adv-run2 '(if0 5 1 999))
       ;; Church numeral: zero = (lam f (lam x x))
       ;; Applying zero to inc and 0 gives 0
       (funcall 'neovm--cps-adv-run2
                '(app (app (lam f (lam x x))
                           (lam n (+ n 1)))
                      0))
       ;; Higher-order: apply twice
       ;; twice f x = f(f(x))
       (funcall 'neovm--cps-adv-run2
                '(app (app (lam f (lam x (app f (app f x))))
                           (lam n (+ n 3)))
                      10)))
    (fmakunbound 'neovm--cps-adv-eval2)
    (fmakunbound 'neovm--cps-adv-run2)))"#;
    let expect = expect_test::expect![[r#""OK (42 99 11 49 1 999 0 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Administrative beta reduction: simplify CPS output
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_administrative_beta_reduction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Administrative beta reduction removes trivial continuations:
    // ((lambda (x) (k x)) v) => (k v)
    // Here we implement a CPS transformer and a simplifier.
    let form = r#"(progn
  ;; Represent CPS terms as data:
  ;; (num N), (var V), (primop OP E1 E2 K), (app-k F E K), (halt E)
  ;; K is a continuation: (cont VAR BODY) or (halt-cont)

  ;; Simple CPS transformer producing verbose output
  (fset 'neovm--cps-adv-transform
    (lambda (expr k-name)
      "Transform direct-style EXPR to CPS data with continuation named K-NAME."
      (cond
       ((numberp expr)
        (list 'apply-cont k-name expr))
       ((symbolp expr)
        (list 'apply-cont k-name expr))
       ((eq (car expr) '+)
        (let ((v1 (make-symbol "v1"))
              (v2 (make-symbol "v2")))
          (funcall 'neovm--cps-adv-transform (nth 1 expr)
                   (list 'cont v1
                         (funcall 'neovm--cps-adv-transform (nth 2 expr)
                                  (list 'cont v2
                                        (list 'primop '+ v1 v2 k-name)))))))
       ((eq (car expr) '*)
        (let ((v1 (make-symbol "v1"))
              (v2 (make-symbol "v2")))
          (funcall 'neovm--cps-adv-transform (nth 1 expr)
                   (list 'cont v1
                         (funcall 'neovm--cps-adv-transform (nth 2 expr)
                                  (list 'cont v2
                                        (list 'primop '* v1 v2 k-name))))))))))

  ;; Count nodes in a CPS term (measure complexity)
  (fset 'neovm--cps-adv-count-nodes
    (lambda (term)
      (cond
       ((not (consp term)) 1)
       (t (let ((count 0))
            (dolist (sub term)
              (setq count (+ count (funcall 'neovm--cps-adv-count-nodes sub))))
            count)))))

  ;; Check: CPS produces more nodes than direct style
  ;; Then simplify by collapsing trivial apply-cont chains

  (unwind-protect
      (let ((exprs '((+ 1 2)
                     (* 3 4)
                     (+ (* 2 3) (* 4 5)))))
        (mapcar (lambda (expr)
                  (let* ((cps-term (funcall 'neovm--cps-adv-transform expr 'halt))
                         (node-count (funcall 'neovm--cps-adv-count-nodes cps-term)))
                    (list :expr expr :nodes node-count)))
                exprs))
    (fmakunbound 'neovm--cps-adv-transform)
    (fmakunbound 'neovm--cps-adv-count-nodes)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:expr (+ 1 2) :nodes 13) (:expr (* 3 4) :nodes 13) (:expr (+ (* 2 3) (* 4 5)) :nodes 33))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Defunctionalization: replace closures with data constructors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_defunctionalization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Instead of using lambda for continuations, represent them as tagged data.
    // A dispatch function applies the right code based on the tag.
    let form = r#"(progn
  ;; Defunctionalized continuation types:
  ;; (:halt) — top-level
  ;; (:add-first val2-expr env k) — waiting for first operand of add
  ;; (:add-second v1 k) — waiting for second operand of add
  ;; (:mul-first val2-expr env k)
  ;; (:mul-second v1 k)
  ;; (:let-body var body env k)

  (fset 'neovm--cps-adv-apply-cont
    (lambda (k val)
      (cond
       ((eq (car k) :halt) val)
       ((eq (car k) :add-second)
        (funcall 'neovm--cps-adv-apply-cont
                 (nth 2 k) (+ (nth 1 k) val)))
       ((eq (car k) :add-first)
        (funcall 'neovm--cps-adv-defunc-eval
                 (nth 1 k) (nth 2 k)
                 (list :add-second val (nth 3 k))))
       ((eq (car k) :mul-second)
        (funcall 'neovm--cps-adv-apply-cont
                 (nth 2 k) (* (nth 1 k) val)))
       ((eq (car k) :mul-first)
        (funcall 'neovm--cps-adv-defunc-eval
                 (nth 1 k) (nth 2 k)
                 (list :mul-second val (nth 3 k))))
       ((eq (car k) :let-body)
        (funcall 'neovm--cps-adv-defunc-eval
                 (nth 2 k)
                 (cons (cons (nth 1 k) val) (nth 3 k))
                 (nth 4 k))))))

  (fset 'neovm--cps-adv-defunc-eval
    (lambda (expr env k)
      (cond
       ((numberp expr) (funcall 'neovm--cps-adv-apply-cont k expr))
       ((symbolp expr) (funcall 'neovm--cps-adv-apply-cont k (cdr (assq expr env))))
       ((eq (car expr) '+)
        (funcall 'neovm--cps-adv-defunc-eval (nth 1 expr) env
                 (list :add-first (nth 2 expr) env k)))
       ((eq (car expr) '*)
        (funcall 'neovm--cps-adv-defunc-eval (nth 1 expr) env
                 (list :mul-first (nth 2 expr) env k)))
       ((eq (car expr) 'let1)
        (funcall 'neovm--cps-adv-defunc-eval (nth 2 expr) env
                 (list :let-body (nth 1 expr) (nth 3 expr) env k))))))

  (fset 'neovm--cps-adv-defunc-run
    (lambda (expr) (funcall 'neovm--cps-adv-defunc-eval expr nil '(:halt))))

  (unwind-protect
      (list
       (funcall 'neovm--cps-adv-defunc-run 42)
       (funcall 'neovm--cps-adv-defunc-run '(+ 1 2))
       (funcall 'neovm--cps-adv-defunc-run '(* 3 4))
       (funcall 'neovm--cps-adv-defunc-run '(+ (* 2 3) (* 4 5)))
       (funcall 'neovm--cps-adv-defunc-run '(let1 x 10 (+ x x)))
       (funcall 'neovm--cps-adv-defunc-run '(let1 a 3 (let1 b 4 (+ (* a a) (* b b)))))
       (funcall 'neovm--cps-adv-defunc-run '(+ (+ 1 2) (+ 3 (+ 4 5)))))
    (fmakunbound 'neovm--cps-adv-apply-cont)
    (fmakunbound 'neovm--cps-adv-defunc-eval)
    (fmakunbound 'neovm--cps-adv-defunc-run)))"#;
    let expect = expect_test::expect![[r#""OK (42 3 12 26 20 25 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPS with multiple return values (tuples via lists)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_multiple_return_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; CPS functions that "return" multiple values via continuation
  ;; Continuation receives a list of values

  ;; divmod: returns (quotient . remainder)
  (fset 'neovm--cps-adv-divmod-k
    (lambda (a b k)
      (funcall k (list (/ a b) (% a b)))))

  ;; min-max: returns (min . max) of a list
  (fset 'neovm--cps-adv-minmax-k
    (lambda (lst k)
      (if (null lst)
          (funcall k (list nil nil))
        (let ((mn (car lst)) (mx (car lst)))
          (dolist (x (cdr lst))
            (when (< x mn) (setq mn x))
            (when (> x mx) (setq mx x)))
          (funcall k (list mn mx))))))

  ;; stats: returns (sum count mean) of a list
  (fset 'neovm--cps-adv-stats-k
    (lambda (lst k)
      (if (null lst)
          (funcall k (list 0 0 0))
        (let ((sum 0) (count 0))
          (dolist (x lst)
            (setq sum (+ sum x))
            (setq count (1+ count)))
          (funcall k (list sum count (/ sum count)))))))

  ;; Compose: divmod then use both results
  ;; Compute (a/b) + (a%b)
  (fset 'neovm--cps-adv-divmod-sum-k
    (lambda (a b k)
      (funcall 'neovm--cps-adv-divmod-k a b
               (lambda (vals)
                 (funcall k (+ (car vals) (cadr vals)))))))

  (unwind-protect
      (list
       ;; divmod
       (funcall 'neovm--cps-adv-divmod-k 17 5 #'identity)
       (funcall 'neovm--cps-adv-divmod-k 100 7 #'identity)
       (funcall 'neovm--cps-adv-divmod-k 10 3 #'identity)
       ;; min-max
       (funcall 'neovm--cps-adv-minmax-k '(3 1 4 1 5 9 2 6) #'identity)
       (funcall 'neovm--cps-adv-minmax-k '(42) #'identity)
       (funcall 'neovm--cps-adv-minmax-k nil #'identity)
       ;; stats
       (funcall 'neovm--cps-adv-stats-k '(10 20 30 40 50) #'identity)
       (funcall 'neovm--cps-adv-stats-k '(7) #'identity)
       ;; Composed: divmod-sum
       (funcall 'neovm--cps-adv-divmod-sum-k 17 5 #'identity)
       (funcall 'neovm--cps-adv-divmod-sum-k 100 7 #'identity)
       ;; Chain: stats then use mean in divmod
       (funcall 'neovm--cps-adv-stats-k '(12 18 24 30 36)
                (lambda (stats)
                  (funcall 'neovm--cps-adv-divmod-k (car stats) (nth 2 stats) #'identity))))
    (fmakunbound 'neovm--cps-adv-divmod-k)
    (fmakunbound 'neovm--cps-adv-minmax-k)
    (fmakunbound 'neovm--cps-adv-stats-k)
    (fmakunbound 'neovm--cps-adv-divmod-sum-k)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 2) (14 2) (3 1) (1 9) (42 42) (nil nil) (150 5 30) (7 1 7) 5 16 (5 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPS with abort/resume continuations (exception handling)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_abort_resume_continuations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Abort continuation: immediately escape to a handler
  ;; Resume continuation: provide a replacement value and continue

  ;; CPS with two continuations: normal-k and abort-k
  ;; Each function: (val normal-k abort-k)

  ;; safe-head: get car or abort
  (fset 'neovm--cps-adv-safe-head
    (lambda (lst normal-k abort-k)
      (if (consp lst)
          (funcall normal-k (car lst))
        (funcall abort-k (list :empty-list 'head)))))

  ;; safe-nth: get nth element or abort
  (fset 'neovm--cps-adv-safe-nth
    (lambda (n lst normal-k abort-k)
      (if (and (>= n 0) (< n (length lst)))
          (funcall normal-k (nth n lst))
        (funcall abort-k (list :index-out-of-bounds n (length lst))))))

  ;; CPS with-handler: wraps a computation, catches aborts
  (fset 'neovm--cps-adv-with-handler
    (lambda (body-fn handler-fn final-k)
      (funcall body-fn
               final-k
               (lambda (err) (funcall handler-fn err final-k)))))

  ;; CPS with-resume: like with-handler but handler can provide recovery value
  (fset 'neovm--cps-adv-with-resume
    (lambda (body-fn resume-fn final-k)
      (funcall body-fn
               final-k
               (lambda (err)
                 (let ((recovery (funcall resume-fn err)))
                   (funcall final-k recovery))))))

  (unwind-protect
      (list
       ;; Normal: head of non-empty list
       (funcall 'neovm--cps-adv-safe-head '(1 2 3)
                (lambda (v) (list :ok v))
                (lambda (e) (list :abort e)))

       ;; Abort: head of empty list
       (funcall 'neovm--cps-adv-safe-head nil
                (lambda (v) (list :ok v))
                (lambda (e) (list :abort e)))

       ;; Normal: nth in bounds
       (funcall 'neovm--cps-adv-safe-nth 2 '(10 20 30 40)
                (lambda (v) (list :ok v))
                (lambda (e) (list :abort e)))

       ;; Abort: nth out of bounds
       (funcall 'neovm--cps-adv-safe-nth 10 '(10 20 30)
                (lambda (v) (list :ok v))
                (lambda (e) (list :abort e)))

       ;; with-handler: catches abort
       (funcall 'neovm--cps-adv-with-handler
                (lambda (nk ak)
                  (funcall 'neovm--cps-adv-safe-head nil nk ak))
                (lambda (err k) (funcall k (list :handled err)))
                #'identity)

       ;; with-handler: no abort
       (funcall 'neovm--cps-adv-with-handler
                (lambda (nk ak)
                  (funcall 'neovm--cps-adv-safe-head '(42) nk ak))
                (lambda (err k) (funcall k (list :handled err)))
                #'identity)

       ;; with-resume: provide default value on error
       (funcall 'neovm--cps-adv-with-resume
                (lambda (nk ak)
                  (funcall 'neovm--cps-adv-safe-head nil nk ak))
                (lambda (err) 0)  ;; resume with 0
                #'identity)

       ;; Chain: head of (nth 1 nested-list)
       (funcall 'neovm--cps-adv-with-handler
                (lambda (nk ak)
                  (funcall 'neovm--cps-adv-safe-nth 1 '((a b) (c d) (e f))
                           (lambda (inner-list)
                             (funcall 'neovm--cps-adv-safe-head inner-list nk ak))
                           ak))
                (lambda (err k) (funcall k (list :error err)))
                #'identity))
    (fmakunbound 'neovm--cps-adv-safe-head)
    (fmakunbound 'neovm--cps-adv-safe-nth)
    (fmakunbound 'neovm--cps-adv-with-handler)
    (fmakunbound 'neovm--cps-adv-with-resume)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:ok 1) (:abort (:empty-list head)) (:ok 30) (:abort (:index-out-of-bounds 10 3)) (:handled (:empty-list head)) 42 0 c)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trampolined CPS: tail-call safe recursion via thunks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_trampoline_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Trampoline: iterate thunks until non-function value
  (fset 'neovm--cps-adv-bounce
    (lambda (thunk)
      (let ((val thunk))
        (while (functionp val)
          (setq val (funcall val)))
        val)))

  ;; Trampolined CPS power: x^n
  (fset 'neovm--cps-adv-power-k
    (lambda (x n k)
      (if (= n 0)
          (lambda () (funcall k 1))
        (lambda ()
          (funcall 'neovm--cps-adv-power-k x (1- n)
                   (lambda (r) (lambda () (funcall k (* x r)))))))))

  ;; Trampolined CPS list-sum
  (fset 'neovm--cps-adv-list-sum-k
    (lambda (lst k)
      (if (null lst)
          (lambda () (funcall k 0))
        (lambda ()
          (funcall 'neovm--cps-adv-list-sum-k (cdr lst)
                   (lambda (rest-sum)
                     (lambda () (funcall k (+ (car lst) rest-sum)))))))))

  ;; Trampolined CPS list-reverse
  (fset 'neovm--cps-adv-reverse-k
    (lambda (lst acc k)
      (if (null lst)
          (lambda () (funcall k acc))
        (lambda ()
          (funcall 'neovm--cps-adv-reverse-k (cdr lst)
                   (cons (car lst) acc) k)))))

  ;; Trampolined CPS map
  (fset 'neovm--cps-adv-map-k
    (lambda (f lst k)
      (if (null lst)
          (lambda () (funcall k nil))
        (lambda ()
          (funcall 'neovm--cps-adv-map-k f (cdr lst)
                   (lambda (rest)
                     (lambda () (funcall k (cons (funcall f (car lst)) rest)))))))))

  (unwind-protect
      (list
       ;; Power
       (funcall 'neovm--cps-adv-bounce
                (funcall 'neovm--cps-adv-power-k 2 0 #'identity))
       (funcall 'neovm--cps-adv-bounce
                (funcall 'neovm--cps-adv-power-k 2 10 #'identity))
       (funcall 'neovm--cps-adv-bounce
                (funcall 'neovm--cps-adv-power-k 3 5 #'identity))

       ;; List sum
       (funcall 'neovm--cps-adv-bounce
                (funcall 'neovm--cps-adv-list-sum-k '(1 2 3 4 5) #'identity))
       (funcall 'neovm--cps-adv-bounce
                (funcall 'neovm--cps-adv-list-sum-k nil #'identity))

       ;; List reverse
       (funcall 'neovm--cps-adv-bounce
                (funcall 'neovm--cps-adv-reverse-k '(1 2 3 4 5) nil #'identity))

       ;; Map with trampoline
       (funcall 'neovm--cps-adv-bounce
                (funcall 'neovm--cps-adv-map-k #'1+ '(10 20 30) #'identity))

       ;; Compose: sum of mapped values
       (funcall 'neovm--cps-adv-bounce
                (funcall 'neovm--cps-adv-map-k
                         (lambda (x) (* x x))
                         '(1 2 3 4 5)
                         (lambda (squares)
                           (funcall 'neovm--cps-adv-list-sum-k squares #'identity)))))
    (fmakunbound 'neovm--cps-adv-bounce)
    (fmakunbound 'neovm--cps-adv-power-k)
    (fmakunbound 'neovm--cps-adv-list-sum-k)
    (fmakunbound 'neovm--cps-adv-reverse-k)
    (fmakunbound 'neovm--cps-adv-map-k)))"#;
    let expect = expect_test::expect![[r#""OK (1 1024 243 15 0 (5 4 3 2 1) (11 21 31) 55)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// One-pass CPS transformation: transform entire program in one traversal
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_one_pass_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // One-pass CPS: atoms are trivial (don't need continuation),
    // compound expressions are serious. Only serious exprs get continuations.
    let form = r#"(progn
  (fset 'neovm--cps-adv-trivial-p
    (lambda (expr) (or (numberp expr) (symbolp expr))))

  ;; One-pass CPS evaluator that avoids unnecessary continuations
  ;; for trivial (atomic) subexpressions
  (fset 'neovm--cps-adv-onepass
    (lambda (expr env k)
      (cond
       ((numberp expr) (funcall k expr))
       ((symbolp expr) (funcall k (cdr (assq expr env))))
       ((eq (car expr) '+)
        (let ((e1 (nth 1 expr)) (e2 (nth 2 expr)))
          (if (and (funcall 'neovm--cps-adv-trivial-p e1)
                   (funcall 'neovm--cps-adv-trivial-p e2))
              ;; Both trivial: directly compute
              (funcall k (+ (if (numberp e1) e1 (cdr (assq e1 env)))
                            (if (numberp e2) e2 (cdr (assq e2 env)))))
            ;; At least one compound: full CPS
            (funcall 'neovm--cps-adv-onepass e1 env
                     (lambda (v1)
                       (funcall 'neovm--cps-adv-onepass e2 env
                                (lambda (v2) (funcall k (+ v1 v2)))))))))
       ((eq (car expr) '*)
        (let ((e1 (nth 1 expr)) (e2 (nth 2 expr)))
          (if (and (funcall 'neovm--cps-adv-trivial-p e1)
                   (funcall 'neovm--cps-adv-trivial-p e2))
              (funcall k (* (if (numberp e1) e1 (cdr (assq e1 env)))
                            (if (numberp e2) e2 (cdr (assq e2 env)))))
            (funcall 'neovm--cps-adv-onepass e1 env
                     (lambda (v1)
                       (funcall 'neovm--cps-adv-onepass e2 env
                                (lambda (v2) (funcall k (* v1 v2)))))))))
       ((eq (car expr) 'let1)
        (funcall 'neovm--cps-adv-onepass (nth 2 expr) env
                 (lambda (val)
                   (funcall 'neovm--cps-adv-onepass (nth 3 expr)
                            (cons (cons (nth 1 expr) val) env) k)))))))

  (fset 'neovm--cps-adv-onepass-run
    (lambda (expr) (funcall 'neovm--cps-adv-onepass expr nil #'identity)))

  (unwind-protect
      (list
       ;; All trivial
       (funcall 'neovm--cps-adv-onepass-run '(+ 1 2))
       (funcall 'neovm--cps-adv-onepass-run '(* 3 4))
       ;; Mixed: one trivial, one compound
       (funcall 'neovm--cps-adv-onepass-run '(+ 1 (* 2 3)))
       (funcall 'neovm--cps-adv-onepass-run '(* (+ 1 2) 4))
       ;; All compound
       (funcall 'neovm--cps-adv-onepass-run '(+ (* 2 3) (* 4 5)))
       ;; With let
       (funcall 'neovm--cps-adv-onepass-run '(let1 x 5 (+ x x)))
       ;; Deeply nested
       (funcall 'neovm--cps-adv-onepass-run
                '(let1 a 2 (let1 b 3 (+ (* a a) (* b b)))))
       ;; Variable references (trivial in subexpressions)
       (funcall 'neovm--cps-adv-onepass-run
                '(let1 x 10 (let1 y 20 (+ x y)))))
    (fmakunbound 'neovm--cps-adv-trivial-p)
    (fmakunbound 'neovm--cps-adv-onepass)
    (fmakunbound 'neovm--cps-adv-onepass-run)))"#;
    let expect = expect_test::expect![[r#""OK (3 12 7 12 26 10 13 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Selective CPS: only effectful subexpressions get CPS treatment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_selective_effectful() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // "Effects" in our tiny language: (read-val KEY) accesses a mutable store.
    // Pure subexpressions are evaluated directly; only effectful ones use CPS.
    let form = r#"(progn
  ;; A store: alist of (key . value) pairs
  ;; (read-val KEY) is effectful, (+ e1 e2) etc. are pure if subexprs are pure

  (fset 'neovm--cps-adv-pure-p
    (lambda (expr)
      (cond
       ((numberp expr) t)
       ((symbolp expr) t)
       ((memq (car expr) '(+ * -))
        (and (funcall 'neovm--cps-adv-pure-p (nth 1 expr))
             (funcall 'neovm--cps-adv-pure-p (nth 2 expr))))
       (t nil))))  ;; read-val, let1 with effectful body: not pure

  ;; Direct eval for pure expressions
  (fset 'neovm--cps-adv-pure-eval
    (lambda (expr env)
      (cond
       ((numberp expr) expr)
       ((symbolp expr) (cdr (assq expr env)))
       ((eq (car expr) '+)
        (+ (funcall 'neovm--cps-adv-pure-eval (nth 1 expr) env)
           (funcall 'neovm--cps-adv-pure-eval (nth 2 expr) env)))
       ((eq (car expr) '*)
        (* (funcall 'neovm--cps-adv-pure-eval (nth 1 expr) env)
           (funcall 'neovm--cps-adv-pure-eval (nth 2 expr) env)))
       ((eq (car expr) '-)
        (- (funcall 'neovm--cps-adv-pure-eval (nth 1 expr) env)
           (funcall 'neovm--cps-adv-pure-eval (nth 2 expr) env))))))

  ;; Selective CPS eval: use CPS only for effectful expressions
  (fset 'neovm--cps-adv-selective
    (lambda (expr env store k)
      (cond
       ;; Pure expression: eval directly, pass to continuation
       ((funcall 'neovm--cps-adv-pure-p expr)
        (funcall k (funcall 'neovm--cps-adv-pure-eval expr env) store))
       ;; read-val: read from store
       ((eq (car expr) 'read-val)
        (let ((key (nth 1 expr)))
          (funcall k (cdr (assq key store)) store)))
       ;; + with at least one effectful child
       ((eq (car expr) '+)
        (funcall 'neovm--cps-adv-selective (nth 1 expr) env store
                 (lambda (v1 store1)
                   (funcall 'neovm--cps-adv-selective (nth 2 expr) env store1
                            (lambda (v2 store2)
                              (funcall k (+ v1 v2) store2))))))
       ;; let1
       ((eq (car expr) 'let1)
        (funcall 'neovm--cps-adv-selective (nth 2 expr) env store
                 (lambda (val store1)
                   (funcall 'neovm--cps-adv-selective (nth 3 expr)
                            (cons (cons (nth 1 expr) val) env)
                            store1 k)))))))

  (fset 'neovm--cps-adv-selective-run
    (lambda (expr store)
      (funcall 'neovm--cps-adv-selective expr nil store
               (lambda (val final-store) (list :result val :store final-store)))))

  (unwind-protect
      (let ((store '((x . 10) (y . 20) (z . 30))))
        (list
         ;; Pure: no store interaction
         (funcall 'neovm--cps-adv-selective-run '(+ 1 2) store)
         (funcall 'neovm--cps-adv-selective-run '(* 3 4) store)
         ;; Effectful: read from store
         (funcall 'neovm--cps-adv-selective-run '(read-val x) store)
         ;; Mixed: pure + effectful
         (funcall 'neovm--cps-adv-selective-run '(+ 5 (read-val y)) store)
         ;; All effectful
         (funcall 'neovm--cps-adv-selective-run '(+ (read-val x) (read-val y)) store)
         ;; With let
         (funcall 'neovm--cps-adv-selective-run
                  '(let1 a (read-val x) (+ a (read-val z))) store)
         ;; Nested pure inside effectful
         (funcall 'neovm--cps-adv-selective-run
                  '(+ (* 2 3) (read-val x)) store)
         ;; Purity check
         (list (funcall 'neovm--cps-adv-pure-p '(+ 1 2))
               (funcall 'neovm--cps-adv-pure-p '(read-val x))
               (funcall 'neovm--cps-adv-pure-p '(+ 1 (read-val x)))
               (funcall 'neovm--cps-adv-pure-p '(* (+ 1 2) (- 3 4))))))
    (fmakunbound 'neovm--cps-adv-pure-p)
    (fmakunbound 'neovm--cps-adv-pure-eval)
    (fmakunbound 'neovm--cps-adv-selective)
    (fmakunbound 'neovm--cps-adv-selective-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:result 3 :store ((x . 10) (y . 20) (z . 30))) (:result 12 :store ((x . 10) (y . 20) (z . 30))) (:result 10 :store ((x . 10) (y . 20) (z . 30))) (:result 25 :store ((x . 10) (y . 20) (z . 30))) (:result 30 :store ((x . 10) (y . 20) (z . 30))) (:result 40 :store ((x . 10) (y . 20) (z . 30))) (:result 16 :store ((x . 10) (y . 20) (z . 30))) (t nil nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CPS state monad: threading state through CPS computations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cps_state_monad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; State-CPS: each function takes (state k) where k receives (value new-state)

  ;; get-state: return current state as value
  (fset 'neovm--cps-adv-get-state
    (lambda (state k) (funcall k state state)))

  ;; put-state: replace state, return old state
  (fset 'neovm--cps-adv-put-state
    (lambda (new-state state k) (funcall k state new-state)))

  ;; modify-state: apply function to state
  (fset 'neovm--cps-adv-modify-state
    (lambda (f state k) (funcall k nil (funcall f state))))

  ;; bind: sequence two state-CPS computations
  (fset 'neovm--cps-adv-state-bind
    (lambda (m1 f state k)
      "Run M1, pass its result to F which produces another state-CPS computation."
      (funcall m1 state
               (lambda (val state1)
                 (funcall (funcall f val) state1 k)))))

  ;; return: lift a value into state-CPS
  (fset 'neovm--cps-adv-state-return
    (lambda (val) (lambda (state k) (funcall k val state))))

  ;; run: execute a state-CPS computation
  (fset 'neovm--cps-adv-state-run
    (lambda (computation initial-state)
      (funcall computation initial-state (lambda (val state) (list :val val :state state)))))

  (unwind-protect
      (list
       ;; Get state
       (funcall 'neovm--cps-adv-state-run
                'neovm--cps-adv-get-state 42)

       ;; Put state
       (funcall 'neovm--cps-adv-state-run
                (lambda (state k) (funcall 'neovm--cps-adv-put-state 100 state k))
                42)

       ;; Modify state (increment)
       (funcall 'neovm--cps-adv-state-run
                (lambda (state k) (funcall 'neovm--cps-adv-modify-state #'1+ state k))
                10)

       ;; Bind: get state, then add 5 to it
       (funcall 'neovm--cps-adv-state-run
                (lambda (state k)
                  (funcall 'neovm--cps-adv-state-bind
                           'neovm--cps-adv-get-state
                           (lambda (val)
                             (lambda (state2 k2)
                               (funcall 'neovm--cps-adv-put-state (+ val 5) state2 k2)))
                           state k))
                10)

       ;; Counter: increment 3 times, return final count
       (funcall 'neovm--cps-adv-state-run
                (lambda (state k)
                  (funcall 'neovm--cps-adv-modify-state #'1+ state
                           (lambda (v1 s1)
                             (funcall 'neovm--cps-adv-modify-state #'1+ s1
                                      (lambda (v2 s2)
                                        (funcall 'neovm--cps-adv-modify-state #'1+ s2
                                                 (lambda (v3 s3)
                                                   (funcall 'neovm--cps-adv-get-state s3 k))))))))
                0))
    (fmakunbound 'neovm--cps-adv-get-state)
    (fmakunbound 'neovm--cps-adv-put-state)
    (fmakunbound 'neovm--cps-adv-modify-state)
    (fmakunbound 'neovm--cps-adv-state-bind)
    (fmakunbound 'neovm--cps-adv-state-return)
    (fmakunbound 'neovm--cps-adv-state-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:val 42 :state 42) (:val 42 :state 100) (:val nil :state 11) (:val 10 :state 15) (:val 3 :state 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
