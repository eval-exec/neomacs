//! Strict combo oracle probes, batch 76: org-export (ox-ascii — export an org
//! document to plain text) and mml-parse (MIME multipart compose). These are
//! the heaviest commonly-used libraries remaining untested.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_p0_org_export_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Table of Contents\\n_________________\\n\\n1. Heading 1\\n.. 1. Sub heading\\n\\n\\n1 Heading 1\\n===========\\n\\n1.1 Sub heading\\n~~~~~~~~~~~~~~~\\n\\n  Paragraph text here.\\n  - item one\\n  - item two\\n\"""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (org-mode)
  (insert "* Heading 1\n** Sub heading\nParagraph text here.\n- item one\n- item two\n")
  (org-export-as 'ascii))
"##,
        &["org/org.el", "org/ox.el", "org/ox-ascii.el"],
        expect,
    );
}

#[test]
fn div_p0_org_export_ascii_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Table of Contents\\n_________________\\n\\n1. Table test\\n\\n\\n1 Table test\\n============\\n\\n   Name  Value \\n  -------------\\n   a         1 \\n   b         2 \\n\"""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (org-mode)
  (insert "* Table test\n| Name | Value |\n|---+---|\n| a | 1 |\n| b | 2 |\n")
  (org-export-as 'ascii))
"##,
        &["org/org.el", "org/ox.el", "org/ox-ascii.el"],
        expect,
    );
}

#[test]
fn div_p0_mml_parse_multipart() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((part (type . \"text/plain\") (disposition . \"inline\") (tag-location . 1) (contents . \"Hello body\\n\")))""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "<#part type=\"text/plain\" disposition=\"inline\">\nHello body\n<#/part>\n")
  (mml-parse))
"##,
        &["gnus/mml.el"],
        expect,
    );
}
