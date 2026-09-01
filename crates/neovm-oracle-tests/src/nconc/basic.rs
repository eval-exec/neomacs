//! Oracle parity tests for `nconc`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_nconc_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(nconc '(1 2) '(3 4))", expect);
    assert_ok_eq("(1 2 3 4)", &o, &n);

    let expect = expect_test::expect![[r#""OK (a b c d e f)""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(nconc '(a b) '(c) '(d e f))", expect);
    assert_ok_eq("(a b c d e f)", &o, &n);

    let expect = expect_test::expect![[r#""OK (5 6)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(nconc nil '(5 6))", expect);
    assert_ok_eq("(5 6)", &o, &n);

    let expect = expect_test::expect![[r#""OK (7 8)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(nconc '(7 8) nil)", expect);
    assert_ok_eq("(7 8)", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(nconc nil)", expect);
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK (99)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(nconc '(99))", expect);
    assert_ok_eq("(99)", &o, &n);

    let expect = expect_test::expect![[r#""OK (1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(nconc nil nil nil '(1))", expect);
    assert_ok_eq("(1)", &o, &n);
}
