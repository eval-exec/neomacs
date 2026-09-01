//! Oracle parity tests for `event-convert-list`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_event_convert_list_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 24""#]];
    crate::common::assert_oracle_parity_expect("(event-convert-list '(control ?x))", expect);
    let expect = expect_test::expect![[r#""OK 134217752""#]];
    crate::common::assert_oracle_parity_expect("(event-convert-list '(meta control ?x))", expect);
}

#[test]
fn oracle_prop_event_convert_list_lookup_key_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((m (make-sparse-keymap))) (define-key m (vector (event-convert-list '(control ?x))) 'foo) (lookup-key m (vector (event-convert-list '(control ?x)))))";
    let expect = expect_test::expect![[r#""OK foo""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("foo", &oracle, &neovm);
}

#[test]
fn oracle_prop_event_convert_list_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(event-convert-list 1)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_event_convert_list_control_ascii_lower(
        ch in 97u32..123u32, // a-z
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(event-convert-list (list 'control {}))", ch);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm, oracle, "event-convert-list parity failed for: {}", form);
    }
}
