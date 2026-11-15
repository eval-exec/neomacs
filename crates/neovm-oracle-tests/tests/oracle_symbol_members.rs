//! PEER 5786 W1: Lisp-visible startup symbols must be obarray members.
//!
//! GNU interns every `features` entry and every `coding-system-list` name
//! through `intern_c_string`, so `(eq sym (intern-soft (symbol-name sym)))`
//! holds for all of them. The form counts non-members on each side and both
//! editors must answer zero; the coding-system list SIZE differs because
//! Neomacs covers fewer systems, so this row pins membership only.
#![allow(dead_code)]
use crate::common;

#[test]
fn features_and_coding_systems_are_obarray_members_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list
  (length (delq t (mapcar (lambda (sym) (eq sym (intern-soft (symbol-name sym))))
                          features)))
  (length (delq t (mapcar (lambda (sym) (eq sym (intern-soft (symbol-name sym))))
                          (coding-system-list)))))"#;
    let expected = expect_test::expect![[r#""OK (0 0)""#]];
    common::assert_oracle_parity_expect(form, expected);
}
