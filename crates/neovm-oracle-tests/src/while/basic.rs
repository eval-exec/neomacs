//! Oracle parity tests for `while`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_while_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 10""#]];
    // counter accumulation
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((i 0) (sum 0)) (while (< i 5) (setq sum (+ sum i) i (1+ i))) sum)",
        expect,
    );
    assert_ok_eq("10", &o, &n);

    let expect = expect_test::expect![[r#""OK 99""#]];
    // zero iterations
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((x 99)) (while nil (setq x 0)) x)",
        expect,
    );
    assert_ok_eq("99", &o, &n);

    let expect = expect_test::expect![[r#""OK 1""#]];
    // single iteration
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((done nil) (count 0)) (while (not done) (setq count (1+ count) done t)) count)",
        expect,
    );
    assert_ok_eq("1", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    // while returns nil
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(while nil)", expect);
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK 60""#]];
    // list consumption
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((xs '(10 20 30)) (total 0)) (while xs (setq total (+ total (car xs)) xs (cdr xs))) total)",
        expect,
    );
    assert_ok_eq("60", &o, &n);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_while_countdown(
        limit in 1i64..15i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((i {}) (c 0)) (while (> i 0) (setq c (1+ c) i (1- i))) c)",
            limit
        );
        let expected = format!("{}", limit);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(&expected, &oracle, &neovm);
    }
}
