//! Oracle parity tests for GNU labeled restriction semantics.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_with_restriction_label_restores_stack_and_widen_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "abcdef")
                    (with-restriction 2 5 :label 'tag
                      (list (point-min) (point-max)
                            (save-restriction
                              (without-restriction :label 'tag
                                (list (point-min) (point-max))))
                            (point-min) (point-max)
                            (progn (widen) (list (point-min) (point-max)))
                            (progn (without-restriction :label 'tag
                                     (list (point-min) (point-max)))))))"#;
    let expect = expect_test::expect![[r#""OK (2 5 (1 7) 2 5 (2 5) (1 7))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
