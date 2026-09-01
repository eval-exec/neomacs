//! Advanced oracle parity tests for `defalias`.
//!
//! Tests `defalias` with docstrings, overwriting, lambda targets, chaining
//! (alias-to-alias), introspection via `symbol-function`, `fboundp`/`fmakunbound`
//! cleanup, and building a command dispatch table.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic defalias creating a function alias to a built-in
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_basic_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defalias 'neovm--da-add '+)
  (unwind-protect
      (list
        (funcall 'neovm--da-add 1 2 3)
        (funcall 'neovm--da-add 10 20)
        (funcall 'neovm--da-add)
        (fboundp 'neovm--da-add))
    (fmakunbound 'neovm--da-add)))"#;
    let expect = expect_test::expect![[r#""OK (6 30 0 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defalias with DOCSTRING parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_with_docstring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // defalias accepts an optional docstring; verify the function works
    // and that documentation-property reflects the docstring
    let form = r#"(progn
  (defalias 'neovm--da-documented
    (lambda (x y) (+ x y))
    "Add two numbers together.")
  (unwind-protect
      (list
        (funcall 'neovm--da-documented 3 4)
        (funcall 'neovm--da-documented 100 200)
        (fboundp 'neovm--da-documented)
        ;; The docstring should be accessible
        (stringp (documentation 'neovm--da-documented)))
    (fmakunbound 'neovm--da-documented)))"#;
    let expect = expect_test::expect![[r#""OK (7 300 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defalias overwriting an existing function definition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_overwrite_existing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defalias 'neovm--da-overwrite (lambda (x) (* x 2)))
  (unwind-protect
      (let ((r1 (funcall 'neovm--da-overwrite 5)))
        (defalias 'neovm--da-overwrite (lambda (x) (* x 10)))
        (let ((r2 (funcall 'neovm--da-overwrite 5)))
          (defalias 'neovm--da-overwrite (lambda (x) (- x 1)))
          (let ((r3 (funcall 'neovm--da-overwrite 5)))
            (list r1 r2 r3))))
    (fmakunbound 'neovm--da-overwrite)))"#;
    let expect = expect_test::expect![[r#""OK (10 50 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defalias to a lambda with complex body (closure-like behavior)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_to_lambda_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defalias 'neovm--da-classify
    (lambda (n)
      (cond
        ((< n 0) 'negative)
        ((= n 0) 'zero)
        ((< n 10) 'small)
        ((< n 100) 'medium)
        (t 'large))))
  (unwind-protect
      (list
        (funcall 'neovm--da-classify -5)
        (funcall 'neovm--da-classify 0)
        (funcall 'neovm--da-classify 7)
        (funcall 'neovm--da-classify 42)
        (funcall 'neovm--da-classify 999)
        ;; Use mapcar with the defalias'd function
        (mapcar 'neovm--da-classify '(-100 -1 0 1 9 10 99 100 1000)))
    (fmakunbound 'neovm--da-classify)))"#;
    let expect = expect_test::expect![[
        r#""OK (negative zero small medium large (negative negative zero small small medium medium large large))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defalias chaining: alias to alias to alias
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a chain: neovm--da-c -> neovm--da-b -> neovm--da-a -> lambda
    // Then verify indirect-function resolves the full chain.
    let form = r#"(progn
  (defalias 'neovm--da-a (lambda (x) (+ x 100)))
  (defalias 'neovm--da-b 'neovm--da-a)
  (defalias 'neovm--da-c 'neovm--da-b)
  (unwind-protect
      (list
        ;; All three should produce the same result
        (funcall 'neovm--da-a 5)
        (funcall 'neovm--da-b 5)
        (funcall 'neovm--da-c 5)
        ;; indirect-function should resolve to the same lambda
        (eq (indirect-function 'neovm--da-a)
            (indirect-function 'neovm--da-c))
        ;; symbol-function at each level
        (symbolp (symbol-function 'neovm--da-c))
        (symbolp (symbol-function 'neovm--da-b))
        (functionp (symbol-function 'neovm--da-a))
        ;; Redefine the base and verify chain follows
        (progn
          (defalias 'neovm--da-a (lambda (x) (* x 999)))
          (funcall 'neovm--da-c 2)))
    (fmakunbound 'neovm--da-a)
    (fmakunbound 'neovm--da-b)
    (fmakunbound 'neovm--da-c)))"#;
    let expect = expect_test::expect![[r#""OK (105 105 105 t t t t 1998)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defalias + symbol-function introspection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_symbol_function_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defalias 'neovm--da-intr (lambda (a b) (cons a b)))
  (unwind-protect
      (let ((fn (symbol-function 'neovm--da-intr)))
        (list
          ;; Can call via funcall on retrieved function
          (funcall fn 'x 'y)
          ;; functionp checks
          (functionp fn)
          (functionp 'neovm--da-intr)
          ;; symbol-function returns the definition, not the symbol
          (symbolp fn)
          ;; Round-trip: fset with the retrieved function
          (progn
            (fset 'neovm--da-intr-copy fn)
            (funcall 'neovm--da-intr-copy 'hello 'world))))
    (fmakunbound 'neovm--da-intr)
    (fmakunbound 'neovm--da-intr-copy)))"#;
    let expect = expect_test::expect![[r#""OK ((x . y) t t nil (hello . world))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defalias + fboundp + fmakunbound lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_fboundp_fmakunbound_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (let ((results nil))
    ;; Initially unbound
    (setq results (cons (fboundp 'neovm--da-lc) results))
    ;; Define it
    (defalias 'neovm--da-lc (lambda () 'alive))
    (setq results (cons (fboundp 'neovm--da-lc) results))
    (setq results (cons (funcall 'neovm--da-lc) results))
    ;; Unbind it
    (fmakunbound 'neovm--da-lc)
    (setq results (cons (fboundp 'neovm--da-lc) results))
    ;; Calling unbound should error
    (setq results
          (cons (condition-case err
                    (progn (funcall 'neovm--da-lc) 'no-error)
                  (void-function 'caught-void))
                results))
    ;; Re-define with new body
    (defalias 'neovm--da-lc (lambda () 'resurrected))
    (setq results (cons (funcall 'neovm--da-lc) results))
    ;; Cleanup
    (fmakunbound 'neovm--da-lc)
    (setq results (cons (fboundp 'neovm--da-lc) results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (nil t alive nil caught-void resurrected nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: command dispatch table using defalias
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_dispatch_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a command dispatch system: register named handlers via defalias,
    // look them up dynamically, execute them, collect results.
    let form = r#"(progn
  ;; Define handler functions
  (defalias 'neovm--dispatch-greet
    (lambda (name) (concat "Hello, " name "!")))
  (defalias 'neovm--dispatch-shout
    (lambda (name) (upcase (concat name "!!!"))))
  (defalias 'neovm--dispatch-reverse
    (lambda (name) (concat (nreverse (string-to-list name)))))
  (defalias 'neovm--dispatch-length
    (lambda (name) (number-to-string (length name))))

  ;; Dispatch table: command-name -> handler symbol
  (defvar neovm--dispatch-table
    '(("greet" . neovm--dispatch-greet)
      ("shout" . neovm--dispatch-shout)
      ("reverse" . neovm--dispatch-reverse)
      ("length" . neovm--dispatch-length)))

  (fset 'neovm--dispatch-run
    (lambda (command arg)
      (let ((entry (assoc command neovm--dispatch-table)))
        (if entry
            (if (fboundp (cdr entry))
                (list 'ok (funcall (cdr entry) arg))
              (list 'error "handler not callable"))
          (list 'error (concat "unknown command: " command))))))

  (unwind-protect
      (let ((results nil))
        ;; Run valid commands
        (setq results (cons (funcall 'neovm--dispatch-run "greet" "Alice") results))
        (setq results (cons (funcall 'neovm--dispatch-run "shout" "Bob") results))
        (setq results (cons (funcall 'neovm--dispatch-run "reverse" "Emacs") results))
        (setq results (cons (funcall 'neovm--dispatch-run "length" "testing") results))
        ;; Unknown command
        (setq results (cons (funcall 'neovm--dispatch-run "dance" "nobody") results))
        ;; Unregister a handler and try again
        (fmakunbound 'neovm--dispatch-shout)
        (setq results (cons (funcall 'neovm--dispatch-run "shout" "Charlie") results))
        ;; Re-register with different behavior
        (defalias 'neovm--dispatch-shout
          (lambda (name) (concat "WHISPER: " (downcase name))))
        (setq results (cons (funcall 'neovm--dispatch-run "shout" "Charlie") results))
        ;; Batch processing: run multiple commands
        (let ((batch '(("greet" . "X") ("reverse" . "abc") ("length" . "hi"))))
          (setq results
                (cons (mapcar (lambda (pair)
                                (funcall 'neovm--dispatch-run (car pair) (cdr pair)))
                              batch)
                      results)))
        (nreverse results))
    (fmakunbound 'neovm--dispatch-greet)
    (fmakunbound 'neovm--dispatch-shout)
    (fmakunbound 'neovm--dispatch-reverse)
    (fmakunbound 'neovm--dispatch-length)
    (fmakunbound 'neovm--dispatch-run)
    (makunbound 'neovm--dispatch-table)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok \"Hello, Alice!\") (ok \"BOB!!!\") (ok \"scamE\") (ok \"7\") (error \"unknown command: dance\") (error \"handler not callable\") (ok \"WHISPER: charlie\") ((ok \"Hello, X!\") (ok \"cba\") (ok \"2\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defalias with autoload-like indirection and predicate composition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defalias_predicate_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build composed predicates using defalias: AND, OR, NOT combinators
    let form = r#"(progn
  (fset 'neovm--pred-and
    (lambda (pred1 pred2)
      (lambda (x) (and (funcall pred1 x) (funcall pred2 x)))))
  (fset 'neovm--pred-or
    (lambda (pred1 pred2)
      (lambda (x) (or (funcall pred1 x) (funcall pred2 x)))))
  (fset 'neovm--pred-not
    (lambda (pred)
      (lambda (x) (not (funcall pred x)))))

  ;; Compose: positive AND even
  (defalias 'neovm--pred-positive (lambda (x) (> x 0)))
  (defalias 'neovm--pred-even (lambda (x) (= (% x 2) 0)))
  (defalias 'neovm--pred-pos-even
    (funcall 'neovm--pred-and
             (symbol-function 'neovm--pred-positive)
             (symbol-function 'neovm--pred-even)))
  ;; Compose: NOT positive OR even
  (defalias 'neovm--pred-neg-or-even
    (funcall 'neovm--pred-or
             (funcall 'neovm--pred-not (symbol-function 'neovm--pred-positive))
             (symbol-function 'neovm--pred-even)))

  (unwind-protect
      (let ((test-vals '(-4 -3 -2 -1 0 1 2 3 4 5 6)))
        (list
          ;; Filter: positive AND even
          (let ((filtered nil))
            (dolist (v test-vals)
              (when (funcall 'neovm--pred-pos-even v)
                (setq filtered (cons v filtered))))
            (nreverse filtered))
          ;; Filter: NOT positive OR even
          (let ((filtered nil))
            (dolist (v test-vals)
              (when (funcall 'neovm--pred-neg-or-even v)
                (setq filtered (cons v filtered))))
            (nreverse filtered))
          ;; Direct predicate checks
          (mapcar 'neovm--pred-positive '(-1 0 1))
          (mapcar 'neovm--pred-even '(1 2 3 4))))
    (fmakunbound 'neovm--pred-and)
    (fmakunbound 'neovm--pred-or)
    (fmakunbound 'neovm--pred-not)
    (fmakunbound 'neovm--pred-positive)
    (fmakunbound 'neovm--pred-even)
    (fmakunbound 'neovm--pred-pos-even)
    (fmakunbound 'neovm--pred-neg-or-even)))"#;
    let expect =
        expect_test::expect![[r#""OK ((2 4 6) (-4 -3 -2 -1 0 2 4 6) (nil nil t) (nil t nil t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
