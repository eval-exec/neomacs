//! Complex combo batch 429 — 18 probes into remaining window/display/
//! mouse/event/posn areas: event-start/end/posn-*, posn-at-point,
//! coordinates-in-window-p, mouse-position, window-body-edges,
//! window-inside-absolute-pixel-edges, window-scroll-bars deep,
//! window-fringes deep, window-font-width/height, window-total-width/height,
//! window-total-size, window-use-time, window-old-buffer,
//! truncate-partial-width-windows, and charset-priority-list.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// event-start / event-end / posn-* event position accessors.
#[test]
fn div_cx429_event_posn_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((key . 97) (key . 97))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
      (list (event-start '(key-press (key . ?a))) (event-end '(key-press (key . ?a))))
    (error (car e)))
"##,
        expect,
    );
}

/// posn-at-point / posn-at-x-y: position to screen coordinates.
#[test]
fn div_cx429_posn_at_point_xy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil (#<window 1 on *scratch*> 1 (0 . 0) 0 nil 1 (0 . 0) nil (0 . 0) (0 . 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world\nsecond line")
  (list (condition-case e (posn-at-point 3) (error (car e)))
        (condition-case e (posn-at-x-y 0 0) (error (car e)))))
"##,
        expect,
    );
}

/// coordinates-in-window-p: checking if coordinates are in window.
#[test]
fn div_cx429_coordinates_in_window_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 . 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (condition-case e
      (coordinates-in-window-p '(0 . 0) w)
    (error (car e))))
"##,
        expect,
    );
}

/// mouse-position / mouse-pixel-position (may be stubbed in batch).
#[test]
fn div_cx429_mouse_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (framep (car (mouse-position))) (error (car e)))
      (condition-case e (framep (car (mouse-pixel-position))) (error (car e))))
"##,
        expect,
    );
}

/// window-body-edges: edges excluding header/mode line.
#[test]
fn div_cx429_window_body_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 80 23)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (window-body-edges w))
"##,
        expect,
    );
}

/// window-inside-absolute-pixel-edges: pixel-based coordinates.
#[test]
fn div_cx429_window_inside_absolute_pixel_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (window-inside-absolute-pixel-edges w))
"##,
        expect,
    );
}

/// window-scroll-bars: scroll bar position and width.
#[test]
fn div_cx429_window_scroll_bars_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 0 t nil 0 t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (window-scroll-bars w))
"##,
        expect,
    );
}

/// window-fringes: fringe width and offset.
#[test]
fn div_cx429_window_fringes_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (window-fringes w))
"##,
        expect,
    );
}

/// window-font-width / window-font-height: average char dimensions.
#[test]
fn div_cx429_window_font_width_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (list (window-font-width w)
        (window-font-height w)))
"##,
        expect,
    );
}

/// window-total-width / window-total-height: total window size.
#[test]
fn div_cx429_window_total_width_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (window-total-width)
      (window-total-height))
"##,
        expect,
    );
}

/// window-total-size: total size in a given dimension.
#[test]
fn div_cx429_window_total_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 80""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (window-total-size w t))
"##,
        expect,
    );
}

/// window-use-time: timestamp of last window use.
#[test]
fn div_cx429_window_use_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (window-use-time w))
"##,
        expect,
    );
}

/// window-old-buffer: buffer previously displayed in window.
#[test]
fn div_cx429_window_old_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
      (window-old-buffer (selected-window))
    (error (car e)))
"##,
        expect,
    );
}

/// truncate-partial-width-windows: truncation behavior.
#[test]
fn div_cx429_truncate_partial_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function truncate-partial-width-windows)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (truncate-partial-width-windows)
      (truncate-partial-width-windows nil))
"##,
        expect,
    );
}

/// charset-priority-list / set-charset-priority.
#[test]
fn div_cx429_charset_priority_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (179 unicode-bmp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (length (charset-priority-list))
      (car (charset-priority-list)))
"##,
        expect,
    );
}

/// coding-system-priority-list / set-coding-system-priority.
#[test]
fn div_cx429_coding_system_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (20 utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (length (coding-system-priority-list))
      (car (coding-system-priority-list)))
"##,
        expect,
    );
}

/// face-name / face-id on newly created faces.
#[test]
fn div_cx429_face_name_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"neo-cx429-fnid\" 186)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-face 'neo-cx429-fnid)))
  (list (face-name f) (face-id f)))
"##,
        expect,
    );
}

/// read-event / read-key-sequence in batch (should error).
#[test]
fn div_cx429_read_key_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-timeout (0.01) (read-key-sequence "test: "))
  (error (car e)))
"##,
        expect,
    );
}
