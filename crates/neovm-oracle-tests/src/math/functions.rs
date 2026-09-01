//! Oracle parity tests for math functions: `floor`, `ceiling`, `round`,
//! `truncate`, `float`, `expt`, `sqrt`, `log`, `sin`, `cos`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_floor_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(floor 3.7)", expect);
    assert_ok_eq("3", &o, &n);

    let expect = expect_test::expect![[r#""OK -4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(floor -3.7)", expect);
    assert_ok_eq("-4", &o, &n);

    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(floor 4.0)", expect);
    assert_ok_eq("4", &o, &n);
}

#[test]
fn oracle_prop_ceiling_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(ceiling 3.2)", expect);
    assert_ok_eq("4", &o, &n);

    let expect = expect_test::expect![[r#""OK -3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(ceiling -3.2)", expect);
    assert_ok_eq("-3", &o, &n);

    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(ceiling 4.0)", expect);
    assert_ok_eq("4", &o, &n);
}

#[test]
fn oracle_prop_round_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(round 3.5)", expect);
    assert_ok_eq("4", &o, &n);

    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(round 2.5)", expect);
    assert_ok_eq("2", &o, &n);
    let expect = expect_test::expect![[r#""OK 3""#]];
    // banker's rounding

    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(round 3.3)", expect);
    assert_ok_eq("3", &o, &n);

    let expect = expect_test::expect![[r#""OK -4""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(round -3.7)", expect);
    assert_ok_eq("-4", &o, &n);
}

#[test]
fn oracle_prop_truncate_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(truncate 3.9)", expect);
    assert_ok_eq("3", &o, &n);

    let expect = expect_test::expect![[r#""OK -3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(truncate -3.9)", expect);
    assert_ok_eq("-3", &o, &n);
}

#[test]
fn oracle_prop_float_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 42.0""#]];
    crate::common::assert_oracle_parity_expect("(float 42)", expect);
    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(float 0)", expect);
    let expect = expect_test::expect![[r#""OK -7.0""#]];
    crate::common::assert_oracle_parity_expect("(float -7)", expect);
    let expect = expect_test::expect![[r#""OK 3.14""#]];
    crate::common::assert_oracle_parity_expect("(float 3.14)", expect);
}

#[test]
fn oracle_prop_expt_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 1024""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(expt 2 10)", expect);
    assert_ok_eq("1024", &o, &n);

    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(expt 3 0)", expect);
    assert_ok_eq("1", &o, &n);

    let expect = expect_test::expect![[r#""OK 1.4142135623730951""#]];
    crate::common::assert_oracle_parity_expect("(expt 2.0 0.5)", expect);
}

#[test]
fn oracle_prop_sqrt_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2.0""#]];
    crate::common::assert_oracle_parity_expect("(sqrt 4.0)", expect);
    let expect = expect_test::expect![[r#""OK 3.0""#]];
    crate::common::assert_oracle_parity_expect("(sqrt 9.0)", expect);
    let expect = expect_test::expect![[r#""OK 1.4142135623730951""#]];
    crate::common::assert_oracle_parity_expect("(sqrt 2.0)", expect);
}

#[test]
fn oracle_prop_log_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(log 1)", expect);
    let expect = expect_test::expect![[r#""OK 1.0""#]];
    crate::common::assert_oracle_parity_expect("(log 10 10)", expect);
}

#[test]
fn oracle_prop_sin_cos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0.0""#]];
    crate::common::assert_oracle_parity_expect("(sin 0)", expect);
    let expect = expect_test::expect![[r#""OK 1.0""#]];
    crate::common::assert_oracle_parity_expect("(cos 0)", expect);
    let expect = expect_test::expect![[r#""OK 0.8414709848078965""#]];
    crate::common::assert_oracle_parity_expect("(sin 1.0)", expect);
    let expect = expect_test::expect![[r#""OK 0.5403023058681398""#]];
    crate::common::assert_oracle_parity_expect("(cos 1.0)", expect);
}

#[test]
fn oracle_prop_floor_with_divisor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(floor 7 2)", expect);
    assert_ok_eq("3", &o, &n);

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(floor 10 3)", expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_prop_isnan_and_special_floats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(isnan 0.0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(isnan 1.0)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(isnan 0.0e+NaN)", expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_floor_proptest(
        n in -1000i64..1000i64,
        d in 1i64..100i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(floor {} {})", n, d);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_truncate_proptest(
        n in -1000i64..1000i64,
        d in 1i64..100i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(truncate {} {})", n, d);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }
}
