//! Oracle parity tests for comprehensive defvar, setq, set, boundp, makunbound
//! interactions: initial values, docstrings, computed symbols, multiple setq,
//! dynamic scoping, variable registry patterns, and configuration management.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// defvar with docstring and initial value - full lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_setq_docstring_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defvar with docstring: value semantics + docstring doesn't affect binding
    // Also: defvar without value is declaration-only (void but special)
    let form = r#"(unwind-protect
    (progn
      ;; defvar with value and docstring
      (defvar neovm--dsq-a 42 "Variable A for testing.")
      ;; defvar without initial value (declaration only)
      (defvar neovm--dsq-b)
      ;; defvar with complex initial value and docstring
      (defvar neovm--dsq-c '(1 2 3) "A list variable.")
      (list
       ;; A is bound with value 42
       (boundp 'neovm--dsq-a) neovm--dsq-a
       ;; B is declared but void
       (boundp 'neovm--dsq-b)
       (condition-case nil neovm--dsq-b (void-variable 'void))
       ;; C is bound with list
       (boundp 'neovm--dsq-c) neovm--dsq-c
       ;; Re-defvar A with different value: should NOT change
       (progn (defvar neovm--dsq-a 99 "New docstring.") neovm--dsq-a)
       ;; But setq can change A
       (progn (setq neovm--dsq-a 100) neovm--dsq-a)
       ;; Re-defvar A still doesn't override setq
       (progn (defvar neovm--dsq-a 200) neovm--dsq-a)
       ;; defvar B with value now: since B is void, this SETS it
       (progn (defvar neovm--dsq-b "hello") neovm--dsq-b)))
  (makunbound 'neovm--dsq-a)
  (makunbound 'neovm--dsq-b)
  (makunbound 'neovm--dsq-c))"#;
    let expect = expect_test::expect![[r#""OK (t 42 nil void t (1 2 3) 42 100 100 \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setq multiple variables at once
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_setq_multiple_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // setq with multiple pairs: evaluates and assigns left-to-right,
    // later assignments can reference earlier ones
    let form = r#"(unwind-protect
    (progn
      (defvar neovm--dsq-m1 nil)
      (defvar neovm--dsq-m2 nil)
      (defvar neovm--dsq-m3 nil)
      (defvar neovm--dsq-m4 nil)
      ;; Multi-setq: each value form can reference previously set vars
      (let ((result-of-setq
             (setq neovm--dsq-m1 10
                   neovm--dsq-m2 (* neovm--dsq-m1 2)
                   neovm--dsq-m3 (+ neovm--dsq-m1 neovm--dsq-m2)
                   neovm--dsq-m4 (list neovm--dsq-m1 neovm--dsq-m2 neovm--dsq-m3))))
        (list
         neovm--dsq-m1       ;; 10
         neovm--dsq-m2       ;; 20
         neovm--dsq-m3       ;; 30
         neovm--dsq-m4       ;; (10 20 30)
         ;; setq returns the last value assigned
         result-of-setq
         ;; Single setq with expression
         (progn
           (setq neovm--dsq-m1 (+ neovm--dsq-m1 neovm--dsq-m2 neovm--dsq-m3))
           neovm--dsq-m1)    ;; 60
         ;; setq with cons/list building
         (progn
           (setq neovm--dsq-m2 (cons 'new neovm--dsq-m4))
           neovm--dsq-m2))))
  (makunbound 'neovm--dsq-m1)
  (makunbound 'neovm--dsq-m2)
  (makunbound 'neovm--dsq-m3)
  (makunbound 'neovm--dsq-m4))"#;
    let expect =
        expect_test::expect![[r#""OK (10 20 30 (10 20 30) (10 20 30) 60 (new 10 20 30))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set with computed symbol name (indirect assignment)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_setq_set_computed_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // set takes a symbol (evaluated) and a value - allows dynamic variable names
    let form = r#"(unwind-protect
    (progn
      (defvar neovm--dsq-dyn-a nil)
      (defvar neovm--dsq-dyn-b nil)
      (defvar neovm--dsq-dyn-c nil)
      ;; set with quoted symbol (standard usage)
      (set 'neovm--dsq-dyn-a 42)
      ;; set with computed symbol name via intern
      (set (intern "neovm--dsq-dyn-b") '(x y z))
      ;; set with symbol from a variable
      (let ((sym 'neovm--dsq-dyn-c))
        (set sym "dynamic"))
      ;; Verify all three
      (list
       neovm--dsq-dyn-a     ;; 42
       neovm--dsq-dyn-b     ;; (x y z)
       neovm--dsq-dyn-c     ;; "dynamic"
       ;; symbol-value is equivalent to accessing the variable
       (symbol-value 'neovm--dsq-dyn-a)
       (symbol-value 'neovm--dsq-dyn-b)
       ;; set returns the value
       (set 'neovm--dsq-dyn-a 'replaced)
       neovm--dsq-dyn-a
       ;; difference: setq vs set
       ;; setq: (setq x val) - x is not evaluated
       ;; set: (set sym val) - sym IS evaluated to get the symbol
       (let ((which-var 'neovm--dsq-dyn-b))
         (set which-var 'via-set)
         neovm--dsq-dyn-b)))
  (makunbound 'neovm--dsq-dyn-a)
  (makunbound 'neovm--dsq-dyn-b)
  (makunbound 'neovm--dsq-dyn-c))"#;
    let expect = expect_test::expect![[
        r#""OK (42 (x y z) \"dynamic\" 42 (x y z) replaced replaced via-set)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// boundp before/after binding with makunbound and rebinding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_setq_boundp_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full lifecycle: unbound -> set -> bound -> makunbound -> unbound -> set -> bound
    // Also test boundp with various types of bindings
    let form = r#"(unwind-protect
    (let ((results nil))
      ;; Before any binding
      (push (boundp 'neovm--dsq-lifecycle) results)
      ;; Bind via setq (creates the binding)
      (defvar neovm--dsq-lifecycle nil)
      (setq neovm--dsq-lifecycle 'first)
      (push (boundp 'neovm--dsq-lifecycle) results)
      (push neovm--dsq-lifecycle results)
      ;; Makunbound
      (makunbound 'neovm--dsq-lifecycle)
      (push (boundp 'neovm--dsq-lifecycle) results)
      ;; Access void variable signals error
      (push (condition-case nil
                neovm--dsq-lifecycle
              (void-variable 'void-error)) results)
      ;; Rebind via set
      (set 'neovm--dsq-lifecycle 'second)
      (push (boundp 'neovm--dsq-lifecycle) results)
      (push neovm--dsq-lifecycle results)
      ;; Makunbound again
      (makunbound 'neovm--dsq-lifecycle)
      (push (boundp 'neovm--dsq-lifecycle) results)
      ;; defvar now works since it's void
      (defvar neovm--dsq-lifecycle 'third)
      (push (boundp 'neovm--dsq-lifecycle) results)
      (push neovm--dsq-lifecycle results)
      ;; Let-binding doesn't affect global boundp after unwind
      (let ((neovm--dsq-lifecycle 'let-bound))
        (push neovm--dsq-lifecycle results))
      ;; After let exits, original value restored
      (push neovm--dsq-lifecycle results)
      (nreverse results))
  (makunbound 'neovm--dsq-lifecycle))"#;
    let expect = expect_test::expect![[
        r#""OK (nil t first nil void-error t second nil t third let-bound third)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defvar without initial value: declares special only
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_setq_declare_special_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defvar without init declares the variable as special (dynamic)
    // This affects how let-bindings work across function calls
    let form = r#"(progn
  ;; Declare special without initial value
  (defvar neovm--dsq-special-only)

  (fset 'neovm--dsq-read-special
    (lambda () neovm--dsq-special-only))

  (unwind-protect
      (list
       ;; Not yet bound
       (boundp 'neovm--dsq-special-only)
       ;; Error on access
       (condition-case nil
           (funcall 'neovm--dsq-read-special)
         (void-variable 'got-void))
       ;; Set it, then read through function
       (progn (setq neovm--dsq-special-only 'global-val) nil)
       (funcall 'neovm--dsq-read-special)
       ;; Let-bind sees dynamic binding
       (let ((neovm--dsq-special-only 'let-bound))
         (funcall 'neovm--dsq-read-special))
       ;; After let, global restored
       (funcall 'neovm--dsq-read-special)
       ;; Nested let-bindings
       (let ((neovm--dsq-special-only 'outer))
         (let ((neovm--dsq-special-only 'inner))
           (let ((inner-val (funcall 'neovm--dsq-read-special)))
             (list inner-val))))
       ;; Global intact
       neovm--dsq-special-only)
    (fmakunbound 'neovm--dsq-read-special)
    (makunbound 'neovm--dsq-special-only)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil got-void nil global-val let-bound global-val (inner) global-val)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: variable registry system using set/boundp/symbol-value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_setq_variable_registry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a variable registry that tracks defined variables with metadata,
    // supports get/set/unset/list operations, and uses actual Elisp symbols
    let form = r#"(progn
  (defvar neovm--dsq-registry nil "List of registered variable symbols.")

  (fset 'neovm--dsq-reg-define
    (lambda (name initial-value doc)
      "Register and define a variable with metadata."
      (let ((sym (intern (concat "neovm--dsq-reg-" (symbol-name name)))))
        ;; Set the variable value
        (set sym initial-value)
        ;; Store metadata on symbol plist
        (put sym 'registry-doc doc)
        (put sym 'registry-type (type-of initial-value))
        (put sym 'registry-default initial-value)
        ;; Track in registry list
        (unless (memq sym neovm--dsq-registry)
          (setq neovm--dsq-registry (cons sym neovm--dsq-registry)))
        sym)))

  (fset 'neovm--dsq-reg-get
    (lambda (name)
      "Get registered variable value."
      (let ((sym (intern (concat "neovm--dsq-reg-" (symbol-name name)))))
        (if (boundp sym)
            (symbol-value sym)
          (error "Not registered: %s" name)))))

  (fset 'neovm--dsq-reg-set
    (lambda (name value)
      "Set registered variable value."
      (let ((sym (intern (concat "neovm--dsq-reg-" (symbol-name name)))))
        (if (memq sym neovm--dsq-registry)
            (set sym value)
          (error "Not registered: %s" name)))))

  (fset 'neovm--dsq-reg-reset
    (lambda (name)
      "Reset registered variable to its default."
      (let ((sym (intern (concat "neovm--dsq-reg-" (symbol-name name)))))
        (set sym (get sym 'registry-default)))))

  (fset 'neovm--dsq-reg-info
    (lambda (name)
      "Get registry info for a variable."
      (let ((sym (intern (concat "neovm--dsq-reg-" (symbol-name name)))))
        (list :name name
              :value (if (boundp sym) (symbol-value sym) 'unbound)
              :doc (get sym 'registry-doc)
              :type (get sym 'registry-type)
              :default (get sym 'registry-default)))))

  (unwind-protect
      (progn
        ;; Register some variables
        (funcall 'neovm--dsq-reg-define 'width 80 "Terminal width")
        (funcall 'neovm--dsq-reg-define 'height 24 "Terminal height")
        (funcall 'neovm--dsq-reg-define 'theme 'dark "UI theme")
        (funcall 'neovm--dsq-reg-define 'verbose nil "Verbose mode")

        (let ((initial-state
               (list (funcall 'neovm--dsq-reg-get 'width)
                     (funcall 'neovm--dsq-reg-get 'height)
                     (funcall 'neovm--dsq-reg-get 'theme)
                     (funcall 'neovm--dsq-reg-get 'verbose))))

          ;; Modify some
          (funcall 'neovm--dsq-reg-set 'width 120)
          (funcall 'neovm--dsq-reg-set 'theme 'light)

          (let ((modified-state
                 (list (funcall 'neovm--dsq-reg-get 'width)
                       (funcall 'neovm--dsq-reg-get 'theme))))

            ;; Reset width to default
            (funcall 'neovm--dsq-reg-reset 'width)

            (list
             initial-state
             modified-state
             ;; After reset
             (funcall 'neovm--dsq-reg-get 'width)
             ;; Registry info
             (funcall 'neovm--dsq-reg-info 'theme)
             ;; Number of registered vars
             (length neovm--dsq-registry)
             ;; All registered vars are bound
             (let ((all-bound t))
               (dolist (sym neovm--dsq-registry)
                 (unless (boundp sym) (setq all-bound nil)))
               all-bound)))))
    ;; Cleanup all registered variables
    (dolist (sym neovm--dsq-registry)
      (makunbound sym))
    (fmakunbound 'neovm--dsq-reg-define)
    (fmakunbound 'neovm--dsq-reg-get)
    (fmakunbound 'neovm--dsq-reg-set)
    (fmakunbound 'neovm--dsq-reg-reset)
    (fmakunbound 'neovm--dsq-reg-info)
    (makunbound 'neovm--dsq-registry)))"#;
    let expect = expect_test::expect![[
        r#""OK ((80 24 dark nil) (120 light) 80 (:name theme :value light :doc \"UI theme\" :type symbol :default dark) 4 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: configuration management with defvar, set, and dynamic scoping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defvar_setq_config_management() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A configuration system using defvar for defaults, let for temporary
    // overrides, and dynamic scoping for nested config contexts
    let form = r#"(progn
  (defvar neovm--dsq-cfg-debug nil)
  (defvar neovm--dsq-cfg-log-level 'info)
  (defvar neovm--dsq-cfg-max-retries 3)
  (defvar neovm--dsq-cfg-timeout 30)

  (fset 'neovm--dsq-cfg-snapshot
    (lambda ()
      "Capture current configuration as an alist."
      (list (cons 'debug neovm--dsq-cfg-debug)
            (cons 'log-level neovm--dsq-cfg-log-level)
            (cons 'max-retries neovm--dsq-cfg-max-retries)
            (cons 'timeout neovm--dsq-cfg-timeout))))

  (fset 'neovm--dsq-cfg-with-debug
    (lambda (body-fn)
      "Execute BODY-FN with debug mode enabled and trace logging."
      (let ((neovm--dsq-cfg-debug t)
            (neovm--dsq-cfg-log-level 'trace))
        (funcall body-fn))))

  (fset 'neovm--dsq-cfg-with-production
    (lambda (body-fn)
      "Execute BODY-FN with production settings."
      (let ((neovm--dsq-cfg-debug nil)
            (neovm--dsq-cfg-log-level 'error)
            (neovm--dsq-cfg-max-retries 5)
            (neovm--dsq-cfg-timeout 60))
        (funcall body-fn))))

  (unwind-protect
      (let ((default-snapshot (funcall 'neovm--dsq-cfg-snapshot)))
        ;; Debug context
        (let ((debug-snapshot
               (funcall 'neovm--dsq-cfg-with-debug 'neovm--dsq-cfg-snapshot)))
          ;; Production context
          (let ((prod-snapshot
                 (funcall 'neovm--dsq-cfg-with-production 'neovm--dsq-cfg-snapshot)))
            ;; Nested: production with debug override
            (let ((nested-snapshot
                   (funcall 'neovm--dsq-cfg-with-production
                            (lambda ()
                              (funcall 'neovm--dsq-cfg-with-debug
                                       'neovm--dsq-cfg-snapshot)))))
              ;; After all contexts: defaults restored
              (let ((restored-snapshot (funcall 'neovm--dsq-cfg-snapshot)))
                ;; Mutate global config
                (setq neovm--dsq-cfg-timeout 120)
                (setq neovm--dsq-cfg-max-retries 10)
                (let ((mutated-snapshot (funcall 'neovm--dsq-cfg-snapshot)))
                  ;; Debug context still gets mutated globals for non-overridden vars
                  (let ((debug-after-mutate
                         (funcall 'neovm--dsq-cfg-with-debug 'neovm--dsq-cfg-snapshot)))
                    (list
                     default-snapshot
                     debug-snapshot
                     prod-snapshot
                     nested-snapshot
                     ;; Defaults restored
                     (equal default-snapshot restored-snapshot)
                     ;; After mutation
                     mutated-snapshot
                     ;; Debug context inherits mutated timeout and max-retries
                     (cdr (assq 'timeout debug-after-mutate))
                     (cdr (assq 'max-retries debug-after-mutate))
                     ;; But debug and log-level are overridden
                     (cdr (assq 'debug debug-after-mutate))
                     (cdr (assq 'log-level debug-after-mutate))))))))))
    (fmakunbound 'neovm--dsq-cfg-snapshot)
    (fmakunbound 'neovm--dsq-cfg-with-debug)
    (fmakunbound 'neovm--dsq-cfg-with-production)
    (makunbound 'neovm--dsq-cfg-debug)
    (makunbound 'neovm--dsq-cfg-log-level)
    (makunbound 'neovm--dsq-cfg-max-retries)
    (makunbound 'neovm--dsq-cfg-timeout)))"#;
    let expect = expect_test::expect![[
        r#""OK (((debug) (log-level . info) (max-retries . 3) (timeout . 30)) ((debug . t) (log-level . trace) (max-retries . 3) (timeout . 30)) ((debug) (log-level . error) (max-retries . 5) (timeout . 60)) ((debug . t) (log-level . trace) (max-retries . 5) (timeout . 60)) t ((debug) (log-level . info) (max-retries . 10) (timeout . 120)) 120 10 t trace)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
