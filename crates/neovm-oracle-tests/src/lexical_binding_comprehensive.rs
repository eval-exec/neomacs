//! Oracle parity tests for comprehensive lexical binding semantics:
//! lexical vs dynamic binding differences, closure capturing, multiple closures
//! sharing environments, mutable lexical variables via setq, let/let* scopes,
//! nested multi-level closures, closures as higher-order function arguments,
//! and lexical binding interaction with defvar special variables.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Lexical vs dynamic binding fundamental differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexical_binding_vs_dynamic_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In lexical binding, a function sees its defining environment, not the
    // caller's environment.  Here `reader` is defined where `x` is 10 in the
    // lexical scope, and calling it from a context where a new `x` is 99
    // still returns 10 (lexical), not 99 (dynamic).
    let form = r#"(let ((x 10))
  (let ((reader (lambda () x)))
    (let ((x 99))
      ;; Under lexical binding, reader still sees x=10
      (list (funcall reader) x))))"#;
    let expect = expect_test::expect![[r#""OK (10 99)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(10 99)", &o, &n);
}

#[test]
fn oracle_prop_lexical_binding_closure_survives_scope_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The closure captures the lexical binding of `val` even after the let
    // that created `val` has returned.  Calling the closure later still works.
    let form = r#"(let ((make-getter
           (lambda (val)
             (lambda () val))))
  (let ((g1 (funcall make-getter 42))
        (g2 (funcall make-getter 'hello))
        (g3 (funcall make-getter '(a b c))))
    (list (funcall g1) (funcall g2) (funcall g3))))"#;
    let expect = expect_test::expect![[r#""OK (42 hello (a b c))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple closures sharing the same lexical environment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexical_binding_shared_env_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three closures (increment, decrement, read) all share the same
    // lexical variable `count`.  Mutations from one are visible to all.
    let form = r#"(let ((count 0))
  (let ((inc (lambda () (setq count (1+ count))))
        (dec (lambda () (setq count (1- count))))
        (read-count (lambda () count)))
    (funcall inc)
    (funcall inc)
    (funcall inc)
    (funcall dec)
    (let ((after-ops (funcall read-count)))
      (funcall inc)
      (funcall inc)
      (list after-ops (funcall read-count)))))"#;
    let expect = expect_test::expect![[r#""OK (2 4)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(2 4)", &o, &n);
}

#[test]
fn oracle_prop_lexical_binding_shared_env_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Closure-based stack: push, pop, peek, and size all share `items`.
    let form = r#"(let ((items nil))
  (let ((push-fn  (lambda (x) (setq items (cons x items)) x))
        (pop-fn   (lambda ()
                    (if items
                        (let ((top (car items)))
                          (setq items (cdr items))
                          top)
                      'empty)))
        (peek-fn  (lambda () (if items (car items) 'empty)))
        (size-fn  (lambda () (length items))))
    (list
      (funcall size-fn)           ;; 0
      (funcall push-fn 'a)        ;; a
      (funcall push-fn 'b)        ;; b
      (funcall push-fn 'c)        ;; c
      (funcall size-fn)           ;; 3
      (funcall peek-fn)           ;; c
      (funcall pop-fn)            ;; c
      (funcall pop-fn)            ;; b
      (funcall peek-fn)           ;; a
      (funcall size-fn)           ;; 1
      (funcall pop-fn)            ;; a
      (funcall pop-fn)            ;; empty
      (funcall size-fn))))"#;
    let expect = expect_test::expect![[r#""OK (0 a b c 3 c c b a 1 a empty 0)""#]];
    // 0
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mutable lexical variables via setq inside closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexical_binding_setq_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Accumulator pattern: each call to the returned closure adds to a running
    // total.  Multiple independent accumulators from the same factory.
    let form = r#"(let ((make-acc
           (lambda (init)
             (let ((total init))
               (lambda (n)
                 (setq total (+ total n))
                 total)))))
  (let ((a1 (funcall make-acc 0))
        (a2 (funcall make-acc 100)))
    (list
      (funcall a1 5)    ;; 5
      (funcall a1 3)    ;; 8
      (funcall a2 10)   ;; 110
      (funcall a1 2)    ;; 10
      (funcall a2 20)   ;; 130
      (funcall a2 -30)  ;; 100
      (funcall a1 0))))"#;
    let expect = expect_test::expect![[r#""OK (5 8 110 10 130 100 10)""#]];
    // 10
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(5 8 110 10 130 100 10)", &o, &n);
}

// ---------------------------------------------------------------------------
// let/let* creating lexical scopes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexical_binding_let_star_sequential_refs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // let* allows sequential references: each binding sees the previous ones.
    // Closures capture the final values of those bindings.
    let form = r#"(let* ((a 1)
        (b (+ a 10))
        (c (* b 2))
        (d (- c a))
        (snapshot (lambda () (list a b c d))))
  ;; Verify that let* computed correctly and closure captures all
  (let ((result (funcall snapshot)))
    (list
      result
      ;; a=1, b=11, c=22, d=21
      (equal result '(1 11 22 21))
      ;; Shadowing a in new let doesn't affect the closure
      (let ((a 999))
        (funcall snapshot)))))"#;
    let expect = expect_test::expect![[r#""OK ((1 11 22 21) t (1 11 22 21))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_lexical_binding_let_parallel_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In parallel `let`, all bindings see the *outer* scope, not each other.
    // This is different from `let*`.
    let form = r#"(let ((x 1) (y 2))
  (let ((x y) (y x))
    ;; x should be 2 (old y), y should be 1 (old x) — swap
    (list x y)))"#;
    let expect = expect_test::expect![[r#""OK (2 1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(2 1)", &o, &n);
}

// ---------------------------------------------------------------------------
// Nested closures with multiple capture levels
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexical_binding_nested_closure_three_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three levels of closure nesting: outermost captures `a`, middle captures
    // `a` and `b`, innermost captures `a`, `b`, and `c`.  Each independently
    // functional after all let scopes have exited.
    let form = r#"(let ((a 1))
  (let ((outer-fn
         (lambda (b)
           (lambda (c)
             (lambda (d)
               (list a b c d (* a b c d)))))))
    ;; Create partially applied chain
    (let ((mid (funcall outer-fn 2)))
      (let ((inner (funcall mid 3)))
        (let ((result1 (funcall inner 4))
              ;; Different inner with same mid
              (inner2 (funcall mid 5)))
          (list result1
                (funcall inner2 6)
                ;; Re-using mid creates new independent inner
                (funcall (funcall mid 10) 10)))))))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 24) (1 2 5 6 60) (1 2 10 10 200))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_lexical_binding_closure_returning_closure_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A closure that returns a new closure on each call, but all returned
    // closures share the same mutable state from the outermost let.
    let form = r#"(let ((history nil))
  (let ((make-logger
         (lambda (prefix)
           (lambda (msg)
             (let ((entry (concat prefix ": " msg)))
               (setq history (cons entry history))
               entry)))))
    (let ((log-info (funcall make-logger "INFO"))
          (log-err  (funcall make-logger "ERROR")))
      (funcall log-info "started")
      (funcall log-err "disk full")
      (funcall log-info "retrying")
      (funcall log-err "timeout")
      (funcall log-info "done")
      (list (length history)
            (reverse history)))))"#;
    let expect = expect_test::expect![[
        r#""OK (5 (\"INFO: started\" \"ERROR: disk full\" \"INFO: retrying\" \"ERROR: timeout\" \"INFO: done\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closures as arguments to higher-order functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexical_binding_closure_with_mapcar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build closures in a loop, then pass them to mapcar/funcall.
    // Each closure captures a unique lexical binding.
    let form = r#"(let ((multipliers nil))
  (dolist (factor '(2 3 5 7 11))
    (let ((f factor))
      (setq multipliers (cons (lambda (x) (* x f)) multipliers))))
  (setq multipliers (nreverse multipliers))
  ;; Apply each multiplier to the value 10
  (let ((results (mapcar (lambda (fn) (funcall fn 10)) multipliers)))
    ;; Also use mapcar to apply each to a different value
    (let ((vals '(1 2 3 4 5))
          (zipped nil))
      (let ((i 0))
        (while (< i (length multipliers))
          (setq zipped
                (cons (funcall (nth i multipliers) (nth i vals))
                      zipped))
          (setq i (1+ i))))
      (list results (nreverse zipped)))))"#;
    let expect = expect_test::expect![[r#""OK ((20 30 50 70 110) (2 6 15 28 55))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_lexical_binding_closure_compose_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a composition chain using closures and reduce-like fold.
    let form = r#"(let ((compose (lambda (f g) (lambda (x) (funcall f (funcall g x))))))
  ;; Compose a chain: add1, double, square, negate
  (let ((fns (list (lambda (x) (+ x 1))
                   (lambda (x) (* x 2))
                   (lambda (x) (* x x))
                   (lambda (x) (- x)))))
    ;; Manual fold: compose all functions right-to-left
    ;; Result: add1(double(square(negate(x))))
    (let ((composed (car (last fns))))
      (let ((rest-fns (nreverse (butlast (reverse fns)))))
        ;; Actually let's just compose step by step
        (let ((chain (lambda (x) (- x))))  ;; negate
          (setq chain (funcall compose (lambda (x) (* x x)) chain))   ;; square . negate
          (setq chain (funcall compose (lambda (x) (* x 2)) chain))   ;; double . square . negate
          (setq chain (funcall compose (lambda (x) (+ x 1)) chain))   ;; add1 . double . square . negate
          (list
            (funcall chain 3)   ;; add1(double(square(negate(3)))) = add1(double(square(-3))) = add1(double(9)) = add1(18) = 19
            (funcall chain 0)   ;; add1(double(square(0))) = 1
            (funcall chain -2)  ;; add1(double(square(2))) = add1(double(4)) = add1(8) = 9
            (funcall chain 1)))))))  ;; add1(double(square(-1))) = add1(double(1)) = add1(2) = 3"#;
    let expect = expect_test::expect![[r#""OK (19 1 9 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lexical binding with defvar (special variable override)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexical_binding_defvar_special_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defvar makes a variable special (dynamically scoped) even within
    // lexical-binding mode.  Verify that let-binding a special variable
    // is visible through the call stack (dynamic), while a non-special
    // variable remains lexical.
    let form = r#"(progn
  (defvar neovm--lbc-dynvar 'initial)

  (fset 'neovm--lbc-read-dyn (lambda () neovm--lbc-dynvar))

  (fset 'neovm--lbc-test-dynamic
    (lambda ()
      (let ((baseline (funcall 'neovm--lbc-read-dyn)))
        (let ((neovm--lbc-dynvar 'rebound))
          ;; Dynamic: reader sees the rebound value
          (let ((during (funcall 'neovm--lbc-read-dyn)))
            ;; Nest deeper
            (let ((neovm--lbc-dynvar 'deep-rebound))
              (let ((deep (funcall 'neovm--lbc-read-dyn)))
                (list baseline during deep))))))))

  (unwind-protect
      (list
        (funcall 'neovm--lbc-test-dynamic)
        ;; After unwind, original value restored
        (funcall 'neovm--lbc-read-dyn))
    (fmakunbound 'neovm--lbc-read-dyn)
    (fmakunbound 'neovm--lbc-test-dynamic)
    (makunbound 'neovm--lbc-dynvar)))"#;
    let expect = expect_test::expect![[r#""OK ((initial rebound deep-rebound) initial)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_lexical_binding_defvar_mixed_with_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mix special (dynamic) and lexical variables in the same let form.
    // The special variable propagates dynamically; the lexical one does not.
    let form = r#"(progn
  (defvar neovm--lbc-mix-dyn 'dyn-default)

  (fset 'neovm--lbc-mix-reader
    (lambda ()
      ;; This reads the dynamic var through the call stack
      neovm--lbc-mix-dyn))

  (unwind-protect
      (let ((neovm--lbc-mix-dyn 'dyn-rebound)
            (lex-var 'lex-value))
        ;; Create a closure capturing lex-var
        (let ((lex-reader (lambda () lex-var)))
          (list
            ;; Dynamic var visible through function call
            (funcall 'neovm--lbc-mix-reader)
            ;; Lexical var captured
            (funcall lex-reader)
            ;; Shadow lexical in new scope — closure unaffected
            (let ((lex-var 'shadowed))
              (list (funcall lex-reader) lex-var))
            ;; Rebind dynamic deeper
            (let ((neovm--lbc-mix-dyn 'dyn-deeper))
              (funcall 'neovm--lbc-mix-reader)))))
    (fmakunbound 'neovm--lbc-mix-reader)
    (makunbound 'neovm--lbc-mix-dyn)))"#;
    let expect =
        expect_test::expect![[r#""OK (dyn-rebound lex-value (lex-value shadowed) dyn-deeper)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: closure-based object with method dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexical_binding_closure_object_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate object-oriented dispatch using closures.  A "bank account"
    // object with deposit, withdraw, balance, and transaction-history methods.
    let form = r#"(let ((make-account
           (lambda (owner initial-balance)
             (let ((balance initial-balance)
                   (txns nil))
               (lambda (method &rest args)
                 (cond
                  ((eq method 'deposit)
                   (let ((amt (car args)))
                     (setq balance (+ balance amt))
                     (setq txns (cons (list 'deposit amt balance) txns))
                     balance))
                  ((eq method 'withdraw)
                   (let ((amt (car args)))
                     (if (> amt balance)
                         'insufficient-funds
                       (setq balance (- balance amt))
                       (setq txns (cons (list 'withdraw amt balance) txns))
                       balance)))
                  ((eq method 'balance) balance)
                  ((eq method 'owner) owner)
                  ((eq method 'history) (reverse txns))
                  (t (list 'unknown-method method))))))))
  (let ((acct1 (funcall make-account "Alice" 1000))
        (acct2 (funcall make-account "Bob" 500)))
    (funcall acct1 'deposit 200)
    (funcall acct1 'withdraw 150)
    (funcall acct2 'deposit 300)
    (funcall acct1 'withdraw 2000)  ;; insufficient
    (funcall acct2 'withdraw 100)
    (list
      (funcall acct1 'owner)
      (funcall acct1 'balance)
      (funcall acct2 'balance)
      (length (funcall acct1 'history))
      (length (funcall acct2 'history))
      (funcall acct1 'history)
      (funcall acct2 'history))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" 1050 700 2 2 ((deposit 200 1200) (withdraw 150 1050)) ((deposit 300 800) (withdraw 100 700)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lexical binding: independent closures from loop iterations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lexical_binding_loop_independent_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classic "closures in a loop" test.  Each iteration's `let` creates a
    // fresh lexical binding, so each closure captures its own value.
    let form = r#"(let ((fns nil))
  (let ((i 0))
    (while (< i 5)
      (let ((captured i))
        (setq fns (cons (lambda () (* captured captured)) fns)))
      (setq i (1+ i))))
  ;; fns are in reverse order: 4^2, 3^2, 2^2, 1^2, 0^2
  (let ((reversed-results (mapcar 'funcall fns))
        (forward-results (mapcar 'funcall (reverse fns))))
    (list reversed-results forward-results)))"#;
    let expect = expect_test::expect![[r#""OK ((16 9 4 1 0) (0 1 4 9 16))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bytecode frames push no lexenv specpdl entry when the function captures no
// environment (GNU bytecode.c setup_frame is specpdl-free). The observable
// surface is the lexbound scan behind internal--define-uninitialized-variable:
// declaring a variable dynamic from inside a compiled env-less function while
// the name is lexically bound in the interpreted caller must match GNU both
// in whether it signals and in what the surrounding scope sees afterwards.
// Regression for the env==None LexicalEnv-push removal in Vm::run_frame.
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_declare_from_compiled_frame_under_interpreted_lexical_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defun neovm--oracle-dv-declare ()
    (internal--define-uninitialized-variable 'neovm--oracle-dv-foo))
  (byte-compile 'neovm--oracle-dv-declare)
  (unwind-protect
      (let ((neovm--oracle-dv-foo 1))
        (list
         (condition-case err
             (progn (neovm--oracle-dv-declare) 'no-signal)
           (error (car err)))
         neovm--oracle-dv-foo
         (special-variable-p 'neovm--oracle-dv-foo)
         ;; a closure over the same name still sees the lexical binding
         (funcall (lambda () neovm--oracle-dv-foo))))
    (fmakunbound 'neovm--oracle-dv-declare)
    (when (boundp 'neovm--oracle-dv-foo)
      (makunbound 'neovm--oracle-dv-foo))))"#;
    crate::common::assert_oracle_parity(form);
}

#[test]
fn oracle_prop_defvar_declare_from_interpreted_frame_under_lexical_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interpreted-caller control for the compiled-frame case above: the
    // interpreter's own LexicalEnv entries (eval/lambda scopes) are untouched
    // by the bytecode-frame change and must keep GNU's behavior.
    let form = r#"(unwind-protect
    (let ((neovm--oracle-dv-bar 2))
      (list
       (condition-case err
           (progn (internal--define-uninitialized-variable 'neovm--oracle-dv-bar)
                  'no-signal)
         (error (car err)))
       neovm--oracle-dv-bar
       (special-variable-p 'neovm--oracle-dv-bar)))
  (when (boundp 'neovm--oracle-dv-bar)
    (makunbound 'neovm--oracle-dv-bar)))"#;
    crate::common::assert_oracle_parity(form);
}
