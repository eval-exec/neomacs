//! Strict combo oracle probes, batch 247: align / align-region. align-regexp
//! over = and : separators with columnar spacing, and align with rule spec.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_align_regexp_equals_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "a = 1\nfoo = 2\nlonger = 3\n")
  (align-regexp (point-min) (point-max) "\\(\\s-*\\)=" 1 1 nil)
  (buffer-string))
"##;
    let expect = expect_test::expect![[r#""OK \"a\t= 1\\nfoo\t= 2\\nlonger\t= 3\\n\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_align_regrep_repeat_column_spacing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "x:1:a\nfoo:2:bb\nlonger:333:ccc\n")
  (align-regexp (point-min) (point-max) "\\(\\s-*\\):" 1 1 t)
  (buffer-string))
"##;
    let expect =
        expect_test::expect![[r#""OK \"x\t:1\t:a\\nfoo\t:2\t:bb\\nlonger\t:333\t:ccc\\n\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_align_columns_numeric_padding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "1 a\n20 b\n300 c\n")
  (align (point-min) (point-max))
  (buffer-string))
"##;
    let expect = expect_test::expect![[r#""OK \"1 a\\n20 b\\n300 c\\n\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
