//! Strict combo oracle probes, batch 189: cl-lib sequence combinators.
//! cl-some/every/notany/notevery, cl-reduce with :start/:end, cl-count/
//! count-if, cl-find, cl-position/-if, cl-remove-if/-if-not, and cl-subsetp/
//! intersection/set-difference.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_some_every_notany_notevery() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (cl-some #'cl-oddp '(2 4 5 6))
      (cl-some #'cl-oddp '(2 4 6))
      (cl-every #'cl-evenp '(2 4 6))
      (cl-every #'cl-evenp '(2 4 5))
      (cl-notany #'cl-oddp '(2 4 6))
      (cl-notany #'cl-oddp '(2 4 5))
      (cl-notevery #'cl-evenp '(2 4 5))
      (cl-notevery #'cl-evenp '(2 4 6)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-some)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_reduce_count_find_position_remove_if() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (cl-reduce #'+ '(1 2 3 4 5))
      (cl-reduce #'+ '(1 2 3 4 5) :start 1 :end 4)
      (cl-reduce #'* '(1 2 3 4) :initial-value 10)
      (cl-count ?a '(a b c a d a))
      (cl-count ?a '(a b c a d a) :start 2)
      (cl-count-if #'cl-oddp '(1 2 3 4 5 6))
      (cl-find 3 '(1 2 3 4 5))
      (cl-find 9 '(1 2 3))
      (cl-position 3 '(1 2 3 4 5))
      (cl-position-if #'cl-evenp '(1 3 5 6 7))
      (cl-remove-if #'cl-oddp '(1 2 3 4 5 6))
      (cl-remove-if-not #'cl-evenp '(1 2 3 4 5 6)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-reduce)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_setops_subsetp_union_intersection_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (cl-subsetp '(1 2) '(1 2 3 4))
      (cl-subsetp '(1 5) '(1 2 3 4))
      (cl-union '(1 2 3) '(2 3 4))
      (cl-intersection '(1 2 3) '(2 3 4))
      (cl-set-difference '(1 2 3 4) '(2 4))
      (cl-set-exclusive-or '(1 2 3) '(2 3 4))
      (cl-member-if #'cl-evenp '(1 3 5 6 7))
      (cl-assoc 'b '((a . 1) (b . 2) (c . 3)))
      (cl-rassoc 2 '((a . 1) (b . 2) (c . 3))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-subsetp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
