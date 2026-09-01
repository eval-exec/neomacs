//! Completion case-handling divergence probes.
//!
//! Confirmed bug: under `completion-ignore-case`, GNU try-completion returns
//! the completion prefix preserving the INPUT case, while Neomacs returns it
//! in the candidate's case. This file probes that case-preservation behavior
//! across several ignore-case scenarios.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_ccase_prefix_upper_input_lower_candidates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ab\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignore-case t)) (try-completion "A" '("abc" "abd")))
"##,
        expect,
    );
}

#[test]
fn div_ccase_prefix_lower_input_upper_candidates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"AB\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignore-case t)) (try-completion "a" '("ABC" "ABD")))
"##,
        expect,
    );
}

#[test]
fn div_ccase_mixed_case_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ab\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignore-case t)) (try-completion "Ab" '("abc" "abd")))
"##,
        expect,
    );
}

#[test]
fn div_ccase_all_completions_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"apple\" \"APPLE\" \"apricot\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignore-case t)) (all-completions "A" '("apple" "APPLE" "apricot")))
"##,
        expect,
    );
}

#[test]
fn div_ccase_exact_match_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignore-case t)) (try-completion "ABC" '("abc")))
"##,
        expect,
    );
}

#[test]
fn div_ccase_single_candidate_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"apple\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignore-case t)) (try-completion "A" '("apple")))
"##,
        expect,
    );
}

#[test]
fn div_ccase_flex_style_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignore-case t) (completion-styles '(flex))) (try-completion "AB" '("axby")))
"##,
        expect,
    );
}

#[test]
fn div_ccase_test_completion_ignore_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t \"FOOBAR\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignore-case t))
  (list (test-completion "ABC" '("abc"))
        (test-completion "abc" '("ABC"))
        (try-completion "FOO" '("foobar" "FOOBAR"))))
"##,
        expect,
    );
}

#[test]
fn div_ccase_upper_collection_diverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"BA\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignore-case t))
  (try-completion "BA" '("banana" "BANANA" "bagel")))
"##,
        expect,
    );
}

#[test]
fn div_ccase_completion_pcm_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignore-case t) (completion-styles '(partial)))
  (all-completions "B" '("abc" "aBd")))
"##,
        expect,
    );
}
