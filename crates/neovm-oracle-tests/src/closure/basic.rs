//! Oracle parity tests for closure semantics and lexical binding.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_closure_captures_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 10))
                  (let ((f (lambda () x)))
                    (funcall f)))";
    let expect = expect_test::expect![[r#""OK 10""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("10", &o, &n);
}

#[test]
fn oracle_prop_closure_mutation_through_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A closure that mutates a captured variable
    let form = "(let ((counter 0))
                  (let ((inc (lambda () (setq counter (1+ counter))))
                        (get (lambda () counter)))
                    (funcall inc)
                    (funcall inc)
                    (funcall inc)
                    (funcall get)))";
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_prop_closure_shared_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two closures sharing the same captured variable
    let form = "(let ((x 0))
                  (let ((add (lambda (n) (setq x (+ x n))))
                        (get (lambda () x)))
                    (funcall add 5)
                    (funcall add 3)
                    (funcall get)))";
    let expect = expect_test::expect![[r#""OK 8""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("8", &o, &n);
}

#[test]
fn oracle_prop_closure_independent_captures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each closure captures its own copy of the variable
    let form = "(let ((fns nil))
                  (let ((i 0))
                    (while (< i 3)
                      (let ((captured i))
                        (setq fns (cons (lambda () captured) fns)))
                      (setq i (1+ i))))
                  (mapcar 'funcall (reverse fns)))";
    let expect = expect_test::expect![r#""OK (0 1 2)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_closure_over_function_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((make-adder (lambda (n) (lambda (x) (+ n x)))))
                  (let ((add5 (funcall make-adder 5))
                        (add10 (funcall make-adder 10)))
                    (list (funcall add5 3) (funcall add10 3))))";
    let expect = expect_test::expect![[r#""OK (8 13)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(8 13)", &o, &n);
}

#[test]
fn oracle_prop_closure_nested_lets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((a 1))
                  (let ((b 2))
                    (let ((c 3))
                      (let ((f (lambda () (+ a b c))))
                        (funcall f)))))";
    let expect = expect_test::expect![[r#""OK 6""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("6", &o, &n);
}

#[test]
fn oracle_prop_closure_with_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Closure that wraps error handling
    let form = "(let ((safe-div (lambda (a b)
                                  (condition-case nil
                                      (/ a b)
                                    (arith-error 'division-by-zero)))))
                  (list (funcall safe-div 10 2)
                        (funcall safe-div 10 0)
                        (funcall safe-div 15 3)))";
    let expect = expect_test::expect![r#""OK (5 division-by-zero 5)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_closure_as_callback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pass a closure as a callback
    let form = "(let ((apply-twice (lambda (f x) (funcall f (funcall f x)))))
                  (funcall apply-twice (lambda (x) (* x 2)) 3))";
    let expect = expect_test::expect![[r#""OK 12""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("12", &o, &n);
}

#[test]
fn oracle_prop_closure_accumulator_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Closure-based accumulator
    let form = "(let ((acc nil))
                  (let ((push (lambda (x) (setq acc (cons x acc))))
                        (result (lambda () (reverse acc))))
                    (funcall push 'a)
                    (funcall push 'b)
                    (funcall push 'c)
                    (funcall result)))";
    let expect = expect_test::expect![[r#""OK (a b c)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(a b c)", &o, &n);
}

#[test]
fn oracle_prop_closure_compose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Function composition through closures
    let form = "(let ((compose (lambda (f g) (lambda (x) (funcall f (funcall g x))))))
                  (let ((double-then-add1 (funcall compose '1+ (lambda (x) (* x 2)))))
                    (list (funcall double-then-add1 3)
                          (funcall double-then-add1 5)
                          (funcall double-then-add1 0))))";
    let expect = expect_test::expect![[r#""OK (7 11 1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(7 11 1)", &o, &n);
}

#[test]
fn oracle_prop_closure_memoize_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple memoization via closure + hash table
    let form = "(let ((cache (make-hash-table :test 'equal)))
                  (let ((memoized-square
                         (lambda (x)
                           (or (gethash x cache)
                               (puthash x (* x x) cache)))))
                    (list (funcall memoized-square 3)
                          (funcall memoized-square 4)
                          (funcall memoized-square 3)
                          (hash-table-count cache))))";
    let expect = expect_test::expect![[r#""OK (9 16 9 2)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(9 16 9 2)", &o, &n);
}

#[test]
fn oracle_prop_closure_with_rest_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((sum-all (lambda (&rest args)
                                 (let ((total 0))
                                   (while args
                                     (setq total (+ total (car args))
                                           args (cdr args)))
                                   total))))
                  (list (funcall sum-all 1 2 3)
                        (funcall sum-all 10 20)
                        (funcall sum-all)))";
    let expect = expect_test::expect![[r#""OK (6 30 0)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(6 30 0)", &o, &n);
}

#[test]
fn oracle_prop_closure_with_optional_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((greet (lambda (name &optional greeting)
                               (concat (or greeting \"Hello\") \", \" name \"!\"))))
                  (list (funcall greet \"World\")
                        (funcall greet \"World\" \"Hi\")))";
    let expect = expect_test::expect![[r#""OK (\"Hello, World!\" \"Hi, World!\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
