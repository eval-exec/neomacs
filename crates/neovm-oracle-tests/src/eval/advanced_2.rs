//! Oracle parity tests for advanced `eval` patterns (part 2):
//! dynamic form construction, lexical vs dynamic scoping, backquote
//! with eval, double evaluation, dispatch tables, error propagation,
//! eval vs funcall comparison, and eval-driven interpreters.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Dynamically constructed forms with list/cons/append
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval2_dynamic_form_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build complex forms programmatically and eval them
    let form = r#"(let* ((ops '(+ - * max min))
                         (args '(10 3))
                         (results
                          (mapcar
                           (lambda (op)
                             (let ((form (cons op args)))
                               (cons op (eval form))))
                           ops))
                         ;; Build a nested form: (+ (* 3 4) (- 10 2))
                         (nested (list '+
                                       (list '* 3 4)
                                       (list '- 10 2)))
                         (nested-result (eval nested))
                         ;; Build a progn with multiple setq forms
                         (var-names '(neovm--ev2-a neovm--ev2-b neovm--ev2-c))
                         (values '(10 20 30))
                         (setq-forms (mapcar
                                      (lambda (pair)
                                        (list 'setq (car pair) (cdr pair)))
                                      (seq-mapn #'cons var-names values)))
                         (progn-form (cons 'progn
                                          (append setq-forms
                                                  (list (cons 'list
                                                              var-names)))))
                         (progn-result (eval progn-form)))
                    (list results nested-result progn-result))"#;
    let expect = expect_test::expect![[
        r#""OK (((+ . 13) (- . 7) (* . 30) (max . 10) (min . 3)) 20 (10 20 30))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval in dynamic binding context with setq side effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval2_dynamic_scoping_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // eval sees dynamically scoped variables and setq modifies them
    let form = r#"(progn
                    (defvar neovm--ev2-counter 0)
                    (unwind-protect
                        (let ((neovm--ev2-counter 100))
                          (let* ((r1 (eval '(setq neovm--ev2-counter
                                                  (1+ neovm--ev2-counter))))
                                 (r2 (eval '(setq neovm--ev2-counter
                                                  (* neovm--ev2-counter 2))))
                                 (r3 (eval 'neovm--ev2-counter))
                                 ;; eval a let that shadows the dynamic var
                                 (r4 (eval '(let ((neovm--ev2-counter 999))
                                              neovm--ev2-counter)))
                                 ;; After the let, original binding restored
                                 (r5 (eval 'neovm--ev2-counter)))
                            (list r1 r2 r3 r4 r5)))
                      (makunbound 'neovm--ev2-counter)))"#;
    let expect = expect_test::expect![[r#""OK (101 202 202 999 202)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval with backquote/unquote constructed forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval2_backquote_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use backquote to build forms, then eval them
    let form = r#"(let ((x 5)
                        (y 10)
                        (op '+))
                    (let* (;; Simple backquote: splice values into form
                           (form1 `(,op ,x ,y))
                           (r1 (eval form1))
                           ;; Backquote with nested lists
                           (form2 `(let ((a ,x) (b ,y))
                                     (list a b (,op a b))))
                           (r2 (eval form2))
                           ;; Backquote with splicing
                           (extra-args '(20 30))
                           (form3 `(+ ,x ,@extra-args))
                           (r3 (eval form3))
                           ;; Nested backquote producing a lambda
                           (form4 `(lambda (z) (+ z ,x)))
                           (fn (eval form4))
                           (r4 (funcall fn 100))
                           ;; Backquote in conditional form construction
                           (test-val 42)
                           (form5 `(cond
                                     ((= ,test-val 0) 'zero)
                                     ((> ,test-val 40) 'big)
                                     (t 'small)))
                           (r5 (eval form5)))
                      (list r1 r2 r3 r4 r5
                            ;; Verify form structure
                            form1 form3)))"#;
    let expect = expect_test::expect![[r#""OK (15 (5 10 15) 55 105 big (+ 5 10) (+ 5 20 30))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Double evaluation (eval of eval) and multi-level quoting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval2_nested_eval_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple levels of eval peeling off quote layers
    let form = r#"(let* (;; Single eval: evaluate a quoted form
                         (r1 (eval '(+ 1 2)))
                         ;; Double eval: first eval yields a form, second evaluates it
                         (r2 (eval (eval '''(+ 3 4))))
                         ;; Triple quote + triple eval
                         (r3 (eval (eval (eval ''''(+ 5 6)))))
                         ;; eval that returns a symbol, then eval the symbol
                         (neovm--ev2-dynamic-val 99)
                         (r4 (eval (eval '''neovm--ev2-dynamic-val)))
                         ;; eval producing a list form, then eval that
                         (r5 (eval (eval '(quote (list 1 2 3)))))
                         ;; Construct a form that when evaluated constructs
                         ;; another form, and eval both levels
                         (r6 (eval
                              (eval '(list 'concat
                                           (quote "hello")
                                           (quote " ")
                                           (quote "world"))))))
                    (list r1 r2 r3 r4 r5 r6))"#;
    let expect = expect_test::expect![[
        r#""OK (3 (+ 3 4) (+ 5 6) neovm--ev2-dynamic-val (1 2 3) \"hello world\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval with dynamically constructed let/setq/defun forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval2_constructed_binding_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Programmatically build let and setq forms and eval them
    let form = r#"(let* (;; Build a let form with computed bindings
                         (vars '((a . 10) (b . 20) (c . 30)))
                         (let-bindings (mapcar
                                        (lambda (v)
                                          (list (car v) (cdr v)))
                                        vars))
                         (let-body '(list a b c (+ a b c)))
                         (let-form (list 'let let-bindings let-body))
                         (r1 (eval let-form))
                         ;; Build a let* with sequential deps
                         (let*-form '(let* ((x 5)
                                            (y (* x 2))
                                            (z (+ x y)))
                                       (list x y z)))
                         (r2 (eval let*-form))
                         ;; Build a progn with multiple setq
                         (neovm--ev2-tmp nil)
                         (setq-chain (list 'progn
                                           '(setq neovm--ev2-tmp 1)
                                           '(setq neovm--ev2-tmp
                                                  (* neovm--ev2-tmp 10))
                                           '(setq neovm--ev2-tmp
                                                  (+ neovm--ev2-tmp 5))
                                           'neovm--ev2-tmp))
                         (r3 (eval setq-chain))
                         ;; Build and eval a defun, then call it
                         (defun-form '(progn
                                        (fset 'neovm--ev2-square
                                              (lambda (x) (* x x)))
                                        (funcall 'neovm--ev2-square 7)))
                         (r4 (eval defun-form)))
                    (unwind-protect
                        (list r1 r2 r3 r4)
                      (fmakunbound 'neovm--ev2-square)))"#;
    let expect = expect_test::expect![[r#""OK ((10 20 30 60) (5 10 15) 15 49)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval dispatch table: assoc-based function dispatch via eval
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval2_dispatch_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A dispatch table mapping command names to forms/actions
    let form = r#"(let* ((dispatch-table
                          '((add . (+ arg1 arg2))
                            (sub . (- arg1 arg2))
                            (mul . (* arg1 arg2))
                            (div . (/ arg1 arg2))
                            (pow . (expt arg1 arg2))
                            (mod . (% arg1 arg2))
                            (max . (max arg1 arg2))
                            (avg . (/ (+ arg1 arg2) 2.0))))
                         (dispatch
                          (lambda (cmd a1 a2)
                            (let ((entry (assq cmd dispatch-table)))
                              (if entry
                                  (let ((arg1 a1)
                                        (arg2 a2))
                                    (eval (cdr entry)))
                                (list 'unknown-command cmd)))))
                         ;; Execute various commands
                         (r-add (funcall dispatch 'add 10 3))
                         (r-sub (funcall dispatch 'sub 10 3))
                         (r-mul (funcall dispatch 'mul 10 3))
                         (r-pow (funcall dispatch 'pow 2 10))
                         (r-mod (funcall dispatch 'mod 17 5))
                         (r-avg (funcall dispatch 'avg 10 20))
                         (r-unk (funcall dispatch 'sqrt 9 0))
                         ;; Batch dispatch
                         (commands '((add 100 200)
                                     (mul 5 6)
                                     (sub 50 30)
                                     (max 42 99)))
                         (batch-results
                          (mapcar (lambda (cmd)
                                    (funcall dispatch
                                             (car cmd)
                                             (cadr cmd)
                                             (caddr cmd)))
                                  commands)))
                    (list r-add r-sub r-mul r-pow r-mod r-avg r-unk
                          batch-results))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable arg1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval with error propagation: condition-case around eval
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval2_error_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Eval forms that may signal errors, catch and classify them
    let form = r#"(let* ((safe-eval
                          (lambda (form)
                            (condition-case err
                                (list 'ok (eval form))
                              (void-variable
                               (list 'void-var (cadr err)))
                              (void-function
                               (list 'void-fn (cadr err)))
                              (wrong-type-argument
                               (list 'type-error (car (cdr err))))
                              (arith-error
                               (list 'arith-error))
                              (wrong-number-of-arguments
                               (list 'arity-error))
                              (error
                               (list 'generic-error (car err))))))
                         ;; Valid form
                         (r1 (funcall safe-eval '(+ 1 2 3)))
                         ;; Void variable
                         (r2 (funcall safe-eval 'neovm--ev2-nonexistent-var))
                         ;; Void function
                         (r3 (funcall safe-eval '(neovm--ev2-nonexistent-fn 1)))
                         ;; Wrong type
                         (r4 (funcall safe-eval '(+ 1 "two")))
                         ;; Division by zero
                         (r5 (funcall safe-eval '(/ 10 0)))
                         ;; Nested error: eval within eval
                         (r6 (funcall safe-eval
                                      '(eval '(/ 1 0))))
                         ;; Error in let binding
                         (r7 (funcall safe-eval
                                      '(let ((x (/ 1 0))) x)))
                         ;; Error in condition body
                         (r8 (funcall safe-eval
                                      '(if t (+ 1 "bad") 'ok))))
                    (list r1 r2 r3 r4 r5 r6 r7 r8))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok 6) (void-var neovm--ev2-nonexistent-var) (void-fn neovm--ev2-nonexistent-fn) (type-error number-or-marker-p) (arith-error) (arith-error) (arith-error) (type-error number-or-marker-p))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval vs funcall comparison: verify semantic equivalence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval2_eval_vs_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare eval of (fn args...) vs funcall for various scenarios
    let form = r#"(progn
                    (fset 'neovm--ev2-adder
                          (lambda (a b) (+ a b)))
                    (fset 'neovm--ev2-formatter
                          (lambda (name age)
                            (format "%s is %d" name age)))
                    (unwind-protect
                        (let* (;; Simple: eval '(f x y) vs funcall 'f x y
                               (e1 (eval '(neovm--ev2-adder 3 4)))
                               (f1 (funcall 'neovm--ev2-adder 3 4))
                               ;; With string args
                               (e2 (eval '(neovm--ev2-formatter "Bob" 25)))
                               (f2 (funcall 'neovm--ev2-formatter "Bob" 25))
                               ;; With computed args via eval
                               (e3 (eval (list 'neovm--ev2-adder
                                               '(* 3 4)
                                               '(+ 5 6))))
                               (f3 (funcall 'neovm--ev2-adder (* 3 4) (+ 5 6)))
                               ;; With apply
                               (a1 (apply 'neovm--ev2-adder '(10 20)))
                               (e4 (eval '(neovm--ev2-adder 10 20)))
                               ;; Verify all pairs match
                               (all-match (and (equal e1 f1)
                                               (equal e2 f2)
                                               (equal e3 f3)
                                               (equal a1 e4)))
                               ;; Lambda in eval vs direct funcall
                               (e5 (eval '((lambda (x) (* x x)) 7)))
                               (f5 (funcall (lambda (x) (* x x)) 7))
                               ;; mapcar with eval vs funcall
                               (forms '((+ 1 1) (+ 2 2) (+ 3 3)))
                               (e-map (mapcar #'eval forms))
                               (f-map (list (funcall '+ 1 1)
                                            (funcall '+ 2 2)
                                            (funcall '+ 3 3))))
                          (list e1 f1 (equal e1 f1)
                                e2 f2 (equal e2 f2)
                                e3 f3 (equal e3 f3)
                                all-match
                                e5 f5 (equal e5 f5)
                                e-map f-map (equal e-map f-map)))
                      (fmakunbound 'neovm--ev2-adder)
                      (fmakunbound 'neovm--ev2-formatter)))"#;
    let expect = expect_test::expect![[
        r#""OK (7 7 t \"Bob is 25\" \"Bob is 25\" t 23 23 t t 49 49 t (2 4 6) (2 4 6) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
