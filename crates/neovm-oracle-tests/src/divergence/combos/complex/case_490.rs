/// Batch 490: fringe, scroll-bar, tool-bar, menu-bar remaining edge probes.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx490_fringe_exist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 2) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (fringe-columns) (fringe-mode))
"##,
        expect,
    );
}

#[test]
fn div_cx490_scroll_bar_exist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (scroll-bar-mode -1) (horizontal-scroll-bar-mode -1))
"##,
        expect,
    );
}

#[test]
fn div_cx490_menu_bar_exist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (menu-bar-mode -1)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx490_tool_bar_exist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (tool-bar-mode -1)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx490_tab_bar_exist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'tab-bar)
  (list (fboundp 'tab-bar-new-tab) (fboundp 'tab-bar-close-tab)))
"##,
        expect,
    );
}

#[test]
fn div_cx490_horizontal_scroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (scroll-left 1)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx490_recenter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (recenter 0)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx490_move_to_window_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (move-to-window-line 0)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx490_scroll_other_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (scroll-other-window 1)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx490_window_scroll_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-scroll-bar-height w))
"##,
        expect,
    );
}

#[test]
fn div_cx490_window_fringe_bitmap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-fringes w))
"##,
        expect,
    );
}

#[test]
fn div_cx490_window_divider() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (window-divider-mode 1)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx490_display_fill_indicator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'display-fill-column-indicator)
  (with-temp-buffer
    (display-fill-column-indicator-mode 1)
    display-fill-column-indicator-mode))
"##,
        expect,
    );
}

#[test]
fn div_cx490_display_line_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'display-line-numbers)
  (with-temp-buffer
    (display-line-numbers-mode 1)
    display-line-numbers-mode))
"##,
        expect,
    );
}

#[test]
fn div_cx490_visual_line_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (visual-line-mode 1)
  visual-line-mode)
"##,
        expect,
    );
}
