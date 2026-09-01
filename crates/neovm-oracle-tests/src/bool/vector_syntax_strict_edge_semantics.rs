//! Oracle parity tests for bool-vector, syntax-table, and fillarray.
//!
//! GNU src/alloc.c, src/syntax.c, src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_bool_vector_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(= (length (bool-vector)) 0)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_bool_vector_with_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(= (length (bool-vector t nil t)) 3)"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_bool_vector_aref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(aref (bool-vector t nil t) 0)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_bool_vector_aref_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(aref (bool-vector t nil t) 1)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_standard_syntax_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(syntax-table-p (standard-syntax-table))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_syntax_table_p_on_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(syntax-table-p [1 2 3])"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_set_syntax_table_returns_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(syntax-table-p (set-syntax-table (standard-syntax-table)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_fillarray_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK [99 99 99]""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(fillarray [1 2 3] 99)"#, expect);
    assert_ok_eq("[99 99 99]", &o, &n);
}

#[test]
fn oracle_fillarray_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"xxx\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(fillarray "abc" ?x)"#, expect);
    assert_ok_eq("\"xxx\"", &o, &n);
}

#[test]
fn oracle_modify_syntax_entry_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 119""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (modify-syntax-entry ?a "w") (char-syntax ?a))"#,
        expect,
    );
    assert_ok_eq("119", &o, &n);
}
