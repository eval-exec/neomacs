//! Complex combo batch 432 — 17 probes into X11/GUI backend stubs,
//! selection, cut buffer, font selection, frame focus, display
//! connection, and remaining pixel/frame edge operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// x-create-frame / x-focus-frame: frame creation and focus.
#[test]
fn div_cx432_x_create_focus_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (error ((height . 24) (width . 80) (top . 0) (left . 0)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (x-focus-frame (selected-frame)) (error (car e)))
      (condition-case e (x-parse-geometry "80x24+0+0") (error (car e))))
"##,
        expect,
    );
}

/// x-dnd-*: drag and drop functions (likely stubbed).
#[test]
fn div_cx432_x_dnd_protocol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (x-dnd-get-drop-x-y) (error (car e))))
"##,
        expect,
    );
}

/// x-own-selection-internal / x-get-selection-internal.
#[test]
fn div_cx432_x_own_get_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (error wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (x-own-selection-internal 'PRIMARY "test") (error (car e)))
      (condition-case e (x-get-selection-internal 'PRIMARY) (error (car e))))
"##,
        expect,
    );
}

/// x-set-cut-buffer / x-get-cut-buffer (X11 cut buffers).
#[test]
fn div_cx432_x_cut_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (void-function void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (x-set-cut-buffer "cut test") (error (car e)))
      (condition-case e (x-get-cut-buffer) (error (car e))))
"##,
        expect,
    );
}

/// x-display-list / x-open-connection / x-close-connection.
#[test]
fn div_cx432_x_display_connect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (x-display-list) (error (car e))))
"##,
        expect,
    );
}

/// x-window-property / x-change-window-property.
#[test]
fn div_cx432_x_window_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (error error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (x-window-property "WM_NAME") (error (car e)))
      (condition-case e (x-change-window-property "TEST" "data") (error (car e))))
"##,
        expect,
    );
}

/// x-get-atom-name / x-intern-atom.
#[test]
fn div_cx432_x_atom_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (void-function error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (x-intern-atom "WM_PROTOCOLS") (error (car e)))
      (condition-case e (x-get-atom-name 1) (error (car e))))
"##,
        expect,
    );
}

/// x-select-font / x-list-fonts deep.
#[test]
fn div_cx432_x_select_list_fonts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (error error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (length (x-list-fonts "monospace")) (error (car e)))
      (condition-case e (x-list-fonts "*") (error (car e))))
"##,
        expect,
    );
}

/// gui-backend-* selection functions.
#[test]
fn div_cx432_gui_backend_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (wrong-number-of-arguments nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (gui-backend-get-selection 'PRIMARY) (error (car e)))
      (condition-case e (gui-backend-selection-owner-p 'PRIMARY) (error (car e)))
      (condition-case e (gui-backend-selection-exists-p 'PRIMARY) (error (car e))))
"##,
        expect,
    );
}

/// face-attribute with multiple resolution frames.
#[test]
fn div_cx432_face_attribute_multi_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (face-attribute 'bold :weight nil 'default)
      (face-attribute 'italic :slant nil 'default)
      (face-attribute 'default :inherit nil 'default))
"##,
        expect,
    );
}

/// window-state-put with ignore-window-parameters.
#[test]
fn div_cx432_window_state_put_ignore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "test")
  (let ((state (window-state-get (selected-window))))
    (window-state-put state nil 'safe)))
"##,
        expect,
    );
}

/// display-pixel-dimensions / display-mm-dimensions (monitor).
#[test]
fn div_cx432_display_pixel_mm_monitor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 25 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (display-pixel-width) (display-pixel-height)
      (display-mm-width) (display-mm-height))
"##,
        expect,
    );
}

/// font-get with custom font property.
#[test]
fn div_cx432_font_get_custom_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (custom-value Monospace)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (font-spec :family "Monospace")))
  (font-put f :neo-cx432-prop 'custom-value)
  (list (font-get f :neo-cx432-prop)
        (font-get f :family)))
"##,
        expect,
    );
}

/// menu-bar-open / tooltip functions in batch.
#[test]
fn div_cx432_menu_tooltip_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (wrong-type-argument \"test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (menu-bar-open (selected-frame)) (error (car e)))
      (condition-case e (tooltip-show "test") (error (car e))))
"##,
        expect,
    );
}

/// x-send-client-message (X11 client messaging).
#[test]
fn div_cx432_x_send_client_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-number-of-arguments""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (x-send-client-message (selected-frame) (selected-frame) 0 nil "TEST")
  (error (car e)))
"##,
        expect,
    );
}
