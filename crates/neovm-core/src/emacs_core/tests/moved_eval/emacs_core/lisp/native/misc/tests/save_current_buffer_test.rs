//! `save-current-buffer` against the runtime-startup image.
//!
//! The canonical hook proof is minted in core, so eval cannot construct
//! `runtime_startup_context`. The test calls that harness and the public
//! buffer manager. The rest of `misc_test.rs` stays in eval. Assertions
//! are unchanged.

use neovm_value::value::{Value, eq_value};

#[test]
fn sf_save_current_buffer_restores() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // Create a buffer and make it current
    let buf_id = ev.edit.buffers.create_buffer("*test*");
    ev.edit.buffers.set_current(buf_id);

    // save-current-buffer with body that just returns 42
    let result = ev.eval_str("(save-current-buffer 42)").unwrap();
    assert!(eq_value(&result, &Value::fixnum(42)));
    // Current buffer should still be *test*
    assert_eq!(ev.edit.buffers.current_buffer().unwrap().id(), buf_id);
}
