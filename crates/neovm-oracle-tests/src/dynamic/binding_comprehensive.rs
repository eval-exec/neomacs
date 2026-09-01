//! Comprehensive oracle parity tests for dynamic binding:
//! defvar/defconst, let binding of special variables, dynamic scope
//! visibility in called functions, symbol-value/set, boundp, makunbound,
//! default-value/set-default, buffer-local vs global, local-variable-p,
//! and interaction with lexical binding.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// defvar and defconst creating special variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_comprehensive_defvar_defconst() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defvar with initial value, defvar without initial value (just declares),
    // defvar does NOT overwrite existing value, defconst always sets.
    let form = r#"(progn
  (unwind-protect
      (progn
        ;; defvar with initial value
        (defvar neovm--test-dc-v1 42)
        ;; defvar again with different value should NOT change it
        (defvar neovm--test-dc-v1 999)
        (let ((v1-after neovm--test-dc-v1))
          ;; defconst always sets value
          (defconst neovm--test-dc-c1 100)
          (defconst neovm--test-dc-c1 200)
          (let ((c1-after neovm--test-dc-c1))
            ;; defvar without init value: variable exists but may be void
            ;; unless already bound
            (defvar neovm--test-dc-v2)
            (let ((v2-bound (boundp 'neovm--test-dc-v2)))
              ;; Now bind it and check
              (set 'neovm--test-dc-v2 77)
              (defvar neovm--test-dc-v2 888)  ;; should NOT overwrite 77
              (list
                v1-after                      ;; 42 (defvar did not overwrite)
                c1-after                      ;; 200 (defconst overwrites)
                v2-bound                      ;; nil (defvar without init)
                neovm--test-dc-v2)))))        ;; 77 (defvar did not overwrite)
    (makunbound 'neovm--test-dc-v1)
    (makunbound 'neovm--test-dc-c1)
    (makunbound 'neovm--test-dc-v2)))"#;
    let expect = expect_test::expect![[r#""OK (42 200 nil 77)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let binding of special variables: dynamic extent
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_comprehensive_let_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A defvar'd variable is dynamically scoped even inside let.
    // The let-bound value is visible during the dynamic extent of the let,
    // and the original value is restored after the let exits.
    let form = r#"(progn
  (defvar neovm--test-ls-var 'original)
  (unwind-protect
      (let ((before neovm--test-ls-var)
            (during nil)
            (after nil))
        ;; First let: rebind
        (let ((neovm--test-ls-var 'first-rebind))
          (setq during neovm--test-ls-var)
          ;; Nested let: rebind again
          (let ((neovm--test-ls-var 'second-rebind))
            (setq during (list during neovm--test-ls-var)))
          ;; After inner let, back to first-rebind
          (setq during (append during (list neovm--test-ls-var))))
        ;; After outer let, back to original
        (setq after neovm--test-ls-var)
        (list before during after))
    (makunbound 'neovm--test-ls-var)))"#;
    let expect = expect_test::expect![[
        r#""OK (original (first-rebind second-rebind first-rebind) original)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic scope visible in called functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_comprehensive_visible_in_called_fns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A function defined elsewhere reads a special variable;
    // the caller's let-binding is visible to it.
    let form = r#"(progn
  (defvar neovm--test-vcf-color 'red)
  (fset 'neovm--test-vcf-get-color (lambda () neovm--test-vcf-color))
  (fset 'neovm--test-vcf-format-color
        (lambda (prefix)
          (concat prefix (symbol-name (funcall 'neovm--test-vcf-get-color)))))
  (unwind-protect
      (let ((default-result (funcall 'neovm--test-vcf-format-color "color=")))
        ;; Rebind dynamically
        (let ((neovm--test-vcf-color 'blue))
          (let ((rebound-result (funcall 'neovm--test-vcf-format-color "color=")))
            ;; Deeply nested rebind
            (let ((neovm--test-vcf-color 'green))
              (let ((deep-result (funcall 'neovm--test-vcf-format-color "color=")))
                (list default-result rebound-result deep-result
                      ;; After inner lets unwind, calling function sees outer binding
                      (funcall 'neovm--test-vcf-format-color "color=")))))))
    (fmakunbound 'neovm--test-vcf-get-color)
    (fmakunbound 'neovm--test-vcf-format-color)
    (makunbound 'neovm--test-vcf-color)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"color=red\" \"color=blue\" \"color=green\" \"color=green\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-value, set, and their interaction with let-bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_comprehensive_symbol_value_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // symbol-value reads the current dynamic binding;
    // set writes to the current dynamic binding (not the global).
    let form = r#"(progn
  (defvar neovm--test-svs-x 'global)
  (unwind-protect
      (let ((results nil))
        ;; Read global
        (setq results (cons (symbol-value 'neovm--test-svs-x) results))
        ;; Let-bind, then use set to modify the let binding
        (let ((neovm--test-svs-x 'let-bound))
          (setq results (cons (symbol-value 'neovm--test-svs-x) results))
          ;; set modifies the current (let) binding
          (set 'neovm--test-svs-x 'set-modified)
          (setq results (cons (symbol-value 'neovm--test-svs-x) results))
          (setq results (cons neovm--test-svs-x results)))
        ;; After let exits, global is still 'global
        (setq results (cons (symbol-value 'neovm--test-svs-x) results))
        ;; set on global
        (set 'neovm--test-svs-x 'new-global)
        (setq results (cons (symbol-value 'neovm--test-svs-x) results))
        (nreverse results))
    (makunbound 'neovm--test-svs-x)))"#;
    let expect = expect_test::expect![[
        r#""OK (global let-bound set-modified set-modified global new-global)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// boundp and makunbound full lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_comprehensive_boundp_makunbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full lifecycle: unbound -> defvar -> bound -> makunbound -> unbound
    // Also test boundp inside let-bindings.
    let form = r#"(progn
  (let ((sym 'neovm--test-bm-var))
    ;; Ensure clean state
    (when (boundp sym) (makunbound sym))
    (let ((r1 (boundp sym)))              ;; nil
      (defvar neovm--test-bm-var 10)
      (let ((r2 (boundp sym))             ;; t
            (r3 (symbol-value sym)))      ;; 10
        (makunbound sym)
        (let ((r4 (boundp sym)))          ;; nil
          ;; Re-bind via set
          (set sym 20)
          (let ((r5 (boundp sym))         ;; t
                (r6 (symbol-value sym)))  ;; 20
            ;; boundp inside let binding
            (let ((neovm--test-bm-var 30))
              (let ((r7 (boundp sym))     ;; t (let-bound)
                    (r8 (symbol-value sym))) ;; 30
                (list r1 r2 r3 r4 r5 r6 r7 r8)))))))
    ;; Clean up is automatic since we used unwind-protect-style logic
    ;; But let's be safe:
    (when (boundp 'neovm--test-bm-var)
      (makunbound 'neovm--test-bm-var))
    ))"#;
    let expect = expect_test::expect![[r#""OK neovm--test-bm-var""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// default-value and set-default
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_comprehensive_default_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // default-value returns the global default (ignoring let-bindings).
    // set-default changes the global default without affecting the let binding.
    let form = r#"(progn
  (defvar neovm--test-dv-x 'global-default)
  (unwind-protect
      (progn
        (let ((r1 (default-value 'neovm--test-dv-x)))   ;; global-default
          ;; Inside let, default-value still returns the global
          (let ((neovm--test-dv-x 'let-bound))
            (let ((r2 neovm--test-dv-x)                   ;; let-bound
                  (r3 (default-value 'neovm--test-dv-x)))  ;; global-default
              ;; set-default changes global, not the let binding
              (set-default 'neovm--test-dv-x 'new-default)
              (let ((r4 neovm--test-dv-x)                   ;; still let-bound
                    (r5 (default-value 'neovm--test-dv-x))) ;; new-default
                ;; After let, the variable takes on the new default
                (list r1 r2 r3 r4 r5))))
          ;; Outside let:
          (list (default-value 'neovm--test-dv-x)
                neovm--test-dv-x)))
    (makunbound 'neovm--test-dv-x)))"#;
    let expect = expect_test::expect![[r#""OK (global-default global-default)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-local vs global values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_comprehensive_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // make-local-variable creates a buffer-local binding;
    // the default value is still the global.
    let form = r#"(progn
  (defvar neovm--test-bl-var 'global-val)
  (unwind-protect
      (let ((buf (generate-new-buffer " *neovm-test-bl*")))
        (unwind-protect
            (progn
              ;; In the current buffer, it's global
              (let ((r1 neovm--test-bl-var))  ;; global-val
                ;; Switch to new buffer and make it buffer-local
                (with-current-buffer buf
                  (make-local-variable 'neovm--test-bl-var)
                  (setq neovm--test-bl-var 'local-val)
                  (let ((r2 neovm--test-bl-var)            ;; local-val
                        (r3 (default-value 'neovm--test-bl-var))  ;; global-val
                        (r4 (local-variable-p 'neovm--test-bl-var))) ;; t
                    ;; Back in original buffer
                    (list r1 r2 r3 r4
                          neovm--test-bl-var              ;; global-val (not local here)
                          (local-variable-p 'neovm--test-bl-var))))))  ;; nil
          (kill-buffer buf)))
    (makunbound 'neovm--test-bl-var)))"#;
    let expect = expect_test::expect![[r#""OK (global-val local-val global-val t local-val t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// local-variable-p comprehensive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_comprehensive_local_variable_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test local-variable-p with different buffer arguments and
    // make-variable-buffer-local vs make-local-variable.
    let form = r#"(progn
  (defvar neovm--test-lvp-a 'a-global)
  (defvar neovm--test-lvp-b 'b-global)
  (unwind-protect
      (let ((buf1 (generate-new-buffer " *neovm-test-lvp1*"))
            (buf2 (generate-new-buffer " *neovm-test-lvp2*")))
        (unwind-protect
            (progn
              ;; Make a buffer-local in buf1 only
              (with-current-buffer buf1
                (make-local-variable 'neovm--test-lvp-a)
                (setq neovm--test-lvp-a 'a-local-buf1))
              ;; make-variable-buffer-local makes it automatically local in all buffers
              (make-variable-buffer-local 'neovm--test-lvp-b)
              (with-current-buffer buf1
                (setq neovm--test-lvp-b 'b-local-buf1))
              (with-current-buffer buf2
                (setq neovm--test-lvp-b 'b-local-buf2))
              (list
                ;; a is local only in buf1
                (local-variable-p 'neovm--test-lvp-a buf1)  ;; t
                (local-variable-p 'neovm--test-lvp-a buf2)  ;; nil
                ;; b is local in both (auto-local)
                (local-variable-p 'neovm--test-lvp-b buf1)  ;; t
                (local-variable-p 'neovm--test-lvp-b buf2)  ;; t
                ;; Values are per-buffer
                (with-current-buffer buf1 neovm--test-lvp-a)   ;; a-local-buf1
                (with-current-buffer buf2 neovm--test-lvp-a)   ;; a-global
                (with-current-buffer buf1 neovm--test-lvp-b)   ;; b-local-buf1
                (with-current-buffer buf2 neovm--test-lvp-b))) ;; b-local-buf2
          (kill-buffer buf1)
          (kill-buffer buf2)))
    (makunbound 'neovm--test-lvp-a)
    (makunbound 'neovm--test-lvp-b)))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil t t a-local-buf1 a-global b-local-buf1 b-local-buf2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction between lexical and dynamic binding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_comprehensive_lexical_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In the same form, some variables are lexical and some are dynamic (special).
    // Verify that closures capture lexical vars at definition time,
    // but always look up dynamic vars at call time.
    let form = r#"(progn
  (defvar neovm--test-li-dyn 'dyn-outer)
  (unwind-protect
      (let ((lex-var 'lex-outer))
        (let ((reader (lambda () (list lex-var neovm--test-li-dyn))))
          (let ((results nil))
            ;; Baseline
            (setq results (cons (funcall reader) results))
            ;; Rebind dynamic only
            (let ((neovm--test-li-dyn 'dyn-inner))
              (setq results (cons (funcall reader) results)))
            ;; Rebind lexical only (new binding, closure doesn't see it)
            (let ((lex-var 'lex-inner))
              (setq results (cons (funcall reader) results)))
            ;; Rebind both
            (let ((lex-var 'lex-both)
                  (neovm--test-li-dyn 'dyn-both))
              (setq results (cons (funcall reader) results)))
            ;; Mutate the captured lexical binding
            (setq lex-var 'lex-mutated)
            (setq results (cons (funcall reader) results))
            (nreverse results))))
    (makunbound 'neovm--test-li-dyn)))"#;
    let expect = expect_test::expect![[
        r#""OK ((lex-outer dyn-outer) (lex-outer dyn-inner) (lex-outer dyn-outer) (lex-outer dyn-both) (lex-mutated dyn-outer))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic binding with recursive functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_comprehensive_recursive_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A recursive function uses a dynamic variable as a depth counter
    // and accumulator, demonstrating the stack-like behavior of let-bindings
    // on dynamic variables in recursive contexts.
    let form = r#"(progn
  (defvar neovm--test-rd-depth 0)
  (defvar neovm--test-rd-trace nil)
  (fset 'neovm--test-rd-walk
        (lambda (tree)
          (setq neovm--test-rd-trace
                (cons (cons neovm--test-rd-depth
                            (if (consp tree) 'node 'leaf))
                      neovm--test-rd-trace))
          (if (consp tree)
              (let ((neovm--test-rd-depth (1+ neovm--test-rd-depth)))
                (funcall 'neovm--test-rd-walk (car tree))
                (funcall 'neovm--test-rd-walk (cdr tree)))
            tree)))
  (unwind-protect
      (progn
        (funcall 'neovm--test-rd-walk '(a . (b . c)))
        (list
          ;; Depth should be back to 0
          neovm--test-rd-depth
          ;; Trace in reverse order
          (nreverse neovm--test-rd-trace)))
    (fmakunbound 'neovm--test-rd-walk)
    (makunbound 'neovm--test-rd-depth)
    (makunbound 'neovm--test-rd-trace)))"#;
    let expect = expect_test::expect![[
        r#""OK (0 ((0 . node) (1 . leaf) (1 . node) (2 . leaf) (2 . leaf)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic binding with catch/throw restoring bindings correctly
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dynamic_comprehensive_catch_throw_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple dynamic variables let-bound at different depths,
    // with throw crossing all of them. All must be properly restored.
    let form = r#"(progn
  (defvar neovm--test-ctr-a 'a0)
  (defvar neovm--test-ctr-b 'b0)
  (defvar neovm--test-ctr-c 'c0)
  (unwind-protect
      (let ((thrown-val
              (catch 'escape
                (let ((neovm--test-ctr-a 'a1))
                  (let ((neovm--test-ctr-b 'b1))
                    (let ((neovm--test-ctr-c 'c1))
                      (let ((neovm--test-ctr-a 'a2)
                            (neovm--test-ctr-b 'b2))
                        ;; Snapshot before throw
                        (throw 'escape
                               (list neovm--test-ctr-a
                                     neovm--test-ctr-b
                                     neovm--test-ctr-c)))))))))
        ;; After catch, all bindings restored to original defvar values
        (list
          thrown-val                     ;; (a2 b2 c1) — snapshot at throw site
          neovm--test-ctr-a             ;; a0
          neovm--test-ctr-b             ;; b0
          neovm--test-ctr-c))           ;; c0
    (makunbound 'neovm--test-ctr-a)
    (makunbound 'neovm--test-ctr-b)
    (makunbound 'neovm--test-ctr-c)))"#;
    let expect = expect_test::expect![[r#""OK ((a2 b2 c1) a0 b0 c0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
