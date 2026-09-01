//! Strict combo oracle probes, batch 367: cl complement predicates + mismatch +
//! search. cl-substitute-if-not, cl-remove-if-not, cl-count-if-not,
//! cl-position-if-not, cl-mismatch, and cl-search.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_if_not_complement_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-substitute-if-not 'X #'numberp '(1 a 2 b 3))
      (cl-remove-if-not #'numberp '(1 a 2 b 3))
      (cl-count-if-not #'numberp '(1 a 2 b 3 c))
      (cl-position-if-not #'numberp '(1 2 a 3 b))
      (cl-find-if-not #'numberp '(1 2 a 3))))
"##;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 7 45)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_mismatch_search_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-mismatch '(1 2 3) '(1 2 4))
      (cl-mismatch '(1 2) '(1 2 3))
      (cl-mismatch '(1 2 3) '(1 2 3))
      (cl-search '(2 3) '(1 2 3 4))
      (cl-search '(c d) '(a b c d e))
      (cl-search '(x) '(a b c)))
"##;
    let expect = expect_test::expect![[r#""OK (2 2 nil 1 2 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_mismatch_search_with_key_from_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-search '(1 2) '((1 2) (3 4)) :key #'identity :test #'equal)
      (cl-search "cd" "abcdefg" :test #'char-equal)
      (cl-mismatch '(1 2 3 4) '(1 2 9 4) :from-end t)
      (cl-mismatch "abc" "abd"))
"##;
    let expect = expect_test::expect![[r#""OK (nil 2 2 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
