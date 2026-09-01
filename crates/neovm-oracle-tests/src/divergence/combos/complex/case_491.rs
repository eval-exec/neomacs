/// Batch 491: mouse-face, mouse-highlight, mouse-avoidance, mouse-sel, mouse-drag.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx491_mouse_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'mouse)
  (list (boundp 'mouse-face) (fboundp 'mouse-set-point)))
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'mouse)
  (boundp 'mouse-highlight))
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_avoidance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'avoid)
  (list (fboundp 'mouse-avoidance-mode) (boundp 'mouse-avoidance-mode)))
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_drag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'mouse-drag)
  (list (fboundp 'mouse-drag-throw) (boundp 'mouse-drag-mode)))
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_sensitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"mouse-sel\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'mouse-sel)
  (list (boundp 'mouse-sel-mode) (fboundp 'mouse-select-region)))
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_visible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (boundp 'make-pointer-invisible) (boundp 'mouse-wheel-follow-mouse))
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_wheel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'mwheel)
  (list (boundp 'mouse-wheel-mode) (fboundp 'mwheel-install)))
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_autoselect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(boundp 'mouse-autoselect-window)
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_avoidance_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'avoid)
  (fboundp 'mouse-avoidance-nudge-mouse))
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_avoidance_delta() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'avoid)
  (boundp 'mouse-avoidance-nudge-dist))
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_wheel_scroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(boundp 'mouse-wheel-scroll-amount)
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_wheel_tilt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(boundp 'mouse-wheel-tilt-scroll)
"##,
        expect,
    );
}

#[test]
fn div_cx491_display_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(boundp 'display-mouse-p)
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (framep (car (mouse-pixel-position))) (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx491_mouse_reset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (framep (car (mouse-position))) (error (car e)))
"##,
        expect,
    );
}
