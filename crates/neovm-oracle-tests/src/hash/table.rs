//! Oracle parity tests for hash-table operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_hash_table_put_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table :test 'equal))) (puthash \"key\" 42 h) (gethash \"key\" h))",
        expect,
    );
    assert_ok_eq("42", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    // missing key returns nil
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table))) (gethash 'missing h))",
        expect,
    );
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK fallback""#]];
    // missing key with default
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table))) (gethash 'missing h 'fallback))",
        expect,
    );
    assert_ok_eq("fallback", &o, &n);

    let expect = expect_test::expect![[r#""OK 2""#]];
    // overwrite existing key
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table))) (puthash 'k 1 h) (puthash 'k 2 h) (gethash 'k h))",
        expect,
    );
    assert_ok_eq("2", &o, &n);
}

#[test]
fn oracle_prop_hash_table_remhash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table))) (puthash 'a 1 h) (remhash 'a h) (gethash 'a h))",
        expect,
    );
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK 0""#]];
    // remhash on missing key is harmless
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table))) (remhash 'gone h) (hash-table-count h))",
        expect,
    );
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_prop_hash_table_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table))) (puthash 'a 1 h) (puthash 'b 2 h) (puthash 'c 3 h) (hash-table-count h))",
        expect,
    );
    assert_ok_eq("3", &o, &n);

    let expect = expect_test::expect![[r#""OK 0""#]];
    // empty table
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(hash-table-count (make-hash-table))", expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_prop_hash_table_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(hash-table-p (make-hash-table))", expect);
    assert_ok_eq("t", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(hash-table-p '(not a table))", expect);
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(hash-table-p 42)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_hash_table_clrhash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table))) (puthash 'x 1 h) (puthash 'y 2 h) (clrhash h) (hash-table-count h))",
        expect,
    );
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_prop_hash_table_equal_structural_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK hit""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table :test 'equal))) (puthash (list 1 2 3) 'hit h) (gethash (list 1 2 3) h))",
        expect,
    );
    assert_ok_eq("hit", &o, &n);

    let expect = expect_test::expect![[r#""OK vec""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table :test 'equal))) (puthash [1 2 3] 'vec h) (gethash [1 2 3] h))",
        expect,
    );
    assert_ok_eq("vec", &o, &n);

    let expect = expect_test::expect![[r#""OK (1 b)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table :test 'equal))) (puthash (list 1 2) 'a h) (puthash (list 1 2) 'b h) (list (hash-table-count h) (gethash (list 1 2) h)))",
        expect,
    );
    assert_ok_eq("(1 b)", &o, &n);
}

#[test]
fn oracle_prop_hash_table_eq_float_literal_lookup_is_not_identical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // eq-test hash tables use object identity semantics for keys.
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table :test 'eq))) (puthash 1.0 'hit h) (gethash 1.0 h))",
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_hash_table_eq_float_variable_lookup_hits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK hit""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let* ((h (make-hash-table :test 'eq)) (x 1.0)) (puthash x 'hit h) (gethash x h))",
        expect,
    );
    assert_ok_eq("hit", &o, &n);
}

#[test]
fn oracle_prop_hash_table_eq_float_distinct_literals_count_separately() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    // Two separately read float literals should be distinct eq keys.
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((h (make-hash-table :test 'eq))) (puthash 1.0 'a h) (puthash 1.0 'b h) (hash-table-count h))",
        expect,
    );
    assert_ok_eq("2", &o, &n);
}
