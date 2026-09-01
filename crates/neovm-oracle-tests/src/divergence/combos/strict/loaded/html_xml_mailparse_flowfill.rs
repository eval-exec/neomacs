//! Strict combo oracle probes, batch 38: encoding/parsing loaded libraries via
//! assert_oracle_parity_with_load — sgml-mode (html-escape-string), xml.el
//! (xml-parse-region), mail/mail-parse.el (mail-header-parse-address-list,
//! which uses ietf-drums — so the comma bug may propagate), and flow-fill.el.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_h5_html_escape_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function html-escape-string)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (html-escape-string "<a href=\"x\">&amp;</a>")
      (html-escape-string "plain & < > \" ' text")
      (length (html-escape-string "&<>\"'")))
"##,
        &["textmodes/sgml-mode.el"],
        expect,
    );
}

#[test]
fn div_h5_xml_parse_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (root (a b))""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "<root><a>text</a><b attr=\"1\"/></root>")
  (let ((parsed (xml-parse-region (point-min) (point-max))))
    (list (and parsed (xml-node-name (car parsed)))
          (and parsed (mapcar #'xml-node-name (xml-node-children (car parsed)))))))
"##,
        &["xml.el"],
        expect,
    );
}

#[test]
fn div_h5_mail_header_parse_address_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function mail-header-parse-address-list)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (mail-header-parse-address-list "a@b.com, c@d.com")
      (mail-header-parse-address-list "John <a@b.com>, Jane <c@d.com>")
      (length (mail-header-parse-address-list "a@b.com, c@d.com, e@f.com")))
"##,
        &["mail/mail-parse.el"],
        expect,
    );
}

#[test]
fn div_h5_fill_flowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"No buffer named line one\\nline two\\nline three\")""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (fill-flowed "line one\nline two\nline three")
      (fill-flowed "> soft\n> wrap\n> here"))
"##,
        &["mail/flow-fill.el"],
        expect,
    );
}

#[test]
fn div_h5_xml_parse_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1\" \"two\" nil 1)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "<root a=\"1\" b=\"two\"><child/></root>")
  (let ((parsed (car (xml-parse-region (point-min) (point-max)))))
    (list (xml-get-attribute parsed 'a)
          (xml-get-attribute parsed 'b)
          (xml-get-attribute-or-nil parsed 'c)
          (length (xml-node-children parsed)))))
"##,
        &["xml.el"],
        expect,
    );
}
