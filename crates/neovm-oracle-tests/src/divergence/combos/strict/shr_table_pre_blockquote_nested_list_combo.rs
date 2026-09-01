//! Strict combo oracle probes, batch 371: shr complex HTML (tables, pre,
//! blockquote, nested lists). shr-insert-document over complex HTML structures.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_shr_table_pre_render() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'shr)
(with-temp-buffer
  (let ((shr-width 40) (shr-inhibit-images t))
    (shr-insert-document "<html><body><table><tr><td>A</td><td>B</td></tr><tr><td>C</td><td>D</td></tr></table><pre>code line 1\ncode line 2</pre></body></html>")
    (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp \"<html><body><table><tr><td>A</td><td>B</td></tr><tr><td>C</td><td>D</td></tr></table><pre>code line 1\\ncode line 2</pre></body></html>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_shr_blockquote_nested_ul_ol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'shr)
(with-temp-buffer
  (let ((shr-width 40) (shr-inhibit-images t))
    (shr-insert-document "<html><body><blockquote>Quoted text here</blockquote><ul><li>item one<ul><li>nested item</li></ul></li><li>item two</li></ul><ol><li>first</li><li>second</li></ol></body></html>")
    (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp \"<html><body><blockquote>Quoted text here</blockquote><ul><li>item one<ul><li>nested item</li></ul></li><li>item two</li></ul><ol><li>first</li><li>second</li></ol></body></html>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_shr_strong_em_code_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'shr)
(with-temp-buffer
  (let ((shr-width 40) (shr-inhibit-images t))
    (shr-insert-document "<html><body><p>Normal <strong>bold</strong> <em>italic</em> <code>code</code> <a href=\"http://x.com\">link</a> text.</p></body></html>")
    (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp \"<html><body><p>Normal <strong>bold</strong> <em>italic</em> <code>code</code> <a href=\\\"http://x.com\\\">link</a> text.</p></body></html>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
