//! VM reader/message test re-homed from neovm-eval's VM test module.
//!
//! The fixture drives `read`, a core-owned minibuffer/reader command (R0
//! placement: it binds the minibuffer and the reader entry). Eval's unit
//! runtime carries the upper entry as an upper-layer refusal, so the test
//! belongs to the runtime that owns the primitive. Core's `new_context`
//! constructs that same surface with the canonical hook-installation proof.
//! The Lisp form, init closure and expected string are unchanged.

use neovm_value::value::Value;

fn vm_eval_with_init_str(
    src: &str,
    init: impl FnOnce(&mut crate::emacs_core::eval::Context),
) -> String {
    let mut eval = crate::test_utils::new_context();
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[test]
fn vm_reader_message_and_completion_builtins_use_shared_runtime_entry() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(let ((buf (get-buffer "vm-message-buffer")))
                 (list
                  (format "%s" buf)
                  (format "%S" buf)
                  (format-message "%s" buf)
                  (progn (message "%s" buf) (current-message))
                  (progn (message-box "%S" buf) (current-message))
                  (progn (message-or-box "%S" buf) (current-message))
                  (try-completion "app" '("application" "apple"))
                  (test-completion "alpha" '("alpha" "beta"))
                  (read-from-string "(a . b)")
                  (read "(1 2)")))"#,
            |eval| {
                eval.edit.buffers.create_buffer("vm-message-buffer");
                // GNU populates `current-message` only on the interactive
                // echo-area path (xdisp.c `message3_frame_nolog`); the default
                // VM test Context is noninteractive.
                eval.set_variable("noninteractive", Value::NIL);
            },
        ),
        r##"OK ("vm-message-buffer" "#<buffer vm-message-buffer>" "vm-message-buffer" "vm-message-buffer" "vm-message-buffer" "vm-message-buffer" "appl" t ((a . b) . 7) (1 2))"##
    );
}
