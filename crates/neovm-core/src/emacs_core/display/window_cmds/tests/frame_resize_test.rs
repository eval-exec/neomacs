use super::*;

#[test]
fn font_resize_propagates_nonlocal_exits_from_window_minimum_policy() {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*font-minimum-exit*");
    let fid = eval.frames.create_frame("minimum-exit", 800, 600, buffer);
    let frame = eval.frames.get_mut(fid).unwrap();
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.char_width = 8.0;
    frame.char_height = 16.0;
    let host = RecordingDisplayHost::with_resolved_frame_font(remapped_mono_font_metrics());
    let requests = host.resized.clone();
    eval.set_display_host(Box::new(host));
    let result = eval
        .eval_str(
            r#"(progn
          (fset 'frame-windows-min-size (lambda (&rest args) (throw 'minimum 'escaped)))
          (catch 'minimum
            (internal-set-lisp-face-attribute 'default :font "Remapped Mono-24" nil)
            'swallowed))"#,
        )
        .unwrap();
    assert_eq!(result, Value::symbol("escaped"));
    assert!(requests.borrow().is_empty());
}

#[test]
fn explicit_frame_minimum_overrides_inhibited_font_resize_on_only_that_axis() {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*font-minimum*");
    let fid = eval.frames.create_frame("minimum", 800, 600, buffer);
    let frame = eval.frames.get_mut(fid).unwrap();
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.char_width = 8.0;
    frame.char_height = 16.0;
    let host = RecordingDisplayHost::with_resolved_frame_font(remapped_mono_font_metrics());
    let requests = host.resized.clone();
    eval.set_display_host(Box::new(host));
    eval.eval_str(
        r#"(progn
       (modify-frame-parameters nil '((min-width . 120)))
       (setq frame-inhibit-implied-resize t)
       (internal-set-lisp-face-attribute 'default :font "Remapped Mono-24" nil))"#,
    )
    .unwrap();
    let requests = requests.borrow();
    let request = requests
        .last()
        .expect("minimum must trigger a native resize");
    assert_eq!(request.width, 120 * 16);
    assert_eq!(request.height, 600);
}

#[test]
fn native_resize_counts_the_entire_grown_minibuffer_in_frame_height() {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*grown-minibuffer*");
    let fid = eval.frames.create_frame("grown", 640, 384, buffer);
    let frame = eval.frames.get_mut(fid).unwrap();
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.char_width = 8.0;
    frame.char_height = 16.0;
    frame.resize_pixelwise(640, 384);
    frame.grow_mini_window_with_max_lines(2, 10.0);
    assert_eq!(
        frame.minibuffer_leaf.as_ref().unwrap().bounds().height,
        48.0
    );
    eval.set_display_host(Box::new(RecordingDisplayHost::new()));
    eval.apply_resize_input_event(720, 384, 1.0, fid.0, false);
    assert_eq!(
        eval.eval_str("(frame-text-lines)").unwrap().as_int(),
        Some(24)
    );
    assert_eq!(
        eval.eval_str("(frame-parameter nil 'height)")
            .unwrap()
            .as_int(),
        Some(24)
    );
    assert_eq!(
        eval.eval_str("(window-pixel-height (minibuffer-window))")
            .unwrap()
            .as_int(),
        Some(48)
    );
}

#[test]
fn child_initial_text_columns_include_explicit_fringe_and_border_chrome() {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*child-chrome*");
    let fid = eval.frames.create_frame("parent", 640, 480, buffer);
    let frame = eval.frames.get_mut(fid).unwrap();
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.char_width = 8.0;
    frame.char_height = 16.0;
    let host = RecordingDisplayHost::new();
    let realized = host.realized.clone();
    eval.set_display_host(Box::new(host));
    let result = eval
        .eval_str(
            r#"
      (let ((child (x-create-frame
          (list (cons 'parent-frame (selected-frame))
                '(width . 40) '(height . 10) '(minibuffer . nil)
                '(internal-border-width . 3) '(left-fringe . 7) '(right-fringe . 11)
                '(vertical-scroll-bars . nil) '(menu-bar-lines . 0)
                '(tool-bar-lines . 0) '(tab-bar-lines . 0)))))
        (list (frame-text-cols child) (frame-text-width child) (frame-native-width child)))
    "#,
        )
        .unwrap();
    assert_eq!(result.to_string(), "(40 320 344)");
    assert!(
        realized.borrow().is_empty(),
        "child must not create a top-level native window"
    );
}

#[test]
fn inhibited_font_change_preserves_an_unsent_explicit_resize() {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*font-resize*");
    let fid = eval.frames.create_frame("F1", 800, 600, buffer);
    let frame = eval.frames.get_mut(fid).unwrap();
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.char_width = 8.0;
    frame.char_height = 16.0;
    let host = RecordingDisplayHost::with_resolved_frame_font(remapped_mono_font_metrics());
    let requests = host.resized.clone();
    eval.set_display_host(Box::new(host));
    eval.eval_str("(internal-set-lisp-face-attribute 'default :font \"Remapped Mono-24\" nil)")
        .unwrap();
    assert_eq!(requests.borrow().len(), 1);
    eval.eval_str("(modify-frame-parameters nil '((width . 80) (height . 25)))")
        .unwrap();

    let mut host = RecordingDisplayHost::with_resolved_frame_font(resolved_frame_font(
        "Small Mono",
        "SmallMono-Regular",
        128,
        FontPxProbeResult {
            pixel_size: 17,
            height: 18,
            ascent: 14,
            descent: 4,
            max_width: 9,
            space_width: 9,
            average_width: 9,
        },
    ));
    host.resized = requests.clone();
    eval.set_display_host(Box::new(host));
    eval.eval_str("(setq frame-inhibit-implied-resize t)")
        .unwrap();
    eval.eval_str("(internal-set-lisp-face-attribute 'default :font \"Small Mono-13\" nil)")
        .unwrap();
    assert_eq!(
        requests.borrow().len(),
        1,
        "inhibition must not request another native resize"
    );

    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::Resize {
        width: 745,
        height: 450,
        scale_factor: 1.0,
        emacs_frame_id: fid.0,
    })
    .unwrap();
    eval.eval_str("(frame-native-width)").unwrap();
    let requests = requests.borrow();
    assert_eq!(
        requests.len(),
        2,
        "the explicit request must still reach the host"
    );
    assert_eq!((requests[1].width, requests[1].height), (745, 450));
}

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
