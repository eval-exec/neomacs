//! Advanced oracle parity tests for backquote / quasiquote:
//! nested levels, computed splicing, code generation patterns,
//! and macro-expansion interplay.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Nested backquote with multiple unquote levels
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_nested_two_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two levels of backquote: the outer backquote produces a form that
    // itself contains a backquote.  When we eval the result twice we
    // should get the fully-resolved value.
    let form = r#"(let ((x 1) (y 2))
                    (let ((template `(list ,x `(+ ,,y 3))))
                      (list template (eval (eval template)))))"#;
    let expect = expect_test::expect![r#""ERR (invalid-function 1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ,@ splice at beginning, middle, end, and sole position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_splice_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Splice at beginning
    let form = "(let ((xs '(1 2 3))) `(,@xs 4 5))";
    let expect = expect_test::expect![r#""OK (1 2 3 4 5)""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Splice at end
    let form = "(let ((xs '(4 5 6))) `(1 2 3 ,@xs))";
    let expect = expect_test::expect![r#""OK (1 2 3 4 5 6)""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Splice in middle
    let form = "(let ((xs '(b c d))) `(a ,@xs e))";
    let expect = expect_test::expect![r#""OK (a b c d e)""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Splice as sole contents
    let form = "(let ((xs '(x y z))) `(,@xs))";
    let expect = expect_test::expect![r#""OK (x y z)""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Multiple splices adjacent
    let form = "(let ((a '(1 2)) (b '()) (c '(3))) `(,@a ,@b ,@c))";
    let expect = expect_test::expect![r#""OK (1 2 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Splice of nil (empty) in various positions
    let form = "(let ((e nil)) `(a ,@e b ,@e c))";
    let expect = expect_test::expect![r#""OK (a b c)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backquote constructing defun forms dynamically
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_construct_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use backquote to build a defun form, eval it, call the function,
    // and clean up with unwind-protect.
    let form = r#"(let ((fname 'neovm--test-bq-adv-generated)
                        (arglist '(a b))
                        (body '((+ a b))))
                    (eval `(defun ,fname ,arglist ,@body))
                    (unwind-protect
                        (list (funcall fname 10 20)
                              (funcall fname -3 7))
                      (fmakunbound fname)))"#;
    let expect = expect_test::expect![r#""OK (30 4)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backquote inside macro expansion (defmacro using backquote + ,@)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_macro_expansion_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A macro that builds a pipeline of function calls via backquote:
    //   (neovm--test-bq-pipe x f1 f2 f3) => (f3 (f2 (f1 x)))
    let form = r#"(progn
                    (defmacro neovm--test-bq-pipe (val &rest fns)
                      (let ((result val))
                        (dolist (f fns)
                          (setq result `(funcall ,f ,result)))
                        result))
                    (unwind-protect
                        (let ((double (lambda (x) (* 2 x)))
                              (inc    (lambda (x) (1+ x)))
                              (square (lambda (x) (* x x))))
                          (list
                           (neovm--test-bq-pipe 3 double inc)       ;; (3*2)+1 = 7
                           (neovm--test-bq-pipe 3 inc double)       ;; (3+1)*2 = 8
                           (neovm--test-bq-pipe 2 double double square))) ;; ((2*2)*2)^2 = 64
                      (fmakunbound 'neovm--test-bq-pipe)))"#;
    let expect = expect_test::expect![r#""OK (7 8 64)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_backquote_full_expansion_keeps_equal_literal_tails_distinct() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `backquote-list*' builds a fresh quoted tail for each source
    // occurrence.  Recursive eager expansion must not merge equal cons trees:
    // callers can observe their identity with `eq' (as face/theme code does).
    let form = r#"(let* ((load-in-progress t)
                         (expanded
                          (macroexpand-all
                           '(list
                             `(a ((t (:foreground ,value :weight bold))))
                             `(b ((t (:foreground ,value :weight bold)))))))
                         (first (nth 1 expanded))
                         (second (nth 2 expanded))
                         (tail
                          (lambda (expression)
                            (cadr
                             (nth 2
                                  (nth 2
                                       (nth 2
                                            (nth 1
                                                 (nth 2 expression))))))))
                         (first-tail (funcall tail first))
                         (second-tail (funcall tail second)))
                    (list (equal first-tail second-tail)
                          (eq first-tail second-tail)))"#;
    let expect = expect_test::expect![r#""OK (t nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backquote with computed splicing via mapcar
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_computed_splice_mapcar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // ,@(mapcar ...) to splice computed values
    let form = r#"(let ((vars '(a b c))
                        (vals '(1 2 3)))
                    `(let ,(mapcar (lambda (v val) (list v val))
                                   vars vals)
                       (+ a b c)))"#;
    let expect = expect_test::expect![r#""ERR (wrong-number-of-arguments mapcar 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    // mapcar producing forms to splice into progn
    let form = r#"(let ((names '(x y z))
                        (values '(10 20 30)))
                    `(progn ,@(mapcar (lambda (n v) `(setq ,n ,v))
                                      names values)))"#;
    let expect = expect_test::expect![r#""ERR (wrong-number-of-arguments mapcar 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Eval the generated let form to verify correctness end-to-end
    let form = r#"(let ((vars '(a b c))
                        (vals '(10 20 30)))
                    (eval `(let ,(mapcar #'list vars vals)
                             (list a b c))))"#;
    let expect = expect_test::expect![r#""ERR (wrong-number-of-arguments mapcar 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested backquote pattern: `(a ,(b `(c ,d)))
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_nested_inner_quote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Inner backquote stays quoted; outer comma substitutes
    let form = r#"(let ((x 'hello))
                    `(a ,x `(b ,x)))"#;
    let expect = expect_test::expect![r#""OK (a hello `(b ,x))""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    // With a function call producing the inner value
    let form = r#"(let ((x 5))
                    `(outer ,x ,(+ x 1) `(inner ,,x)))"#;
    let expect = expect_test::expect![r#""OK (outer 5 6 `(inner ,5))""#];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Triple nesting pattern
    let form = r#"(let ((a 1))
                    `(level1 ,a `(level2 ,a ,,a `(level3 ,a ,,a))))"#;
    let expect = expect_test::expect![r#""OK (level1 1 `(level2 ,a ,1 `(level3 ,a ,,a)))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: code generation - generate let forms from alist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_generate_let_from_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a let form from an alist and eval it
    let form = r#"(let ((bindings '((x . 10) (y . 20) (z . 30)))
                        (body-forms '((+ x y z))))
                    (let ((let-form
                           `(let ,(mapcar (lambda (pair)
                                           (list (car pair) (cdr pair)))
                                         bindings)
                              ,@body-forms)))
                      (list let-form (eval let-form))))"#;
    let expect = expect_test::expect![r#""OK ((let ((x 10) (y 20) (z 30)) (+ x y z)) 60)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: pattern-based code rewriter using backquote
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_code_rewriter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A simple code rewriter macro: transforms (neovm--test-bq-rewrite expr)
    // by walking the form and replacing arithmetic operators with
    // checked versions that signal on overflow.
    let form = r#"(progn
                    (defmacro neovm--test-bq-cond-let (bindings &rest body)
                      "A cond-let: bind each var, body runs with all bindings."
                      (let ((let-bindings
                             (mapcar (lambda (b) (list (car b) (cadr b)))
                                     bindings))
                            (checks
                             (mapcar (lambda (b)
                                       `(unless ,(car b)
                                          (error "binding %s is nil" ',(car b))))
                                     bindings)))
                        `(let ,let-bindings
                           ,@checks
                           ,@body)))
                    (unwind-protect
                        (list
                         ;; All bindings non-nil
                         (neovm--test-bq-cond-let ((a 1) (b 2) (c 3))
                           (+ a b c))
                         ;; Verify the macro expansion shape
                         (macroexpand
                          '(neovm--test-bq-cond-let ((x 10) (y 20))
                            (list x y))))
                      (fmakunbound 'neovm--test-bq-cond-let)))"#;
    let expect = expect_test::expect![[
        r#""OK (6 (let ((x 10) (y 20)) (unless x (error \"binding %s is nil\" 'x)) (unless y (error \"binding %s is nil\" 'y)) (list x y)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
