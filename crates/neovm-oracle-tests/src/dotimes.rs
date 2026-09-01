//! Oracle parity tests for `dotimes`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_dotimes_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((sum 0))
                  (dotimes (i 5)
                    (setq sum (+ sum i)))
                  sum)";
    let expect = expect_test::expect![[r#""OK 10""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("10", &o, &n);
}

#[test]
fn oracle_prop_dotimes_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((count 0))
                  (dotimes (i 0)
                    (setq count (1+ count)))
                  count)";
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_prop_dotimes_with_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // dotimes with result form
    let form = "(let ((sum 0))
                  (dotimes (i 5 sum)
                    (setq sum (+ sum i))))";
    let expect = expect_test::expect![[r#""OK 10""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("10", &o, &n);
}

#[test]
fn oracle_prop_dotimes_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Collect values using dotimes
    let form = "(let ((result nil))
                  (dotimes (i 5)
                    (setq result (cons (* i i) result)))
                  (nreverse result))";
    let expect = expect_test::expect![[r#""OK (0 1 4 9 16)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(0 1 4 9 16)", &o, &n);
}

#[test]
fn oracle_prop_dotimes_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested dotimes
    let form = "(let ((sum 0))
                  (dotimes (i 3)
                    (dotimes (j 4)
                      (setq sum (1+ sum))))
                  sum)";
    let expect = expect_test::expect![[r#""OK 12""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("12", &o, &n);
}

#[test]
fn oracle_prop_dotimes_returns_nil_by_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(dotimes (i 3))", expect);
    assert_ok_eq("nil", &o, &n);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_dotimes_sum(
        limit in 0u32..20u32,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((sum 0)) (dotimes (i {} sum) (setq sum (+ sum i))))",
            limit
        );
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }
}
