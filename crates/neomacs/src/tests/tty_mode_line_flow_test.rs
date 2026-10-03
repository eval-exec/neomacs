//! A child mode-line throw preserves the TTY tree's active presentations.

use super::*;
use neovm_core::emacs_core::Value;

#[test]
fn tty_child_mode_line_throw_discards_staged_presentations_before_activation() {
    // nextest runs each test in a process; initialize the immutable knob
    // before any formatter reads it.
    unsafe { std::env::set_var("NEOVM_MODE_LINE_FLOW", "1") };
    neovm_core::logging::init_for_tests();
    let mut eval = Context::new();
    eval.eval_str(
        "(setq noninteractive nil inhibit-redisplay nil \
               mode-line-format nil header-line-format nil tab-line-format nil)",
    )
    .expect("display variables");
    let root_buffer = eval.buffer_manager_mut().create_buffer("tty-flow-root");
    let child_buffer = eval.buffer_manager_mut().create_buffer("tty-flow-child");
    for buffer_id in [root_buffer, child_buffer] {
        eval.buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("display buffer")
            .insert("body\n");
    }
    let root = eval
        .frame_manager_mut()
        .create_frame("tty-flow-root", 640, 384, root_buffer);
    let child = eval
        .frame_manager_mut()
        .create_frame("tty-flow-child", 320, 160, child_buffer);
    eval.frame_manager_mut()
        .get_mut(child)
        .expect("child")
        .parent_frame = Value::make_frame(root.0);
    REDISPLAY_RUNTIME.with(RedisplayRuntime::disable_cosmic_metrics);
    let (_, initial_children) = run_tty_layout_tree(&mut eval).expect("initial TTY tree");
    assert_eq!(
        initial_children.len(),
        1,
        "the child participates in layout"
    );
    let original_root = eval
        .frame_manager()
        .get(root)
        .expect("root")
        .active_presentation();
    let original_child = eval
        .frame_manager()
        .get(child)
        .expect("child")
        .active_presentation();
    assert!(original_root.is_some());
    assert!(original_child.is_some());
    // A live Lisp assignment invalidates retained display rows; the raw
    // Buffer setter only writes the slot and bypasses that boundary.
    eval.eval_str(
        "(let ((original (current-buffer))) \
           (unwind-protect \
               (progn \
                 (set-buffer \"tty-flow-child\") \
                 (setq mode-line-format '(:eval (throw 'tty-flow-exit \"escaped\"))) \
                 (force-mode-line-update t)) \
             (set-buffer original)))",
    )
    .expect("install and invalidate the throwing child mode line");
    eval.redisplay_fn = Some(Box::new(|eval| {
        assert!(
            run_tty_layout_tree(eval).is_none(),
            "a child exit aborts the tree"
        );
        assert!(eval.has_mode_line_display_flow());
    }));
    let caught = eval
        .eval_str("(catch 'tty-flow-exit (redisplay t))")
        .expect("child throw reaches the enclosing catch");
    assert_eq!(caught.as_utf8_str(), Some("escaped"));
    assert!(!eval.has_mode_line_display_flow());
    for (frame_id, previous) in [(root, original_root), (child, original_child)] {
        let frame = eval
            .frame_manager()
            .get(frame_id)
            .expect("frame after throw");
        assert_eq!(frame.active_presentation(), previous);
        assert!(!frame.has_prepared_display_presentations());
    }
}
