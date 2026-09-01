//! Divergence tests: apropos, help, info, man stubs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_apropos_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'apropos-command)
  (fboundp 'apropos-variable)
  (fboundp 'apropos-documentation)
  (fboundp 'apropos-library))"#,
        expect,
    );
}

#[test]
fn divergence_help_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'describe-function)
  (fboundp 'describe-variable)
  (fboundp 'describe-key)
  (fboundp 'describe-mode)
  (fboundp 'describe-char))"#,
        expect,
    );
}

#[test]
fn divergence_info_functions_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'info-lookup-symbol)
  (fboundp 'info-display-manual)
  (featurep 'info))"#,
        expect,
    );
}

#[test]
fn divergence_man_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'man)
  (fboundp 'woman)
  (featurep 'man))"#,
        expect,
    );
}

#[test]
fn divergence_elisp_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'elisp-index-search)
  (fboundp 'emacs-index-search))"#,
        expect,
    );
}

#[test]
fn divergence_completion_styles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t (basic partial-completion emacs22) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (listp completion-styles)
  (member 'basic completion-styles)
  (fboundp 'completion-styles-alist))"#,
        expect,
    );
}

#[test]
fn divergence_completion_categories() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'completion-category-defaults)
  (fboundp 'completion-category-overrides)
  (boundp 'completion-category-defaults))"#,
        expect,
    );
}

#[test]
fn divergence_minibuffer_completion_auto() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'minibuffer-complete)
  (fboundp 'minibuffer-complete-word)
  (fboundp 'minibuffer-complete-and-exit))"#,
        expect,
    );
}

#[test]
fn divergence_corfu_company() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'company-mode)
  (fboundp 'corfu-mode)
  (featurep 'company)
  (featurep 'corfu))"#,
        expect,
    );
}

#[test]
fn divergence_which_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'which-key-mode)
  (featurep 'which-key))"#,
        expect,
    );
}
