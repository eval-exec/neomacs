//! Circular traversal parity with GNU Emacs 31.1. Refresh expectations using
//! NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.
#![allow(dead_code)]
#[path = "../src/common.rs"]
mod common;

#[test]
fn circular_lists_cycle_tails_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = include_str!(
        "../../neovm-core/src/emacs_core/runtime/eval/tests/circular_lists_cases/cycle_tails.el"
    );
    let expected = expect_test::expect![[
        r#""OK (((circular-list 1) (circular-list 1) (circular-list 1) (circular-list 1)) ((circular-list 1) (circular-list 1) (circular-list 1) (circular-list 2)) ((circular-list 3) (circular-list 3) (circular-list 3) (circular-list 1)) ((circular-list 3) (circular-list 3) (circular-list 3) (circular-list 4)) ((circular-list 2) (circular-list 2) (circular-list 2) (circular-list 3)))""#
    ]];
    common::assert_oracle_parity_expect(form, expected);
}
