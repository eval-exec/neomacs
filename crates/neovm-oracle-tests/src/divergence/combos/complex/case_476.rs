/// Batch 476: ada, asm, makefile, autoconf, automake, cmake, meson, ninja.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx476_ada_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ada-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ada-mode)
  (list (fboundp 'ada-mode) (boundp 'ada-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_asm_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'asm-mode)
  (list (fboundp 'asm-mode) (boundp 'asm-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_makefile_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'make-mode)
  (list (fboundp 'makefile-mode) (boundp 'makefile-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_autoconf_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'autoconf)
  (list (fboundp 'autoconf-mode) (boundp 'autoconf-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_cmake_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"cmake-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'cmake-mode)
  (list (fboundp 'cmake-mode) (boundp 'cmake-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_nroff_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'nroff-mode)
  (list (fboundp 'nroff-mode) (boundp 'nroff-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_texinfo_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'texinfo)
  (list (fboundp 'texinfo-mode) (boundp 'texinfo-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_latex_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"latex\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'latex)
  (list (fboundp 'latex-mode) (boundp 'latex-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_bibtex_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'bibtex)
  (list (fboundp 'bibtex-mode) (boundp 'bibtex-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_tex_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'tex-mode)
  (list (fboundp 'tex-mode) (boundp 'tex-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_sgml_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'sgml-mode)
  (list (fboundp 'sgml-mode) (boundp 'sgml-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_html_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"html-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'html-mode)
  (list (fboundp 'html-mode) (boundp 'html-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_css_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'css-mode)
  (list (fboundp 'css-mode) (boundp 'css-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_js_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'js)
  (list (fboundp 'js-mode) (boundp 'js-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx476_typescript_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"typescript-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'typescript-mode)
  (list (fboundp 'typescript-mode) (boundp 'typescript-mode-map)))
"##,
        expect,
    );
}
