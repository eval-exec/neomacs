//! Strict combo oracle probes, batch 244: cl-extra. cl-coerce over list/vector/
//! string, cl-ldiff/cl-tailp, cl-remprop/cl-getf plist ops, cl-substitute/
//! cl-nsubstitute, and cl-fill.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_coerce_ldiff_tailp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-extra)
(list (cl-coerce "abc" 'list)
      (cl-coerce '(1 2 3) 'vector)
      (cl-coerce [a b c] 'list)
      (cl-coerce 5 'list)
      (cl-coerce '(?a ?b ?c) 'string)
      (cl-ldiff '(1 2 3 4) '(3 4))
      (cl-ldiff '(1 2 3 4) nil)
      (cl-tailp '(3 4) '(1 2 3 4))
      (cl-tailp '(2 4) '(1 2 3 4)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_rempf_getf_substitute_fill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-extra)
(let ((p (copy-list '(a 1 b 2 c 3))))
  (list (cl-getf p 'b)
        (cl-getf p 'z 'missing)
        (progn (cl-remprop p 'b) p)
        (cl-substitute 9 2 '(1 2 3 2 4))
        (cl-substitute 9 2 '(1 2 3 2 4) :count 1)
        (cl-nsubstitute 9 2 (list 1 2 3 2 4))
        (cl-fill (list 0 0 0) 5)
        (cl-fill (list 0 0 0 0) 7 :start 1 :end 3)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function copy-list)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_pairlis_pairlis_adjoin_tree_equal_copy_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-extra)
(list (cl-pairlis '(a b c) '(1 2 3))
      (cl-adjoin 'c '(a b c))
      (cl-adjoin 'x '(a b c))
      (cl-adjoin 2 '(1 2 3) :test #'=)
      (cl-tree-equal '(1 (2 3)) '(1 (2 3)))
      (cl-copy-tree '(1 (2 (3 4)) 5))
      (cl-copy-list '(1 2 3))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-copy-tree)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
