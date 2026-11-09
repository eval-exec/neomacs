//! Popup-reader buffer selection after an asynchronous display callback.
//!
//! Port of upstream 314906d1b9's
//! `popup_key_reader_follows_selected_buffer_after_async_display` from
//! neovm-core's display tests. Harness lines adapted for the split: the modal
//! reader lives in neovm-display and is reached through its test-support
//! facade; Context fields use the split's buffer/frame accessors.

use neovm_value::value::Value;

/// Popup navigation follows the selected window after callbacks display another buffer.
#[test]
fn popup_key_reader_follows_selected_buffer_after_async_display() {
    use crate::emacs_core::eval::RedisplayHookPolicyGuard;
    use crate::test_utils::input_wait::InputGateOwner;
    use crate::window::DisplayFrameManagerExt;
    let _policy = RedisplayHookPolicyGuard::legacy();
    let mut ctx = crate::test_utils::runtime_startup_context();
    let old = ctx.edit.buffers.create_buffer("*popup-old*");
    ctx.edit.buffers.set_current(old);
    let frame = ctx.frame_manager_mut().create_frame("F1", 80, 24, old);
    assert!(ctx.frame_manager_mut().select_frame(frame));
    let _input_owner = InputGateOwner::install(&mut ctx);
    ctx.eval_str(
        r#"(progn
        (setq timer-list nil timer-idle-list nil unread-command-events nil popup-read-timer-fired nil)
        (use-local-map (make-sparse-keymap))
        (local-set-key "a" 'popup-old-command)
        (with-current-buffer (get-buffer-create "*popup-new*")
          (fundamental-mode)
          (use-local-map (make-sparse-keymap))
          (local-set-key "a" 'popup-new-command))
        (run-at-time 0.01 nil
          (lambda ()
            (save-current-buffer (switch-to-buffer "*popup-new*"))
            (setq popup-read-timer-fired t unread-command-events (list ?a)))))"#,
    )
    .unwrap();
    let (keys, binding) =
        neovm_display::emacs_core::display::read_popup_menu_key_sequence_for_test(&mut ctx)
            .unwrap();
    assert_eq!(ctx.eval_symbol("popup-read-timer-fired").unwrap(), Value::T);
    assert_eq!(keys, vec![Value::fixnum('a' as i64)]);
    assert_eq!(binding, Value::symbol("popup-new-command"));
    assert_eq!(
        ctx.eval_str("(eq (current-buffer) (window-buffer))")
            .unwrap(),
        Value::T
    );
}
