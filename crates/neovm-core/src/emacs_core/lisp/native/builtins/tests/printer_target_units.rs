//! Stream destination classification, separate from GNU-refreshed stream oracles.
//! GNU src/print.c:132-162 treats only nil/t, buffers and markers as direct streams.
use super::print_target_is_direct;
use crate::buffer::LispCharPos1;
use crate::emacs_core::eval::Context;
use crate::emacs_core::marker::make_registered_buffer_marker;
use crate::emacs_core::value::Value;

#[test]
fn print_stream_target_kind_rejects_strings_even_when_the_named_buffer_exists() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let buffer = context.buffers.create_buffer("terpri-state-named-stream");
    let marker =
        make_registered_buffer_marker(&mut context.buffers, buffer, LispCharPos1::new(1), false);
    assert!(!print_target_is_direct(Value::string(
        "terpri-state-named-stream"
    )));
    assert!(!print_target_is_direct(Value::string(
        "terpri-state-missing-stream"
    )));
    for target in [Value::NIL, Value::T, Value::make_buffer(buffer), marker] {
        assert!(print_target_is_direct(target));
    }
}
