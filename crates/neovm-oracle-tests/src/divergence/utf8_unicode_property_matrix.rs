//! UTF-8 / multibyte *Unicode property matrix* divergence probes.
//!
//! Probes the breadth of Unicode property data Neomacs ships, across many
//! properties and scripts: `east-asian-width`, `line-break`, `script`,
//! `numeric-value`, `age`, simple case mappings, and a broad `char-charset`
//! classification across scripts. Gaps in property tables surface here.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_utf8_east_asian_width_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(mapcar (lambda (c) (get-char-code-property c 'east-asian-width))
        (list ?a ?é ?\x3042 ?\x4e2d ?\xff21 ?\x1f600 ?\x2502 ?\s ?- ?\x3000))
"#,
        expect,
    );
}

#[test]
fn div_utf8_line_break_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(mapcar (lambda (c) (get-char-code-property c 'line-break))
        (list ?a ?\s ?- ?\x2014 ?\x3042 ?\x4e2d ?\x1f600))
"#,
        expect,
    );
}

#[test]
fn div_utf8_script_property_many_scripts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(mapcar (lambda (c) (get-char-code-property c 'script))
        (list ?a ?é ?\x391 ?\x410 ?\x5d0 ?\x627 ?\x905 ?\xe01
              ?\x3042 ?\x4e2d ?\xac00 ?\x1308 ?\x13e3 ?\x2d31 ?\x1200))
"#,
        expect,
    );
}

#[test]
fn div_utf8_numeric_value_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 5 9 0 1 0 0 1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(mapcar (lambda (c) (get-char-code-property c 'numeric-value))
        (list ?0 ?5 ?9 ?\x660 ?\x661 ?\x966 ?\xff10 ?\x2160 ?\x2163))
"#,
        expect,
    );
}

#[test]
fn div_utf8_age_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(mapcar (lambda (c) (get-char-code-property c 'age))
        (list ?a ?\x3042 ?\x1f600 ?\x1f9e0 ?\x1f9d1 ?\x32ff))
"#,
        expect,
    );
}

#[test]
fn div_utf8_simple_case_mapping_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((nil nil nil nil) (nil nil nil nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (mapcar (lambda (c) (get-char-code-property c 'simple-lowercase))
              (list ?A ?É ?\x391 ?\x410))
      (mapcar (lambda (c) (get-char-code-property c 'simple-uppercase))
              (list ?a ?é ?\x3b1 ?\x430)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_char_charset_matrix_across_scripts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (ascii unicode-bmp unicode-bmp unicode-bmp unicode-bmp unicode-bmp unicode-bmp unicode-bmp unicode-bmp unicode-bmp unicode-bmp unicode-bmp unicode-bmp)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(mapcar #'char-charset
        (list ?a ?é ?\x391 ?\x410 ?\x5d0 ?\x627 ?\x905 ?\xe01
              ?\x3042 ?\x4e2d ?\xac00 ?\x1308 ?\x1200))
"#,
        expect,
    );
}

#[test]
fn div_utf8_decomposition_type_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (get-char-code-property ?ﬁ 'decomposition-type)
      (get-char-code-property ?\xff21 'decomposition-type)
      (get-char-code-property ?\x2126 'decomposition-type)
      (get-char-code-property ?\x1f71 'decomposition-type))
"#,
        expect,
    );
}
