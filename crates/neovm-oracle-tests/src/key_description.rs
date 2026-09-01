//! Oracle parity tests for key description and modifier parsing primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_single_key_description_modifier_outputs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(list (single-key-description (event-convert-list '(control ?x))) (single-key-description (event-convert-list '(meta control ?x))))";
    let expect = expect_test::expect![[r#""OK (\"C-x\" \"C-M-x\")""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(\"C-x\" \"C-M-x\")", &oracle, &neovm);
}

#[test]
fn oracle_prop_key_description_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(key-description (vector (event-convert-list '(control ?x)) (event-convert-list '(meta control ?x))))";
    let expect = expect_test::expect![[r#""OK \"C-x C-M-x\"""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("\"C-x C-M-x\"", &oracle, &neovm);
}

#[test]
fn oracle_prop_internal_event_symbol_parse_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (x meta control)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(internal-event-symbol-parse-modifiers 'C-M-x)",
        expect,
    );
    assert_ok_eq("(x meta control)", &oracle, &neovm);
}

#[test]
fn oracle_prop_internal_event_symbol_parse_modifiers_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(internal-event-symbol-parse-modifiers 1)",
        expect,
    );
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_key_description_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 1)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(key-description 1)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}
