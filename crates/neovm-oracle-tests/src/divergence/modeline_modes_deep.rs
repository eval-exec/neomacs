//! Divergence tests: mode-line, header-line, mode hooks deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_mode_line_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'mode-line-format)
  (listp mode-line-format)
  (boundp 'mode-line-modified)
  (listp mode-line-modified)
  (boundp 'mode-line-buffer-identification)
  (listp mode-line-buffer-identification)) "#,
        expect,
    );
}

#[test]
fn divergence_header_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'header-line-format)
  (boundp 'mode-line-front-space)
  (boundp 'mode-line-mule-info)
  (boundp 'mode-line-client)
  (boundp 'mode-line-remote)
  (boundp 'mode-line-frame-identification)) "#,
        expect,
    );
}

#[test]
fn divergence_mode_line_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'mode-line-position)
  (listp mode-line-position)
  (fboundp 'line-number-at-pos)
  (integerp (line-number-at-pos))) "#,
        expect,
    );
}

#[test]
fn divergence_mode_line_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'mode-line-modes)
  (listp mode-line-modes)
  (boundp 'mode-name)
  (stringp mode-name)
  (boundp 'major-mode)
  (symbolp major-mode)) "#,
        expect,
    );
}

#[test]
fn divergence_mode_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'after-change-major-mode-hook)
  (boundp 'change-major-mode-hook)
  (listp after-change-major-mode-hook)
  (listp change-major-mode-hook)
  (fboundp 'run-mode-hooks)) "#,
        expect,
    );
}

#[test]
fn divergence_delayed_mode_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'delay-mode-hooks)
  (booleanp delay-mode-hooks)
  (fboundp 'delay-mode-hooks-update)) "#,
        expect,
    );
}

#[test]
fn divergence_global_mode_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'global-mode-string)
  (listp global-mode-string)
  (fboundp 'format-mode-line)
  (stringp (format-mode-line mode-line-format))) "#,
        expect,
    );
}

#[test]
fn divergence_minor_mode_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'minor-mode-alist)
  (listp minor-mode-alist)
  (boundp 'minor-mode-overriding-map-alist)
  (listp minor-mode-overriding-map-alist)) "#,
        expect,
    );
}

#[test]
fn divergence_special_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'special-mode)
  (fboundp 'fundamental-mode)
  (fboundp 'text-mode)
  (fboundp 'prog-mode)
  (featurep 'prog-mode)) "#,
        expect,
    );
}

#[test]
fn derivation_mode_derive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'derived-mode-p)
  (fboundp 'provided-mode-derived-p)
  (fboundp 'set-buffer-major-mode)) "#,
        expect,
    );
}
