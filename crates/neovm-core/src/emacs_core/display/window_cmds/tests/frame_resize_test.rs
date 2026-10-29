use super::*;

#[test]
fn overlapping_native_observations_keep_the_latest_requested_grid_for_font_changes() {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*overlapping-resize*");
    let fid = eval.frames.create_frame("overlapping", 800, 600, buffer);
    let frame = eval.frames.get_mut(fid).unwrap();
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.char_width = 8.0;
    frame.char_height = 16.0;
    for (key, value) in [
        ("vertical-scroll-bars", Value::NIL),
        ("left-fringe", Value::fixnum(0)),
        ("right-fringe", Value::fixnum(0)),
    ] {
        frame.set_parameter(Value::symbol(key), value);
    }
    frame.resize_pixelwise(800, 600);
    let host = RecordingDisplayHost::with_resolved_frame_font(remapped_mono_font_metrics());
    let requests = host.resized.clone();
    eval.set_display_host(Box::new(host));
    eval.eval_str("(set-frame-width nil 91)").unwrap();
    let a = requests.borrow().last().unwrap().clone();
    eval.eval_str("(set-frame-width nil 101)").unwrap();
    let b = requests.borrow().last().unwrap().clone();
    let other = eval.frames.create_frame("other", 300, 200, buffer);
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    for event in [
        crate::keyboard::InputEvent::Focus {
            focused: true,
            emacs_frame_id: fid.0,
        },
        crate::keyboard::InputEvent::Resize {
            width: a.width,
            height: a.height,
            scale_factor: 2.0,
            emacs_frame_id: fid.0,
        },
        crate::keyboard::InputEvent::Resize {
            width: 320,
            height: 240,
            scale_factor: 1.0,
            emacs_frame_id: other.0,
        },
        crate::keyboard::InputEvent::key_press(crate::keyboard::KeyEvent::char('r')),
    ] {
        tx.send(event).unwrap();
    }
    assert_eq!(
        eval.eval_str("(frame-native-width)").unwrap().as_int(),
        Some(a.width as i64)
    );
    assert_eq!(
        (
            eval.frames.get(other).unwrap().width,
            eval.frames.get(other).unwrap().height
        ),
        (320, 240)
    );
    assert_eq!(eval.read_char().unwrap(), Value::fixnum('r' as i64));
    eval.eval_str("(modify-frame-parameters nil '((left . 10)))")
        .unwrap();
    eval.eval_str(r#"(internal-set-lisp-face-attribute 'default :font "Remapped Mono-24" nil)"#)
        .unwrap();
    let c = requests.borrow().last().unwrap().clone();
    assert_eq!(
        c.width,
        101 * 16,
        "the newest requested grid must survive an older allocation and a geometry query"
    );
    for observed in [&b, &b, &c] {
        eval.apply_resize_input_event(observed.width, observed.height, 1.0, fid.0, false)
            .expect("resize redisplay");
        assert_eq!(
            eval.eval_str("(frame-native-width)").unwrap().as_int(),
            Some(observed.width as i64)
        );
    }
    assert!(
        eval.frames.get(fid).unwrap().pending_gui_resize.is_none(),
        "attaining the target releases pending intent"
    );
    eval.apply_resize_input_event(84 * 16, c.height, 1.0, fid.0, false)
        .expect("resize redisplay");
    assert_eq!(
        eval.eval_str("(frame-text-cols)").unwrap().as_int(),
        Some(84),
        "a later unsolicited allocation remains authoritative"
    );
}

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
    eval.apply_resize_input_event(720, 384, 1.0, fid.0, false)
        .expect("resize redisplay");
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
fn frame_scroll_bar_width_reports_only_the_allocated_scroll_bar_area() {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*scroll-bar-area*");
    let fid = eval
        .frames
        .create_frame("scroll-bar-area", 640, 480, buffer);
    eval.frames
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
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*scroll-bar-defaults*");
    let fid = eval
        .frames
        .create_frame("scroll-bar-defaults", 640, 480, buffer);
    let frame = eval.frames.get_mut(fid).unwrap();
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

#[test]
fn moving_a_child_frame_marks_it_for_redisplay() {
    use crate::emacs_core::eval::RedisplayHookPolicyGuard;

    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*child-move*");
    let fid = eval.frames.create_frame("child-move", 640, 480, buffer);
    eval.frames
        .get_mut(fid)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingDisplayHost::new()));
    let child = eval
        .eval_str(
            r#"(setq test-child
                 (x-create-frame
                  (list (cons 'parent-frame (selected-frame))
                        '(width . 40) '(height . 10) '(minibuffer . nil)
                        '(left . 40) '(top . 40) '(undecorated . t)
                        '(vertical-scroll-bars . nil) '(menu-bar-lines . 0)
                        '(tool-bar-lines . 0) '(tab-bar-lines . 0))))"#,
        )
        .unwrap();
    let child_id = crate::window::FrameId(child.as_frame_id().expect("frame object"));
    // The placement only reaches a GUI compositor through the frame's next
    // published display state, and moving a frame changes neither the root
    // window nor the redisplay signature: the parameter setter must mark the
    // frame itself (GNU moves the native window in place through
    // set_frame_offset_hook, src/frame.c:5366-5369).
    eval.clear_frame_redisplay_marks();
    eval.eval_str("(modify-frame-parameters test-child '((left . 400) (top . 300)))")
        .unwrap();
    assert!(
        eval.frame_redisplay_pending(child_id),
        "a re-anchored child frame must be marked for redisplay"
    );
    // Re-applying the same placement is not a move and must not manufacture a
    // redisplay request.
    eval.clear_frame_redisplay_marks();
    eval.eval_str("(modify-frame-parameters test-child '((left . 400) (top . 300)))")
        .unwrap();
    assert!(!eval.frame_redisplay_pending(child_id));
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

    let tx = crate::host_input::HostInputPublisher::install(&mut eval);
    // 80 columns at 9px plus 16px of fringes and the GNU 16px scroll-bar
    // default: the explicit native resize is column-aligned, so it must reach
    // the host unchanged.
    tx.send(crate::keyboard::InputEvent::Resize {
        width: 752,
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
    assert_eq!((requests[1].width, requests[1].height), (752, 450));
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
    // 91 columns at 9px plus 16px of fringes and the GNU 16px scroll-bar
    // default (x_set_scroll_bar_default_width, src/xfns.c:2393-2403).
    assert_eq!((request.width, request.height), (851, 688));
    eval.apply_resize_input_event(request.width, request.height, 1.0, frame_id.0, false)
        .expect("resize redisplay");
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
    // 91 columns at 9px + 16px fringes + the GNU 16px scroll-bar default.
    assert_eq!(requests.borrow().last().unwrap().width, 851);
    // A compositor can refuse the request and return the old size immediately;
    // a later configure event may repeat that same completion.
    for _ in 0..2 {
        eval.apply_resize_input_event(745, 688, 2.0, frame_id.0, false)
            .expect("resize redisplay");
        let actual = eval.eval_str("(list (frame-native-width) (frame-native-height) (frame-text-cols) (frame-text-lines))").unwrap();
        assert!(crate::emacs_core::value::equal_value(&actual, &original, 0));
    }
    eval.eval_str("(set-frame-width nil 101)").unwrap();
    let request = requests.borrow().last().unwrap().clone();
    assert_eq!((request.width, request.height), (941, 688));
}
