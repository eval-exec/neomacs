//! Oracle parity tests for `caar`, `cadr`, `cdar`, `cddr`, `cdr-safe`,
//! and deeper car/cdr combinations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 2-level: caar, cadr, cdar, cddr
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_caar_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK a""#];
    crate::common::assert_oracle_parity_expect("(caar '((a b) (c d)))", expect);
    let expect = expect_test::expect![r#""OK 1""#];
    crate::common::assert_oracle_parity_expect("(caar '((1 . 2) . (3 . 4)))", expect);
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect("(caar '((nil)))", expect);
}

#[test]
fn oracle_prop_cadr_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK b""#];
    crate::common::assert_oracle_parity_expect("(cadr '(a b c))", expect);
    let expect = expect_test::expect![r#""OK 2""#];
    crate::common::assert_oracle_parity_expect("(cadr '(1 2))", expect);
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect("(cadr '(x))", expect);
}

#[test]
fn oracle_prop_cdar_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (b c)""#];
    crate::common::assert_oracle_parity_expect("(cdar '((a b c) d))", expect);
    let expect = expect_test::expect![r#""OK 2""#];
    crate::common::assert_oracle_parity_expect("(cdar '((1 . 2) 3))", expect);
}

#[test]
fn oracle_prop_cddr_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (c d)""#];
    crate::common::assert_oracle_parity_expect("(cddr '(a b c d))", expect);
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect("(cddr '(1 2))", expect);
    let expect = expect_test::expect![r#""OK c""#];
    crate::common::assert_oracle_parity_expect("(cddr '(a b . c))", expect);
}

// ---------------------------------------------------------------------------
// cdr-safe
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cdr_safe() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (2 3)""#];
    crate::common::assert_oracle_parity_expect("(cdr-safe '(1 2 3))", expect);
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect("(cdr-safe nil)", expect);
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect("(cdr-safe 42)", expect);
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect(r#"(cdr-safe "hello")"#, expect);
    let expect = expect_test::expect![r#""OK b""#];
    crate::common::assert_oracle_parity_expect("(cdr-safe '(a . b))", expect);
}

// ---------------------------------------------------------------------------
// 3-level combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_caaar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK deep""#];
    crate::common::assert_oracle_parity_expect("(caaar '(((deep) mid) top))", expect);
}

#[test]
fn oracle_prop_caadr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK second-car""#];
    crate::common::assert_oracle_parity_expect("(caadr '(first (second-car rest) third))", expect);
}

#[test]
fn oracle_prop_caddr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK c""#];
    // caddr = third element
    crate::common::assert_oracle_parity_expect("(caddr '(a b c d e))", expect);
    let expect = expect_test::expect![r#""OK 3""#];
    crate::common::assert_oracle_parity_expect("(caddr '(1 2 3))", expect);
}

#[test]
fn oracle_prop_cadddr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK d""#];
    // cadddr = fourth element
    crate::common::assert_oracle_parity_expect("(cadddr '(a b c d e))", expect);
}

// ---------------------------------------------------------------------------
// Complex: destructuring with car/cdr combos
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_destructure_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use car/cdr combos to destructure association list entries
    let form = "(let ((entries '((name . \"Alice\")
                                  (age . 30)
                                  (role . engineer))))
                  (list (caar entries)
                        (cdar entries)
                        (caadr entries)
                        (cdadr entries)
                        (car (caddr entries))
                        (cdr (caddr entries))))";
    let expect = expect_test::expect![[r#""OK (name \"Alice\" age 30 role engineer)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_car_cdr_tree_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Navigate a tree structure using car/cdr combos
    let form = "(let ((tree '((a (b c)) (d (e f)) (g (h i)))))
                  (list
                    ;; First subtree
                    (caar tree)
                    (caadar tree)
                    (car (cdadar tree))
                    ;; Second subtree
                    (caadr tree)
                    ;; Third subtree
                    (caaddr tree)))";
    let expect = expect_test::expect![r#""OK (a b c d g)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_car_cdr_safe_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // car-safe/cdr-safe for safe navigation of potentially non-list values
    let form = "(let ((data '((a 1) nil (c 3))))
                  (list (car-safe (car data))
                        (car-safe (cadr data))
                        (cdr-safe (car data))
                        (cdr-safe (cadr data))
                        (car-safe (caddr data))))";
    let expect = expect_test::expect![r#""OK (a nil (1) nil c)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_car_cdr_build_and_navigate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a complex structure then navigate it
    let form = "(let ((s (cons (cons (cons 'deep nil)
                                    (cons 'mid nil))
                              (cons (cons 'right nil)
                                    'end))))
                  (list (caaar s)
                        (cdaar s)
                        (caadr s)
                        (cddr s)
                        (cdar s)))";
    let expect = expect_test::expect![r#""OK (deep nil right end (mid))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_nth_via_car_cdr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify nth equivalence with car/cdr chains
    let form = "(let ((lst '(a b c d e f)))
                  (list (eq (nth 0 lst) (car lst))
                        (eq (nth 1 lst) (cadr lst))
                        (eq (nth 2 lst) (caddr lst))
                        (eq (nth 3 lst) (cadddr lst))
                        (equal (nthcdr 2 lst) (cddr lst))
                        (equal (nthcdr 3 lst) (cdddr lst))))";
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(t t t t t t)", &o, &n);
}

#[test]
fn oracle_prop_car_cdr_lambda_list_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate parsing a lambda-list: (name args body...)
    let form = "(let ((defn '(my-func (x y &optional z)
                               (+ x y (or z 0)))))
                  (let ((name (car defn))
                        (args (cadr defn))
                        (body (cddr defn))
                        (required-args
                         (let ((result nil)
                               (remaining (cadr defn)))
                           (while (and remaining
                                       (not (eq (car remaining)
                                                '&optional)))
                             (setq result (cons (car remaining) result)
                                   remaining (cdr remaining)))
                           (nreverse result)))
                        (optional-args
                         (let ((found nil)
                               (remaining (cadr defn)))
                           (while (and remaining
                                       (not (eq (car remaining)
                                                '&optional)))
                             (setq remaining (cdr remaining)))
                           (when remaining
                             (setq found (cdr remaining)))
                           found)))
                    (list name required-args optional-args
                          (length body))))";
    let expect = expect_test::expect![r#""OK (my-func (x y) (z) 1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
