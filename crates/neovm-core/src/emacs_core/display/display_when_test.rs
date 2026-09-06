use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn nil_and_t_are_taken_literally_without_evaluation() {
    let mut eval = Context::new();
    let buffer = Value::make_buffer(eval.buffers.current_buffer_id().expect("buffer"));
    assert!(!eval.display_when_form_holds(Value::NIL, buffer, 1, 1));
    assert!(eval.display_when_form_holds(Value::symbol("t"), buffer, 1, 1));
}

#[test]
fn form_sees_object_position_and_buffer_position_bound_dynamically() {
    let mut eval = Context::new();
    let buffer_id = eval.buffers.current_buffer_id().expect("buffer");
    let buffer = Value::make_buffer(buffer_id);
    let probe = eval
        .eval_str("'(and (bufferp object) (= position 7) (= buffer-position 42))")
        .expect("read");
    assert!(eval.display_when_form_holds(probe, buffer, 7, 42));
    let other = eval.eval_str("'(= position 8)").expect("read");
    assert!(!eval.display_when_form_holds(other, buffer, 7, 42));
    // The bindings are undone afterwards (GNU `unbind_to`).
    assert!(eval.eval_str("(boundp 'position)").expect("eval").is_nil());
    assert!(
        eval.eval_str("(boundp 'buffer-position)")
            .expect("eval")
            .is_nil()
    );
}

#[test]
fn an_error_in_form_counts_as_nil_like_dsafe_eval() {
    let mut eval = Context::new();
    let buffer = Value::make_buffer(eval.buffers.current_buffer_id().expect("buffer"));
    let erroring = eval.eval_str("'(car 1)").expect("read");
    assert!(!eval.display_when_form_holds(erroring, buffer, 1, 1));
    // And leaves no binding behind.
    assert!(eval.eval_str("(boundp 'object)").expect("eval").is_nil());
}
