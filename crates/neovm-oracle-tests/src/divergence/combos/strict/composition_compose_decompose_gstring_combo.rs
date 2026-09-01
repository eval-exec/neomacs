//! Strict combo oracle probes, batch 332: composition (compose-string /
//! decompose). compose-region, compose-string, composition properties,
//! and decompose-region.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_compose_string_region_decompose() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'composite)
(with-temp-buffer
  (insert "hello world")
  (compose-region 1 5)
  (let ((composed-at-1 (get-text-property 1 'composition)))
    (decompose-region 1 5)
    (list (or (null composed-at-1) (consp composed-at-1) (integerp composed-at-1))
          (get-text-property 1 'composition)
          (buffer-string))))
"##;
    let expect = expect_test::expect![[r#""OK (t nil \"hello world\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_compose_string_explicit_components() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'composite)
(with-temp-buffer
  (insert "abcdef")
  (compose-string 1 4 1 [?X ?Y ?Z])
  (let ((c1 (get-text-property 1 'composition)))
    (list (or (null c1) (consp c1) (vectorp c1))
          (get-text-property 4 'composition)
          (buffer-string))))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_auto_compose_chars_composition_func() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'composite)
(with-temp-buffer
  (insert "testing composition here")
  (let ((comp-count 0))
    (compose-region 1 10 (vector ?1 ?2 ?3 ?4 ?5 ?6 ?7 ?8 ?9 ?0))
    (list (get-text-property 1 'composition)
          (get-text-property 5 'composition)
          (get-text-property 11 'composition)
          (buffer-size))))
"##;
    let expect = expect_test::expect![[
        r#""OK (((9 . [49 50 51 52 53 54 55 56 57 48])) ((9 . [49 50 51 52 53 54 55 56 57 48])) nil 24)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
