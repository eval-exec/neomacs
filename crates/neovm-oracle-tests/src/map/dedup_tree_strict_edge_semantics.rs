//! Oracle parity for mapconcat + delete-dups + copy-alist + copy-tree.
//! GNU src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_mapconcat_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a,b,c\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(mapconcat 'identity '("a" "b" "c") ",")"#,
        expect,
    );
    assert_ok_eq("\"a,b,c\"", &o, &n);
}

#[test]
fn oracle_mapconcat_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(mapconcat 'identity nil ",")"#, expect);
    assert_ok_eq("\"\"", &o, &n);
}

#[test]
fn oracle_mapconcat_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"x\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(mapconcat 'identity '("x") "-")"#, expect);
    assert_ok_eq("\"x\"", &o, &n);
}

#[test]
fn oracle_delete_dups_removes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a b c d)""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(delete-dups '(a b a c b d))"#, expect);
    assert_ok_eq("(a b c d)", &o, &n);
}

#[test]
fn oracle_delete_dups_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(delete-dups nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_copy_alist_is_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(equal '((a . 1) (b . 2)) (copy-alist '((a . 1) (b . 2))))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_copy_sequence_list_is_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(equal '(a b c) (copy-sequence '(a b c)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_maphash_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (defvar neovm--test-mh-count 0) (let ((h (make-hash-table))) (puthash 'a 1 h) (puthash 'b 2 h) (maphash (lambda (_k _v) (setq neovm--test-mh-count (1+ neovm--test-mh-count))) h)) neovm--test-mh-count)"#,
        expect,
    );
    assert_ok_eq("2", &o, &n);
}
