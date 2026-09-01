//! Divergence tests: whitespace mode, trailing whitespace, tab settings.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_whitespace_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'whitespace-mode)
  (fboundp 'global-whitespace-mode)
  (featurep 'whitespace)) "#,
        expect,
    );
}

#[test]
fn whitespace_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable whitespace-style)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'whitespace-cleanup)
  (fboundp 'whitespace-buffer)
  (boundp 'whitespace-style)
  (listp whitespace-style)) "#,
        expect,
    );
}

#[test]
fn divergence_tab_settings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t (t complete))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'tab-width)
  (integerp tab-width)
  (boundp 'indent-tabs-mode)
  (booleanp indent-tabs-mode)
  (boundp 'tab-always-indent)
  (member tab-always-indent '(nil t complete))) "#,
        expect,
    );
}

#[test]
fn divergence_line_endings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'require-final-newline)
  (boundp 'mode-require-final-newline)
  (boundp 'buffer-file-coding-system)) "#,
        expect,
    );
}

#[test]
fn divergence_electric_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'electric-indent-mode)
  (boundp 'electric-indent-chars)
  (listp electric-indent-chars)
  (boundp 'electric-indent-mode)
  (booleanp electric-indent-mode)) "#,
        expect,
    );
}

#[test]
fn divergence_paren_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t (parenthesis expression mixed) t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'show-paren-mode)
  (boundp 'show-paren-style)
  (member show-paren-style '(parenthesis expression mixed))
  (boundp 'show-paren-mode)
  (booleanp show-paren-mode)) "#,
        expect,
    );
}

#[test]
fn divergence_electric_pair() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable electric-pair-pairs)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'electric-pair-mode)
  (boundp 'electric-pair-pairs)
  (listp electric-pair-pairs)
  (boundp 'electric-pair-mode)
  (booleanp electric-pair-mode)) "#,
        expect,
    );
}

#[test]
fn divergence_line_number_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable display-line-numbers-type)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'display-line-numbers-mode)
  (fboundp 'global-display-line-numbers-mode)
  (boundp 'display-line-numbers-type)
  (member display-line-numbers-type '(t relative visual nil))) "#,
        expect,
    );
}

#[test]
fn divergence_line_number_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'line-number-mode)
  (boundp 'column-number-mode)
  (boundp 'size-indication-mode)
  (booleanp line-number-mode)
  (booleanp column-number-mode)
  (booleanp size-indication-mode)) "#,
        expect,
    );
}

#[test]
fn divergence_delete_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'delete-selection-mode)
  (boundp 'delete-selection-mode)
  (booleanp delete-selection-mode)
  (featurep 'delsel)) "#,
        expect,
    );
}
