//! Diagnostic: repeated precise scroll input must survive actual redisplay.
use neomacs_layout_engine::LayoutEngine;
use neovm_core::emacs_core::Context;
use neovm_core::window::{FrameId, Window, WindowId};

#[test]
fn lisp_precision_scroll_survives_redisplay_in_both_directions() {
    use neomacs_layout_engine::engine::WindowLayoutQueryEngine;
    use neovm_core::emacs_core::load::apply_runtime_startup_state;
    use neovm_core::emacs_core::load_boot::create_bootstrap_evaluator_cached_with_features;
    use neovm_core::emacs_core::value::Value;
    use neovm_core::window::DisplayFrameManagerExt;
    use neovm_core::window::WindowLayoutQueryOutcome;
    use neovm_core::window::display::WindowDisplayContextExt;

    let mut eval = create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"])
        .expect("bootstrap Lisp scroll commands");
    apply_runtime_startup_state(&mut eval).expect("runtime startup");
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("precision-scroll", 800, 600, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neo"));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.store_variable_by_name("precision-scroll-frame", Value::make_frame(frame.0));
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    })
    .expect("install the layout-query handler");
    eval.eval_str(
        r#"
        (require 'pixel-scroll)
        (setq noninteractive nil)
        (select-frame precision-scroll-frame)
        (switch-to-buffer (get-buffer-create "*precision-scroll*"))
        (dotimes (i 400) (insert (format "Line %03d scrolling regression\n" i)))
        (goto-char (point-min))
        (set-window-start nil (point-min) t)
    "#,
    )
    .expect("scrollable buffer");
    let mut engine = LayoutEngine::new_without_font_metrics();
    present(&mut engine, &mut eval, frame);
    let initial = viewport(&eval, frame, window);
    let mut previous = initial;
    for down in [true, false] {
        for step in 0..40 {
            let form = if down {
                "(pixel-scroll-precision-scroll-down 3)"
            } else {
                "(pixel-scroll-precision-scroll-up 3)"
            };
            let result = eval
                .eval_str(&format!(
                    "(condition-case err (progn {form} 'ok) (error err))"
                ))
                .expect("invoke GNU precision scroll command");
            assert_eq!(
                neovm_core::emacs_core::print::print_value(&result),
                "ok",
                "{form} step {step}"
            );
            let applied = viewport(&eval, frame, window);
            present(&mut engine, &mut eval, frame);
            let presented = viewport(&eval, frame, window);
            assert_eq!(
                presented, applied,
                "redisplay must preserve the command's viewport"
            );
            assert!(
                if down {
                    presented > previous
                } else {
                    presented < previous
                },
                "scroll must survive redisplay: down={down}, step={step}, before={previous:?}, after={presented:?}"
            );
            previous = presented;
        }
    }
    assert_eq!(
        previous, initial,
        "opposite gestures return to the initial viewport"
    );
}

fn present(engine: &mut LayoutEngine, eval: &mut Context, frame: FrameId) {
    let neomacs_layout_engine::engine::FrameLayoutAttempt::Prepared(state) =
        engine.redisplay_frame_attempt(eval, frame)
    else {
        panic!("redisplay aborted")
    };
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .activate_display_presentation(neovm_core::window::geometry::PresentationId::new(
            state.presentation_id.get(),
        ))
        .expect("activate prepared presentation");
}

fn viewport(eval: &Context, frame: FrameId, window: WindowId) -> (i64, i32) {
    match eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .find_window(window)
        .unwrap()
    {
        Window::Leaf {
            window_start,
            vscroll,
            ..
        } => (window_start.as_i64(), -*vscroll),
        _ => panic!("expected leaf"),
    }
}
