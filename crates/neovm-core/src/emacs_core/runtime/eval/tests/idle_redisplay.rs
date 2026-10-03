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
