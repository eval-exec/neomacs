/// Batch 475: gud, gdb, etags, ebrowse, ccmode, cflow, emerge, compile deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx475_gud_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'gud)
  (list (boundp 'gud-mode-map) (fboundp 'gud-gdb)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_gdb_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'gdb-mi)
  (list (fboundp 'gdb) (boundp 'gdb-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_etags_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'etags)
  (list (fboundp 'find-tag) (fboundp 'tags-search)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_ebrowse_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ebrowse)
  (list (fboundp 'ebrowse) (boundp 'ebrowse-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_ccmode_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'cc-mode)
  (list (fboundp 'c-mode) (boundp 'c-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_cflow_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"cflow\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'cflow)
  (list (fboundp 'cflow) (boundp 'cflow-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_emerge_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'emerge)
  (list (fboundp 'emerge-files) (boundp 'emerge-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_compile_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'compile)
  (list (fboundp 'compile) (boundp 'compilation-buffer-name-function)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_grep_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'grep)
  (list (fboundp 'grep) (fboundp 'lgrep) (boundp 'grep-find-command)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_python_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'python)
  (list (fboundp 'run-python) (boundp 'python-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_ruby_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ruby-mode)
  (list (fboundp 'ruby-mode) (boundp 'ruby-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_perl_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'cperl-mode)
  (list (fboundp 'cperl-mode) (boundp 'cperl-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_tcl_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'tcl)
  (list (fboundp 'tcl-mode) (boundp 'tcl-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_fortran_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'fortran)
  (list (fboundp 'fortran-mode) (boundp 'fortran-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx475_pascal_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'pascal)
  (list (fboundp 'pascal-mode) (boundp 'pascal-mode-map)))
"##,
        expect,
    );
}
