use super::*;

#[test]
fn clearing_fullscreen_requests_native_window_restoration() {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*fullscreen*");
    let frame_id = eval.frames.create_frame("F1", 745, 688, buffer);
    eval.frames
        .get_mut(frame_id)
        .unwrap()
        .set_window_system(Some(Value::symbol("x")));
    let host = RecordingDisplayHost::new();
    let changes = host.fullscreen_changes.clone();
    eval.set_display_host(Box::new(host));
    eval.eval_str("(modify-frame-parameters nil '((fullscreen . fullboth)))")
        .unwrap();
    eval.eval_str("(modify-frame-parameters nil '((fullscreen . nil)))")
        .unwrap();
    assert_eq!(
        changes.borrow().len(),
        2,
        "clearing fullscreen must reach the native host too"
    );
    assert_eq!(
        &*changes.borrow(),
        &[
            (frame_id, FrameFullscreen::Fullboth),
            (frame_id, FrameFullscreen::Windowed)
        ]
    );
    assert!(
        eval.eval_str("(frame-parameter nil 'fullscreen)")
            .unwrap()
            .is_nil()
    );
}

#[test]
fn width_only_gui_resize_keeps_exact_native_height_with_chrome_and_partial_row() {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*resize*");
    let frame_id = eval.frames.create_frame("F1", 745, 688, buffer);
    let frame = eval.frames.get_mut(frame_id).unwrap();
    frame.set_window_system(Some(Value::symbol("x")));
    frame.install_gnu_gui_default_parameters();
    frame.char_width = 9.0;
    frame.char_height = 18.0;
    frame.menu_bar_height = 18;
    frame.tool_bar_height = 41;
    frame.displays_chrome = true;
    frame.resize_pixelwise(745, 688);
    let host = RecordingDisplayHost::new();
    let requests = host.resized.clone();
    eval.set_display_host(Box::new(host));
    eval.eval_str("(set-frame-width nil 91)").unwrap();
    let request = requests.borrow().last().unwrap().clone();
    // GNU frame.c:Fset_frame_width passes FRAME_TEXT_HEIGHT unchanged,
    // preserving both chrome and any fractional character-row remainder.
    assert_eq!((request.width, request.height), (844, 688));
    eval.apply_resize_input_event(request.width, request.height, 1.0, frame_id.0, false);
    assert_eq!(
        eval.eval_str("(frame-text-cols)").unwrap().as_int(),
        Some(91)
    );
    assert_eq!(
        eval.eval_str("(frame-native-height)").unwrap().as_int(),
        Some(688)
    );
}

#[test]
fn rejected_and_duplicate_native_resize_completions_preserve_actual_geometry() {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*resize*");
    let frame_id = eval.frames.create_frame("F1", 745, 688, buffer);
    let frame = eval.frames.get_mut(frame_id).unwrap();
    frame.set_window_system(Some(Value::symbol("x")));
    frame.install_gnu_gui_default_parameters();
    frame.char_width = 9.0;
    frame.char_height = 18.0;
    frame.menu_bar_height = 18;
    frame.tool_bar_height = 41;
    frame.displays_chrome = true;
    frame.resize_pixelwise(745, 688);
    let host = RecordingDisplayHost::new();
    let requests = host.resized.clone();
    eval.set_display_host(Box::new(host));
    let original = eval.eval_str("(list (frame-native-width) (frame-native-height) (frame-text-cols) (frame-text-lines))").unwrap();

    eval.eval_str("(set-frame-width nil 91)").unwrap();
    assert_eq!(requests.borrow().last().unwrap().width, 844);
    // A compositor can refuse the request and return the old size immediately;
    // a later configure event may repeat that same completion.
    for _ in 0..2 {
        eval.apply_resize_input_event(745, 688, 2.0, frame_id.0, false);
        let actual = eval.eval_str("(list (frame-native-width) (frame-native-height) (frame-text-cols) (frame-text-lines))").unwrap();
        assert!(crate::emacs_core::value::equal_value(&actual, &original, 0));
    }
    eval.eval_str("(set-frame-width nil 101)").unwrap();
    let request = requests.borrow().last().unwrap().clone();
    assert_eq!((request.width, request.height), (934, 688));
}
