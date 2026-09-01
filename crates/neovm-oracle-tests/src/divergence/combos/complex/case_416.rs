//! Complex combo batch 416 — 20 probes into display metrics, GUI
//! stubs, selection/font/image queries, frame parameters, and system
//! interface functions that are likely stubbed or simplified.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// display-pixel-width / display-pixel-height: screen dimensions.
#[test]
fn div_cx416_display_pixel_dimensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 25 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (display-pixel-width)
      (display-pixel-height)
      (display-mm-width)
      (display-mm-height))
"##,
        expect,
    );
}

/// display-screens / display-backing-store: display capabilities.
#[test]
fn div_cx416_display_screens() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 not-useful not-useful static-gray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (display-screens)
      (display-backing-store)
      (display-save-under)
      (display-visual-class))
"##,
        expect,
    );
}

/// x-display-color-p / x-display-grayscale-p: color capability.
#[test]
fn div_cx416_x_display_color() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"X windows are not in use or not initialized\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (x-display-color-p)
      (x-display-grayscale-p)
      (display-color-p))
"##,
        expect,
    );
}

/// display-monitor-attributes-list: monitor info.
#[test]
fn div_cx416_display_monitor_attrs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (length (display-monitor-attributes-list))
  (error (car e)))
"##,
        expect,
    );
}

/// x-server-vendor / x-server-version: X server info.
#[test]
fn div_cx416_x_server_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (error error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (x-server-vendor) (error (car e)))
      (condition-case e (x-server-version) (error (car e))))
"##,
        expect,
    );
}

/// x-list-fonts: enumerating available fonts.
#[test]
fn div_cx416_x_list_fonts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (error error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e
          (length (x-list-fonts "monospace"))
        (error (car e)))
      (condition-case e
          (x-list-fonts "*")
        (error (car e))))
"##,
        expect,
    );
}

/// image-type-available-p: checking image format support.
#[test]
fn div_cx416_image_type_available() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (image-type-available-p 'png)
      (image-type-available-p 'jpeg)
      (image-type-available-p 'gif)
      (image-type-available-p 'svg))
"##,
        expect,
    );
}

/// image-size / image-mask-p: image properties.
#[test]
fn div_cx416_image_size_mask() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (wrong-type-argument error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e
          (image-size (list 'image :type 'png :data "")
                      nil t)
        (error (car e)))
      (condition-case e
          (image-mask-p (list 'image :type 'png :data ""))
        (error (car e))))
"##,
        expect,
    );
}

/// gui-get-selection / gui-set-selection: selection access.
#[test]
fn div_cx416_gui_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (gui-get-selection 'PRIMARY) (error (car e)))
      (condition-case e (gui-set-selection 'PRIMARY "test") (error (car e))))
"##,
        expect,
    );
}

/// selection-coding-system: encoding for selections.
#[test]
fn div_cx416_selection_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function next-selection-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (next-selection-coding-system)
      (set-selection-coding-system 'utf-8)
      (condition-case e (x-selection-exists-p 'PRIMARY) (error (car e))))
"##,
        expect,
    );
}

/// x-parse-geometry: parsing X geometry strings.
#[test]
fn div_cx416_x_parse_geometry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((height . 24) (width . 80)) ((height . 600) (width . 800) (top . 20) (left . 10)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (x-parse-geometry "80x24")
      (x-parse-geometry "800x600+10+20"))
"##,
        expect,
    );
}

/// x-get-resource: X resources database.
#[test]
fn div_cx416_x_get_resource() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (error error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (x-get-resource "emacs*Background" "Background") (error (car e)))
      (condition-case e (x-get-resource "nonexistent" "Nonexistent") (error (car e))))
"##,
        expect,
    );
}

/// tool-bar-mode / menu-bar-mode: UI chrome modes.
#[test]
fn div_cx416_tool_menu_bar_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (tool-bar-mode -1) (error (car e)))
      (condition-case e (menu-bar-mode -1) (error (car e))))
"##,
        expect,
    );
}

/// frame-parameter deeper queries.
#[test]
fn div_cx416_frame_parameter_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (mono dark nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (selected-frame)))
  (list (frame-parameter f 'display-type)
        (frame-parameter f 'background-mode)
        (frame-parameter f 'cursor-type)
        (frame-parameter f 'title)))
"##,
        expect,
    );
}

/// frame-list / frame-live-p / frame-selected-window.
#[test]
fn div_cx416_frame_list_live() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (selected-frame)))
  (list (length (frame-list))
        (frame-live-p f)
        (windowp (frame-selected-window f))))
"##,
        expect,
    );
}

/// terminal-name / terminal-live-p.
#[test]
fn div_cx416_terminal_name_live() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"initial_terminal\" t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((term (frame-terminal (selected-frame))))
  (list (terminal-name term)
        (terminal-live-p term)
        (terminal-live-p (get-buffer-create "*test*"))))
"##,
        expect,
    );
}

/// device-class: terminal device classification.
#[test]
fn div_cx416_device_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((term (frame-terminal (selected-frame))))
  (device-class term))
"##,
        expect,
    );
}

/// color-rgb-to-hex / color-name-to-rgb deeper.
#[test]
fn div_cx416_color_rgb_to_hex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#ffff00000000\" \"#0000ffff0000\" \"#00000000ffff\" (1.0 0.0 0.0) (1.0 1.0 1.0))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (color-rgb-to-hex 1.0 0 0)
      (color-rgb-to-hex 0 1.0 0)
      (color-rgb-to-hex 0 0 1.0)
      (color-name-to-rgb "red")
      (color-name-to-rgb "alice blue"))
"##,
        expect,
    );
}

/// color-srgb-to-xyz / color-xyz-to-srgb: color space conversion.
#[test]
fn div_cx416_color_space_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((0.4124564 0.21266729 0.0193339) (0.7991526701664677 0.7180676680115904 0.7044985157025838))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'color)
  (list (condition-case e (color-srgb-to-xyz 1.0 0 0) (error (car e)))
        (condition-case e (color-xyz-to-srgb 0.5 0.5 0.5) (error (car e)))))
"##,
        expect,
    );
}

/// tty-display-color-p / tty-display-color-cells.
#[test]
fn div_cx416_tty_display_color() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (tty-display-color-p)
      (tty-display-color-cells))
"##,
        expect,
    );
}
