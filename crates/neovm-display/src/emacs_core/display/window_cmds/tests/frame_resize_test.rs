use super::*;
use crate::window::{DisplayFrameExt, DisplayFrameManagerExt};

#[test]
fn font_resize_propagates_nonlocal_exits_from_window_minimum_policy() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.edit.buffers.create_buffer("*font-minimum-exit*");
    let fid = eval
        .frame_manager_mut()
        .create_frame("minimum-exit", 800, 600, buffer);
    let frame = eval.frame_manager_mut().get_mut(fid).unwrap();
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
fn native_resize_counts_the_entire_grown_minibuffer_in_frame_height() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.edit.buffers.create_buffer("*grown-minibuffer*");
    let fid = eval
        .frame_manager_mut()
        .create_frame("grown", 640, 384, buffer);
    let frame = eval.frame_manager_mut().get_mut(fid).unwrap();
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
fn moving_a_child_frame_marks_it_for_redisplay() {
    use neovm_eval::emacs_core::eval::RedisplayHookPolicyGuard;

    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.edit.buffers.create_buffer("*child-move*");
    let fid = eval
        .frame_manager_mut()
        .create_frame("child-move", 640, 480, buffer);
    eval.frame_manager_mut()
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
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.edit.buffers.create_buffer("*child-chrome*");
    let fid = eval
        .frame_manager_mut()
        .create_frame("parent", 640, 480, buffer);
    let frame = eval.frame_manager_mut().get_mut(fid).unwrap();
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
fn clearing_fullscreen_requests_native_window_restoration() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.edit.buffers.create_buffer("*fullscreen*");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("F1", 745, 688, buffer);
    eval.frame_manager_mut()
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
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.edit.buffers.create_buffer("*resize*");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("F1", 745, 688, buffer);
    let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
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
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.edit.buffers.create_buffer("*resize*");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("F1", 745, 688, buffer);
    let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
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
        assert!(neovm_value::value::equal_value(&actual, &original, 0));
    }
    eval.eval_str("(set-frame-width nil 101)").unwrap();
    let request = requests.borrow().last().unwrap().clone();
    assert_eq!((request.width, request.height), (941, 688));
}
