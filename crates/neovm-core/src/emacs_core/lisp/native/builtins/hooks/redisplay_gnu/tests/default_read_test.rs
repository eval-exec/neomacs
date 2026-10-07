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
    // Fault-inject a genuinely void value cell using the storage API. A raw
    // UNBOUND write through this built-in's Obj forwarder leaves a Forwarded
    // object containing UNBOUND, not the Plainval/Qunbound absence that GNU's
    // default-value reader signals on. This unit probes the C reader's unwind
    // contract; GNU Lisp itself forbids unbinding forwarded built-in variables.
    eval.obarray.makunbound("window-size-change-functions");
    assert!(
        crate::emacs_core::data::default_value_by_id(
            &eval,
            crate::emacs_core::intern::intern("window-size-change-functions"),
        )
        .is_none(),
        "fixture must install a genuinely void default cell"
    );
    eval.gnu_redisplay_hooks.frame_window_change.insert(frame);
    let bindings = eval.specpdl.len();

    // GNU window.c records before running callbacks; its later Fdefault_value
    // can signal outside safe_calln, but the earlier callback still requests
    // window_change_record on unwind. This exercises the entire pass.
    let flow = run(&mut eval).expect_err("void size default must escape safe function handling");
    let signal = flow.as_signal().expect("default-value signal");
    assert_eq!(
        signal.symbol,
        crate::emacs_core::intern::intern("void-variable")
    );
    assert_eq!(
        signal.data,
        vec![Value::symbol("window-size-change-functions")]
    );
    assert_eq!(
        eval.obarray.symbol_value("d5-default-hook-calls").copied(),
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
        eval.obarray.symbol_value("inhibit-redisplay").copied(),
        Some(Value::NIL)
    );
}
