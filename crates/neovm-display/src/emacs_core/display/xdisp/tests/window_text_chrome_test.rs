use super::*;

fn chrome_pixel_height(eval: &mut Context, window: i64, selector: Value) -> i64 {
    builtin_window_text_pixel_size_ctx(
        eval,
        vec![
            Value::make_window(window as u64),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            selector,
        ],
    )
    .expect("window-text-pixel-size")
    .cons_cdr()
    .as_fixnum()
    .expect("height")
}

fn chrome_context() -> (Context, i64) {
    let (mut eval, window) = pixel_size_tty_context();
    eval.eval_str("(setq mode-line-format nil header-line-format nil tab-line-format nil)")
        .expect("clear formats");
    let buffer = eval.edit.buffers.current_buffer().expect("buffer").id();
    eval.edit
        .buffers
        .get_mut(buffer)
        .expect("buffer")
        .insert("a");
    // Deliberately distinct accepted measurements: selecting one band must
    // not substitute a character-height estimate or another band's metric.
    eval.frame_manager_mut()
        .selected_frame_mut()
        .expect("frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: crate::window::WindowId(window as u64),
            tab_line_height: 3,
            header_line_height: 5,
            mode_line_height: 7,
            ..Default::default()
        }]);
    (eval, window)
}

#[test]
fn window_text_pixel_size_chrome_absent_formats_add_no_rows() {
    let (mut eval, window) = chrome_context();
    let body = chrome_pixel_height(&mut eval, window, Value::NIL);
    for selector in [
        Value::T,
        Value::symbol("mode-line"),
        Value::symbol("header-line"),
        Value::symbol("tab-line"),
        Value::symbol("not-a-line"),
    ] {
        assert_eq!(chrome_pixel_height(&mut eval, window, selector), body);
    }
}

#[test]
fn window_text_pixel_size_chrome_selects_distinct_measured_bands_and_all() {
    let (mut eval, window) = chrome_context();
    eval.eval_str(
        "(setq mode-line-format '(\"m\") header-line-format '(\"h\") tab-line-format '(\"t\"))",
    )
    .expect("enable formats");
    let body = chrome_pixel_height(&mut eval, window, Value::NIL);
    for (selector, addition) in [
        (Value::symbol("mode-line"), 7),
        (Value::symbol("header-line"), 5),
        (Value::symbol("tab-line"), 3),
        (Value::T, 15),
        (Value::symbol("not-a-line"), 0),
    ] {
        assert_eq!(
            chrome_pixel_height(&mut eval, window, selector),
            body + addition
        );
    }
}

#[test]
fn window_text_pixel_size_chrome_obeys_window_parameter_overrides() {
    let (mut eval, window) = chrome_context();
    let body = chrome_pixel_height(&mut eval, window, Value::NIL);
    for (name, height) in [
        ("mode-line-format", 7),
        ("header-line-format", 5),
        ("tab-line-format", 3),
    ] {
        eval.eval_str(&format!(
            "(set-window-parameter nil '{name} '(\"override\"))"
        ))
        .expect("parameter requests absent buffer band");
        assert_eq!(
            chrome_pixel_height(&mut eval, window, Value::T),
            body + height
        );
        eval.eval_str(&format!(
            "(setq {name} '(\"buffer\")) (set-window-parameter nil '{name} 'none)"
        ))
        .expect("none suppresses buffer band");
        assert_eq!(chrome_pixel_height(&mut eval, window, Value::T), body);
        eval.eval_str(&format!("(set-window-parameter nil '{name} nil)"))
            .expect("nil parameter falls back to buffer format");
        assert_eq!(
            chrome_pixel_height(&mut eval, window, Value::T),
            body + height
        );
        eval.eval_str(&format!("(setq {name} nil)"))
            .expect("reset buffer format");
    }
}

fn enabled_band_after_absent_publication(formats: &[&str], selector: Option<&str>) {
    let (mut eval, window) = chrome_context();
    eval.frame_manager_mut()
        .selected_frame_mut()
        .expect("frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: crate::window::WindowId(window as u64),
            regions_materialized: true,
            regions: crate::window::PresentedWindowRegions {
                outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 80.0, 24.0),
                text_body: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 80.0, 24.0),
                ..Default::default()
            },
            ..Default::default()
        }]);
    let body = chrome_pixel_height(&mut eval, window, Value::NIL);
    for format in formats {
        eval.eval_str(&format!("(setq {format} '(\"enabled\"))"))
            .expect("enable a previously absent band");
    }
    let frame = eval.frame_manager().selected_frame().expect("frame");
    let cached = frame
        .redisplay_snapshot(crate::window::WindowId(window as u64))
        .expect("completed publication remains installed");
    assert!(cached.regions_materialized);
    assert!(
        cached.regions.mode_line.is_none()
            && cached.regions.header_line.is_none()
            && cached.regions.tab_line.is_none(),
        "precondition: published absent chrome"
    );
    let estimated = frame.char_height.max(1.0) as i64;
    let selected = selector.map(Value::symbol).unwrap_or(Value::T);
    assert_eq!(
        chrome_pixel_height(&mut eval, window, selected),
        body + estimated * formats.len() as i64
    );
    // This guards measurement immediately after a live format change. It does
    // not assert that the public cached-height query must invalidate known0.
}

#[test]
fn window_text_pixel_size_chrome_published_absent_then_mode_enabled() {
    enabled_band_after_absent_publication(&["mode-line-format"], Some("mode-line"));
}

#[test]
fn window_text_pixel_size_chrome_published_absent_then_header_enabled() {
    enabled_band_after_absent_publication(&["header-line-format"], Some("header-line"));
}

#[test]
fn window_text_pixel_size_chrome_published_absent_then_tab_enabled() {
    enabled_band_after_absent_publication(&["tab-line-format"], Some("tab-line"));
}

#[test]
fn window_text_pixel_size_chrome_published_absent_then_all_enabled() {
    enabled_band_after_absent_publication(
        &["mode-line-format", "header-line-format", "tab-line-format"],
        None,
    );
}

#[test]
fn window_text_pixel_size_chrome_uses_selector_symbol_identity() {
    let (mut eval, window) = chrome_context();
    eval.eval_str(
        "(setq mode-line-format '(\"m\") header-line-format '(\"h\") tab-line-format '(\"t\"))",
    )
    .expect("enable all chrome");
    let body = chrome_pixel_height(&mut eval, window, Value::NIL);
    for name in ["t", "mode-line", "header-line", "tab-line"] {
        let selector = eval
            .eval_str(&format!("(make-symbol \"{name}\")"))
            .expect("distinct uninterned selector");
        assert_eq!(chrome_pixel_height(&mut eval, window, selector), body);
    }
}
