//! Oracle parity tests for `string<`, `string=`, `value<`, `length<`,
//! `length=`, and `length>` comparison predicates.
//!
//! GNU implements `string<`/`string=` in `src/fns.c` (string-lessp/string-equal
//! with symbol arguments), `value<` in `src/data.c`, and
//! `length<`/`length=`/`length>` in `src/fns.c`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string< / string=
// ---------------------------------------------------------------------------

#[test]
fn oracle_string_lt_true() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string< "a" "b")"#, expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_string_lt_false_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string< "a" "a")"#, expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_string_eq_true() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string= "hello" "hello")"#, expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_string_eq_false() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string= "hello" "world")"#, expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_string_eq_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string< 42 "foo")"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

// ---------------------------------------------------------------------------
// value<
// ---------------------------------------------------------------------------

#[test]
fn oracle_value_lt_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(r#"(value< 1 2)"#, expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_value_lt_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(value< "a" "b")"#, expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_value_lt_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(r#"(value< 1 1)"#, expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_value_lt_wrong_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments value< 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(r#"(value< 1)"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

// ---------------------------------------------------------------------------
// length< / length= / length>
// ---------------------------------------------------------------------------

#[test]
fn oracle_length_lt_true() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(length< '(a) 2)"#, expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_length_eq_true() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(length= '(a b) 2)"#, expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_length_gt_true() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(length> '(a b c) 1)"#, expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_length_eq_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument fixnump \"foo\")""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(length< 42 "foo")"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_length_predicates_reject_non_fixnum_threshold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    for form in [
        r#"(length< '(a) '(a b))"#,
        r#"(length= '(a b) "ab")"#,
        r#"(length> '(a b c) '(a))"#,
    ] {
        let (oracle, neovm) = eval_oracle_and_neovm(form);
        assert_err_kind(&oracle, &neovm, "wrong-type-argument");
    }
}
