//! Strict combo oracle probes, batch 355: cl car/cdr compositions deep.
//! cl-caar, cl-cadr, cl-cdar, cl-cddr, cl-caaar, cl-caddr, cl-cdddr,
//! cl-cddddr, and cl-list* / cl-list.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_caar_cadr_cdar_cddr_two_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((data '((1 2 3) (4 5 6) (7 8 9))))
  (list (cl-caar data)
        (cl-cadr data)
        (cl-cdar data)
        (cl-cddr data)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-caar)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_caaar_caddr_cdddr_cddddr_three_four() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((deep '(((1 2) (3 4)) ((5 6))))
      (flat '(a b c d e)))
  (list (cl-caaar deep)
        (cl-caddr flat)
        (cl-cdddr flat)
        (cl-cddddr flat)
        (cl-first flat)
        (cl-second flat)
        (cl-third flat)
        (cl-fourth flat)
        (cl-fifth flat)))
"##;
    let expect = expect_test::expect![[r#""OK (1 c (d e) (e) a b c d e)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_list_star_rest_last_nth_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-list* 1 2 3)
      (cl-list* 1 2 '(3 4))
      (cl-list* 1)
      (cl-list 1 2 3)
      (cl-last '(1 2 3 4) 2)
      (cl-nth 2 '(a b c d))
      (cl-tenth '(1 2 3 4 5 6 7 8 9 10)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-list)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
