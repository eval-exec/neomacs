use super::*;

#[test]
fn resolved_scroll_observer_sees_complete_commit_and_current_command_only() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("preview");
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("one\ntwo\nthree\n");
    let frame = eval
        .frame_manager_mut()
        .create_frame("preview", 800, 600, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = calls.clone();
    eval.scroll_preview_fn = Some(Box::new(move |eval, frame, window, inputs| {
        let state = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .find_window(window)
            .unwrap()
            .redisplay_state()
            .unwrap();
        assert_eq!(state.window_start.as_i64(), 5);
        assert_eq!(state.point.as_i64(), 5);
        assert_eq!(state.vscroll, -4);
        assert_eq!(inputs.len(), 1);
        observed.set(observed.get() + 1);
    }));
    let update = || crate::window::WindowScrollUpdate {
        frame,
        window,
        buffer,
        start: crate::buffer::LispCharPos1::new(5),
        point: crate::buffer::LispCharPos1::new(5),
        hidden_top_pixels: 4,
    };
    update().commit(&mut eval).unwrap();
    assert_eq!(calls.get(), 0);
    let stream = neomacs_display_protocol::input_progress::InputStream::default();
    let command = eval.input_progress.begin_command();
    eval.input_progress.consumed(stream.issue().unwrap());
    update().commit(&mut eval).unwrap();
    assert_eq!(calls.get(), 1);
    drop(command);
    update().commit(&mut eval).unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn compositor_pixel_scroll_policy_requires_definition_and_binding_witnesses() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("scroll-policy");
    let frame = eval
        .frame_manager_mut()
        .create_frame("scroll-policy", 800, 600, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    assert!(!eval.permits_compositor_pixel_scroll(window));
    eval.eval_str("(use-global-map (make-sparse-keymap)) (setq neomacs-compositor-scrolling t pixel-scroll-precision-mode t)").unwrap();
    for name in [
        "pixel-scroll-precision",
        "pixel-scroll-precision-scroll-down",
        "pixel-scroll-precision-scroll-down-page",
        "pixel-scroll-precision-scroll-up",
        "pixel-scroll-precision-scroll-up-page",
        "pixel-scroll-precision-interpolate",
    ] {
        eval.eval_str(&format!("(fset '{name} (lambda () nil)) (put '{name} 'neomacs--scroll-definition (symbol-function '{name}))")).unwrap();
    }
    eval.eval_str("(define-key (current-global-map) [wheel-up] 'pixel-scroll-precision) (define-key (current-global-map) [wheel-down] 'pixel-scroll-precision)").unwrap();
    assert!(eval.permits_compositor_pixel_scroll(window));
    let other_buffer = eval
        .buffer_manager_mut()
        .create_buffer("other-scroll-policy");
    let other_window = eval
        .frame_manager_mut()
        .split_window(
            frame,
            window,
            crate::window::SplitDirection::Horizontal,
            other_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .unwrap();
    assert!(eval.compositor_scrolling_enabled(other_window));
    assert!(!eval.permits_compositor_pixel_scroll(other_window));
    eval.buffer_manager_mut()
        .get_mut(other_buffer)
        .unwrap()
        .set_buffer_local("neomacs-compositor-scrolling", Value::NIL);
    assert!(!eval.compositor_scrolling_enabled(other_window));
    assert!(eval.compositor_scrolling_enabled(window));
    let publications = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = publications.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        observed
            .borrow_mut()
            .push(eval.permits_compositor_pixel_scroll(window));
        crate::test_utils::mock_redisplay::accept_all_frames(eval);
    }));
    eval.redisplay().expect("redisplay");
    eval.redisplay().expect("redisplay");
    assert_eq!(&*publications.borrow(), &[true]);
    eval.eval_str("(setq neomacs-compositor-scrolling nil)")
        .unwrap();
    eval.redisplay().expect("redisplay");
    assert_eq!(
        &*publications.borrow(),
        &[true, false],
        "policy-only changes must publish revocation"
    );
    eval.eval_str("(setq neomacs-compositor-scrolling t)")
        .unwrap();
    eval.redisplay().expect("redisplay");
    assert_eq!(&*publications.borrow(), &[true, false, true]);
    eval.eval_str("(define-key (current-global-map) [wheel-down] '(menu-item \"scroll\" pixel-scroll-precision :filter ignore))").unwrap();
    assert!(
        !eval.permits_compositor_pixel_scroll(window),
        "menu filters require actual dispatch"
    );
    eval.eval_str("(define-key (current-global-map) [wheel-down] 'ignore)")
        .unwrap();
    assert!(!eval.permits_compositor_pixel_scroll(window));
    eval.eval_str("(define-key (current-global-map) [wheel-down] 'pixel-scroll-precision) (setq pre-command-hook '(ignore))").unwrap();
    assert!(!eval.permits_compositor_pixel_scroll(window));
    eval.eval_str("(setq pre-command-hook nil)").unwrap();
    let local = |eval: &mut Context, name: &str, value| {
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .set_buffer_local(name, value);
    };
    local(
        &mut eval,
        "pre-command-hook",
        Value::list(vec![Value::symbol("ignore")]),
    );
    assert!(
        !eval.permits_compositor_pixel_scroll(window),
        "displayed buffer hooks veto prediction"
    );
    local(&mut eval, "pre-command-hook", Value::NIL);
    eval.buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local(
            "pre-command-hook",
            Value::list(vec![Value::symbol("ignore")]),
        );
    assert!(
        eval.permits_compositor_pixel_scroll(window),
        "timer buffer hooks do not belong to the displayed buffer"
    );
    eval.eval_str("(fset 'eldoc-pre-command-refresh-echo-area (lambda () nil)) (put 'eldoc-pre-command-refresh-echo-area 'neomacs--scroll-definition (symbol-function 'eldoc-pre-command-refresh-echo-area))").unwrap();
    local(
        &mut eval,
        "pre-command-hook",
        Value::list(vec![
            Value::symbol("eldoc-pre-command-refresh-echo-area"),
            Value::T,
        ]),
    );
    assert!(eval.permits_compositor_pixel_scroll(window));
    local(
        &mut eval,
        "eldoc-last-message",
        Value::string("documentation"),
    );
    assert!(!eval.permits_compositor_pixel_scroll(window));
    local(&mut eval, "eldoc-last-message", Value::NIL);
    eval.eval_str("(fset 'eldoc-pre-command-refresh-echo-area (lambda () t))")
        .unwrap();
    assert!(!eval.permits_compositor_pixel_scroll(window));
    local(&mut eval, "pre-command-hook", Value::NIL);
    eval.eval_str("(setq pre-command-hook nil) (fset 'pixel-scroll-precision (lambda () t))")
        .unwrap();
    assert!(!eval.permits_compositor_pixel_scroll(window));
}
