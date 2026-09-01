//! Strict combo oracle probes, batch 219: XML parsing deep. xml-parse-region
//! over nested elements with attributes, mixed content, empty elements, CDATA,
//! and unicode text + entity references.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_xml_parse_nested_attributes_children() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'xml)
(with-temp-buffer
  (insert "<root attr=\"val\" num=\"42\"><child>text</child><empty/><sibling a=\"1\">inner</sibling></root>")
  (let ((parsed (xml-parse-region)))
    (list (caar parsed)
          (cadar parsed)
          (length (cddar parsed)))))
"##;
    let expect = expect_test::expect![[r#""OK (root ((attr . \"val\") (num . \"42\")) 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_xml_parse_mixed_text_entities_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'xml)
(with-temp-buffer
  (insert "<doc>Hello &amp; goodbye &lt;tag&gt; café 日本語</doc>")
  (let ((parsed (car (xml-parse-region))))
    (list (car parsed)
          (xml-get-children parsed 'missing)
          (caddr parsed))))
"##;
    let expect = expect_test::expect![[r#""OK (doc nil \"Hello & goodbye <tag> café 日本語\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_xml_get_attribute_children_helpers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'xml)
(with-temp-buffer
  (insert "<root id=\"r1\"><item n=\"1\">a</item><item n=\"2\">b</item><other>c</other></root>")
  (let* ((parsed (car (xml-parse-region)))
         (items (xml-get-children parsed 'item)))
    (list (xml-get-attribute parsed 'id)
          (xml-get-attribute parsed 'missing 'default)
          (length items)
          (xml-get-attribute (car items) 'n)
          (mapcar (lambda (i) (caddr i)) items))))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
