//! Divergence tests: frame parameters deep, display attrs, font specs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_frame_font() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"tty\" \"unspecified-fg\" \"unspecified-bg\" \"white\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((f (selected-frame)))
  (list (frame-parameter f 'font)
        (frame-parameter f 'foreground-color)
        (frame-parameter f 'background-color)
        (frame-parameter f 'cursor-color)
        (frame-parameter f 'mouse-color)))"#,
        expect,
    );
}

#[test]
fn divergence_frame_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((f (selected-frame)))
  (list (> (frame-width f) 0)
        (> (frame-height f) 0)
        (integerp (frame-width f))
        (integerp (frame-height f))
        (fboundp 'set-frame-size)
        (fboundp 'set-frame-position)))"#,
        expect,
    );
}

#[test]
fn divergence_frame_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((f (selected-frame)))
  (list (stringp (frame-parameter f 'name))
        (fboundp 'modify-frame-parameters)
        (frame-parameter f 'title)
        (stringp (or (frame-parameter f 'title) ""))))"#,
        expect,
    );
}

#[test]
fn divergence_frame_terminal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\\\"\" 4 37)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((f (selected-frame)))
  (list (terminal-live-p (frame-terminal f))
        (fboundp 'terminal-list)
        (fboundp 'frame-terminal)))#" ,
    );
}

#[test]
fn divergence_frame_child_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    assert_oracle_parity(
        r#"(list
  (fboundp 'make-frame)
  (fboundp 'make-frame-invisible)
  (fboundp 'make-frame-visible)
  (fboundp 'iconify-frame)
  (fboundp 'delete-frame)
  (fboundp 'frame-parent)
  (fboundp 'frame-ancestor-p))"#,
        expect,
    );
}

#[test]
fn divergence_display_color_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\\\"\" 5 39)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'display-color-cells)
  (fboundp 'display-color-p)
  (fboundp 'display-grayscale-p)
  (display-color-p (selected-frame)))#" ,
    );
}

#[test]
fn divergence_font_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    assert_oracle_parity(
        r#"(list
  (fboundp 'font-spec)
  (fboundp 'font-get)
  (fboundp 'font-put)
  (fboundp 'font-face-attributes)
  (fboundp 'font-xlfd-name))"#,
        expect,
    );
}

#[test]
fn divergence_cursor_appearance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'cursor-type)
  (boundp 'x-stretch-cursor)
  (boundp 'blink-cursor-interval)
  (boundp 'blink-cursor-delay)
  (integerp blink-cursor-interval))"#,
        expect,
    );
}

#[test]
fn divergence_pointer_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'x-pointer-shape)
  (boundp 'x-sensitive-text-pointer-shape)
  (fboundp 'x-set-selection)
  (fboundp 'x-get-selection))"#,
        expect,
    );
}

#[test]
fn divergence_tooltips() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'tooltip-mode)
  (boundp 'tooltip-delay)
  (boundp 'tooltip-short-delay)
  (fboundp 'tooltip-show)
  (featurep 'tooltip))"#,
        expect,
    );
}
