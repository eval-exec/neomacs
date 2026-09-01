//! Oracle parity tests for `eval`, `apply`, `funcall` with complex patterns:
//! quoted forms, dynamically constructed code, variable argument lists,
//! funcall vs apply differences, meta-circular evaluation, function
//! composition via apply, and dynamic dispatch using eval.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// eval with quoted forms and various levels of quoting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_apply_quoted_forms_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; eval strips one level of quoting
  (eval '(+ 1 2))
  (eval ''hello)
  (eval (eval '''(+ 3 4)))
  ;; eval with backquote
  (let ((x 5))
    (eval `(+ ,x 10)))
  ;; eval of a nested quote returns the inner quote
  (eval '''foo)
  ;; eval of nil and t
  (eval nil)
  (eval t)
  ;; eval of a self-evaluating vector
  (eval [1 2 3]))"#;
    let expect = expect_test::expect![[r#""OK (3 hello (+ 3 4) 15 'foo nil t [1 2 3])""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval with dynamically constructed code
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_dynamically_constructed_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build progressively complex forms and evaluate them
    let form = r#"(let ((results nil))
  ;; Build arithmetic expressions dynamically
  (let ((ops '(+ - * max min))
        (args '(10 3)))
    (dolist (op ops)
      (let ((form (cons op args)))
        (setq results (cons (list op (eval form)) results)))))

  ;; Build a let form with computed bindings
  (let ((vars '(a b c))
        (vals '(100 200 300)))
    (let ((bindings (mapcar #'list vars vals))
          (body '(+ a (+ b c))))
      (setq results
            (cons (list 'let-eval (eval (list 'let bindings body)))
                  results))))

  ;; Build a cond form dynamically
  (let ((conditions
         (list (list '(> 5 10) ''case-a)
               (list '(= 3 3) ''case-b)
               (list t ''case-c))))
    (setq results
          (cons (list 'cond-eval (eval (cons 'cond conditions)))
                results)))

  (nreverse results))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments mapcar 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// apply with variable argument lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_variable_arg_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; apply with progressively longer trailing arg lists
  (apply #'+ '())
  (apply #'+ '(1))
  (apply #'+ '(1 2))
  (apply #'+ '(1 2 3 4 5 6 7 8 9 10))
  ;; apply with mixed fixed and list args
  (apply #'list 'a '(b c))
  (apply #'list 'a 'b '(c d))
  (apply #'list 'a 'b 'c '(d e f))
  ;; apply with empty final list
  (apply #'+ 1 2 3 '())
  ;; apply with concat
  (apply #'concat '("hello" " " "world"))
  ;; apply with lambda
  (apply (lambda (a b c) (list c b a)) '(1 2 3))
  ;; apply with &rest lambda
  (apply (lambda (&rest xs) (length xs)) '(a b c d e))
  ;; Nested apply
  (apply #'apply (list #'+ '(1 2 3))))"#;
    let expect = expect_test::expect![[
        r#""OK (0 1 3 55 (a b c) (a b c d) (a b c d e f) 6 \"hello world\" (3 2 1) 5 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// funcall vs apply: behavioral differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_vs_apply_differences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // funcall passes args directly; apply spreads the last arg
    let form = r#"(let ((fn (lambda (&rest args) args)))
  (list
   ;; funcall: each arg is separate
   (funcall fn 1 2 3)
   ;; apply: last arg is spread
   (apply fn '(1 2 3))
   ;; Both produce the same result here
   (equal (funcall fn 1 2 3) (apply fn '(1 2 3)))
   ;; But these differ:
   ;; funcall passes the list as one arg
   (funcall fn '(1 2 3))
   ;; apply with mixed: fixed args + spread
   (apply fn 'a 'b '(c d))
   ;; funcall with computed function
   (funcall (if t #'+ #'-) 10 3)
   ;; apply with computed function
   (apply (if nil #'+ #'-) '(10 3))
   ;; funcall chain
   (funcall #'funcall #'+ 1 2)
   ;; apply chain
   (apply #'apply (list #'+ '(3 4)))))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3) (1 2 3) t ((1 2 3)) (a b c d) 13 7 3 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: eval as a meta-circular interpreter step
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_meta_circular_step() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a mini evaluator that handles a subset of Elisp
    // and compare its results with eval
    let form = r#"(progn
  (fset 'neovm--test-eaa-myeval
    (lambda (expr env)
      (cond
       ;; Self-evaluating
       ((integerp expr) expr)
       ((stringp expr) expr)
       ((null expr) nil)
       ((eq expr t) t)
       ;; Variable lookup
       ((symbolp expr)
        (let ((binding (assq expr env)))
          (if binding (cdr binding)
            (symbol-value expr))))
       ;; Quote
       ((eq (car expr) 'quote) (cadr expr))
       ;; If
       ((eq (car expr) 'myif)
        (if (funcall 'neovm--test-eaa-myeval (nth 1 expr) env)
            (funcall 'neovm--test-eaa-myeval (nth 2 expr) env)
          (funcall 'neovm--test-eaa-myeval (nth 3 expr) env)))
       ;; Let (simple, non-sequential)
       ((eq (car expr) 'mylet)
        (let ((new-env env))
          (dolist (binding (nth 1 expr))
            (setq new-env
                  (cons (cons (car binding)
                              (funcall 'neovm--test-eaa-myeval
                                       (cadr binding) env))
                        new-env)))
          (funcall 'neovm--test-eaa-myeval (nth 2 expr) new-env)))
       ;; Arithmetic
       ((memq (car expr) '(myplus mymul))
        (let ((a (funcall 'neovm--test-eaa-myeval (nth 1 expr) env))
              (b (funcall 'neovm--test-eaa-myeval (nth 2 expr) env)))
          (if (eq (car expr) 'myplus) (+ a b) (* a b))))
       ;; Comparison
       ((eq (car expr) 'myeq)
        (equal (funcall 'neovm--test-eaa-myeval (nth 1 expr) env)
               (funcall 'neovm--test-eaa-myeval (nth 2 expr) env)))
       ;; List constructor
       ((eq (car expr) 'mylist)
        (mapcar (lambda (e) (funcall 'neovm--test-eaa-myeval e env))
                (cdr expr)))
       (t (error "Unknown form: %S" expr)))))

  (unwind-protect
      (let ((tests
             (list
              ;; Simple values
              (list 42 nil)
              (list "hello" nil)
              ;; Arithmetic
              (list '(myplus 3 4) nil)
              (list '(mymul (myplus 2 3) 4) nil)
              ;; Variables from env
              (list 'x '((x . 10) (y . 20)))
              ;; Let
              (list '(mylet ((a 5) (b 7)) (myplus a b)) nil)
              ;; Nested let with shadowing
              (list '(mylet ((x 1))
                      (mylet ((x 2) (y x))
                        (myplus x y)))
                    nil)
              ;; Conditional
              (list '(myif (myeq 1 1) (quote yes) (quote no)) nil)
              (list '(myif (myeq 1 2) (quote yes) (quote no)) nil)
              ;; List construction
              (list '(mylist 1 (myplus 2 3) (mymul 4 5)) nil))))
        (mapcar (lambda (test)
                  (funcall 'neovm--test-eaa-myeval
                           (car test) (cadr test)))
                tests))
    (fmakunbound 'neovm--test-eaa-myeval)))"#;
    let expect = expect_test::expect![[r#""OK (42 \"hello\" 7 20 10 12 3 yes no (1 5 20))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: apply for function composition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_function_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use apply to implement function composition, pipeline, and juxt
    let form = r#"(progn
  ;; compose: (compose f g) returns a function that applies g then f
  (fset 'neovm--test-eaa-compose
    (lambda (f g)
      (lambda (&rest args)
        (funcall f (apply g args)))))

  ;; pipe: apply a chain of functions left-to-right
  (fset 'neovm--test-eaa-pipe
    (lambda (val &rest fns)
      (let ((result val))
        (dolist (fn fns)
          (setq result (funcall fn result)))
        result)))

  ;; juxt: apply multiple functions to same args, collect results
  (fset 'neovm--test-eaa-juxt
    (lambda (&rest fns)
      (lambda (&rest args)
        (mapcar (lambda (fn) (apply fn args)) fns))))

  (unwind-protect
      (let* ((double (lambda (x) (* x 2)))
             (add1   (lambda (x) (+ x 1)))
             (square (lambda (x) (* x x)))
             (neg    (lambda (x) (- x)))
             ;; compose: square then add1 = (x+1)^2
             (sq-add1 (funcall 'neovm--test-eaa-compose square add1))
             ;; compose: add1 then square = x^2 + 1
             (add1-sq (funcall 'neovm--test-eaa-compose add1 square)))
        (list
         ;; compose tests
         (funcall sq-add1 3)   ; (3+1)^2 = 16
         (funcall add1-sq 3)   ; 3^2 + 1 = 10
         ;; pipe: 2 -> double -> add1 -> square = (2*2+1)^2 = 25
         (funcall 'neovm--test-eaa-pipe 2 double add1 square)
         ;; pipe: 5 -> neg -> add1 = -5+1 = -4
         (funcall 'neovm--test-eaa-pipe 5 neg add1)
         ;; juxt: apply (add1, double, square) to 5
         (funcall (funcall 'neovm--test-eaa-juxt add1 double square) 5)
         ;; juxt with multi-arg functions
         (funcall (funcall 'neovm--test-eaa-juxt #'+ #'- #'*) 10 3)
         ;; Composition chain: double three times = *8
         (let ((triple-double
                (funcall 'neovm--test-eaa-compose
                         double
                         (funcall 'neovm--test-eaa-compose double double))))
           (funcall triple-double 5))))
    (fmakunbound 'neovm--test-eaa-compose)
    (fmakunbound 'neovm--test-eaa-pipe)
    (fmakunbound 'neovm--test-eaa-juxt)))"#;
    let expect = expect_test::expect![[r#""OK (16 10 25 -4 (6 10 25) (13 7 30) 40)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: dynamic dispatch using eval and funcall
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_dynamic_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a simple object system with method dispatch using eval/funcall
    let form = r#"(progn
  ;; Make an "object" as an alist of methods
  (fset 'neovm--test-eaa-make-point
    (lambda (x y)
      (let ((self nil))
        (setq self
              (list
               (cons 'get-x (lambda () x))
               (cons 'get-y (lambda () y))
               (cons 'move (lambda (dx dy)
                             (setq x (+ x dx))
                             (setq y (+ y dy))
                             (list x y)))
               (cons 'distance-to-origin
                     (lambda ()
                       ;; Use integer arithmetic for cross-platform consistency
                       (+ (* x x) (* y y))))
               (cons 'to-string
                     (lambda ()
                       (format "(%d,%d)" x y)))))
        self)))

  ;; Dispatch: look up method and call it
  (fset 'neovm--test-eaa-send
    (lambda (obj method &rest args)
      (let ((handler (cdr (assq method obj))))
        (if handler
            (apply handler args)
          (error "Unknown method: %s" method)))))

  (unwind-protect
      (let ((p (funcall 'neovm--test-eaa-make-point 3 4)))
        (list
         (funcall 'neovm--test-eaa-send p 'get-x)
         (funcall 'neovm--test-eaa-send p 'get-y)
         (funcall 'neovm--test-eaa-send p 'to-string)
         (funcall 'neovm--test-eaa-send p 'distance-to-origin)
         (funcall 'neovm--test-eaa-send p 'move 2 -1)
         (funcall 'neovm--test-eaa-send p 'to-string)
         (funcall 'neovm--test-eaa-send p 'distance-to-origin)))
    (fmakunbound 'neovm--test-eaa-make-point)
    (fmakunbound 'neovm--test-eaa-send)))"#;
    let expect = expect_test::expect![[r#""OK (3 4 \"(3,4)\" 25 (5 3) \"(5,3)\" 34)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval + apply combined: dynamic function table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_apply_function_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A function dispatch table where operations are looked up by name,
    // and args are dynamically assembled and applied.
    let form = r#"(let ((ops (list
              (cons 'sum (lambda (&rest ns) (apply #'+ ns)))
              (cons 'product (lambda (&rest ns) (apply #'* ns)))
              (cons 'avg (lambda (&rest ns)
                           (if ns (/ (apply #'+ ns) (length ns)) 0)))
              (cons 'range (lambda (&rest ns)
                             (if ns (- (apply #'max ns) (apply #'min ns)) 0)))
              (cons 'count (lambda (&rest ns) (length ns))))))
  ;; Execute a sequence of operations on data sets
  (let ((commands
         '((sum 1 2 3 4 5)
           (product 1 2 3 4 5)
           (avg 10 20 30)
           (range 5 1 9 3 7)
           (count a b c d e f))))
    (mapcar
     (lambda (cmd)
       (let* ((op-name (car cmd))
              (args (cdr cmd))
              (fn (cdr (assq op-name ops))))
         (list op-name (apply fn args))))
     commands)))"#;
    let expect =
        expect_test::expect![[r#""OK ((sum 15) (product 120) (avg 20) (range 8) (count 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
