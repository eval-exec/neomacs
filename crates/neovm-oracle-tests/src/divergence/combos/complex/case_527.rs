/// Batch 527: window-margins, window-fringes, window-scroll-bars deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx527_window_margins_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 . 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (set-window-margins w 3 4)
  (window-margins w))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_fringes_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (set-window-fringes w 5 10 nil)
  (window-fringes w))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_scroll_bars_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 0 t nil 0 t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (set-window-scroll-bars w nil 8 nil t)
  (window-scroll-bars w))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_margins_clear() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (set-window-margins w nil nil)
  (window-margins w))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_fringes_clear() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (set-window-fringes w nil nil nil)
  (window-fringes w))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_dedicated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (set-window-dedicated-p w t)
  (window-dedicated-p w))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (list (window-combined-p w nil) (window-combined-p w t)))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_next_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-next-buffers w))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_new_total() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-new-total w))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_new_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-new-pixel w))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_new_normal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-new-total w))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_total_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (list (window-total-width w) (window-total-height w)))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_pixel_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (list (window-pixel-left w) (window-pixel-top w)))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_resize_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (window-resize-apply (selected-window))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx527_window_freeze() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 0 t nil 0 t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (set-window-fringes w 10 10)
  (set-window-scroll-bars w 'left 10 t)
  (window-scroll-bars w))
"##,
        expect,
    );
}
