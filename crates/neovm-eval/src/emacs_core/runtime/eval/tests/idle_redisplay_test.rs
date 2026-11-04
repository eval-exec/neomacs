//! P3.5 J: an idle redisplay skips layout, and `pre-redisplay-function` runs
//! on every redisplay that is not inhibited, as GNU's `prepare_menu_bars`
//! does (xdisp.c:14237-14262): with nil -- the selected window only -- when
//! nothing needs redisplay, with t otherwise.

use super::*;

/// A context with one frame showing a buffer of text, a counting
/// `redisplay_fn`, and a recording `pre-redisplay-function`.
/// This spy resets buffer revisions but never seals a GNU accepted frame.
/// Counter fixtures explicitly select the legacy contract; the marker-value
/// fixture keeps the ambient policy to exercise GNU ownership when enabled.
fn idle_context() -> (Context, std::rc::Rc<std::cell::Cell<usize>>) {
    let mut eval = Context::new();
    let buf_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.frames
        .create_frame("idle-redisplay", 80 * 8, 24 * 16, buf_id);
    let text: String = (0..50).map(|i| format!("line {i}\n")).collect();
    eval.buffers.get_mut(buf_id).expect("buffer").insert(&text);
    eval.eval_str(
        "(progn (goto-char 120)
                (defvar idle-prered-args nil)
                (setq pre-redisplay-function
                      (lambda (windows)
                        (setq idle-prered-args (cons windows idle-prered-args)))))",
    )
    .expect("setup");
    let layouts = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let counter = layouts.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        counter.set(counter.get() + 1);
        // A layout acknowledges the buffers it displayed (the engine's
        // `reset_unchanged_region`, GNU's `mark_window_display_accurate`).
        acknowledge_current_buffer(eval);
    }));
    (eval, layouts)
}

fn acknowledge_current_buffer(eval: &Context) {
    if let Some(buffer) = eval.buffers.current_buffer() {
        buffer.reset_unchanged_region();
    }
}

fn prered_args(eval: &mut Context) -> String {
    let value = eval.eval_str("(reverse idle-prered-args)").expect("args");
    crate::emacs_core::print::print_value(&value)
}

#[test]
fn an_idle_redisplay_skips_layout_but_runs_pre_redisplay_function() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, layouts) = idle_context();
    for _ in 0..3 {
        eval.eval_str("(redisplay)").expect("redisplay");
    }
    assert_eq!(layouts.get(), 1, "the idle redisplays skip layout");
    assert_eq!(prered_args(&mut eval), "(t nil nil)");
    // Point moving is a change: t, and a layout.
    eval.eval_str("(progn (forward-char 1) (redisplay))")
        .expect("move");
    assert_eq!(layouts.get(), 2);
    assert_eq!(prered_args(&mut eval), "(t nil nil t)");
}

#[test]
fn a_forced_idle_redisplay_lays_out_unless_the_idle_skip_is_on() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(Some(false));
    let (mut eval, layouts) = idle_context();
    eval.eval_str("(redisplay t)").expect("first");
    eval.eval_str("(redisplay t)").expect("second");
    assert_eq!(layouts.get(), 2, "force lays out");
    crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(Some(true));
    eval.eval_str("(redisplay t)").expect("third");
    crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(None);
    assert_eq!(layouts.get(), 2, "the idle skip holds under force");
    assert_eq!(prered_args(&mut eval), "(t nil nil)");
}

#[test]
fn window_old_point_follows_its_marker_across_redisplays() {
    let (mut eval, _layouts) = idle_context();
    eval.eval_str("(redisplay)").expect("redisplay");
    let old_point = eval.eval_str("(window-old-point)").expect("old point");
    assert_eq!(
        old_point,
        Value::fixnum(120),
        "old point is point after redisplay"
    );
    // An insertion before it moves the marker, as GNU's `old_pointm` moves.
    eval.eval_str("(save-excursion (goto-char 1) (insert \"abc\"))")
        .expect("insert");
    eval.eval_str("(redisplay)").expect("redisplay");
    assert_eq!(
        eval.eval_str("(window-old-point)").expect("old point"),
        Value::fixnum(123)
    );
}

/// A change made after the layout acknowledged the buffer (here by the
/// layout's own caller, as a window change hook does) is part of the skip
/// signature taken when the redisplay ends, yet was never laid out. GNU's
/// next redisplay redisplays such a buffer, so a forced one must not skip.
#[test]
fn a_forced_idle_redisplay_lays_out_a_change_made_after_the_last_layout() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, layouts) = idle_context();
    let changed_after = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = changed_after.clone();
    let counter = layouts.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        counter.set(counter.get() + 1);
        acknowledge_current_buffer(eval);
        if !flag.replace(true) {
            eval.eval_str("(put-text-property 1 5 'face 'bold)")
                .expect("post-layout change");
        }
    }));
    crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(Some(true));
    eval.eval_str("(redisplay t)").expect("first");
    assert_eq!(layouts.get(), 1);
    eval.eval_str("(redisplay t)").expect("second");
    assert_eq!(
        layouts.get(),
        2,
        "the change after the first layout's acknowledgement is laid out"
    );
    eval.eval_str("(redisplay t)").expect("third");
    crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(None);
    assert_eq!(layouts.get(), 2, "now idle, the forced redisplay skips");
}

