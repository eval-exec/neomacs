//! Oracle parity tests for `sort` with complex predicates and patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_sort_by_abs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(sort (list 3 -1 4 -1 5 -9 2 -6)
                      (lambda (a b) (< (abs a) (abs b))))";
    let expect = expect_test::expect![[r#""OK (-1 -1 2 3 4 5 -6 -9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_sort_strings_by_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(sort (list "cc" "aaa" "b" "dddd" "")
                        (lambda (a b)
                          (< (length a) (length b))))"#;
    let expect = expect_test::expect![[r#""OK (\"\" \"b\" \"cc\" \"aaa\" \"dddd\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_sort_alist_by_cdr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(sort (list '(a . 3) '(b . 1) '(c . 4) '(d . 1) '(e . 5))
                      (lambda (x y) (< (cdr x) (cdr y))))";
    let expect = expect_test::expect![[r#""OK ((b . 1) (d . 1) (a . 3) (c . 4) (e . 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_sort_stable_for_equal_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sort pairs by first element; second element shows original order
    let form = "(sort (list '(1 . a) '(2 . b) '(1 . c) '(2 . d) '(1 . e))
                      (lambda (x y) (< (car x) (car y))))";
    let expect = expect_test::expect![[r#""OK ((1 . a) (1 . c) (1 . e) (2 . b) (2 . d))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_sort_reverse_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(sort (list 1 5 3 2 4) '>)";
    let expect = expect_test::expect![[r#""OK (5 4 3 2 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_sort_with_string_lessp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(sort (list "banana" "apple" "cherry" "date")
                        'string-lessp)"#;
    let expect = expect_test::expect![[r#""OK (\"apple\" \"banana\" \"cherry\" \"date\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_sort_single_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(sort (list 42) '<)";
    let expect = expect_test::expect![[r#""OK (42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_sort_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(sort nil '<)";
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_sort_with_multi_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sort by first key, then second key
    let form = "(sort (list '(1 b) '(2 a) '(1 a) '(2 b) '(1 c))
                      (lambda (x y)
                        (or (< (car x) (car y))
                            (and (= (car x) (car y))
                                 (string-lessp
                                  (symbol-name (cadr x))
                                  (symbol-name (cadr y)))))))";
    let expect = expect_test::expect![[r#""OK ((1 a) (1 b) (1 c) (2 a) (2 b))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_sort_is_destructive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // sort is destructive — result may differ from input pointer
    let form = "(let ((lst (list 3 1 4 1 5)))
                  (let ((sorted (sort lst '<)))
                    (equal sorted '(1 1 3 4 5))))";
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_sort_ascending(
        a in -100i64..100i64,
        b in -100i64..100i64,
        c in -100i64..100i64,
        d in -100i64..100i64,
        e in -100i64..100i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(sort (list {} {} {} {} {}) '<)",
            a, b, c, d, e
        );
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }
}
