//! Repeated Lisp position queries use the real no-font-metrics adapter.
//!
//! The stable-input case is semantic coverage on both sides of the chrome
//! remeasurement change. Its single named helper is also a narrow Callgrind
//! boundary: initialization and 100 warm queries remain outside collection.

use super::*;
use neovm_core::buffer::LispCharPos1;
use neovm_core::window::WindowLayoutQueryOutcome;
use std::cell::Cell;
use std::rc::Rc;

const POSITION_QUERY: &str = "(posn-at-point 11500)";

fn position_query_text(eval: &mut Context) -> String {
    let position = eval
        .eval_str(POSITION_QUERY)
        .expect("the real Lisp position query succeeds");
    assert!(!position.is_nil(), "the target is inside the GUI viewport");
    neovm_core::emacs_core::print::print_value(&position)
}

#[inline(never)]
fn editpath46_callgrind_repeated_posn_at_point_twenty(eval: &mut Context, expected_position: &str) {
    for _ in 0..20 {
        assert_eq!(
            position_query_text(eval),
            expected_position,
            "unchanged source and geometry must keep the full position tuple stable"
        );
    }
}

#[test]
fn repeated_posn_at_point_uses_the_real_adapter_with_stable_output() {
    let text: String = (0..1000)
        .map(|line| format!("(message \"line {line:04}\")\n"))
        .collect();
    assert_eq!(text.len(), 22_000);
    let (mut eval, frame, buffer, window) = incr_editing_frame(&text, 800, 600);
    {
        let frame = eval.frame_manager_mut().get_mut(frame).expect("frame");
        frame.initial = false;
        frame.char_width = 10.0;
        frame.char_height = 20.0;
        frame.font_pixel_size = 20.0;
        frame.set_window_system(Some(Value::symbol("neomacs")));
        frame.set_window_layout_text_size(80, 29);
    }
    eval.set_variable("noninteractive", Value::NIL);
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer).expect("buffer");
        for name in [
            "mode-line-format",
            "header-line-format",
            "tab-line-format",
            "display-line-numbers",
        ] {
            buffer.set_buffer_local(name, Value::NIL);
        }
    }
    eval.eval_str("(goto-char 11220) (set-window-start nil 10979 t)")
        .expect("initialize W100 point and window start");
    assert_eq!(eval.eval_str("(point)").unwrap().as_int(), Some(11_220));
    assert_eq!(
        eval.eval_str("(window-start)").unwrap().as_int(),
        Some(10_979)
    );
    let target = LispCharPos1::new(11_500);
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    let adapter_calls = Rc::new(Cell::new(0usize));
    let observed_calls = Rc::clone(&adapter_calls);
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        observed_calls.set(observed_calls.get() + 1);
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });

    // Keep an owned String across later callbacks, never an unrooted Lisp
    // position list. Every property/field of subsequent results is compared.
    let position = eval.eval_str(POSITION_QUERY).expect("initial position");
    let fields = neovm_core::emacs_core::value::list_to_vec(&position)
        .expect("the position is a proper list");
    assert_eq!(fields.len(), 10);
    assert_eq!(fields[0].as_window_id(), Some(window.0));
    assert_eq!(fields[1].as_int(), Some(target.as_i64()));
    let expected_position = neovm_core::emacs_core::print::print_value(&position);
    assert_eq!(adapter_calls.get(), 1, "the real adapter answered");

    for _ in 0..100 {
        assert_eq!(position_query_text(&mut eval), expected_position);
    }
    assert_eq!(adapter_calls.get(), 101);
    let before_measured_calls = adapter_calls.get();
    editpath46_callgrind_repeated_posn_at_point_twenty(&mut eval, &expected_position);
    assert_eq!(adapter_calls.get() - before_measured_calls, 20);
    assert!(
        eval.frame_manager()
            .get(frame)
            .expect("frame")
            .redisplay_snapshot(window)
            .is_none(),
        "query-only geometry must not publish a current redisplay matrix"
    );
}
