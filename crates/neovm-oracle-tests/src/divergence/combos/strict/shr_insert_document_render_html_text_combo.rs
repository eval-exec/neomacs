//! Strict combo oracle probes, batch 223: shr (simple HTML renderer) text
//! rendering. shr-insert-document over paragraphs/headings/lists/links at a
//! fixed shr-width. This is a known systematic divergence area (whitespace/
//! newline handling); tests are committed failing per convention.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_shr_paragraph_heading_render() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'shr)
(with-temp-buffer
  (let ((shr-width 30)
        (shr-inhibit-images t))
    (shr-insert-document "<html><body><h1>Title</h1><p>A short paragraph here.</p></body></html>")
    (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp \"<html><body><h1>Title</h1><p>A short paragraph here.</p></body></html>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_shr_list_link_render() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'shr)
(with-temp-buffer
  (let ((shr-width 40)
        (shr-inhibit-images t))
    (shr-insert-document "<html><body><ul><li>one</li><li>two</li><li>three</li></ul><a href=\"x\">link text</a></body></html>")
    (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp \"<html><body><ul><li>one</li><li>two</li><li>three</li></ul><a href=\\\"x\\\">link text</a></body></html>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_shr_whitespace_newline_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'shr)
(with-temp-buffer
  (let ((shr-width 20)
        (shr-inhibit-images t))
    (shr-insert-document "<html><body><p>multiple    spaces   and\n\nnewlines</p></body></html>")
    (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp \"<html><body><p>multiple    spaces   and\\n\\nnewlines</p></body></html>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
