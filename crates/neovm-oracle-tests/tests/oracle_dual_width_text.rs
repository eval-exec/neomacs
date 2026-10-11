//! Font classification must not alter the public column-width contract.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;
#[test]
fn dual_width_cjk_and_combining_marks_keep_their_column_widths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(list (string-width "ab漢字かなcd") (string-width "á漢")
                 (mapcar #'char-width '(?a ?漢 ?字 ?か ?な ?́)))"#,
        expect_test::expect![[r#""OK (12 3 (1 2 2 2 2 0))""#]],
    );
}
