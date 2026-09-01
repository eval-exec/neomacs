//! Oracle parity tests for recursive functions.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_recursion_factorial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--test-fact
                        (lambda (n)
                          (if (<= n 1) 1
                            (* n (funcall 'neovm--test-fact (1- n))))))
                  (unwind-protect
                      (list (funcall 'neovm--test-fact 0)
                            (funcall 'neovm--test-fact 1)
                            (funcall 'neovm--test-fact 5)
                            (funcall 'neovm--test-fact 10))
                    (fmakunbound 'neovm--test-fact)))";
    let expect = expect_test::expect![[r#""OK (1 1 120 3628800)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(1 1 120 3628800)", &o, &n);
}

#[test]
fn oracle_prop_recursion_fibonacci() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--test-fib
                        (lambda (n)
                          (cond
                            ((= n 0) 0)
                            ((= n 1) 1)
                            (t (+ (funcall 'neovm--test-fib (- n 1))
                                  (funcall 'neovm--test-fib (- n 2)))))))
                  (unwind-protect
                      (mapcar 'neovm--test-fib '(0 1 2 3 4 5 6 7 8 9 10))
                    (fmakunbound 'neovm--test-fib)))";
    let expect = expect_test::expect![[r#""OK (0 1 1 2 3 5 8 13 21 34 55)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_recursion_list_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Recursive list length
    let form = "(progn
                  (fset 'neovm--test-my-length
                        (lambda (lst)
                          (if (null lst) 0
                            (1+ (funcall 'neovm--test-my-length (cdr lst))))))
                  (unwind-protect
                      (list (funcall 'neovm--test-my-length nil)
                            (funcall 'neovm--test-my-length '(a))
                            (funcall 'neovm--test-my-length '(a b c d e)))
                    (fmakunbound 'neovm--test-my-length)))";
    let expect = expect_test::expect![[r#""OK (0 1 5)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(0 1 5)", &o, &n);
}

#[test]
fn oracle_prop_recursion_flatten() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Recursive tree flattening
    let form = "(progn
                  (fset 'neovm--test-flatten
                        (lambda (tree)
                          (cond
                            ((null tree) nil)
                            ((atom tree) (list tree))
                            (t (append (funcall 'neovm--test-flatten (car tree))
                                       (funcall 'neovm--test-flatten (cdr tree)))))))
                  (unwind-protect
                      (funcall 'neovm--test-flatten '(1 (2 (3 4) 5) (6 7)))
                    (fmakunbound 'neovm--test-flatten)))";
    let expect = expect_test::expect![[r#""OK (1 2 3 4 5 6 7)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(1 2 3 4 5 6 7)", &o, &n);
}

#[test]
fn oracle_prop_recursion_tail_recursive_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tail-recursive accumulator
    let form = "(progn
                  (fset 'neovm--test-sum-acc
                        (lambda (lst acc)
                          (if (null lst) acc
                            (funcall 'neovm--test-sum-acc
                                     (cdr lst)
                                     (+ acc (car lst))))))
                  (unwind-protect
                      (funcall 'neovm--test-sum-acc '(1 2 3 4 5 6 7 8 9 10) 0)
                    (fmakunbound 'neovm--test-sum-acc)))";
    let expect = expect_test::expect![[r#""OK 55""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("55", &o, &n);
}

#[test]
fn oracle_prop_recursion_mutual() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Mutual recursion: even?/odd?
    let form = "(progn
                  (fset 'neovm--test-my-even
                        (lambda (n)
                          (if (= n 0) t
                            (funcall 'neovm--test-my-odd (1- n)))))
                  (fset 'neovm--test-my-odd
                        (lambda (n)
                          (if (= n 0) nil
                            (funcall 'neovm--test-my-even (1- n)))))
                  (unwind-protect
                      (list (funcall 'neovm--test-my-even 0)
                            (funcall 'neovm--test-my-even 1)
                            (funcall 'neovm--test-my-even 4)
                            (funcall 'neovm--test-my-odd 3)
                            (funcall 'neovm--test-my-odd 6))
                    (fmakunbound 'neovm--test-my-even)
                    (fmakunbound 'neovm--test-my-odd)))";
    let expect = expect_test::expect![[r#""OK (t nil t t nil)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(t nil t t nil)", &o, &n);
}

#[test]
fn oracle_prop_recursion_tree_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--test-tree-depth
                        (lambda (tree)
                          (if (atom tree) 0
                            (1+ (max (funcall 'neovm--test-tree-depth (car tree))
                                     (funcall 'neovm--test-tree-depth (cdr tree)))))))
                  (unwind-protect
                      (list (funcall 'neovm--test-tree-depth 'leaf)
                            (funcall 'neovm--test-tree-depth '(a))
                            (funcall 'neovm--test-tree-depth '(a (b (c))))
                            (funcall 'neovm--test-tree-depth '((a b) (c d))))
                    (fmakunbound 'neovm--test-tree-depth)))";
    let expect = expect_test::expect![[r#""OK (0 1 5 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_recursion_ackermann() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Ackermann function — tests deep recursion
    let form = "(progn
                  (fset 'neovm--test-ack
                        (lambda (m n)
                          (cond
                            ((= m 0) (1+ n))
                            ((= n 0) (funcall 'neovm--test-ack (1- m) 1))
                            (t (funcall 'neovm--test-ack
                                        (1- m)
                                        (funcall 'neovm--test-ack m (1- n)))))))
                  (unwind-protect
                      (list (funcall 'neovm--test-ack 0 0)
                            (funcall 'neovm--test-ack 1 1)
                            (funcall 'neovm--test-ack 2 2)
                            (funcall 'neovm--test-ack 3 3))
                    (fmakunbound 'neovm--test-ack)))";
    let expect = expect_test::expect![[r#""OK (1 3 7 61)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(1 3 7 61)", &o, &n);
}

#[test]
fn oracle_prop_recursion_map_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Recursive map over a tree structure
    let form = "(progn
                  (fset 'neovm--test-map-tree
                        (lambda (f tree)
                          (if (atom tree)
                              (funcall f tree)
                            (cons (funcall 'neovm--test-map-tree f (car tree))
                                  (funcall 'neovm--test-map-tree f (cdr tree))))))
                  (unwind-protect
                      (funcall 'neovm--test-map-tree '1+ '(1 (2 3) ((4) 5)))
                    (fmakunbound 'neovm--test-map-tree)))";
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_recursion_with_catch_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Recursion that uses throw to short-circuit
    let form = "(progn
                  (fset 'neovm--test-find-deep
                        (lambda (tree target)
                          (cond
                            ((null tree) nil)
                            ((equal tree target) (throw 'found t))
                            ((atom tree) nil)
                            (t (funcall 'neovm--test-find-deep (car tree) target)
                               (funcall 'neovm--test-find-deep (cdr tree) target)))))
                  (unwind-protect
                      (list
                        (catch 'found (funcall 'neovm--test-find-deep '(1 (2 (3 4) 5) 6) 4))
                        (catch 'found (funcall 'neovm--test-find-deep '(1 (2 (3 4) 5) 6) 99)))
                    (fmakunbound 'neovm--test-find-deep)))";
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_recursion_factorial_proptest(
        n in 0u32..12u32,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--test-fact-p
                     (lambda (n) (if (<= n 1) 1 (* n (funcall 'neovm--test-fact-p (1- n))))))
               (unwind-protect
                   (funcall 'neovm--test-fact-p {})
                 (fmakunbound 'neovm--test-fact-p)))",
            n
        );
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }
}
