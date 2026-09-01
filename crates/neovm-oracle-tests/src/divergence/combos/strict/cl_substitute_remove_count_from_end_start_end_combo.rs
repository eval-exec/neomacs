//! Strict combo oracle probes, batch 376: cl-substitute/remove with :count/
//! :from-end/:start/:end deep edge cases.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_substitute_count_from_end_start_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-substitute 9 2 '(1 2 3 2 4 2 5))
      (cl-substitute 9 2 '(1 2 3 2 4 2 5) :count 2)
      (cl-substitute 9 2 '(1 2 3 2 4 2 5) :from-end t)
      (cl-substitute 9 2 '(1 2 3 2 4 2 5) :count 1 :from-end t)
      (cl-substitute 9 2 '(1 2 3 2 4 2 5) :start 2 :end 6)
      (cl-substitute 9 2 '(1 2 3 2 4 2 5) :start 3 :count 1))
"##;
    let expect = expect_test::expect![[
        r#""OK ((1 9 3 9 4 9 5) (1 9 3 9 4 2 5) (1 9 3 9 4 9 5) (1 2 3 2 4 9 5) (1 2 3 9 4 9 5) (1 2 3 9 4 2 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_remove_count_from_end_key_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-remove 2 '(1 2 3 2 4 2 5))
      (cl-remove 2 '(1 2 3 2 4 2 5) :count 1)
      (cl-remove 2 '(1 2 3 2 4 2 5) :from-end t :count 2)
      (cl-remove 2 '(1 2 3 2 4 2 5) :start 2 :end 5)
      (cl-remove-duplicates '(1 2 3 2 1 4 3) :from-end t)
      (cl-remove-duplicates '((a . 1) (b . 2) (a . 3)) :key #'car))
"##;
    let expect = expect_test::expect![[
        r#""OK ((1 3 4 5) (1 3 2 4 2 5) (1 2 3 4 5) (1 2 3 4 2 5) (1 2 3 4) ((b . 2) (a . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_substitute_if_key_test_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-substitute-if 0 (lambda (x) (> x 3)) '(1 4 2 5 3 6))
      (cl-substitute-if 'big #'cl-evenp '(1 2 3 4) :count 1)
      (cl-remove-if #'cl-oddp '(1 2 3 4 5) :count 2)
      (cl-count-if #'numberp '(1 a 2 b 3) :start 2)
      (cl-position 3 '(1 2 3 4 3 2) :from-end t)
      (cl-find 3 '(1 2 3 4) :start 3)))
"##;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 8 39)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
