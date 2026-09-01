//! Divergence tests: window configurations, window slots deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_window_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'current-window-configuration)
  (fboundp 'set-window-configuration)
  (fboundp 'window-configuration-p)
  (fboundp 'compare-window-configurations)) "#,
        expect,
    );
}

#[test]
fn divergence_window_config_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-configuration-frame)
  (fboundp 'window-configuration-buffer)
  (fboundp 'window-configuration-window)) "#,
        expect,
    );
}

#[test]
fn divergence_window_split_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'split-window-below)
  (fboundp 'split-window-right)
  (fboundp 'delete-window)
  (fboundp 'delete-other-windows)
  (fboundp 'balance-windows)) "#,
        expect,
    );
}

#[test]
fn divergence_window_buffer_swap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'set-window-buffer)
  (fboundp 'window-buffer)
  (fboundp 'get-buffer-window)
  (fboundp 'get-buffer-window-list)) "#,
        expect,
    );
}

#[test]
fn divergence_window_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-tree)
  (fboundp 'window-at)
  (fboundp 'window-absolute-pixel-edges)
  (fboundp 'window-body-edges)
  (fboundp 'window-top-line)
  (fboundp 'window-left-column)) "#,
        expect,
    );
}

#[test]
fn divergence_window_size_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-size-fixed-p)
  (fboundp 'window-resizable)
  (fboundp 'window-size)
  (fboundp 'window-full-height-p)
  (fboundp 'window-full-width-p)) "#,
        expect,
    );
}

#[test]
fn divergence_window_minibuffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'minibuffer-window)
  (fboundp 'minibuffer-window-active-p)
  (fboundp 'window-minibuffer-p)
  (fboundp 'set-minibuffer-window)) "#,
        expect,
    );
}

#[test]
fn divergence_window_margins() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t (nil no-fringe minimal default))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'set-window-margins)
  (fboundp 'window-margins)
  (fboundp 'set-window-fringes)
  (fboundp 'window-fringes)
  (boundp 'fringe-mode)
  (member fringe-mode '(nil no-fringe minimal default))) "#,
        expect,
    );
}

#[test]
fn divergence_window_scroll_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'scroll-up-command)
  (fboundp 'scroll-down-command)
  (fboundp 'scroll-other-window)
  (fboundp 'scroll-other-window-down)
  (boundp 'scroll-conservatively)
  (integerp scroll-conservatively)
  (boundp 'scroll-margin)
  (integerp scroll-margin)) "#,
        expect,
    );
}

#[test]
fn divergence_window_preserve() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-use-time)
  (fboundp 'get-most-recent-window)
  (fboundp 'window-old-buffer)
  (boundp 'window-configuration-change-hook)
  (listp window-configuration-change-hook)) "#,
        expect,
    );
}
