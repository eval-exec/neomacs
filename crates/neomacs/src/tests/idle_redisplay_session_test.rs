//! P3.5 J: an idle `(redisplay)` skips layout in a real session. It never
//! did: the window's cached old point and its marker disagreed, so every
//! redisplay's visible-state signature differed from the last. With
//! `RUST_LOG=neomacs::redisplay_sig=debug` the log names which part of the
//! visible state moved between two redisplays.

use super::super::frame_layout::{REDISPLAY_RUNTIME, run_tty_layout_tree};
use super::super::{Interactivity, bootstrap_buffers, bootstrap_tty_display_config};
use neomacs_display_runtime::redisplay::RedisplayRuntime;
use neovm_core::emacs_core::load::create_bootstrap_evaluator_cached_with_features;

#[test]
fn an_idle_redisplay_skips_layout_in_a_real_session() {
    neovm_core::logging::init_for_tests();
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(
        &mut eval,
        100,
        36,
        bootstrap_tty_display_config(Interactivity::Interactive),
    );
    REDISPLAY_RUNTIME.with(RedisplayRuntime::disable_cosmic_metrics);
    let layouts = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let counter = layouts.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        counter.set(counter.get() + 1);
        let _ = run_tty_layout_tree(eval);
    }));
    eval.eval_str(
        "(progn (switch-to-buffer (get-buffer-create \"idle.el\")) \
                (dotimes (i 200) (insert (format \"(defun f%d ()\\n\\t%d)\\n\" i i))) \
                (emacs-lisp-mode) (goto-char (point-min)) (forward-line 30))",
    )
    .expect("setup");
    eval.eval_str("(redisplay t)").expect("first redisplay");
    eval.eval_str("(redisplay t)").expect("second redisplay");
    let before = layouts.get();
    for _ in 0..5 {
        eval.eval_str("(redisplay)").expect("idle redisplay");
    }
    assert_eq!(layouts.get() - before, 0, "idle (redisplay) x5 laid out");
}
