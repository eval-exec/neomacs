//! Oracle parity tests for hash table strict edge cases.
//!
//! GNU src/fns.c: hash tables have subtle behavior around default test
//! (eql), key comparison, remhash, clrhash, and gethash default values.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_hash_table_default_test_is_eql() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK eql""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(hash-table-test (make-hash-table))", expect);
    assert_ok_eq("eql", &oracle, &neovm);
}

#[test]
fn oracle_gethash_default_value_when_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK default-val""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(gethash 'missing-key (make-hash-table) 'default-val)"#,
        expect,
    );
    assert_ok_eq("default-val", &oracle, &neovm);
}

#[test]
fn oracle_gethash_nil_default_when_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(gethash 'nonexistent (make-hash-table))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_remhash_returns_nil_on_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    // GNU: remhash returns nil on success (not t).
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((h (make-hash-table)))
    (puthash 'key 42 h)
    (remhash 'key h)))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_remhash_returns_nil_when_key_absent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((h (make-hash-table)))
    (remhash 'no-such-key h)))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_clrhash_empties_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((h (make-hash-table)))
    (puthash 'a 1 h)
    (puthash 'b 2 h)
    (clrhash h)
    (hash-table-count h)))"#,
        expect,
    );
    assert_ok_eq("0", &oracle, &neovm);
}

#[test]
fn oracle_hash_table_count_after_puthash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((h (make-hash-table)))
    (puthash 'a 1 h)
    (puthash 'b 2 h)
    (puthash 'c 3 h)
    (hash-table-count h)))"#,
        expect,
    );
    assert_ok_eq("3", &oracle, &neovm);
}

#[test]
fn oracle_hash_table_equal_test_on_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((h (make-hash-table :test 'equal)))
    (puthash "key" 42 h)
    (gethash "key" h)))"#,
        expect,
    );
    assert_ok_eq("42", &oracle, &neovm);
}

#[test]
fn oracle_hash_table_eql_test_strings_not_found() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    // eql compares strings by identity, so different string objects
    // with same content are NOT eql.
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((h (make-hash-table :test 'eql)))
    (puthash "key" 42 h)
    (gethash "key" h)))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_hash_table_size_is_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    // GNU: default hash-table-size is 0 (meaning "use default size").
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(integerp (hash-table-size (make-hash-table)))",
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}
