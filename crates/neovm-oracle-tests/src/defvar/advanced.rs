//! Oracle parity tests for advanced `defvar` semantics:
//! initial value vs void, redefinition preservation, boundp interaction,
//! makunbound + re-initialization, complex initial values,
//! dynamic scoping across function boundaries, let-binding of specials,
//! and a defvar-based configuration system with defaults.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// defvar with initial value vs without (void semantics)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_adv_initial_value_vs_void() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defvar without value: variable is declared special but remains void
    // defvar with value: variable is declared special and bound
    // Accessing a void variable signals void-variable error
    let form = r#"(progn
  (defvar neovm--test-dv-void1)
  (unwind-protect
      (list
        ;; declared but void
        (boundp 'neovm--test-dv-void1)
        ;; accessing void signals error
        (condition-case err
            (progn neovm--test-dv-void1 'no-error)
          (void-variable 'got-void-error))
        ;; now give it a value
        (progn (defvar neovm--test-dv-void1 42) nil)
        (boundp 'neovm--test-dv-void1)
        neovm--test-dv-void1)
    (makunbound 'neovm--test-dv-void1)))"#;
    let expect = expect_test::expect![[r#""OK (nil got-void-error nil t 42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defvar redefinition: should NOT change existing value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_adv_redefinition_preserves() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Once bound, subsequent defvar with different init is ignored
    // But setq can still change it. Then defvar again still doesn't override.
    let form = r#"(unwind-protect
    (progn
      (defvar neovm--test-dv-redef 'first)
      (let ((v1 neovm--test-dv-redef))
        (defvar neovm--test-dv-redef 'second)
        (let ((v2 neovm--test-dv-redef))
          (setq neovm--test-dv-redef 'mutated)
          (let ((v3 neovm--test-dv-redef))
            (defvar neovm--test-dv-redef 'third)
            (let ((v4 neovm--test-dv-redef))
              (list v1 v2 v3 v4))))))
  (makunbound 'neovm--test-dv-redef))"#;
    let expect = expect_test::expect![[r#""OK (first first mutated mutated)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// boundp before and after defvar, makunbound + re-initialization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_adv_boundp_makunbound_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full lifecycle: unbound -> defvar -> bound -> makunbound -> unbound -> defvar -> bound
    let form = r#"(unwind-protect
    (let ((results nil))
      ;; Start: should be unbound
      (setq results (cons (boundp 'neovm--test-dv-cycle) results))
      ;; defvar binds it
      (defvar neovm--test-dv-cycle 100)
      (setq results (cons (boundp 'neovm--test-dv-cycle) results))
      (setq results (cons neovm--test-dv-cycle results))
      ;; makunbound removes binding
      (makunbound 'neovm--test-dv-cycle)
      (setq results (cons (boundp 'neovm--test-dv-cycle) results))
      ;; defvar again should re-bind since it's void now
      (defvar neovm--test-dv-cycle 200)
      (setq results (cons (boundp 'neovm--test-dv-cycle) results))
      (setq results (cons neovm--test-dv-cycle results))
      (nreverse results))
  (makunbound 'neovm--test-dv-cycle))"#;
    let expect = expect_test::expect![[r#""OK (nil t 100 nil t 200)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defvar with complex initial values (lists, hash tables, lambdas)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_adv_complex_initial_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defvar init form is evaluated only once; complex structures preserved
    let form = r#"(unwind-protect
    (progn
      ;; List as initial value
      (defvar neovm--test-dv-list '(a b (c d) ((e))))
      ;; Vector
      (defvar neovm--test-dv-vec [1 2 3 4 5])
      ;; Computed value (the init form is evaluated)
      (defvar neovm--test-dv-computed (mapcar '1+ '(10 20 30)))
      ;; Alist
      (defvar neovm--test-dv-alist '((key1 . val1) (key2 . val2) (key3 . val3)))
      ;; Nested computation
      (defvar neovm--test-dv-nested
        (let ((x 10))
          (list x (* x 2) (* x 3))))
      (list
        neovm--test-dv-list
        (append neovm--test-dv-vec nil)
        neovm--test-dv-computed
        (cdr (assq 'key2 neovm--test-dv-alist))
        neovm--test-dv-nested))
  (makunbound 'neovm--test-dv-list)
  (makunbound 'neovm--test-dv-vec)
  (makunbound 'neovm--test-dv-computed)
  (makunbound 'neovm--test-dv-alist)
  (makunbound 'neovm--test-dv-nested))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b (c d) ((e))) (1 2 3 4 5) (11 21 31) val2 (10 20 30))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic scoping with defvar across function calls
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_adv_dynamic_scoping_across_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defvar makes a variable dynamically scoped
    // Inner function sees the let-binding from outer caller, not the global
    let form = r#"(progn
  (defvar neovm--test-dv-dynscope 'global)

  (fset 'neovm--test-dv-reader
    (lambda () neovm--test-dv-dynscope))

  (fset 'neovm--test-dv-caller
    (lambda (val)
      (let ((neovm--test-dv-dynscope val))
        ;; reader sees the dynamic binding, not the global
        (funcall 'neovm--test-dv-reader))))

  (unwind-protect
      (list
        ;; Direct read: global
        (funcall 'neovm--test-dv-reader)
        ;; Through caller with let-binding: sees 'local-a
        (funcall 'neovm--test-dv-caller 'local-a)
        ;; Global still intact after call returns
        (funcall 'neovm--test-dv-reader)
        ;; Nested dynamic rebinding
        (let ((neovm--test-dv-dynscope 'outer))
          (list
            (funcall 'neovm--test-dv-reader)
            (funcall 'neovm--test-dv-caller 'inner)
            (funcall 'neovm--test-dv-reader)))
        ;; Global still intact
        neovm--test-dv-dynscope)
    (fmakunbound 'neovm--test-dv-reader)
    (fmakunbound 'neovm--test-dv-caller)
    (makunbound 'neovm--test-dv-dynscope)))"#;
    let expect =
        expect_test::expect![[r#""OK (global local-a global (outer inner outer) global)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let binding of defvar variable (dynamic binding + unwind)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_adv_let_binding_unwind_on_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Dynamic binding must be properly unwound even when an error occurs
    let form = r#"(progn
  (defvar neovm--test-dv-unwind 'original)

  (unwind-protect
      (list
        ;; Normal let-binding and restore
        (let ((before neovm--test-dv-unwind))
          (let ((neovm--test-dv-unwind 'rebound))
            nil)
          (list before neovm--test-dv-unwind))
        ;; Error inside let — binding must still be restored
        (progn
          (condition-case nil
              (let ((neovm--test-dv-unwind 'error-context))
                (error "boom"))
            (error nil))
          neovm--test-dv-unwind)
        ;; Nested lets with unwind
        (progn
          (let ((neovm--test-dv-unwind 'level1))
            (condition-case nil
                (let ((neovm--test-dv-unwind 'level2))
                  (error "inner boom"))
              (error nil))
            ;; level1 should be restored after inner let unwinds
            neovm--test-dv-unwind))
        ;; Final value
        neovm--test-dv-unwind)
    (makunbound 'neovm--test-dv-unwind)))"#;
    let expect = expect_test::expect![[r#""OK ((original original) original level1 original)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defvar with docstring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_adv_docstring_and_symbol_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defvar with docstring should still work for value semantics
    // Also test interaction with symbol properties via put/get
    let form = r#"(unwind-protect
    (progn
      (defvar neovm--test-dv-doc 42 "The answer to everything.")
      (put 'neovm--test-dv-doc 'custom-prop 'hello)
      (put 'neovm--test-dv-doc 'another-prop '(1 2 3))
      (list
        neovm--test-dv-doc
        (get 'neovm--test-dv-doc 'custom-prop)
        (get 'neovm--test-dv-doc 'another-prop)
        (boundp 'neovm--test-dv-doc)
        ;; symbol-plist should contain our properties
        (let ((plist (symbol-plist 'neovm--test-dv-doc))
              (has-custom nil)
              (has-another nil))
          (while plist
            (when (eq (car plist) 'custom-prop)
              (setq has-custom t))
            (when (eq (car plist) 'another-prop)
              (setq has-another t))
            (setq plist (cddr plist)))
          (list has-custom has-another))))
  (makunbound 'neovm--test-dv-doc))"#;
    let expect = expect_test::expect![[r#""OK (42 hello (1 2 3) t (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: defvar-based configuration system with defaults
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_adv_config_system_with_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a mini config system: defvar provides defaults, let for overrides,
    // a "read config" function merges defaults with overrides
    let form = r#"(progn
  (defvar neovm--test-cfg-width 80)
  (defvar neovm--test-cfg-height 24)
  (defvar neovm--test-cfg-color 'white)
  (defvar neovm--test-cfg-verbose nil)

  (fset 'neovm--test-cfg-get-all
    (lambda ()
      (list (cons 'width neovm--test-cfg-width)
            (cons 'height neovm--test-cfg-height)
            (cons 'color neovm--test-cfg-color)
            (cons 'verbose neovm--test-cfg-verbose))))

  (fset 'neovm--test-cfg-with-overrides
    (lambda (overrides body-fn)
      (let ((neovm--test-cfg-width
             (or (cdr (assq 'width overrides)) neovm--test-cfg-width))
            (neovm--test-cfg-height
             (or (cdr (assq 'height overrides)) neovm--test-cfg-height))
            (neovm--test-cfg-color
             (or (cdr (assq 'color overrides)) neovm--test-cfg-color))
            (neovm--test-cfg-verbose
             (or (cdr (assq 'verbose overrides)) neovm--test-cfg-verbose)))
        (funcall body-fn))))

  (unwind-protect
      (list
        ;; Default config
        (funcall 'neovm--test-cfg-get-all)
        ;; Override some values
        (funcall 'neovm--test-cfg-with-overrides
                 '((width . 120) (color . blue))
                 'neovm--test-cfg-get-all)
        ;; Defaults restored after override scope
        (funcall 'neovm--test-cfg-get-all)
        ;; Nested overrides
        (funcall 'neovm--test-cfg-with-overrides
                 '((width . 100))
                 (lambda ()
                   (let ((outer (funcall 'neovm--test-cfg-get-all)))
                     (funcall 'neovm--test-cfg-with-overrides
                              '((height . 50) (verbose . t))
                              (lambda ()
                                (list outer
                                      (funcall 'neovm--test-cfg-get-all)))))))
        ;; Still defaults
        (cdr (assq 'width (funcall 'neovm--test-cfg-get-all))))
    (fmakunbound 'neovm--test-cfg-get-all)
    (fmakunbound 'neovm--test-cfg-with-overrides)
    (makunbound 'neovm--test-cfg-width)
    (makunbound 'neovm--test-cfg-height)
    (makunbound 'neovm--test-cfg-color)
    (makunbound 'neovm--test-cfg-verbose)))"#;
    let expect = expect_test::expect![[
        r#""OK (((width . 80) (height . 24) (color . white) (verbose)) ((width . 120) (height . 24) (color . blue) (verbose)) ((width . 80) (height . 24) (color . white) (verbose)) (((width . 100) (height . 24) (color . white) (verbose)) ((width . 100) (height . 50) (color . white) (verbose . t))) 80)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
