//! P3.5 J: an idle redisplay skips layout, and `pre-redisplay-function` runs
//! on every redisplay that is not inhibited, as GNU's `prepare_menu_bars`
//! does (xdisp.c:14237-14262): with nil -- the selected window only -- when
//! nothing needs redisplay, with t otherwise.

use super::*;

/// A context with one frame showing a buffer of text, a counting
/// `redisplay_fn`, and a recording `pre-redisplay-function`.
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
    eval.redisplay_fn = Some(Box::new(move |_eval| counter.set(counter.get() + 1)));
    (eval, layouts)
}

fn prered_args(eval: &mut Context) -> String {
    let value = eval.eval_str("(reverse idle-prered-args)").expect("args");
    crate::emacs_core::print::print_value(&value)
}

#[test]
fn an_idle_redisplay_skips_layout_but_runs_pre_redisplay_function() {
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
fn a_forced_idle_redisplay_still_lays_out() {
    let (mut eval, layouts) = idle_context();
    eval.eval_str("(redisplay t)").expect("first");
    eval.eval_str("(redisplay t)").expect("second");
    assert_eq!(layouts.get(), 2, "force lays out");
    assert_eq!(prered_args(&mut eval), "(t nil)");
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
