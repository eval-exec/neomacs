/// Batch 473: edmacro, expand, ffap, find-func, find-dired, flow-fill, forms, gametree.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx473_edmacro_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'edmacro)
  (list (fboundp 'edmacro-format-keys) (fboundp 'edmacro-parse-keys)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_expand_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'expand)
  (list (boundp 'expand-list) (fboundp 'expand-abbrev-hook)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_ffap_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ffap)
  (list (fboundp 'ffap) (fboundp 'ffap-menu)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_find_func_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'find-func)
  (list (fboundp 'find-function) (fboundp 'find-variable)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_find_dired_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'find-dired)
  (list (fboundp 'find-dired) (boundp 'find-ls-option)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_flow_fill_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'flow-fill)
  (list (fboundp 'fill-flowed) (fboundp 'fill-flowed-encode)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_forms_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'forms)
  (list (fboundp 'forms-mode) (boundp 'forms-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_gametree_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'gametree)
  (list (fboundp 'gametree-layout-mode) (boundp 'gametree-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_hide_show_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'hideshow)
  (list (fboundp 'hs-minor-mode) (boundp 'hs-special-modes-alist)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_ibuffer_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ibuffer)
  (list (fboundp 'ibuffer) (boundp 'ibuffer-formats)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_jka_compr_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'jka-compr)
  (list (boundp 'jka-compr-compression-info-list) (fboundp 'jka-compr-install)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_kermit_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'kermit)
  (list (fboundp 'kermit) (boundp 'kermit-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_latin_lang() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"language/latin-1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'language/latin-1)
  (list (boundp 'latin-1-language-environment)))
"##,
        expect,
    );
}

#[test]
fn div_cx473_language_env() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t (\"English\" (documentation . \"Nothing special is needed to handle English.\") (sample-text . \"Hello!, Hi!, How are you?\") (charset ascii) (tutorial . \"TUTORIAL\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (boundp 'language-info-alist) (assoc "English" language-info-alist))
"##,
        expect,
    );
}

#[test]
fn div_cx473_ledit_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ledit\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ledit)
  (list (fboundp 'ledit-mode) (boundp 'ledit-version)))
"##,
        expect,
    );
}
