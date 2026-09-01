//! Advanced oracle parity tests for `commandp`, `functionp`, `subrp`,
//! `fboundp` against lambdas, closures, subrs, macros, special forms,
//! autoloads, indirect-function, function aliases, defalias chains.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Comprehensive predicate matrix across many object types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_commandp_functionp_predicate_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test functionp, commandp, subrp, fboundp across a wide set of values.
    let form = r#"(let ((plain-lambda (lambda (x) (+ x 1)))
                        (interactive-lambda (lambda () (interactive) (message "hi")))
                        (interactive-with-spec (lambda () (interactive "p") nil)))
                    (list
                     ;; plain lambda
                     (list 'plain-lambda
                           (functionp plain-lambda)
                           (commandp plain-lambda)
                           (subrp plain-lambda))
                     ;; interactive lambda
                     (list 'interactive-lambda
                           (functionp interactive-lambda)
                           (commandp interactive-lambda)
                           (subrp interactive-lambda))
                     ;; interactive with spec
                     (list 'interactive-with-spec
                           (functionp interactive-with-spec)
                           (commandp interactive-with-spec)
                           (subrp interactive-with-spec))
                     ;; built-in subrs
                     (list 'car-subr
                           (functionp (symbol-function 'car))
                           (commandp (symbol-function 'car))
                           (subrp (symbol-function 'car)))
                     (list 'plus-subr
                           (functionp (symbol-function '+))
                           (commandp (symbol-function '+))
                           (subrp (symbol-function '+')))
                     ;; non-function values
                     (list 'nil-val (functionp nil) (commandp nil))
                     (list 'number (functionp 42) (commandp 42))
                     (list 'string (functionp "hello") (commandp "hello"))
                     (list 'symbol-t (functionp t) (commandp t))
                     (list 'cons-cell (functionp '(1 2)) (commandp '(1 2)))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 28 55)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fboundp and symbol-function with fset/fmakunbound lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fboundp_fset_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test fboundp before fset, after fset, after fmakunbound, and re-fset.
    // Also test that symbol-function retrieves what was fset.
    let form = r#"(progn
  (unwind-protect
      (let ((results nil))
        ;; Initially unbound
        (setq results (cons (fboundp 'neovm--test-fbp-fn1) results))
        ;; Bind to a lambda
        (fset 'neovm--test-fbp-fn1 (lambda (x) (* x x)))
        (setq results (cons (fboundp 'neovm--test-fbp-fn1) results))
        (setq results (cons (functionp (symbol-function 'neovm--test-fbp-fn1)) results))
        (setq results (cons (funcall 'neovm--test-fbp-fn1 5) results))
        ;; Rebind to a different function
        (fset 'neovm--test-fbp-fn1 (lambda (x) (+ x 100)))
        (setq results (cons (funcall 'neovm--test-fbp-fn1 5) results))
        ;; Unbind
        (fmakunbound 'neovm--test-fbp-fn1)
        (setq results (cons (fboundp 'neovm--test-fbp-fn1) results))
        ;; Bind to a subr
        (fset 'neovm--test-fbp-fn1 (symbol-function '+))
        (setq results (cons (subrp (symbol-function 'neovm--test-fbp-fn1)) results))
        (setq results (cons (funcall 'neovm--test-fbp-fn1 3 4) results))
        (nreverse results))
    (fmakunbound 'neovm--test-fbp-fn1)))"#;
    let expect = expect_test::expect![[r#""OK (nil t t 25 105 nil t 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defalias chains and indirect-function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_chain_indirect_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a chain of aliases: fn-a -> fn-b -> fn-c -> actual lambda.
    // Test indirect-function follows the chain to the final definition.
    let form = r#"(progn
  (unwind-protect
      (progn
        ;; Define the actual function
        (fset 'neovm--test-chain-c (lambda (x) (* x x x)))
        ;; Create alias chain
        (defalias 'neovm--test-chain-b 'neovm--test-chain-c)
        (defalias 'neovm--test-chain-a 'neovm--test-chain-b)
        (list
         ;; All are fboundp
         (fboundp 'neovm--test-chain-a)
         (fboundp 'neovm--test-chain-b)
         (fboundp 'neovm--test-chain-c)
         ;; symbol-function of a returns symbol b
         (eq (symbol-function 'neovm--test-chain-a) 'neovm--test-chain-b)
         ;; indirect-function follows chain to the lambda
         (functionp (indirect-function 'neovm--test-chain-a))
         (functionp (indirect-function 'neovm--test-chain-b))
         ;; Calling through the chain works
         (funcall 'neovm--test-chain-a 3)
         (funcall 'neovm--test-chain-b 4)
         (funcall 'neovm--test-chain-c 5)
         ;; commandp through chain
         (commandp 'neovm--test-chain-a)
         ;; subrp through indirect
         (subrp (indirect-function 'neovm--test-chain-a))))
    (fmakunbound 'neovm--test-chain-a)
    (fmakunbound 'neovm--test-chain-b)
    (fmakunbound 'neovm--test-chain-c)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t 27 64 125 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macros vs functions: predicate differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macro_vs_function_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macros are NOT functionp, NOT commandp, NOT subrp.
    // But they are fboundp. Test with defmacro and with raw macro cons form.
    let form = r#"(progn
  (unwind-protect
      (progn
        ;; Define a macro
        (fset 'neovm--test-mac1
              (cons 'macro (lambda (x) (list '+ x 1))))
        ;; Define a function for comparison
        (fset 'neovm--test-fn1
              (lambda (x) (+ x 1)))
        ;; Define an interactive command
        (fset 'neovm--test-cmd1
              (lambda () (interactive) (message "cmd")))
        (list
         ;; Macro predicates
         (list 'macro
               (fboundp 'neovm--test-mac1)
               (functionp (symbol-function 'neovm--test-mac1))
               (commandp 'neovm--test-mac1)
               (subrp (symbol-function 'neovm--test-mac1))
               (consp (symbol-function 'neovm--test-mac1))
               (eq (car (symbol-function 'neovm--test-mac1)) 'macro))
         ;; Function predicates
         (list 'function
               (fboundp 'neovm--test-fn1)
               (functionp (symbol-function 'neovm--test-fn1))
               (commandp 'neovm--test-fn1)
               (subrp (symbol-function 'neovm--test-fn1)))
         ;; Command predicates
         (list 'command
               (fboundp 'neovm--test-cmd1)
               (functionp (symbol-function 'neovm--test-cmd1))
               (commandp 'neovm--test-cmd1)
               (subrp (symbol-function 'neovm--test-cmd1)))
         ;; Special forms
         (list 'special-form-if
               (fboundp 'if)
               (subrp (symbol-function 'if))
               (commandp 'if)
               (functionp (symbol-function 'if)))))
    (fmakunbound 'neovm--test-mac1)
    (fmakunbound 'neovm--test-fn1)
    (fmakunbound 'neovm--test-cmd1)))"#;
    let expect = expect_test::expect![[
        r#""OK ((macro t nil nil nil t t) (function t t nil nil) (command t t t nil) (special-form-if t t nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Function classification engine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_function_classifier() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classify a set of symbols by their binding type:
    // subr, special-form, lambda, macro, alias, unbound.
    let form = r#"(progn
  (unwind-protect
      (progn
        (fset 'neovm--test-cls-fn (lambda (x) x))
        (fset 'neovm--test-cls-mac
              (cons 'macro (lambda (form) form)))
        (fset 'neovm--test-cls-cmd
              (lambda () (interactive) nil))
        (defalias 'neovm--test-cls-alias 'car)
        (let ((classify
               (lambda (sym)
                 (if (not (fboundp sym))
                     'unbound
                   (let ((def (symbol-function sym)))
                     (cond
                      ((and (subrp def)
                            (let ((arity (subr-arity def)))
                              (eq (cdr arity) 'unevalled)))
                       'special-form)
                      ((subrp def) 'subr)
                      ((and (consp def) (eq (car def) 'macro)) 'macro)
                      ((symbolp def) 'alias)
                      ((functionp def)
                       (if (commandp def) 'command 'lambda))
                      (t 'other)))))))
          (let ((syms '(car cons + if and or setq let progn
                        neovm--test-cls-fn
                        neovm--test-cls-mac
                        neovm--test-cls-cmd
                        neovm--test-cls-alias
                        neovm--test-cls-nonexistent)))
            (mapcar (lambda (s)
                      (cons s (funcall classify s)))
                    syms))))
    (fmakunbound 'neovm--test-cls-fn)
    (fmakunbound 'neovm--test-cls-mac)
    (fmakunbound 'neovm--test-cls-cmd)
    (fmakunbound 'neovm--test-cls-alias)))"#;
    let expect = expect_test::expect![[
        r#""OK ((car . subr) (cons . subr) (+ . subr) (if . special-form) (and . special-form) (or . special-form) (setq . special-form) (let . special-form) (progn . special-form) (neovm--test-cls-fn . lambda) (neovm--test-cls-mac . macro) (neovm--test-cls-cmd . command) (neovm--test-cls-alias . alias) (neovm--test-cls-nonexistent . unbound))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closures and functionp/commandp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_predicate_tests() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Closures created via lexical binding should satisfy functionp.
    // Test closures from let-over-lambda patterns.
    let form = r#"(let ((make-counter
                         (lambda (start)
                           (let ((n start))
                             (lambda () (setq n (1+ n)) n))))
                        (make-cmd-counter
                         (lambda (start)
                           (let ((n start))
                             (lambda () (interactive) (setq n (1+ n)) n)))))
                    (let ((c1 (funcall make-counter 0))
                          (c2 (funcall make-counter 10))
                          (cmd (funcall make-cmd-counter 0)))
                      ;; Use the counters
                      (funcall c1) (funcall c1) (funcall c1)
                      (funcall c2) (funcall c2)
                      (list
                       ;; Counter values
                       (funcall c1) ;; 4th call, returns 4
                       (funcall c2) ;; 3rd call, returns 13
                       ;; Predicate checks on closures
                       (functionp c1)
                       (functionp c2)
                       (functionp cmd)
                       (commandp c1)
                       (commandp cmd)
                       (subrp c1)
                       (subrp cmd)
                       ;; indirect-function on closures returns itself
                       (eq (indirect-function c1) c1))))"#;
    let expect = expect_test::expect![[r#""OK (4 13 t t t nil t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Defalias with docstring, interactive commands, and subr-arity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_commandp_arity_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that defalias to a command symbol preserves commandp.
    // Also test subr-arity on aliased subrs.
    let form = r#"(progn
  (unwind-protect
      (progn
        ;; Alias to a subr
        (defalias 'neovm--test-da-plus '+)
        ;; Alias to a lambda
        (defalias 'neovm--test-da-fn (lambda (a b) (+ a b)))
        ;; Alias to an interactive lambda
        (defalias 'neovm--test-da-cmd (lambda () (interactive) 42))
        ;; Chain: alias to alias to subr
        (defalias 'neovm--test-da-chain 'neovm--test-da-plus)
        (list
         ;; Aliased subr works
         (funcall 'neovm--test-da-plus 3 4 5)
         ;; Aliased lambda works
         (funcall 'neovm--test-da-fn 10 20)
         ;; Aliased command works and is commandp
         (commandp 'neovm--test-da-cmd)
         (funcall 'neovm--test-da-cmd)
         ;; Chained alias works
         (funcall 'neovm--test-da-chain 1 2 3)
         ;; subr-arity through alias
         (let ((def (indirect-function 'neovm--test-da-plus)))
           (when (subrp def) (subr-arity def)))
         (let ((def (indirect-function 'neovm--test-da-chain)))
           (when (subrp def) (subr-arity def)))
         ;; functionp on symbols
         (functionp 'neovm--test-da-plus)
         (functionp 'neovm--test-da-fn)
         (functionp 'neovm--test-da-cmd)
         (functionp 'neovm--test-da-chain)))
    (fmakunbound 'neovm--test-da-plus)
    (fmakunbound 'neovm--test-da-fn)
    (fmakunbound 'neovm--test-da-cmd)
    (fmakunbound 'neovm--test-da-chain)))"#;
    let expect = expect_test::expect![[r#""OK (12 30 t 42 6 (0 . many) (0 . many) t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
