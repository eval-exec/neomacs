//! Body-geometry tests that need the core upper layers.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use neovm_display::test_support::authority::window_cmds_eval_with_gui_frame_for_test as eval_with_gui_frame;

#[test]
fn body_queries_use_live_chrome_presence_before_first_redisplay() {
    let results = eval_with_gui_frame(
        crate::test_utils::new_context,
        r#"
      (fset 'reserved-rows '(lambda ()
        (let ((total (window-pixel-height)) (line (frame-char-height)))
          (list (/ (- total (window-body-height nil t)) line)
                (/ (- total (window-text-height nil t)) line)))))
      (setq mode-line-format nil header-line-format nil tab-line-format nil)
      (reserved-rows)
      (setq mode-line-format "MODE" header-line-format "HEADER" tab-line-format "TAB")
      (reserved-rows)
      (set-window-parameter nil 'mode-line-format 'none)
      (reserved-rows)
      (setq mode-line-format nil)
      (set-window-parameter nil 'mode-line-format "OVERRIDE")
      (reserved-rows)
    "#,
    );
    assert_eq!(results[2], "OK (0 0)", "absent chrome takes no rows");
    assert_eq!(
        results[4], "OK (3 3)",
        "mode, header and tab each reserve a row"
    );
    assert_eq!(
        results[6], "OK (2 2)",
        "none suppresses the buffer's mode line"
    );
    assert_eq!(
        results[9], "OK (3 3)",
        "window override enables absent buffer format"
    );
}
