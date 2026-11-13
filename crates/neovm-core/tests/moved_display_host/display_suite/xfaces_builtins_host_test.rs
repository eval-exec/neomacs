//! Re-homed xfaces display-entry test (P1 B1): body is the display original;
//! only the constructor line changed and the core-side entry is the Lisp one.
use super::*;

#[test]
fn xw_color_primitives_follow_live_gui_frame_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.window_system = Some(Value::symbol("neo"));
    }

    assert_eq!(
        builtin_xw_color_defined_p_ctx(
            &eval,
            vec![Value::string("#123456"), Value::make_frame(frame_id.0)],
        )
        .expect("xw-color-defined-p should evaluate"),
        Value::T
    );
    assert_eq!(
        builtin_xw_color_values_ctx(
            &eval,
            vec![Value::string("#123456"), Value::make_frame(frame_id.0)],
        )
        .expect("xw-color-values should evaluate"),
        Value::list(vec![
            Value::fixnum(0x12 * 257),
            Value::fixnum(0x34 * 257),
            Value::fixnum(0x56 * 257),
        ])
    );
    assert_eq!(
        eval.eval_str("(xw-display-color-p (selected-frame))")
            .expect("public Lisp entry: xw-display-color-p should evaluate"),
        Value::T
    );
}
