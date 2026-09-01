//! Oracle parity tests for data construction: `make-string`,
//! `make-list`, `make-vector`, `make-symbol`, `string`, `vector`.
//!
//! GNU src/alloc.c, src/fns.c, src/lread.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_make_string_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"aaa\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(make-string 3 ?a)"#, expect);
    assert_ok_eq("\"aaa\"", &o, &n);
}

#[test]
fn oracle_make_string_zero_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(make-string 0 ?x)"#, expect);
    assert_ok_eq("\"\"", &o, &n);
}

#[test]
fn oracle_make_list_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (x x x)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(make-list 3 'x)"#, expect);
    assert_ok_eq("(x x x)", &o, &n);
}

#[test]
fn oracle_make_list_zero_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(make-list 0 'x)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_make_vector_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK [x x x]""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(make-vector 3 'x)"#, expect);
    assert_ok_eq("[x x x]", &o, &n);
}

#[test]
fn oracle_make_vector_zero_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK []""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(make-vector 0 'x)"#, expect);
    assert_ok_eq("[]", &o, &n);
}

#[test]
fn oracle_make_symbol_creates_uninterned() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(symbolp (make-symbol "test-ms"))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_string_constructor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string 97 98 99)"#, expect);
    assert_ok_eq("\"abc\"", &o, &n);
}

#[test]
fn oracle_vector_constructor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK [1 2 3]""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(vector 1 2 3)"#, expect);
    assert_ok_eq("[1 2 3]", &o, &n);
}

#[test]
fn oracle_make_string_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument wholenump a)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(make-string 'a ?x)"#, expect);
    assert_err_kind(&o, &n, "wrong-type-argument");
}