#[test]
fn category_symbol_writes_invalidate_idle_redisplay() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, layouts) = idle_context();
    eval.eval_str("(progn (put 'idle-category 'face '(:height 100)) (overlay-put (make-overlay 1 20) 'category 'idle-category) (redisplay))").unwrap();
    assert_eq!(layouts.get(), 1);
    eval.eval_str("(progn (put 'idle-category 'face '(:height 200)) (redisplay))")
        .unwrap();
    assert_eq!(layouts.get(), 2, "category face mutation must repaint");
    eval.eval_str("(redisplay)").unwrap();
    assert_eq!(layouts.get(), 2, "unchanged category must remain idle");
    eval.eval_str("(progn (put 'unrelated-wheel-event 'event-kind 'mouse-click) (redisplay))")
        .unwrap();
    assert_eq!(
        layouts.get(),
        2,
        "event metadata is not a layout dependency"
    );
}

/// Frame paint state is not part of the idle signature. A background-only
/// setter must therefore schedule layout even without point or input changes.
#[test]
fn background_alpha_repaints_an_idle_gui_frame() {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(None);
        }
    }
    let _reset = Reset;
    crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(Some(true));
    let (mut eval, layouts) = idle_context();
    let frame = eval.frames.selected_frame().expect("frame").id;
    eval.frames
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    eval.eval_str("(redisplay t)").unwrap();
    eval.eval_str("(redisplay t)").unwrap();
    assert_eq!(layouts.get(), 1, "the unchanged frame is positively idle");
    eval.eval_str("(condition-case nil (modify-frame-parameters nil '((alpha-background . bad))) (error nil))").unwrap();
    assert_eq!(eval.frames.get(frame).unwrap().background_alpha, 1.0);
    assert_eq!(
        eval.frames
            .get(frame)
            .unwrap()
            .parameter("alpha-background"),
        Some(Value::symbol("bad"))
    );
    eval.eval_str("(redisplay t)").unwrap();
    assert_eq!(
        layouts.get(),
        1,
        "a rejected value does not dirty accepted paint"
    );
    eval.eval_str("(modify-frame-parameters nil '((alpha-background . 50)))")
        .unwrap();
    assert_eq!(eval.frames.get(frame).unwrap().background_alpha, 0.5);
    eval.eval_str("(redisplay t)").unwrap();
    assert_eq!(
        layouts.get(),
        2,
        "accepted background-only update must repaint"
    );
    eval.eval_str("(redisplay t)").unwrap();
    assert_eq!(layouts.get(), 2, "no continuous forced repaint");
    eval.eval_str("(modify-frame-parameters nil '((alpha-background . nil)))")
        .unwrap();
    assert_eq!(eval.frames.get(frame).unwrap().background_alpha, 1.0);
    eval.eval_str("(redisplay t)").unwrap();
    assert_eq!(layouts.get(), 3, "nil restores opaque paint");
}

#[test]
fn background_alpha_repaints_an_unselected_idle_child() {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(None);
        }
    }
    let _reset = Reset;
    crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(Some(true));
    let (mut eval, layouts) = idle_context();
    let selected = eval.frames.selected_frame().expect("selected").id;
    let buffer = eval.buffers.current_buffer_id().unwrap();
    let child = eval.frames.create_frame("idle-child", 200, 160, buffer);
    for id in [selected, child] {
        eval.frames
            .get_mut(id)
            .unwrap()
            .set_window_system(Some(Value::symbol("neo")));
    }
    eval.frames.get_mut(child).unwrap().parent_frame = Value::make_frame(selected.0);
    eval.frames.select_frame(selected);
    eval.obarray_mut()
        .set_symbol_value("idle-alpha-child", Value::make_frame(child.0));
    eval.eval_str("(redisplay t)").unwrap();
    eval.eval_str("(redisplay t)").unwrap();
    assert_eq!(layouts.get(), 1);
    eval.eval_str("(modify-frame-parameters idle-alpha-child '((alpha-background . 25)))")
        .unwrap();
    assert_eq!(
        eval.frames.selected_frame().map(|frame| frame.id),
        Some(selected)
    );
    assert_eq!(eval.frames.get(child).unwrap().background_alpha, 0.25);
    eval.eval_str("(redisplay t)").unwrap();
    assert_eq!(layouts.get(), 2, "unselected child paint must reach layout");
    eval.eval_str("(redisplay t)").unwrap();
    assert_eq!(layouts.get(), 2);
}
