//! Divergence tests: pixel vs char positions, window edges, buffer display.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_window_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-edges)
  (fboundp 'window-inside-edges)
  (fboundp 'window-pixel-edges)
  (fboundp 'window-inside-pixel-edges))"#,
        expect,
    );
}

#[test]
fn divergence_window_body_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-body-height)
  (fboundp 'window-body-width)
  (fboundp 'window-total-height)
  (fboundp 'window-total-width))"#,
        expect,
    );
}

#[test]
fn divergence_window_pixel_dimensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-pixel-height)
  (fboundp 'window-pixel-width)
  (fboundp 'window-mode-line-height)
  (fboundp 'window-header-line-height))"#,
        expect,
    );
}

#[test]
fn divergence_window_scroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'scroll-up)
  (fboundp 'scroll-down)
  (fboundp 'scroll-other-window)
  (fboundp 'scroll-other-window-down)
  (fboundp 'recenter))"#,
        expect,
    );
}

#[test]
fn divergence_window_hscroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-hscroll)
  (fboundp 'set-window-hscroll)
  (fboundp 'scroll-left)
  (fboundp 'scroll-right))"#,
        expect,
    );
}

#[test]
fn divergence_window_start_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-start)
  (fboundp 'window-end)
  (fboundp 'set-window-start)
  (fboundp 'set-window-point)
  (fboundp 'pos-visible-in-window-p))"#,
        expect,
    );
}

#[test]
fn divergence_window_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-buffer)
  (fboundp 'window-point)
  (fboundp 'window-dedicated-p)
  (fboundp 'set-window-dedicated-p))"#,
        expect,
    );
}

#[test]
fn divergence_window_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-parameters)
  (fboundp 'window-parameter)
  (fboundp 'set-window-parameter))"#,
        expect,
    );
}

#[test]
fn divergence_window_display_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-display-table)
  (fboundp 'set-window-display-table))"#,
        expect,
    );
}

#[test]
fn divergence_window_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-combination-limit)
  (fboundp 'set-window-combination-limit)
  (fboundp 'window-combination-resize)
  (fboundp 'set-window-combination-resize))"#,
        expect,
    );
}
