//! Circular traversal parity with GNU Emacs 31.1. Refresh expectations using
//! NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.
#![allow(dead_code)]
#[path = "../src/common.rs"]
mod common;

#[test]
fn circular_lists_rassoc_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = include_str!(
        "../../neovm-core/src/emacs_core/runtime/eval/tests/circular_lists_cases/rassoc.el"
    );
    let expected = expect_test::expect![[
        r#""OK (1 3 ((circular-list 1) (circular-list 1) (circular-list 1) (circular-list 1)))""#
    ]];
    common::assert_oracle_parity_expect(form, expected);
}

#[test]
fn circular_lists_concat_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = include_str!(
        "../../neovm-core/src/emacs_core/runtime/eval/tests/circular_lists_cases/concat.el"
    );
    let expected = expect_test::expect![[
        r#""OK ((circular-list 97) (circular-list 97) (circular-list 97) (circular-list 98) \"abé\" \"abc\")""#
    ]];
    common::assert_oracle_parity_expect(form, expected);
}

#[test]
fn circular_lists_apply_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = include_str!(
        "../../neovm-core/src/emacs_core/runtime/eval/tests/circular_lists_cases/apply.el"
    );
    let expected = expect_test::expect![[
        r#""OK ((circular-list 1) (circular-list 1) (circular-list 1) (circular-list 1) (circular-list 1) (circular-list 1))""#
    ]];
    common::assert_oracle_parity_expect(form, expected);
}

#[test]
fn circular_lists_eval_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = include_str!(
        "../../neovm-core/src/emacs_core/runtime/eval/tests/circular_lists_cases/eval.el"
    );
    let expected = expect_test::expect![[
        r#""OK (((circular-list 1) (circular-list 1) (circular-list 1) (circular-list 1) (circular-list 1) (circular-list 1) (circular-list 1) (circular-list 1)) (circular-list a) (circular-list a) (circular-list 3) (wrong-type-argument listp))""#
    ]];
    common::assert_oracle_parity_expect(form, expected);
}

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

#[test]
fn circular_lists_compiled_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = include_str!(
        "../../neovm-core/src/emacs_core/runtime/eval/tests/circular_lists_cases/compiled.el"
    );
    let expected =
        expect_test::expect![[r#""OK ((circular-list 1) (circular-list 1) (circular-list 1))""#]];
    common::assert_oracle_parity_expect(form, expected);
}

#[test]
fn circular_lists_initializer_callbacks_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = include_str!(
        "../../neovm-core/src/emacs_core/runtime/eval/tests/circular_lists_cases/init_callbacks.el"
    );
    let expected = expect_test::expect![[
        r#""OK (((let* retain nil (33 44 55 99) (100 200 400 300) ((before 11 22) (after 33 44) skipped)) (let* terminate nil (33 44 55 300) (100 200 400 300) ((before 11 22) (after 33 44))) (let* cycle nil (circular-list gdn-callback-value) (100 200 400 300) ((before 11 22) (after 33 44))) (let* error nil (error \"callback failure\") (100 200 400 300) ((before 11 22) (after 33 44))) (let terminate nil (11 22 400 300) (100 200 400 300) ((before 100 200) (second 100 200))) (let dotted nil (11 22 400 300) (100 200 400 300) ((before 100 200) (second 100 200))) (let cycle nil (11 22 400 300) (100 200 400 300) ((before 100 200) (second 100 200) (second 100 200)))) ((let* retain t (33 44 55 99) (100 200 400 300) ((before 11 22) (after 33 44) skipped)) (let* terminate t (33 44 55 300) (100 200 400 300) ((before 11 22) (after 33 44))) (let* cycle t (circular-list gdn-callback-value) (100 200 400 300) ((before 11 22) (after 33 44))) (let* error t (error \"callback failure\") (100 200 400 300) ((before 11 22) (after 33 44))) (let terminate t (11 22 400 300) (100 200 400 300) ((before 100 200) (second 100 200))) (let dotted t (11 22 400 300) (100 200 400 300) ((before 100 200) (second 100 200))) (let cycle t (11 22 400 300) (100 200 400 300) ((before 100 200) (second 100 200) (second 100 200)))))""#
    ]];
    common::assert_oracle_parity_expect(form, expected);
}
