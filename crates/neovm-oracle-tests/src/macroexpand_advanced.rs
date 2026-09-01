//! Advanced oracle parity tests for `macroexpand` and `macroexpand-all`:
//! recursive macro expansion, nested macro calls, backquote/unquote in
//! expansions, macros that expand to other macro calls, expansion with
//! side effects, and `macroexpand-1` vs full expansion.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// macroexpand-1 vs macroexpand: single-step vs full expansion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macroexpand_adv_one_step_vs_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define a chain: mac-a -> mac-b -> mac-c -> final form.
    // macroexpand-1 should expand only one level, macroexpand should
    // chase all the way to the non-macro form.
    let form = r#"(progn
  (defmacro neovm--mea-a (x) `(neovm--mea-b ,x))
  (defmacro neovm--mea-b (x) `(neovm--mea-c ,x))
  (defmacro neovm--mea-c (x) `(+ ,x 1))
  (unwind-protect
      (let* ((one-step (macroexpand-1 '(neovm--mea-a 10)))
             (full     (macroexpand   '(neovm--mea-a 10))))
        (list one-step full
              ;; Verify one-step is not fully expanded
              (equal one-step full)
              ;; macroexpand-1 on non-macro form returns it unchanged
              (macroexpand-1 '(+ 1 2))
              ;; macroexpand-1 from middle of chain
              (macroexpand-1 '(neovm--mea-b 42))
              ;; full from middle
              (macroexpand '(neovm--mea-b 42))))
    (fmakunbound 'neovm--mea-a)
    (fmakunbound 'neovm--mea-b)
    (fmakunbound 'neovm--mea-c)))"#;
    let expect = expect_test::expect![[
        r#""OK ((neovm--mea-b 10) (+ 10 1) nil (+ 1 2) (neovm--mea-c 42) (+ 42 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive expansion: macro that re-invokes itself on smaller input
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macroexpand_adv_recursive_self_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A macro that expands to a progn of (print n) followed by itself with n-1,
    // base case n=0 expands to nil. Test both expansion and evaluation.
    let form = r#"(progn
  (defmacro neovm--mea-countdown (n)
    (if (<= n 0)
        nil
      `(cons ,n (neovm--mea-countdown ,(1- n)))))
  (unwind-protect
      (list
        ;; Evaluate: should produce (5 4 3 2 1)
        (neovm--mea-countdown 5)
        ;; macroexpand-1 only peels one layer
        (macroexpand-1 '(neovm--mea-countdown 3))
        ;; full expansion chases all the way
        (macroexpand '(neovm--mea-countdown 0))
        ;; Evaluate countdown from 1
        (neovm--mea-countdown 1))
    (fmakunbound 'neovm--mea-countdown)))"#;
    let expect =
        expect_test::expect![[r#""OK ((5 4 3 2 1) (cons 3 (neovm--mea-countdown 2)) nil (1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested macros with backquote/unquote/splice in expansion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macroexpand_adv_backquote_splice_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multi-level macro expansion with splicing and nested backquotes.
    let form = r#"(progn
  (defmacro neovm--mea-wrap (&rest body)
    `(let ((neovm--mea-result nil))
       ,@body
       neovm--mea-result))

  (defmacro neovm--mea-collect (lst-expr &rest transforms)
    `(neovm--mea-wrap
      (dolist (neovm--mea-item ,lst-expr)
        (setq neovm--mea-result
              (cons (progn ,@(mapcar (lambda (tr) `(funcall ,tr neovm--mea-item))
                                    transforms))
                    neovm--mea-result)))
      (setq neovm--mea-result (nreverse neovm--mea-result))))

  (unwind-protect
      (list
        ;; Expansion shape check
        (macroexpand-1 '(neovm--mea-wrap (setq x 1)))
        ;; Evaluate: collect squares
        (neovm--mea-collect '(1 2 3 4 5) (lambda (x) (* x x)))
        ;; Evaluate: collect with chained transform (last one wins in progn)
        (neovm--mea-collect '(10 20 30)
                            (lambda (x) (+ x 1))
                            (lambda (x) (* x 2)))
        ;; Empty list
        (neovm--mea-collect nil (lambda (x) x)))
    (fmakunbound 'neovm--mea-wrap)
    (fmakunbound 'neovm--mea-collect)))"#;
    let expect = expect_test::expect![[
        r#""OK ((let ((neovm--mea-result nil)) (setq x 1) neovm--mea-result) (1 4 9 16 25) (20 40 60) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro expanding to another macro call with environment override
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macroexpand_adv_env_override_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use macroexpand with environment to override intermediate macro.
    let form = r#"(progn
  (defmacro neovm--mea-outer (x) `(neovm--mea-inner (* ,x 2)))
  (defmacro neovm--mea-inner (x) `(+ ,x 100))

  (unwind-protect
      (list
        ;; Normal full expansion
        (macroexpand '(neovm--mea-outer 5))
        ;; Override neovm--mea-inner in env to suppress expansion
        (macroexpand '(neovm--mea-outer 5)
                     '((neovm--mea-inner . nil)))
        ;; Override neovm--mea-inner in env with different expansion
        (macroexpand '(neovm--mea-outer 5)
                     '((neovm--mea-inner . (lambda (x) (list 'custom x)))))
        ;; Override the outer macro itself
        (macroexpand '(neovm--mea-outer 5)
                     '((neovm--mea-outer . (lambda (x) (list 'replaced x)))))
        ;; Both overridden
        (macroexpand '(neovm--mea-outer 5)
                     '((neovm--mea-outer . (lambda (x) (list 'neovm--mea-inner x)))
                       (neovm--mea-inner . nil))))
    (fmakunbound 'neovm--mea-outer)
    (fmakunbound 'neovm--mea-inner)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ (* 5 2) 100) (neovm--mea-inner (* 5 2)) (custom (* 5 2)) (replaced 5) (neovm--mea-inner 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro expansion with compile-time computation (side effects during expand)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macroexpand_adv_side_effects_during_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macros can perform computation during expansion.
    // Here the macro computes a lookup table at expand time.
    let form = r#"(progn
  (defvar neovm--mea-expand-count 0)

  (defmacro neovm--mea-counted (x)
    (setq neovm--mea-expand-count (1+ neovm--mea-expand-count))
    `(+ ,x ,neovm--mea-expand-count))

  (unwind-protect
      (progn
        (setq neovm--mea-expand-count 0)
        (let* ((e1 (macroexpand '(neovm--mea-counted 10)))
               (c1 neovm--mea-expand-count)
               (e2 (macroexpand '(neovm--mea-counted 20)))
               (c2 neovm--mea-expand-count)
               (e3 (macroexpand '(neovm--mea-counted 30)))
               (c3 neovm--mea-expand-count))
          ;; Each macroexpand call increments the counter,
          ;; and the expansion captures the counter value at expand time.
          (list e1 c1 e2 c2 e3 c3)))
    (fmakunbound 'neovm--mea-counted)
    (makunbound 'neovm--mea-expand-count)))"#;
    let expect = expect_test::expect![[r#""OK ((+ 10 1) 1 (+ 20 2) 2 (+ 30 3) 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_macroexpand_repeated_calls_preserve_fresh_expansion_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `load-in-progress' is an execution context, not permission to memoize
    // macro results.  GNU invokes the expander for every call, so both its
    // side effects and the identity of freshly allocated expansion data remain
    // observable.
    let form = r#"(progn
  (defvar neovm--mea-fresh-count 0)
  (defmacro neovm--mea-fresh ()
    (setq neovm--mea-fresh-count (1+ neovm--mea-fresh-count))
    (list 'quote (list neovm--mea-fresh-count)))
  (unwind-protect
      (let ((load-in-progress t))
        (setq neovm--mea-fresh-count 0)
        (let ((first (macroexpand '(neovm--mea-fresh)))
              (second (macroexpand '(neovm--mea-fresh))))
          (list neovm--mea-fresh-count
                (eq (cadr first) (cadr second))
                (cadr first)
                (cadr second))))
    (fmakunbound 'neovm--mea-fresh)
    (makunbound 'neovm--mea-fresh-count)))"#;
    let expect = expect_test::expect![r#""OK (2 nil (1) (2))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro that generates nested let bindings from keyword args
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macroexpand_adv_keyword_binding_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A macro that takes key-value pairs and generates nested let forms.
    let form = r#"(progn
  (defmacro neovm--mea-with-bindings (&rest args)
    "Takes alternating key value pairs and body forms.
     Last form is the body. Others are key-value pairs."
    (let ((bindings nil)
          (remaining args))
      ;; Collect pairs until only one form remains
      (while (cdr remaining)
        (let ((var (car remaining))
              (val (cadr remaining)))
          (setq bindings (cons (list var val) bindings))
          (setq remaining (cddr remaining))))
      ;; remaining is the body
      (let ((body (car remaining)))
        `(let ,(nreverse bindings) ,body))))

  (unwind-protect
      (list
        ;; Expansion shape
        (macroexpand '(neovm--mea-with-bindings x 1 y 2 (+ x y)))
        ;; Evaluate
        (neovm--mea-with-bindings x 10 y 20 (+ x y))
        ;; Single binding
        (neovm--mea-with-bindings z 42 (* z 2))
        ;; Bindings can reference earlier ones? No, it's let not let*
        ;; but we can nest
        (neovm--mea-with-bindings a 3 b 4
          (neovm--mea-with-bindings c (+ a b) (* c c))))
    (fmakunbound 'neovm--mea-with-bindings)))"#;
    let expect = expect_test::expect![[r#""OK ((let ((x 1) (y 2)) (+ x y)) 30 84 49)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: anaphoric macro (it-binding) and macro-generating macro
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macroexpand_adv_anaphoric_and_macro_generator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Anaphoric `aif` macro that binds `it` to the test result.
    // Plus a macro that generates simple accessor macros for alist fields.
    let form = r#"(progn
  (defmacro neovm--mea-aif (test then &optional else)
    `(let ((it ,test))
       (if it ,then ,else)))

  (defmacro neovm--mea-awhen (test &rest body)
    `(neovm--mea-aif ,test (progn ,@body)))

  (defmacro neovm--mea-def-accessor (name field)
    `(defmacro ,name (record)
       (list 'cdr (list 'assq '',field record))))

  (unwind-protect
      (list
        ;; aif: test is truthy
        (neovm--mea-aif (assq 'x '((x . 10) (y . 20)))
                        (cdr it)
                        'not-found)
        ;; aif: test is nil
        (neovm--mea-aif (assq 'z '((x . 10) (y . 20)))
                        (cdr it)
                        'not-found)
        ;; awhen: uses aif under the hood
        (neovm--mea-awhen (member 3 '(1 2 3 4 5))
          (length it))
        ;; awhen: nil test
        (neovm--mea-awhen (member 99 '(1 2 3))
          (length it))
        ;; macro-generated accessor
        (progn
          (neovm--mea-def-accessor neovm--mea-get-name name)
          (unwind-protect
              (let ((record '((name . "Alice") (age . 30))))
                (list (neovm--mea-get-name record)
                      (macroexpand '(neovm--mea-get-name some-var))))
            (fmakunbound 'neovm--mea-get-name)))
        ;; Expansion chain: awhen -> aif -> let/if
        (macroexpand-1 '(neovm--mea-awhen test-expr body1 body2)))
    (fmakunbound 'neovm--mea-aif)
    (fmakunbound 'neovm--mea-awhen)
    (fmakunbound 'neovm--mea-def-accessor)))"#;
    let expect = expect_test::expect![[
        r#""OK (10 not-found 3 nil (\"Alice\" (cdr (assq 'name some-var))) (neovm--mea-aif test-expr (progn body1 body2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro that builds a cond-like dispatch table with computed clauses
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_macroexpand_adv_dispatch_table_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A macro that at expansion time processes clauses to build an
    // optimized dispatch. Tests that macro expansion correctly handles
    // complex list manipulation during expansion.
    let form = r#"(progn
  (defmacro neovm--mea-dispatch (val &rest clauses)
    "Each clause is (pattern result). Pattern is a literal value or 'otherwise."
    (let ((var (make-symbol "dispatch-val"))
          (cond-clauses nil))
      (dolist (clause (reverse clauses))
        (let ((pattern (car clause))
              (body (cadr clause)))
          (if (eq pattern 'otherwise)
              (setq cond-clauses (cons `(t ,body) cond-clauses))
            (setq cond-clauses
                  (cons `((equal ,var ',pattern) ,body) cond-clauses)))))
      `(let ((,var ,val))
         (cond ,@cond-clauses))))

  (unwind-protect
      (list
        ;; Basic dispatch
        (neovm--mea-dispatch 'banana
          (apple 1) (banana 2) (cherry 3) (otherwise 0))
        ;; Otherwise case
        (neovm--mea-dispatch 'mango
          (apple 1) (banana 2) (otherwise -1))
        ;; String dispatch
        (neovm--mea-dispatch "hello"
          ("hello" 'greeting) ("bye" 'farewell) (otherwise 'unknown))
        ;; Number dispatch
        (neovm--mea-dispatch (+ 1 2)
          (1 "one") (2 "two") (3 "three") (otherwise "other"))
        ;; Expansion shape
        (macroexpand '(neovm--mea-dispatch x (a 1) (b 2) (otherwise 0)))
        ;; Nested dispatch
        (neovm--mea-dispatch 'inner
          (outer (neovm--mea-dispatch 'x (x 100) (otherwise 0)))
          (inner (neovm--mea-dispatch 'y (x 200) (y 300) (otherwise 0)))
          (otherwise -1)))
    (fmakunbound 'neovm--mea-dispatch)))"#;
    let expect = expect_test::expect![[
        r#""OK (2 -1 greeting \"three\" (let ((dispatch-val x)) (cond ((equal dispatch-val 'a) 1) ((equal dispatch-val 'b) 2) (t 0))) 300)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
