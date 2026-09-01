//! Per-defined-color *color-values* matrix (all tty defined colors).
//!
//! One focused #[test] per color in (defined-colors): query color-values.
//! tty color RGB tables may differ between Neomacs and GNU.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_color_val_black() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable black)""#]];
    crate::common::assert_oracle_parity_expect("(color-values black)", expect);
}

#[test]
fn div_color_val_blue() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable blue)""#]];
    crate::common::assert_oracle_parity_expect("(color-values blue)", expect);
}

#[test]
fn div_color_val_cyan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable cyan)""#]];
    crate::common::assert_oracle_parity_expect("(color-values cyan)", expect);
}

#[test]
fn div_color_val_green() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable green)""#]];
    crate::common::assert_oracle_parity_expect("(color-values green)", expect);
}

#[test]
fn div_color_val_magenta() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable magenta)""#]];
    crate::common::assert_oracle_parity_expect("(color-values magenta)", expect);
}

#[test]
fn div_color_val_red() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable red)""#]];
    crate::common::assert_oracle_parity_expect("(color-values red)", expect);
}

#[test]
fn div_color_val_white() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable white)""#]];
    crate::common::assert_oracle_parity_expect("(color-values white)", expect);
}

#[test]
fn div_color_val_yellow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable yellow)""#]];
    crate::common::assert_oracle_parity_expect("(color-values yellow)", expect);
}
