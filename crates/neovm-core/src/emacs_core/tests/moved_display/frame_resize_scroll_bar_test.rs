//! Re-homed from display::window_cmds::tests::frame_resize_test (P1 B1).
//!
//! The two tests are blocked in display's cfg(test) unit runtime by the
//! core-owned `frame-scroll-bar-width` subr, which the absent-layer runtime
//! cannot supply. Core's boot facade has every layer real. Bodies are the
//! display originals; the constructor line is the signed API drift
//! (`crate::test_utils::new_context()` for `Context::new(unit_runtime())`).

use crate::window::DisplayFrameManagerExt;
use neovm_value::value::Value;

#[test]
fn frame_scroll_bar_width_reports_only_the_allocated_scroll_bar_area() {
    let mut eval = crate::test_utils::new_context();
    let buffer = eval.buffer_manager_mut().create_buffer("*scroll-bar-area*");
    let fid = eval
        .frame_manager_mut()
        .create_frame("scroll-bar-area", 640, 480, buffer);
    eval.frame_manager_mut()
        .get_mut(fid)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    let result = eval
        .eval_str(
            r#"(progn
      (modify-frame-parameters nil '((vertical-scroll-bars . right) (scroll-bar-width . 17)))
      (let ((shown (frame-scroll-bar-width)))
        (modify-frame-parameters nil '((vertical-scroll-bars . nil)))
        (list shown (frame-scroll-bar-width))))"#,
        )
        .unwrap();
    assert_eq!(result.to_string(), "(17 0)");
}

#[test]
fn gui_frames_take_gnu_toolkit_scroll_bar_defaults_and_report_realized_parameters() {
    let mut eval = crate::test_utils::new_context();
    let buffer = eval
        .buffer_manager_mut()
        .create_buffer("*scroll-bar-defaults*");
    let fid = eval
        .frame_manager_mut()
        .create_frame("scroll-bar-defaults", 640, 480, buffer);
    let frame = eval.frame_manager_mut().get_mut(fid).unwrap();
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.char_width = 8.0;
    frame.char_height = 17.0;
    // GNU sizes a GUI frame's bars from the toolkit default, not the
    // character cell: x_set_scroll_bar_default_width/height
    // (src/xfns.c:2393-2438) store the constant 16 without a toolkit (and the
    // Adwaita theme's 16 under GTK).  A fresh frame therefore reserves 16
    // pixels (COLUMNS = ceil(16/8) = 2, src/frame.h:1335) and reports the
    // configured thickness from `frame-parameter' even though the frame never
    // named one (src/frame.c:5437-5456), while `frame-scroll-bar-height'
    // answers zero because horizontal bars are off (src/frame.c:4496-4501).
    let result = eval
        .eval_str(
            r#"(list (frame-scroll-bar-width) (frame-scroll-bar-height)
                (window-scroll-bars)
                (frame-parameter nil 'scroll-bar-width)
                (frame-parameter nil 'scroll-bar-height)
                (frame-parameter nil 'vertical-scroll-bars)
                (frame-parameter nil 'horizontal-scroll-bars))"#,
        )
        .unwrap();
    assert_eq!(
        result.to_string(),
        "(16 0 (nil 2 t nil 0 t nil) 16 16 right nil)"
    );

    // Enabling horizontal bars normalizes the frame parameter to t
    // (src/frame.c:4191-4198) and exposes the configured 16-pixel area height.
    let result = eval
        .eval_str(
            r#"(progn
      (modify-frame-parameters nil '((horizontal-scroll-bars . bottom)))
      (list (frame-scroll-bar-height)
            (frame-parameter nil 'horizontal-scroll-bars)
            (window-scroll-bars)))"#,
        )
        .unwrap();
    assert_eq!(result.to_string(), "(16 t (nil 2 t nil 1 t nil))");

    // An explicit height is honored by both the accessor and the parameter.
    let result = eval
        .eval_str(
            r#"(progn
      (modify-frame-parameters nil '((scroll-bar-height . 7)))
      (list (frame-scroll-bar-height)
            (frame-parameter nil 'scroll-bar-height)))"#,
        )
        .unwrap();
    assert_eq!(result.to_string(), "(7 7)");
}
