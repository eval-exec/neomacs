//! Oracle parity tests for advanced `let*` patterns:
//! sequential binding, shadowing, complex initialization,
//! and interaction with closures.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Sequential binding dependency chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star_dependency_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let* ((a 1)
                       (b (+ a 1))
                       (c (* b 2))
                       (d (expt c 2))
                       (e (- d a)))
                  (list a b c d e))";
    let expect = expect_test::expect![[r#""OK (1 2 4 16 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_let_star_accumulating() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let* ((items '(3 1 4 1 5 9))
                       (sum (apply #'+ items))
                       (count (length items))
                       (avg (/ (float sum) count))
                       (max-val (apply #'max items))
                       (min-val (apply #'min items))
                       (range (- max-val min-val)))
                  (list sum count avg max-val min-val range))";
    let expect = expect_test::expect![[r#""OK (23 6 3.8333333333333335 9 1 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let* with closures that capture sequential bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each closure captures a different stage of sequential binding
    let form = "(let* ((base 10)
                       (get-base (lambda () base))
                       (multiplied (* base 3))
                       (get-mult (lambda () multiplied))
                       (combined (+ base multiplied))
                       (get-all (lambda ()
                                  (list base multiplied combined))))
                  (list (funcall get-base)
                        (funcall get-mult)
                        (funcall get-all)))";
    let expect = expect_test::expect![[r#""OK (10 30 (10 30 40))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let* vs let comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_vs_let_star_parallel() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // let binds all in parallel (old values), let* is sequential
    let form = "(let ((x 1) (y 2))
                  (list
                    ;; let: both see original x=1, y=2
                    (let ((x y) (y x))
                      (list x y))
                    ;; let*: x gets y=2, then y gets new x=2
                    (let* ((x y) (y x))
                      (list x y))))";
    let expect = expect_test::expect![[r#""OK ((2 1) (2 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let* for builder pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star_builder_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((config nil)
                         (config (plist-put config :host "localhost"))
                         (config (plist-put config :port 8080))
                         (config (plist-put config :debug t))
                         (config (plist-put config :timeout 30))
                         (host (plist-get config :host))
                         (url (format "http://%s:%d"
                                      host
                                      (plist-get config :port))))
                    (list config url))"#;
    let expect = expect_test::expect![[
        r#""OK ((:host \"localhost\" :port 8080 :debug t :timeout 30) \"http://localhost:8080\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let* with destructuring patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star_destructure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let* ((pair '(hello . world))
                       (first (car pair))
                       (second (cdr pair))
                       (triple '(1 2 3))
                       (a (car triple))
                       (b (cadr triple))
                       (c (caddr triple))
                       (nested '((a 1) (b 2) (c 3)))
                       (keys (mapcar #'car nested))
                       (vals (mapcar #'cadr nested)))
                  (list first second a b c keys vals))";
    let expect = expect_test::expect![[r#""OK (hello world 1 2 3 (a b c) (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
