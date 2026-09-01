//! Oracle parity tests for character-literal parsing (`?x`, `?\M-x`, etc.).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_char_literal_modifier_bits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list ?\M-a ?\C-a ?\M-\C-a ?\S-a)"#;
    let expect = expect_test::expect![[r#""OK (134217825 1 134217729 33554529)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(134217825 1 134217729 33554529)", &oracle, &neovm);
}

#[test]
fn oracle_prop_char_literal_unicode_codepoints() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (128512 66304)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(list ?😀 ?𐌀)", expect);
    assert_ok_eq("(128512 66304)", &oracle, &neovm);
}
