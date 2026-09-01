//! Oracle parity tests for `defalias`, `fset`, `symbol-function`, `fboundp`,
//! `fmakunbound` with complex patterns: docstrings, defalias-vs-fset differences,
//! symbol-function on various types, lifecycle transitions, function wrappers,
//! dispatch tables with dynamic selection, and method resolution order chains.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// defalias with docstring parameter — verify docstring is stored and accessible
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_fset_docstring_stored() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defalias 'neovm--dfp-doc1
    (lambda (x y) (+ x y))
    "Sum two numbers X and Y.")
  (defalias 'neovm--dfp-doc2
    (lambda (s) (upcase s))
    "Uppercase string S.")
  ;; defalias without docstring
  (defalias 'neovm--dfp-doc3
    (lambda (n) (* n n)))
  (unwind-protect
      (list
        ;; Function works correctly
        (funcall 'neovm--dfp-doc1 10 20)
        (funcall 'neovm--dfp-doc2 "hello")
        (funcall 'neovm--dfp-doc3 7)
        ;; Docstring is a string
        (stringp (documentation 'neovm--dfp-doc1))
        (stringp (documentation 'neovm--dfp-doc2))
        ;; Overwrite docstring via new defalias
        (progn
          (defalias 'neovm--dfp-doc1
            (lambda (x y) (- x y))
            "Subtract Y from X now.")
          (funcall 'neovm--dfp-doc1 30 10))
        ;; fboundp still holds after redefine
        (fboundp 'neovm--dfp-doc1))
    (fmakunbound 'neovm--dfp-doc1)
    (fmakunbound 'neovm--dfp-doc2)
    (fmakunbound 'neovm--dfp-doc3)))"#;
    let expect = expect_test::expect![[r#""OK (30 \"HELLO\" 49 t t 20 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defalias vs fset differences: defalias returns the symbol, fset returns the def
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_vs_fset_return_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (let ((da-result (defalias 'neovm--dfp-vs1 (lambda () 'a)))
        (fset-result (fset 'neovm--dfp-vs2 (lambda () 'b))))
    (unwind-protect
        (list
          ;; defalias returns the symbol name
          (symbolp da-result)
          (eq da-result 'neovm--dfp-vs1)
          ;; fset returns the definition (the function itself)
          (functionp fset-result)
          ;; Both functions work
          (funcall 'neovm--dfp-vs1)
          (funcall 'neovm--dfp-vs2)
          ;; defalias can accept a symbol (alias), fset can too
          (progn
            (defalias 'neovm--dfp-vs3 'car)
            (fset 'neovm--dfp-vs4 'cdr)
            (list (funcall 'neovm--dfp-vs3 '(1 2 3))
                  (funcall 'neovm--dfp-vs4 '(1 2 3))))
          ;; Both support overwriting
          (progn
            (defalias 'neovm--dfp-vs1 (lambda () 'a-prime))
            (fset 'neovm--dfp-vs2 (lambda () 'b-prime))
            (list (funcall 'neovm--dfp-vs1)
                  (funcall 'neovm--dfp-vs2))))
      (fmakunbound 'neovm--dfp-vs1)
      (fmakunbound 'neovm--dfp-vs2)
      (fmakunbound 'neovm--dfp-vs3)
      (fmakunbound 'neovm--dfp-vs4))))"#;
    let expect = expect_test::expect![[r#""OK (t t t a b (1 (2 3)) (a-prime b-prime))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fset permits non-functions but rejects cyclic function indirection; defalias
// delegates to the defalias-fset-function property when present.
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fset_non_function_cycle_and_defalias_hook_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/data.c:Ffset stores any Lisp object in a function cell after a
    // cyclic-indirection check.  GNU defalias calls a symbol's
    // `defalias-fset-function` property instead of fset when that property is
    // non-nil, while still applying an explicit docstring afterward.
    let form = r#"
(let ((a 'neomacs--dfp-fcell-a)
      (b 'neomacs--dfp-fcell-b)
      (hook 'neomacs--dfp-fcell-hook)
      (target 'neomacs--dfp-fcell-target))
  (dolist (sym (list a b hook target))
    (ignore-errors (fmakunbound sym))
    (ignore-errors (makunbound sym))
    (setplist sym nil))
  (unwind-protect
      (progn
        (fset hook
              (lambda (sym def)
                (setq neomacs--dfp-fcell-hook-log
                      (list sym def (fboundp sym)))
                'hook-return))
        (put target 'defalias-fset-function hook)
        (setq neomacs--dfp-fcell-hook-log nil)
        (list
         (condition-case err
             (fset nil 1)
           (error (list (car err) (cdr err))))
         (fset nil nil)
         (fboundp nil)
         (condition-case err
             (fmakunbound nil)
           (error (list (car err) (cdr err))))
         (fset a 42)
         (fboundp a)
         (symbol-function a)
         (functionp a)
         (condition-case err
             (funcall a)
           (error (list (car err) (cdr err))))
         (fset a b)
         (condition-case err
             (fset b a)
           (error (list (car err) (cdr err))))
         (defalias target (lambda () 'real) "doc")
         neomacs--dfp-fcell-hook-log
         (fboundp target)
         (symbol-function target)
         (get target 'function-documentation)))
    (ignore-errors (makunbound 'neomacs--dfp-fcell-hook-log))
    (dolist (sym (list a b hook target))
      (ignore-errors (fmakunbound sym))
      (setplist sym nil))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((setting-constant (nil)) nil nil (setting-constant (nil)) 42 t 42 nil (invalid-function (neomacs--dfp-fcell-a)) neomacs--dfp-fcell-b (cyclic-function-indirection (neomacs--dfp-fcell-b)) neomacs--dfp-fcell-target (neomacs--dfp-fcell-target (closure (t) nil 'real) nil) nil nil \"doc\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-function on various types: subr, lambda, macro, alias (symbol)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_fset_symbol_function_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; fset to a lambda
  (fset 'neovm--dfp-sf1 (lambda (x) (* x 2)))
  ;; defalias to a built-in (subr)
  (defalias 'neovm--dfp-sf2 'car)
  ;; defalias chain (symbol -> symbol)
  (defalias 'neovm--dfp-sf3 'neovm--dfp-sf1)
  ;; fset to a macro definition
  (fset 'neovm--dfp-sf4 '(macro . (lambda (x) (list '* x 3))))
  (unwind-protect
      (let ((fn1 (symbol-function 'neovm--dfp-sf1))
            (fn2 (symbol-function 'neovm--dfp-sf2))
            (fn3 (symbol-function 'neovm--dfp-sf3))
            (fn4 (symbol-function 'neovm--dfp-sf4)))
        (list
          ;; Lambda: functionp is t
          (functionp fn1)
          ;; Subr alias: symbol-function returns the symbol (car)
          (eq fn2 'car)
          ;; Chain: symbol-function returns the intermediate symbol
          (eq fn3 'neovm--dfp-sf1)
          (symbolp fn3)
          ;; Macro: consp and (macro . lambda)
          (consp fn4)
          (eq (car fn4) 'macro)
          ;; indirect-function resolves chains
          (functionp (indirect-function 'neovm--dfp-sf3))
          ;; Calling through chain works
          (funcall 'neovm--dfp-sf3 21)
          ;; Calling the macro via macroexpand
          (macroexpand '(neovm--dfp-sf4 5))))
    (fmakunbound 'neovm--dfp-sf1)
    (fmakunbound 'neovm--dfp-sf2)
    (fmakunbound 'neovm--dfp-sf3)
    (fmakunbound 'neovm--dfp-sf4)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t 42 (* 5 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fboundp before/after fset and fmakunbound — full lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_fset_fboundp_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (let ((trace nil))
    ;; Step 1: initially unbound
    (setq trace (cons (fboundp 'neovm--dfp-lc1) trace))
    ;; Step 2: fset makes it bound
    (fset 'neovm--dfp-lc1 (lambda () 'v1))
    (setq trace (cons (fboundp 'neovm--dfp-lc1) trace))
    (setq trace (cons (funcall 'neovm--dfp-lc1) trace))
    ;; Step 3: fmakunbound removes it
    (fmakunbound 'neovm--dfp-lc1)
    (setq trace (cons (fboundp 'neovm--dfp-lc1) trace))
    ;; Step 4: calling unbound signals void-function
    (setq trace (cons
      (condition-case err
          (progn (funcall 'neovm--dfp-lc1) 'no-error)
        (void-function 'caught-void))
      trace))
    ;; Step 5: defalias re-binds
    (defalias 'neovm--dfp-lc1 (lambda () 'v2))
    (setq trace (cons (fboundp 'neovm--dfp-lc1) trace))
    (setq trace (cons (funcall 'neovm--dfp-lc1) trace))
    ;; Step 6: overwrite with fset
    (fset 'neovm--dfp-lc1 (lambda () 'v3))
    (setq trace (cons (funcall 'neovm--dfp-lc1) trace))
    ;; Step 7: symbol-function on unbound after fmakunbound
    (fmakunbound 'neovm--dfp-lc1)
    (setq trace (cons
      (condition-case err
          (symbol-function 'neovm--dfp-lc1)
        (void-function 'sf-void))
      trace))
    ;; Step 8: fmakunbound on already unbound is no-op
    (fmakunbound 'neovm--dfp-lc1)
    (setq trace (cons (fboundp 'neovm--dfp-lc1) trace))
    (nreverse trace)))"#;
    let expect = expect_test::expect![[r#""OK (nil t v1 nil caught-void t v2 v3 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: function wrapper/decorator using defalias
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_fset_function_wrapper() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a generic wrapper (decorator) that adds logging and timing
    // around any function, using fset/defalias to install wrappers.
    let form = r#"(progn
  (defvar neovm--dfp-wrap-log nil)

  ;; make-wrapper: takes original fn, returns wrapped version that
  ;; logs call with args and result
  (fset 'neovm--dfp-make-wrapper
    (lambda (name orig-fn)
      (lambda (&rest args)
        (let ((result (apply orig-fn args)))
          (setq neovm--dfp-wrap-log
                (cons (list 'call name args '=> result)
                      neovm--dfp-wrap-log))
          result))))

  ;; make-validator: wraps fn to validate args are all numbers
  (fset 'neovm--dfp-make-validator
    (lambda (orig-fn)
      (lambda (&rest args)
        (if (cl-every 'numberp args)
            (apply orig-fn args)
          (list 'error "non-numeric arg")))))

  ;; Original functions
  (fset 'neovm--dfp-add (lambda (a b) (+ a b)))
  (fset 'neovm--dfp-mul (lambda (a b) (* a b)))

  (unwind-protect
      (progn
        (setq neovm--dfp-wrap-log nil)

        ;; Wrap add with logging
        (let ((orig-add (symbol-function 'neovm--dfp-add)))
          (fset 'neovm--dfp-add
                (funcall 'neovm--dfp-make-wrapper 'add orig-add)))

        ;; Wrap mul with validation then logging (stacked decorators)
        (let ((orig-mul (symbol-function 'neovm--dfp-mul)))
          (let ((validated (funcall 'neovm--dfp-make-validator orig-mul)))
            (fset 'neovm--dfp-mul
                  (funcall 'neovm--dfp-make-wrapper 'mul validated))))

        ;; Call wrapped functions
        (let ((r1 (funcall 'neovm--dfp-add 3 4))
              (r2 (funcall 'neovm--dfp-add 10 20))
              (r3 (funcall 'neovm--dfp-mul 5 6))
              (r4 (funcall 'neovm--dfp-mul 7 8)))
          (list r1 r2 r3 r4
                (length neovm--dfp-wrap-log)
                (nreverse neovm--dfp-wrap-log))))
    (fmakunbound 'neovm--dfp-make-wrapper)
    (fmakunbound 'neovm--dfp-make-validator)
    (fmakunbound 'neovm--dfp-add)
    (fmakunbound 'neovm--dfp-mul)
    (makunbound 'neovm--dfp-wrap-log)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: dispatch table with dynamic function selection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_fset_dynamic_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a dynamic dispatch system where the dispatch table maps
    // operation names to function symbols, supports fallback handlers,
    // and allows runtime registration/deregistration.
    let form = r#"(progn
  (defvar neovm--dfp-dd-table nil)
  (defvar neovm--dfp-dd-fallback nil)

  (fset 'neovm--dfp-dd-register
    (lambda (op-name fn)
      (let ((sym (intern (concat "neovm--dfp-dd-op-" (symbol-name op-name)))))
        (fset sym fn)
        (setq neovm--dfp-dd-table
              (cons (cons op-name sym)
                    (assq-delete-all op-name neovm--dfp-dd-table)))
        sym)))

  (fset 'neovm--dfp-dd-unregister
    (lambda (op-name)
      (let ((entry (assq op-name neovm--dfp-dd-table)))
        (when entry
          (fmakunbound (cdr entry))
          (setq neovm--dfp-dd-table
                (assq-delete-all op-name neovm--dfp-dd-table))))))

  (fset 'neovm--dfp-dd-dispatch
    (lambda (op-name &rest args)
      (let ((entry (assq op-name neovm--dfp-dd-table)))
        (cond
          ((and entry (fboundp (cdr entry)))
           (apply (cdr entry) args))
          (neovm--dfp-dd-fallback
           (apply neovm--dfp-dd-fallback op-name args))
          (t (list 'error (format "unknown op: %s" op-name)))))))

  (unwind-protect
      (progn
        (setq neovm--dfp-dd-table nil)
        (setq neovm--dfp-dd-fallback
              (lambda (op &rest args)
                (format "FALLBACK[%s]: %S" op args)))

        ;; Register operations
        (funcall 'neovm--dfp-dd-register 'add (lambda (a b) (+ a b)))
        (funcall 'neovm--dfp-dd-register 'sub (lambda (a b) (- a b)))
        (funcall 'neovm--dfp-dd-register 'mul (lambda (a b) (* a b)))
        (funcall 'neovm--dfp-dd-register 'greet
                 (lambda (name) (concat "Hello, " name "!")))

        (let ((r1 (funcall 'neovm--dfp-dd-dispatch 'add 10 20))
              (r2 (funcall 'neovm--dfp-dd-dispatch 'sub 100 37))
              (r3 (funcall 'neovm--dfp-dd-dispatch 'mul 6 7))
              (r4 (funcall 'neovm--dfp-dd-dispatch 'greet "World"))
              ;; Unknown op triggers fallback
              (r5 (funcall 'neovm--dfp-dd-dispatch 'div 10 3)))

          ;; Unregister sub, then try again
          (funcall 'neovm--dfp-dd-unregister 'sub)
          (let ((r6 (funcall 'neovm--dfp-dd-dispatch 'sub 50 25)))
            ;; Re-register sub with different implementation
            (funcall 'neovm--dfp-dd-register 'sub
                     (lambda (a b) (list 'difference (- a b))))
            (let ((r7 (funcall 'neovm--dfp-dd-dispatch 'sub 50 25)))
              ;; Check table state
              (let ((registered-ops
                     (mapcar 'car neovm--dfp-dd-table)))
                (list r1 r2 r3 r4 r5 r6 r7
                      (length neovm--dfp-dd-table)
                      registered-ops))))))
    ;; Clean up dynamically created symbols
    (dolist (entry neovm--dfp-dd-table)
      (fmakunbound (cdr entry)))
    (fmakunbound 'neovm--dfp-dd-register)
    (fmakunbound 'neovm--dfp-dd-unregister)
    (fmakunbound 'neovm--dfp-dd-dispatch)
    (makunbound 'neovm--dfp-dd-table)
    (makunbound 'neovm--dfp-dd-fallback)))"#;
    let expect = expect_test::expect![[
        r#""OK (30 63 42 \"Hello, World!\" \"FALLBACK[div]: (10 3)\" \"FALLBACK[sub]: (50 25)\" (difference 25) 4 (sub greet mul add))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: method resolution order with defalias chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_fset_method_resolution_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a simple MRO (method resolution order) system using defalias.
    // Each "class" has a method table and a parent chain.
    // Method lookup walks the chain until it finds a definition.
    let form = r#"(progn
  (defvar neovm--dfp-mro-classes nil)

  (fset 'neovm--dfp-mro-defclass
    (lambda (name parent)
      (let ((cls (list :name name :parent parent :methods nil)))
        (setq neovm--dfp-mro-classes
              (cons (cons name cls) neovm--dfp-mro-classes))
        cls)))

  (fset 'neovm--dfp-mro-defmethod
    (lambda (class-name method-name fn)
      (let ((cls-entry (assq class-name neovm--dfp-mro-classes)))
        (when cls-entry
          (let ((cls (cdr cls-entry))
                (methods (plist-get (cdr cls-entry) :methods)))
            ;; Add or replace method
            (let ((existing (assq method-name methods)))
              (if existing
                  (setcdr existing fn)
                (plist-put cls :methods
                           (cons (cons method-name fn) methods)))))))))

  (fset 'neovm--dfp-mro-lookup
    (lambda (class-name method-name)
      "Walk MRO chain to find method. Returns (class-found . fn) or nil."
      (let ((current class-name)
            (found nil))
        (while (and current (not found))
          (let ((cls-entry (assq current neovm--dfp-mro-classes)))
            (when cls-entry
              (let ((method (assq method-name
                                  (plist-get (cdr cls-entry) :methods))))
                (if method
                    (setq found (cons current (cdr method)))
                  (setq current (plist-get (cdr cls-entry) :parent)))))))
        found)))

  (fset 'neovm--dfp-mro-call
    (lambda (class-name method-name &rest args)
      (let ((resolution (funcall 'neovm--dfp-mro-lookup class-name method-name)))
        (if resolution
            (list :resolved-in (car resolution)
                  :result (apply (cdr resolution) args))
          (list :error (format "no method %s on %s" method-name class-name))))))

  (unwind-protect
      (progn
        (setq neovm--dfp-mro-classes nil)

        ;; Define class hierarchy: shape -> rectangle -> square
        (funcall 'neovm--dfp-mro-defclass 'shape nil)
        (funcall 'neovm--dfp-mro-defclass 'rectangle 'shape)
        (funcall 'neovm--dfp-mro-defclass 'square 'rectangle)

        ;; shape methods
        (funcall 'neovm--dfp-mro-defmethod 'shape 'describe
                 (lambda (name) (format "%s is a shape" name)))
        (funcall 'neovm--dfp-mro-defmethod 'shape 'color
                 (lambda () "default-gray"))

        ;; rectangle overrides describe, adds area
        (funcall 'neovm--dfp-mro-defmethod 'rectangle 'describe
                 (lambda (name) (format "%s is a rectangle" name)))
        (funcall 'neovm--dfp-mro-defmethod 'rectangle 'area
                 (lambda (w h) (* w h)))

        ;; square overrides area (w=h), inherits describe from rectangle
        (funcall 'neovm--dfp-mro-defmethod 'square 'area
                 (lambda (side _) (* side side)))

        ;; Test resolution
        (let ((results nil))
          ;; square.describe -> resolves in rectangle
          (setq results (cons (funcall 'neovm--dfp-mro-call 'square 'describe "S1") results))
          ;; square.area -> resolves in square
          (setq results (cons (funcall 'neovm--dfp-mro-call 'square 'area 5 5) results))
          ;; square.color -> resolves in shape
          (setq results (cons (funcall 'neovm--dfp-mro-call 'square 'color) results))
          ;; rectangle.area -> resolves in rectangle
          (setq results (cons (funcall 'neovm--dfp-mro-call 'rectangle 'area 3 4) results))
          ;; shape.area -> not found
          (setq results (cons (funcall 'neovm--dfp-mro-call 'shape 'area 1 1) results))
          ;; rectangle.color -> resolves in shape
          (setq results (cons (funcall 'neovm--dfp-mro-call 'rectangle 'color) results))
          ;; shape.describe -> resolves in shape
          (setq results (cons (funcall 'neovm--dfp-mro-call 'shape 'describe "base") results))

          ;; Override square.describe at runtime
          (funcall 'neovm--dfp-mro-defmethod 'square 'describe
                   (lambda (name) (format "%s is a perfect square!" name)))
          (setq results (cons (funcall 'neovm--dfp-mro-call 'square 'describe "S2") results))

          (nreverse results)))
    (fmakunbound 'neovm--dfp-mro-defclass)
    (fmakunbound 'neovm--dfp-mro-defmethod)
    (fmakunbound 'neovm--dfp-mro-lookup)
    (fmakunbound 'neovm--dfp-mro-call)
    (makunbound 'neovm--dfp-mro-classes)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:resolved-in rectangle :result \"S1 is a rectangle\") (:resolved-in square :result 25) (:resolved-in shape :result \"default-gray\") (:resolved-in rectangle :result 12) (:error \"no method area on shape\") (:resolved-in shape :result \"default-gray\") (:resolved-in shape :result \"base is a shape\") (:resolved-in square :result \"S2 is a perfect square!\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fset with nil to void a function, then rebind
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_fset_nil_and_rebind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dfp-nil1 (lambda () 'alive))
  (unwind-protect
      (let ((trace nil))
        ;; Works initially
        (setq trace (cons (funcall 'neovm--dfp-nil1) trace))
        (setq trace (cons (fboundp 'neovm--dfp-nil1) trace))
        ;; fset to nil — symbol-function returns nil, but fboundp is still t
        ;; because the symbol has a function cell (just set to nil)
        (fset 'neovm--dfp-nil1 nil)
        (setq trace (cons (fboundp 'neovm--dfp-nil1) trace))
        (setq trace (cons (symbol-function 'neovm--dfp-nil1) trace))
        ;; Calling nil should error (not a function)
        (setq trace (cons
          (condition-case err
              (progn (funcall 'neovm--dfp-nil1) 'no-error)
            (void-function 'caught-void)
            (invalid-function 'caught-invalid))
          trace))
        ;; Rebind to a real function
        (fset 'neovm--dfp-nil1 (lambda () 'resurrected))
        (setq trace (cons (funcall 'neovm--dfp-nil1) trace))
        ;; fmakunbound truly unbinds
        (fmakunbound 'neovm--dfp-nil1)
        (setq trace (cons (fboundp 'neovm--dfp-nil1) trace))
        (nreverse trace))
    (fmakunbound 'neovm--dfp-nil1)))"#;
    let expect = expect_test::expect![[r#""OK (alive t nil nil caught-void resurrected nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
