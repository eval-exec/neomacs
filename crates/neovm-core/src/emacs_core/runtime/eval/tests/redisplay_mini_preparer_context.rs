//! The installed frontend preparer must preserve GNU's mini-only callback
//! context. The exclusive Context owns each scope; this adds no runtime cache.
use super::*;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn selected_mini_only_frontend_preparer_keeps_caller_buffer_at_lisp_resize() {
    crate::test_utils::init_test_tracing();
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = Context::new();
    let source = eval.buffers.current_buffer_id().expect("source buffer");
    let frame = eval.frames.create_frame("mini-preparer", 80, 25, source);
    let window = {
        let frame = eval.frames.get_mut(frame).expect("mini-only frame");
        let window = frame.root_window().id();
        frame.minibuffer_window = Some(window);
        frame.minibuffer_leaf = None;
        frame.visibility = crate::window::FrameVisibility::Visible;
        window
    };
    let caller = eval.buffers.create_buffer("mini-preparer-caller");
    let after = eval.buffers.create_buffer("mini-preparer-after");
    assert!(!eval.minibuffer_is_active());
    assert!(!eval.has_current_message());
    eval.eval_str(
        "(progn (defvar mini-preparer-scope nil) \
         (set (make-local-variable 'mini-preparer-scope) 'source))",
    )
    .expect("source-local callback observation");
    eval.buffers.set_current(caller);
    eval.eval_str("(set (make-local-variable 'mini-preparer-scope) 'caller)")
        .expect("caller-local callback observation");
    eval.buffers.set_current(source);
    eval.eval_str(
        r#"(progn
           (setq mini-preparer-observation nil resize-mini-frames nil)
           (setq pre-redisplay-function
                 (lambda (_targets) (set-buffer "mini-preparer-caller")))
           (fset 'window--resize-mini-frame
                 (lambda (_frame)
                   (setq mini-preparer-observation
                         (list (buffer-name) mini-preparer-scope
                               resize-mini-frames inhibit-redisplay))
                   (set-buffer "mini-preparer-after"))))"#,
    )
    .expect("mini-only context callbacks");
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let entry_buffer = Rc::new(Cell::new(None));
    let entry_observed = entry_buffer.clone();
    eval.redisplay_prepare_fn = Some(Box::new(move |eval, request| {
        observed.set(observed.get() + 1);
        entry_observed.set(eval.buffers.current_buffer_id());
        assert_eq!(
            request.source,
            RedisplayMiniGeometrySource::ActiveMinibuffer
        );
        assert_eq!(
            (request.frame, request.window, request.buffer),
            (frame, window, source)
        );
        // The real layout producer takes this exact mini-only dispatch before
        // its ordinary row walk. It already has an explicit source buffer.
        eval.gnu_resize_prepared_mini_frame(request.frame)
    }));
    let paints = Rc::new(Cell::new(0));
    let observed = paints.clone();
    let paint_buffer = Rc::new(Cell::new(None));
    let paint_observed = paint_buffer.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        paint_observed.set(eval.buffers.current_buffer_id());
        observed.set(observed.get() + 1);
    }));
    eval.eval_str("(let ((resize-mini-frames t)) (redisplay t))")
        .expect("frontend mini-only preparation");
    assert_eq!(calls.get(), 1);
    assert_eq!(paints.get(), 1);
    let result = eval
        .obarray
        .symbol_value("mini-preparer-observation")
        .copied()
        .expect("callback observation");
    let cells = crate::emacs_core::value::list_to_vec(&result).expect("four callback observations");
    assert_eq!(cells.len(), 4);
    assert_eq!(cells[0].as_utf8_str(), Some("mini-preparer-caller"));
    assert_eq!(cells[1], Value::symbol("caller"));
    assert_eq!(&cells[2..], &[Value::T, Value::T]);
    assert_eq!(entry_buffer.get(), Some(caller));
    assert_eq!(paint_buffer.get(), Some(after));
    assert_eq!(eval.buffers.current_buffer_id(), Some(after));
    assert_eq!(eval.gnu_selected_window(), Some(window));
    assert_eq!(
        eval.frames
            .get(frame)
            .and_then(|frame| frame.find_window(window))
            .and_then(Window::buffer_id),
        Some(source)
    );
    assert!(eval.redisplay_prepare_fn.is_some());
    assert!(eval.redisplay_fn.is_some());
    assert!(!eval.gnu_redisplay_hooks.active.is_active());
    assert_eq!(
        eval.obarray.symbol_value("resize-mini-frames").copied(),
        Some(Value::NIL)
    );
}
