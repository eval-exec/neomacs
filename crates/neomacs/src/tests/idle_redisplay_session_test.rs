//! P3.5 J: an idle `(redisplay)` skips layout in a real session. It never
//! did: the window's cached old point and its marker disagreed, so every
//! redisplay's visible-state signature differed from the last. With
//! `RUST_LOG=neomacs::redisplay_sig=debug` the log names which part of the
//! visible state moved between two redisplays.

use super::super::frame_layout::run_tty_layout_tree;
use super::super::{Interactivity, bootstrap_buffers, bootstrap_tty_display_config};
use neomacs_display_runtime::redisplay::RedisplayRuntime;
use neovm_core::emacs_core::load_boot::create_bootstrap_evaluator_cached_with_features;

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
    super::super::frame_layout::disable_cosmic_metrics_for_test();
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

/// An ordinary key consults `clear-message-function` even with no echo message.
/// Its no-op default must leave the displayed rows available to `next-line`.
#[test]
fn plain_next_line_reuses_retained_rows_after_noop_echo_clear() {
    use neovm_core::emacs_core::eval::data::window_layout_query::{
        WindowLayoutQueryAdapter, WindowLayoutQueryScope,
    };

    neovm_core::logging::init_for_tests();
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(
        &mut eval,
        100,
        36,
        bootstrap_tty_display_config(Interactivity::Interactive),
    );
    super::super::frame_layout::disable_cosmic_metrics_for_test();
    let layouts = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let layout_counter = layouts.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        layout_counter.set(layout_counter.get() + 1);
        let _ = run_tty_layout_tree(eval);
    }));
    super::super::frame_layout::install_window_layout_query_fn(&mut eval);

    // Count actual canonical queries while retaining the frontend's real
    // adapter and its runtime. A mock answer could hide a failed reuse path.
    let mut take = eval.take_window_layout_query();
    let original = take.take_previous();
    take.put_previous(WindowLayoutQueryAdapter::Unavailable);
    eval.restore_window_layout_query(take);
    let WindowLayoutQueryAdapter::Ready(mut canonical_query) = original else {
        panic!("the real TTY layout-query adapter must be installed");
    };
    let row_queries = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let counter = row_queries.clone();
    eval.install_window_layout_query_handler(move |eval, frame, window, scope| {
        if matches!(scope, WindowLayoutQueryScope::Rows { .. }) {
            counter.set(counter.get() + 1);
        }
        canonical_query(eval, frame, window, scope)
    })
    .expect("wrap the real TTY layout-query adapter");

    eval.eval_str(
        r#"(progn
             (switch-to-buffer (get-buffer-create "*line-motion*"))
             (fundamental-mode)
             (dotimes (_ 256) (insert "line\n"))
             (goto-char (point-min))
             (setq noninteractive nil
                   inhibit-redisplay nil
                   line-move-visual t
                   clear-message-function 'clear-minibuffer-message
                   minibuffer-message-timer nil
                   minibuffer-message-overlay nil)
             (message nil))"#,
    )
    .expect("plain buffer with the default no-op message clearer");
    eval.eval_str("(redisplay t)").expect("first presentation");
    eval.eval_str("(redisplay t)").expect("stable presentation");
    assert!(!eval.noninteractive(), "exercise display-iterator motion");
    assert!(layouts.get() > 0, "warm an actual TTY presentation");
    assert!(eval.current_message().is_none());

    let before = row_queries.get();
    for _ in 0..5 {
        // This is the same echo-clearing seam used by ordinary key ingestion,
        // before dispatching the command and redisplaying its new point.
        eval.clear_current_message_for_keyboard_input();
        eval.eval_str("(next-line)").expect("move one display line");
        eval.eval_str("(redisplay t)")
            .expect("present the moved point");
    }
    assert_eq!(
        eval.eval_str("(point)").expect("final point"),
        neovm_core::emacs_core::Value::fixnum(26),
        "five next-line commands must move through five ordinary lines"
    );
    assert_eq!(
        row_queries.get() - before,
        0,
        "a no-op echo clear must not make next-line rebuild canonical rows"
    );

    // Prove the counter observes the interactive fallback, and a callback's
    // actual geometry mutation still requires fresh canonical rows.
    eval.eval_str("(setq clear-message-function (lambda () (setq tab-width 3)))")
        .expect("install a callback that changes display geometry");
    let before = row_queries.get();
    eval.clear_current_message_for_keyboard_input();
    eval.eval_str("(next-line)")
        .expect("move after the callback mutation");
    assert!(
        row_queries.get() > before,
        "a callback display mutation must still rebuild canonical rows"
    );
}
