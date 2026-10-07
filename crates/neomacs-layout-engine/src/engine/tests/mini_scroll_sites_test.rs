//! Real GNU transaction, pre-hook mini producer and accepted frame producer.
//! A scoped numeric guard selects GNU policy on the owning test thread. Every
//! Context and producer is exclusively owned; no Lisp or environment cache is
//! shared across mutator threads.
use super::super::{FrameLayoutAttempt, LayoutEngine};
use neovm_core::emacs_core::Value;
use neovm_core::heap_types::LispString;
use std::cell::Cell;
use std::rc::Rc;

/// Exact frozen GNU fixture shapes; each test owns its Context and both
/// producers, without sharing Lisp state or changing startup policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MiniScrollCase {
    Overlay,
    Plain,
    FinalNewline,
}

fn real_mini_scroll_log(case: MiniScrollCase) -> String {
    let _policy = neovm_core::emacs_core::eval::RedisplayHookPolicyGuard::gnu();
    let mut eval = neovm_core::emacs_core::load::create_runtime_startup_evaluator_cached()
        .expect("real runtime startup includes GNU window sizing Lisp");
    // Exercise the live display entry, not native test batch suppression.
    eval.obarray_mut()
        .set_symbol_value("noninteractive", Value::NIL);
    eval.obarray_mut()
        .set_symbol_value("inhibit-redisplay", Value::NIL);
    assert!(eval.gnu_redisplay_hooks_policy_enabled());
    let root = eval
        .buffer_manager()
        .current_buffer()
        .expect("root buffer")
        .id();
    let buffer = eval
        .buffer_manager_mut()
        .create_buffer(" *Minibuf-scroll-sites*");
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("mini source")
        .insert("probe: ");
    let frame = eval
        .frame_manager_mut()
        .create_frame("mini-scroll-sites", 120, 40, root);
    {
        let frame = eval.frame_manager_mut().get_mut(frame).expect("frame");
        frame.initial = false;
        frame.visibility = neovm_core::window::FrameVisibility::Visible;
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        frame.set_window_layout_text_size(120, 40);
        frame.shrink_mini_window();
    }
    assert!(
        eval.frame_manager_mut().select_frame(frame),
        "select the live fixture frame"
    );
    assert_eq!(
        eval.frame_manager()
            .selected_frame()
            .expect("selected frame")
            .id,
        frame
    );
    let window = eval
        .activate_minibuffer_window_for_buffer(buffer, LispString::from_utf8("probe: "), None)
        .expect("activate real mini owner")
        .expect("mini window");
    eval.buffer_manager_mut().set_current(buffer);
    eval.eval_str(
        r#"(progn
      (setq resize-mini-windows t max-mini-window-height 3
            scroll-minibuffer-conservatively t)
      (set (make-local-variable 'mode-line-format) nil)
      (set (make-local-variable 'header-line-format) nil)
      (set (make-local-variable 'tab-line-format) nil)
      (setq d5-mini-scroll-log nil)
      (set-default 'window-scroll-functions
        (list (lambda (w start)
          (setq d5-mini-scroll-log
            (cons (list 'global start (eq w (selected-window)) inhibit-redisplay)
                  d5-mini-scroll-log)))))
      (set (make-local-variable 'window-scroll-functions)
        (list (lambda (w start)
          (setq d5-mini-scroll-log
            (cons (list 'local start (eq w (selected-window)) inhibit-redisplay)
                  d5-mini-scroll-log))) t)))"#,
    )
    .expect("install local and default observers");
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .expect("frame")
            .selected_window,
        window
    );
    let preparations = Rc::new(Cell::new(0));
    let preparation_calls = preparations.clone();
    let mut preparation = LayoutEngine::new_without_font_metrics();
    eval.redisplay_prepare_fn = Some(Box::new(move |eval, request| {
        preparation_calls.set(preparation_calls.get() + 1);
        assert_eq!(
            (request.frame, request.window, request.buffer),
            (frame, window, buffer)
        );
        assert!(
            eval.gnu_redisplay_transaction_active(),
            "real GNU transaction owns the producer"
        );
        preparation.prepare_minibuffer_geometry(eval, request)
    }));
    let paints = Rc::new(Cell::new(0));
    let paint_calls = paints.clone();
    let mut producer = LayoutEngine::new_without_font_metrics();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        paint_calls.set(paint_calls.get() + 1);
        assert!(
            eval.gnu_redisplay_transaction_active(),
            "real GNU transaction owns frame production"
        );
        let FrameLayoutAttempt::Prepared(presentation) =
            producer.redisplay_frame_attempt(eval, frame)
        else {
            panic!("real frame producer aborted");
        };
        let presentation_id =
            neovm_core::window::geometry::PresentationId::new(presentation.presentation_id.get());
        let windows = {
            let frame = eval.frame_manager_mut().get_mut(frame).expect("live frame");
            frame
                .activate_display_presentation(presentation_id)
                .expect("accept produced presentation");
            frame.all_leaf_ids()
        };
        for window in &windows {
            if let Some(buffer) = eval
                .frame_manager()
                .get(frame)
                .and_then(|frame| frame.find_window(*window))
                .and_then(neovm_core::window::Window::buffer_id)
            {
                eval.buffer_manager()
                    .get(buffer)
                    .expect("displayed buffer")
                    .reset_unchanged_region();
            }
        }
        eval.note_gnu_frame_display_accepted(frame, windows);
        let _ = eval.gnu_take_tty_frame_redraw(frame);
    }));
    if case == MiniScrollCase::FinalNewline {
        // Exact source prefix used by the frozen GNU final-newline probe.
        eval.eval_str(r#"(insert "input\n")"#)
            .expect("final buffer hard newline before the two settling passes");
        assert_eq!(
            eval.eval_str("(point-max)")
                .expect("source EOB")
                .as_fixnum(),
            Some(14)
        );
    }
    eval.eval_str("(redisplay t)")
        .expect("first accepted mini allocation");
    eval.eval_str("(redisplay t)")
        .expect("settle accepted mini allocation");
    assert!(
        preparations.get() >= 1,
        "initial mini preparation actually ran"
    );
    assert!(paints.get() >= 1, "initial real presentation was accepted");
    let prior_preparations = preparations.get();
    let prior_paints = paints.get();
    eval.eval_str("(setq d5-mini-scroll-log nil)")
        .expect("record only measured action");
    if case != MiniScrollCase::Plain {
        eval.eval_str(
            r#"(let ((o (make-overlay (point-max) (point-max))))
          (overlay-put o 'after-string "\nfirst\nsecond\nthird\nfourth\nfifth")
          (goto-char (point-max)))"#,
        )
        .expect("real overlay producer publishes minibuffer display damage");
    } else {
        eval.eval_str(
            r#"(progn (insert "first\nsecond\nthird\nfourth\nfifth")
          (goto-char (point-max)))"#,
        )
        .expect("real buffer mutation publishes minibuffer source damage");
    }
    eval.eval_str("(redisplay t)")
        .expect("one recorded real GNU transaction");
    assert!(
        preparations.get() > prior_preparations,
        "action entered the canonical mini preparation"
    );
    assert!(
        paints.get() > prior_paints,
        "action entered the canonical accepted frame producer"
    );
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .expect("frame")
            .find_window(window)
            .expect("mini")
            .bounds()
            .height
            .round() as i64,
        if case == MiniScrollCase::FinalNewline {
            2
        } else {
            3
        }
    );
    if case == MiniScrollCase::FinalNewline {
        assert_eq!(
            eval.eval_str("(format \"%S\" (list (window-start (selected-window)) (window-point (selected-window))))")
                .expect("frozen GNU source marker and point")
                .as_str_owned().expect("source geometry tuple"),
            "(1 14)",
            "fresh GNU final-newline capture keeps height2/start1/point14"
        );
    }
    eval.eval_str("(format \"%S\" (nreverse d5-mini-scroll-log))")
        .expect("read numeric callback observations")
        .as_str_owned()
        .expect("log string")
}

#[test]
fn gnu_overlay_mini_reaches_conservative_then_recenter_scroll_sites_at_same_start() {
    assert_eq!(
        real_mini_scroll_log(MiniScrollCase::Overlay),
        "((local 1 t nil) (global 1 t nil) (local 1 t nil) (global 1 t nil))",
        "fresh GNU overlay-mini capture reaches two distinct committed-scroll sites after state hooks, both at source start 1"
    );
}

#[test]
fn gnu_plain_mini_overflow_keeps_prepared_source_start_without_scroll_callbacks() {
    assert_eq!(
        real_mini_scroll_log(MiniScrollCase::Plain),
        "nil",
        "fresh GNU plain source overflow accepts resize_mini_window's prepared marker without entering a scrolling site"
    );
}

#[test]
fn gnu_final_newline_mini_reaches_scroll_sites_with_fresh_source_motion() {
    assert_eq!(
        real_mini_scroll_log(MiniScrollCase::FinalNewline),
        "((local 1 t nil) (global 1 t nil) (local 1 t nil) (global 1 t nil))",
        "fresh GNU final-newline capture has two distinct scroll sites at source start1 after reaching the hard-newline EOB row"
    );
}
