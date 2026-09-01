//! Oracle parity tests for `make-string`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_make_string_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"xxxxx\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(make-string 5 ?x)", expect);
    assert_ok_eq(r#""xxxxx""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"AAA\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(make-string 3 ?A)", expect);
    assert_ok_eq(r#""AAA""#, &o, &n);
}

#[test]
fn oracle_prop_make_string_zero_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(make-string 0 ?x)", expect);
    assert_ok_eq(r#""""#, &o, &n);
}

#[test]
fn oracle_prop_make_string_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"    \"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(make-string 4 ?\\s)", expect);
    assert_ok_eq(r#""    ""#, &o, &n);
}

#[test]
fn oracle_prop_make_string_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(length (make-string 3 ?\\n))", expect);
}

#[test]
fn oracle_prop_make_string_length_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 10""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(length (make-string 10 ?z))", expect);
    assert_ok_eq("10", &o, &n);
}

#[test]
fn oracle_prop_make_string_bignum_length_error_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs alloc.c:Fmake_string validates LENGTH with CHECK_FIXNAT:
    // bignum lengths are rejected as `wholenump`, not as generic `integerp`.
    let form = r#"(make-string 1000000000000000000000000000000 ?x)"#;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument wholenump 1000000000000000000000000000000)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_make_string_float_length_error_predicate_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs alloc.c:Fmake_string uses CHECK_FIXNAT for LENGTH, so both
    // non-integer and negative lengths signal `wholenump`.
    let form = r#"
(condition-case err
    (make-string 1.0 ?a)
  (error (list (car err) (cdr err))))
"#;
    let expect = expect_test::expect![[r#""OK (wrong-type-argument (wholenump 1.0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_make_string_length(
        len in 0usize..50usize,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(length (make-string {} ?a))", len);
        let expected = format!("OK {}", len);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), expected.as_str());
        prop_assert_eq!(oracle.as_str(), expected.as_str());
    }
}
