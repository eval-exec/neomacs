//! Strict combo oracle probes, batch 357: cl-getf nested plist +
//! cl-get / cl-remprop deep. cl-getf with default, nested plist access,
//! cl-get with inheritance, and cl-remprop cleanup.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_getf_default_nested_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((p '(a 1 b (c 2 d 3) e 4)))
  (list (cl-getf p 'a)
        (cl-getf p 'b)
        (cl-getf p 'e)
        (cl-getf p 'z 'missing)
        (car (cl-getf p 'b))))
"##;
    let expect = expect_test::expect![[r#""OK (1 (c 2 d 3) 4 missing c)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_get_symbol_plist_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(setplist 'probe-gsp '(a 1 b 2))
(let ((p '(c 3 d 4)))
  (list (get 'probe-gsp 'a)
        (cl-get 'probe-gsp 'a)
        (cl-get 'probe-gsp 'z 'default)
        (cl-get 'probe-gsp 'c)))
"##;
    let expect = expect_test::expect![[r#""OK (1 1 default nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_remprop_getf_setf_in_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((p (copy-list '(a 1 b 2 c 3))))
  (list (cl-getf p 'b)
        (progn (cl-remprop 'p) p)
        (cl-getf p 'a)
        p))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function copy-list)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
