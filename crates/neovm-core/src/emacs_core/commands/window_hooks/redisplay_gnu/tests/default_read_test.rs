use super::*;

#[test]
fn void_default_signals_after_callback_and_records_on_unwind() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
    );
    let selected_window = eval.frames.get(frame).expect("live frame").selected_window;
    let stamp = eval.frames.get(frame).expect("live frame").change_stamp;
    for name in [
        "window-buffer-change-functions",
        "window-size-change-functions",
        "window-selection-change-functions",
        "window-state-change-functions",
        "window-configuration-change-hook",
        "window-state-change-hook",
        "inhibit-redisplay",
    ] {
        eval.obarray.set_symbol_value(name, Value::NIL);
    }
    eval.obarray
        .set_symbol_value("d5-default-hook-calls", Value::fixnum(0));
    let callback = eval
        .eval_str("(lambda (_frame) (setq d5-default-hook-calls (1+ d5-default-hook-calls)))")
        .expect("callback lambda");
    eval.obarray.set_symbol_value(
        "window-buffer-change-functions",
        Value::cons(callback, Value::NIL),
    );
    // GNU forbids unbinding the forwarded built-in window hooks (data.c:
    // 1799-1808). Use an ordinary Lisp variable to exercise the same generic
    // default reader without manufacturing an invalid built-in value cell.
    let void_hook = "d5-void-default-hook";
    eval.obarray.makunbound(void_hook);
    assert!(
        crate::emacs_core::data::default_value_by_id(
            &eval,
            crate::emacs_core::intern::intern(void_hook),
        )
        .is_none(),
        "the ordinary fixture variable must have a void default"
    );
    eval.gnu_redisplay_hooks.frame_window_change.insert(frame);
    let bindings = eval.specpdl.len();

    // Probe the shared reader/walker and change-pass guard, rather than
    // claiming a forwarded window hook can be made void. GNU window.c:
    // 4031-4040 reads defaults outside safe_calln and requests recording
    // before the callback; 4116-4120 installs the unwind record and binding.
    let flow = {
        let mut pass = ChangePass {
            eval: &mut eval,
            record: false,
            binding_count: bindings,
            armed: true,
        };
        pass.eval
            .try_specbind_or_unwind_to(
                bindings,
                crate::emacs_core::intern::intern("inhibit-redisplay"),
                Value::T,
            )
            .expect("bind inhibit-redisplay during the pass");
        global(
            pass.eval,
            &mut pass.record,
            frame,
            "window-buffer-change-functions",
        )
        .expect("run the first default callback");
        global(pass.eval, &mut pass.record, frame, void_hook)
            .expect_err("a later void default must escape safe function handling")
        // Leaving the armed guard records the callback's changes and restores
        // inhibit-redisplay even though the default read signaled.
    };
    let signal = flow.as_signal().expect("default-value signal");
    assert_eq!(
        signal.symbol,
        crate::emacs_core::intern::intern("void-variable")
    );
    assert_eq!(signal.data, vec![Value::symbol(void_hook)]);
    assert_eq!(
        eval.obarray.symbol_value_copied("d5-default-hook-calls"),
        Some(Value::fixnum(1))
    );
    assert_eq!(
        eval.frames.get(frame).expect("live frame").change_stamp,
        stamp.next()
    );
    assert_eq!(
        eval.frames
            .get(frame)
            .expect("live frame")
            .old_selected_window,
        Some(selected_window)
    );
    assert_eq!(eval.gnu_redisplay_hooks.old_selected_frame, Some(frame));
    assert_eq!(
        eval.gnu_redisplay_hooks.old_selected_window,
        Some(selected_window)
    );
    assert!(
        eval.gnu_redisplay_hooks
            .dimensions
            .contains_key(&selected_window)
    );
    assert!(
        !eval
            .gnu_redisplay_hooks
            .frame_window_change
            .contains(&frame)
    );
    assert_eq!(eval.specpdl.len(), bindings);
    assert_eq!(
        eval.obarray.symbol_value_copied("inhibit-redisplay"),
        Some(Value::NIL)
    );
}
