//! Divergence tests: custom themes, customize interface deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_customize_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'customize-variable)
  (fboundp 'customize-group)
  (fboundp 'customize-face)
  (fboundp 'custom-set-variables)
  (fboundp 'custom-set-faces)
  (featurep 'custom))"#,
        expect,
    );
}

#[test]
fn divergence_custom_theme_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'load-theme)
  (fboundp 'enable-theme)
  (fboundp 'disable-theme)
  (fboundp 'custom-theme-p)
  (fboundp 'custom-available-themes))"#,
        expect,
    );
}

#[test]
fn divergence_custom_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'custom-enabled-themes)
  (listp custom-enabled-themes)
  (boundp 'custom-theme-load-path)
  (listp custom-theme-load-path))"#,
        expect,
    );
}

#[test]
fn divergence_custom_save() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'customize-save-variable)
  (fboundp 'customize-save-customized)
  (fboundp 'custom-save-all)
  (fboundp 'custom-save-variables))"#,
        expect,
    );
}

#[test]
fn divergence_custom_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'custom-variable-p)
  (fboundp 'custom-variable-documentation)
  (fboundp 'custom-group-members)
  (fboundp 'custom-group-of-mode))"#,
        expect,
    );
}

#[test]
fn divergence_face_customize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'face-spec-set)
  (fboundp 'face-spec-match-p)
  (fboundp 'face-spec-reset-face)
  (fboundp 'face-spec-recolor-face))"#,
        expect,
    );
}

#[test]
fn divergence_face_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'face-inherits-from-face-p)
  (fboundp 'face-all-attributes)
  (fboundp 'face-default-spec)
  (fboundp 'face-user-default-spec))"#,
        expect,
    );
}

#[test]
fn divergence_widget_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'widget-create)
  (fboundp 'widget-browse)
  (fboundp 'widget-delete)
  (featurep 'wid-edit))"#,
        expect,
    );
}

#[test]
fn divergence_widget_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'widget-type)
  (fboundp 'widgetp)
  (fboundp 'widget-put)
  (fboundp 'widget-get))"#,
        expect,
    );
}

#[test]
fn divergence_custom_dependencies() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'custom-dependencies)
  (fboundp 'custom-load-symbol)
  (fboundp 'custom-note-variable-changed)) "#,
        expect,
    );
}
