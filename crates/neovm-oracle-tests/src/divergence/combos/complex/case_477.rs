/// Batch 477: yaml, toml, json-mode, xml, nxml, markdown, rst, reStructuredText.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx477_yaml_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"yaml-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'yaml-mode)
  (list (fboundp 'yaml-mode) (boundp 'yaml-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_toml_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"toml-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'toml-mode)
  (list (fboundp 'toml-mode) (boundp 'toml-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_json_mode_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"json-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'json-mode)
  (list (fboundp 'json-mode) (boundp 'json-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_nxml_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'nxml-mode)
  (list (fboundp 'nxml-mode) (boundp 'nxml-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_xml_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"xml-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'xml-mode)
  (list (fboundp 'xml-mode) (boundp 'xml-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_markdown_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"markdown-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'markdown-mode)
  (list (fboundp 'markdown-mode) (boundp 'markdown-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_rst_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'rst)
  (list (fboundp 'rst-mode) (boundp 'rst-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_prolog_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'prolog)
  (list (fboundp 'prolog-mode) (boundp 'prolog-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_sql_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'sql)
  (list (fboundp 'sql-mode) (boundp 'sql-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_scheme_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'cmuscheme)
  (list (fboundp 'scheme-mode) (boundp 'scheme-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_lisp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'lisp-mode)
  (list (fboundp 'lisp-mode) (boundp 'lisp-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_racket_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"racket-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'racket-mode)
  (list (fboundp 'racket-mode) (boundp 'racket-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_haskell_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"haskell-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'haskell-mode)
  (list (fboundp 'haskell-mode) (boundp 'haskell-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_ocaml_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ocaml-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ocaml-mode)
  (list (fboundp 'ocaml-mode) (boundp 'ocaml-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx477_fsharp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"fsharp-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'fsharp-mode)
  (list (fboundp 'fsharp-mode) (boundp 'fsharp-mode-map)))
"##,
        expect,
    );
}
