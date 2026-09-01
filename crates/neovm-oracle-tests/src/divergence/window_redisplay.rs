//! Divergence tests: xdisp, redisplay, window-start/end, pos-visible.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_window_start_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((w (selected-window)))
  (list (integer-or-marker-p (window-start w))
        (integer-or-marker-p (window-end w))
        (>= (window-end w) (window-start w))
        (integerp (window-point w))))"#,
        expect,
    );
}

#[test]
fn divergence_set_window_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((w (selected-window)))
  (list (fboundp 'set-window-start)
        (fboundp 'set-window-point)
        (fboundp 'set-window-vscroll)
        (fboundp 'window-vscroll)))"#,
        expect,
    );
}

#[test]
fn divergence_pos_visible_in_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'pos-visible-in-window-p)
  (fboundp 'window-edges)
  (fboundp 'window-inside-edges))"#,
        expect,
    );
}

#[test]
fn divergence_redisplay_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable redisplay-dont-pause)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'redisplay)
  (fboundp 'force-window-update)
  (boundp 'redisplay-dont-pause)
  (booleanp redisplay-dont-pause))"#,
        expect,
    );
}

#[test]
fn divergence_window_text_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((w (selected-window)))
  (list (integerp (window-text-height w))
        (integerp (window-text-width w))
        (integerp (window-body-height w t))
        (integerp (window-body-width w t))))"#,
        expect,
    );
}

#[test]
fn divergence_window_margins() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-margins)
  (fboundp 'set-window-margins)
  (fboundp 'window-fringes)
  (fboundp 'set-window-fringes))"#,
        expect,
    );
}

#[test]
fn divergence_window_scroll_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'scroll-up-command)
  (fboundp 'scroll-down-command)
  (fboundp 'scroll-other-window)
  (fboundp 'scroll-other-window-down))"#,
        expect,
    );
}

#[test]
fn divergence_recenter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'recenter)
  (fboundp 'recenter-top-bottom)
  (boundp 'recenter-redisplay))"#,
        expect,
    );
}

#[test]
fn divergence_window_dedicated() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((w (selected-window)))
  (list (window-dedicated-p w)
        (fboundp 'set-window-dedicated-p)
        (window-parameter w 'window-side)))"#,
        expect,
    );
}

#[test]
fn divergence_window_combination_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'window-combination-limit)
  (fboundp 'set-window-combination-limit)
  (fboundp 'window-combination-resize)
  (fboundp 'split-window)
  (fboundp 'delete-window))"#,
        expect,
    );
}
