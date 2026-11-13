use super::eval_with_gui_frame;

#[test]
fn chrome_height_queries_agree_with_absent_pre_redisplay_chrome() {
    let results = eval_with_gui_frame(
        r#"
        (setq mode-line-format nil header-line-format nil tab-line-format nil)
        (list (window-mode-line-height) (window-header-line-height) (window-tab-line-height))
    "#,
    );
    assert_eq!(results[1], "OK (0 0 0)");
}
