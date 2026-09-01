//! Oracle parity tests for char-table parent/subtype/extra-slot primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_char_table_parent_and_subtype_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let* ((p (make-char-table 'generic 'p)) (c (make-char-table 'generic 'c))) (set-char-table-parent c p) (list (eq (char-table-parent c) p) (char-table-subtype c)))";
    let expect = expect_test::expect![[r#""OK (t generic)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(t generic)", &oracle, &neovm);
}

#[test]
fn oracle_prop_char_table_parent_fallback_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK p""#];
    crate::common::assert_oracle_parity_expect(
        "(let* ((p (make-char-table 'generic 'p)) (c (make-char-table 'generic nil))) (set-char-table-parent c p) (char-table-range c ?A))",
        expect,
    );
}

#[test]
fn oracle_prop_char_table_extra_slot_out_of_range_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range #^[nil nil generic nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] 0)""#
    ]];
    let (oracle_generic, neovm_generic) = crate::common::eval_oracle_and_neovm_expect(
        "(char-table-extra-slot (make-char-table 'generic) 0)",
        expect,
    );
    assert_err_kind(&oracle_generic, &neovm_generic, "args-out-of-range");

    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range #^[nil nil syntax-table nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] 0)""#
    ]];
    let (oracle_syntax, neovm_syntax) = crate::common::eval_oracle_and_neovm_expect(
        "(char-table-extra-slot (make-char-table 'syntax-table) 0)",
        expect,
    );
    assert_err_kind(&oracle_syntax, &neovm_syntax, "args-out-of-range");
}

#[test]
fn oracle_prop_char_table_extra_slot_out_of_range_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range #^[nil nil generic nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] 0)""#
    ]];
    let (oracle_generic, neovm_generic) = crate::common::eval_oracle_and_neovm_expect(
        "(set-char-table-extra-slot (make-char-table 'generic) 0 'x)",
        expect,
    );
    assert_err_kind(&oracle_generic, &neovm_generic, "args-out-of-range");

    let expect = expect_test::expect![[
        r#""ERR (args-out-of-range #^[nil nil syntax-table nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] 0)""#
    ]];
    let (oracle_syntax, neovm_syntax) = crate::common::eval_oracle_and_neovm_expect(
        "(set-char-table-extra-slot (make-char-table 'syntax-table) 0 'x)",
        expect,
    );
    assert_err_kind(&oracle_syntax, &neovm_syntax, "args-out-of-range");
}

#[test]
fn oracle_prop_char_table_parent_and_extra_slot_wrong_type_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument char-table-p 1)""#]];
    let (oracle_parent, neovm_parent) =
        crate::common::eval_oracle_and_neovm_expect("(char-table-parent 1)", expect);
    assert_err_kind(&oracle_parent, &neovm_parent, "wrong-type-argument");

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument char-table-p 1)""#]];
    let (oracle_set_parent, neovm_set_parent) =
        crate::common::eval_oracle_and_neovm_expect("(set-char-table-parent 1 nil)", expect);
    assert_err_kind(&oracle_set_parent, &neovm_set_parent, "wrong-type-argument");

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument char-table-p 1)""#]];
    let (oracle_slot, neovm_slot) =
        crate::common::eval_oracle_and_neovm_expect("(char-table-extra-slot 1 0)", expect);
    assert_err_kind(&oracle_slot, &neovm_slot, "wrong-type-argument");
}
