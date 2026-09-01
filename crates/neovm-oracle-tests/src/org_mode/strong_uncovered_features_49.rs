//! Strong uncovered-features-49 oracle tests — org-export string, org-link, org-protocol.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-export-string-as html
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_export_html() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-export-string-as "* H\nBody *bold*" 'html t)"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-string-as latex
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_export_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-export-string-as "* H\nBody *bold*" 'latex t)"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-string-as ascii
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_export_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-export-string-as "* H\nBody *bold*" 'ascii t)"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-export-string-as with options
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_export_opts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-export-string-as "#+TITLE: T\n* H\nBody" 'html t)"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-link-types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_link_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable org-link-types)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(sort (copy-sequence org-link-types) 'string<)"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-link-protocols
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_link_protocols() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable org-link-protocols)""#]];
    crate::common::assert_oracle_parity_expect(r##"(mapcar 'car org-link-protocols)"##, expect);
}

// ═══════════════════════════════════════════════════════════════════════
// org-link-escape-browser
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_link_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-link-escape-browser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-link-escape-browser "http://example.com?a=1&b=2")
        (org-link-escape-browser "hello world")
        (org-link-escape-browser "test%20"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-link-unescape
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_link_unescape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-link-unescape)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-link-unescape "http://example.com?a=1%26b=2")
        (org-link-unescape "hello%20world")
        (org-link-unescape "test%2520"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-link-plain-re
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_link_plain_re() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable org-link-plain-re)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(string-match-p org-link-plain-re "http://example.com")"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-bracket-link-regexp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_link_bracket_re() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable org-bracket-link-regexp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(string-match-p org-bracket-link-regexp "[[http://example.com][Example]]")"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-protocol-parse-parameters
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_protocol_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-protocol-parse-parameters)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-protocol-parse-parameters "org-protocol://store-link?url=http://example.com&title=Test")"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-protocol-sanitize-uri
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_protocol_sanitize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-protocol-sanitize-uri)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-protocol-sanitize-uri "http://example.com")
        (org-protocol-sanitize-uri "https://test.org/path?a=1&b=2")
        (org-protocol-sanitize-uri "file:///tmp/test.txt"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-protocol-check-protocol-for
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_protocol_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-protocol-check-protocol-for)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-protocol-check-protocol-for "store-link")"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-store-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_store_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\nBody")
  (goto-char (point-min))
  (org-store-link nil)
  (list (car org-stored-links)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-insert-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_insert_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[[http://example.com][Example]]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (org-insert-link nil "http://example.com" "Example")
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-open-at-point on link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_open_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"Link\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n[[http://example.com][Link]]")
  (search-forward "Link")
  (list (org-element-property :type (org-element-context))
        (org-element-property :path (org-element-context))
        (org-element-property :raw-link (org-element-context))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map links
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_map_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"http\" \"//a.com\") (\"file\" \"b.el\") (\"id\" \"xxx\") (\"mailto\" \"d@e.com\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n[[http://a.com][A]] [[file:b.el][B]] [[id:xxx][C]] [[mailto:d@e.com]]")
  (org-element-map (org-element-parse-buffer) 'link
    (lambda (l) (list (org-element-property :type l)
                      (org-element-property :path l)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map timestamps
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_map_timestamps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((active-range 2026 20) (inactive 2026 30))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15 Wed>\n* U\n<2026-01-20>--<2026-01-25>\n* V\n[2026-01-30]")
  (org-element-map (org-element-parse-buffer) 'timestamp
    (lambda (ts) (list (org-element-property :type ts)
                      (org-element-property :year-start ts)
                      (org-element-property :day-start ts)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_map_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((timestamp (:standard-properties [21 nil nil nil 33 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-15>\" :year-start 2026 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 15 :hour-end nil :minute-end nil)) nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\nCLOSED: [2026-01-10]")
  (org-element-map (org-element-parse-buffer) 'planning
    (lambda (p) (list (org-element-property :scheduled p)
                      (org-element-property :deadline p)
                      (org-element-property :closed p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map clock
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_map_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((closed \"1:30\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30")
  (org-element-map (org-element-parse-buffer) 'clock
    (lambda (c) (list (org-element-property :status c)
                      (org-element-property :duration c)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map footnote
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf49_map_footnote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"1\" \"2\") (\"1\" \"2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2]\n\n[fn:1] Def1\n[fn:2] Def2")
  (list (org-element-map (org-element-parse-buffer) 'footnote-reference
          (lambda (f) (org-element-property :label f)))
        (org-element-map (org-element-parse-buffer) 'footnote-definition
          (lambda (f) (org-element-property :label f)))))"##,
        expect,
    );
}
