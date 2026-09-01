//! Oracle parity tests for `modify-syntax-entry`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_modify_syntax_entry_cons_pair_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((st (make-syntax-table))) (modify-syntax-entry '(?a . ?z) \"w\" st) (list (aref st ?a) (aref st ?m) (aref st ?z) (aref st ?A)))";
    let expect = expect_test::expect![[r#""OK ((2) (2) (2) (2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_modify_syntax_entry_digit_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((st (make-syntax-table))) (modify-syntax-entry '(?0 . ?9) \".\" st) (list (aref st ?0) (aref st ?5) (aref st ?9)))";
    let expect = expect_test::expect![[r#""OK ((1) (1) (1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_modify_syntax_entry_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(modify-syntax-entry 1 \"w\")", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_prop_make_syntax_table_inherits_standard_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((st (make-syntax-table))) (list (aref st ?A) (aref st ?0) (aref st ?\\n)))";
    let expect = expect_test::expect![[r#""OK ((2) (2) (0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
