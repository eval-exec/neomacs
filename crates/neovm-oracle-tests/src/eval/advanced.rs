//! Oracle parity tests for advanced `eval` usage:
//! eval with lexical environments, eval of dynamically constructed
//! forms, eval in macroexpand patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// eval basic forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_quoted_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect("(eval '(+ 1 2 3))", expect);
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect("(eval '(list 1 2 3))", expect);
    let expect = expect_test::expect![[r#""OK hello""#]];
    crate::common::assert_oracle_parity_expect("(eval ''hello)", expect);
    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect("(eval 42)", expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(eval "hello")"#, expect);
}

#[test]
fn oracle_prop_eval_constructed_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Dynamically construct and evaluate forms
    let form = "(let ((op '+)
                      (args '(1 2 3 4 5)))
                  (eval (cons op args)))";
    let expect = expect_test::expect![[r#""OK 15""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("15", &o, &n);
}

#[test]
fn oracle_prop_eval_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(eval (eval '(quote (+ 1 2))))", expect);
}

// ---------------------------------------------------------------------------
// eval with let-constructed forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_dynamic_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a let form dynamically and eval it
    let form = "(let ((bindings '((x 10) (y 20)))
                      (body '(+ x y)))
                  (eval (list 'let bindings body)))";
    let expect = expect_test::expect![[r#""OK 30""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("30", &o, &n);
}

#[test]
fn oracle_prop_eval_dynamic_cond() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a cond form dynamically
    let form = "(let ((clauses (list (list '(= 1 2) 'branch-a)
                                    (list '(= 2 2) 'branch-b)
                                    (list t 'default))))
                  (eval (cons 'cond clauses)))";
    let expect = expect_test::expect![[r#""ERR (void-variable branch-b)""#]];
    // Under lexical binding, `eval` without a lexical environment argument
    // evaluates in a null lexical environment, so the quoted symbol `branch-b`
    // inside `cond` is treated as a variable reference that is unbound.
    // Both GNU Emacs and NeoVM signal (void-variable branch-b).
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_eq!(n, o, "neovm and oracle should match");
}

// ---------------------------------------------------------------------------
// eval in metaprogramming patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_code_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate and evaluate code
    let form = "(let ((gen-adder
                       (lambda (n)
                         (list 'lambda '(x) (list '+ 'x n)))))
                  (let ((add5 (eval (funcall gen-adder 5)))
                        (add10 (eval (funcall gen-adder 10))))
                    (list (funcall add5 3)
                          (funcall add10 3))))";
    let expect = expect_test::expect![[r#""OK (8 13)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_eval_template_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Backquote-like template expansion via eval
    let form = "(let ((make-checker
                       (lambda (field value)
                         (list 'lambda '(record)
                               (list 'equal
                                     (list 'cdr
                                           (list 'assq
                                                 (list 'quote field)
                                                 'record))
                                     (list 'quote value))))))
                  (let ((is-alice (eval (funcall make-checker
                                                 'name 'alice)))
                        (is-bob (eval (funcall make-checker
                                               'name 'bob))))
                    (let ((record '((name . alice) (age . 30))))
                      (list (funcall is-alice record)
                            (funcall is-bob record)))))";
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval with progn and multiple forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_progn_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((forms '((setq neovm--test-eval-tmp 0)
                                (setq neovm--test-eval-tmp
                                      (1+ neovm--test-eval-tmp))
                                (setq neovm--test-eval-tmp
                                      (1+ neovm--test-eval-tmp))
                                neovm--test-eval-tmp)))
                  (eval (cons 'progn forms)))";
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("2", &o, &n);
}

// ---------------------------------------------------------------------------
// Complex: mini test framework using eval
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eval_test_framework() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple assertion framework
    let form = "(let ((tests '(((= (+ 1 2) 3) . \"1+2=3\")
                                ((= (* 3 4) 12) . \"3*4=12\")
                                ((= (- 10 7) 3) . \"10-7=3\")
                                ((string= (concat \"a\" \"b\") \"ab\")
                                 . \"concat\")
                                ((= (length '(1 2 3)) 3)
                                 . \"length\")))
                      (passed 0) (failed 0) (failures nil))
                  (dolist (test tests)
                    (if (eval (car test))
                        (setq passed (1+ passed))
                      (setq failed (1+ failed)
                            failures (cons (cdr test) failures))))
                  (list passed failed (nreverse failures)))";
    let expect = expect_test::expect![[r#""OK (5 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
