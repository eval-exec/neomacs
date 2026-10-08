use crate::emacs_core::Context;

#[test]
fn frame_display_context_selects_target_window_buffer_and_restores_caller() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let caller_buffer = eval.buffer_manager_mut().create_buffer("display-caller");
    let target_buffer = eval.buffer_manager_mut().create_buffer("display-target");
    let caller_frame =
        eval.frame_manager_mut()
            .create_frame("display-caller", 800, 600, caller_buffer);
    let target_frame =
        eval.frame_manager_mut()
            .create_frame("display-target", 800, 600, target_buffer);
    assert!(eval.frame_manager_mut().select_frame(caller_frame));
    assert!(
        eval.buffer_manager_mut()
            .switch_current_unrecorded(caller_buffer)
    );

    let observed = eval
        .with_frame_display_context(target_frame, |eval| {
            (
                eval.frame_manager().selected_frame().map(|frame| frame.id),
                eval.buffer_manager().current_buffer_id(),
            )
        })
        .expect("live target frame context");

    assert_eq!(observed, (Some(target_frame), Some(target_buffer)));
    assert_eq!(
        eval.frame_manager().selected_frame().map(|frame| frame.id),
        Some(caller_frame)
    );
    assert_eq!(
        eval.buffer_manager().current_buffer_id(),
        Some(caller_buffer)
    );
}

#[test]
fn frame_display_context_does_not_restore_a_deleted_caller_frame() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let caller_buffer = eval
        .buffer_manager_mut()
        .create_buffer("deleted-frame-caller");
    let target_buffer = eval
        .buffer_manager_mut()
        .create_buffer("deleted-frame-target");
    let caller_frame =
        eval.frame_manager_mut()
            .create_frame("deleted-frame-caller", 800, 600, caller_buffer);
    let target_frame =
        eval.frame_manager_mut()
            .create_frame("deleted-frame-target", 800, 600, target_buffer);
    assert!(eval.frame_manager_mut().select_frame(caller_frame));

    eval.with_frame_display_context(target_frame, |eval| {
        assert!(
            eval.frame_manager_mut()
                .delete_frame(caller_frame)
                .was_deleted()
        );
    })
    .expect("live target frame context");

    let selected = eval
        .frame_manager()
        .selected_frame()
        .expect("restoration must choose a live frame");
    assert_eq!(selected.id, target_frame);
}

#[test]
fn frame_display_context_does_not_restore_a_deleted_target_window() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let first_buffer = eval
        .buffer_manager_mut()
        .create_buffer("deleted-window-first");
    let second_buffer = eval
        .buffer_manager_mut()
        .create_buffer("deleted-window-second");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("deleted-window", 800, 600, first_buffer);
    let first_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let second_window = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            first_window,
            crate::window::SplitDirection::Horizontal,
            second_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split target window");
    assert!(eval.frame_manager_mut().select_frame(frame_id));

    eval.with_frame_display_context(frame_id, |eval| {
        assert!(
            eval.frame_manager_mut()
                .delete_window(frame_id, first_window)
        );
    })
    .expect("live target frame context");

    let frame = eval.frame_manager().get(frame_id).expect("live frame");
    assert_eq!(frame.selected_window, second_window);
    assert!(frame.find_window(frame.selected_window).is_some());
}

/// A Rust panic inside the operation must still restore the caller's buffer
/// and window selection (P4.11): recovery is the guard's Drop, which owns
/// the saved context storage-only.
#[test]
fn frame_display_context_restores_frame_and_buffer_on_rust_panic() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let caller = eval
        .buffer_manager_mut()
        .create_buffer("frame-panic-caller");
    let target = eval
        .buffer_manager_mut()
        .create_buffer("frame-panic-target");
    let caller_frame = eval
        .frame_manager_mut()
        .create_frame("caller", 800, 600, caller);
    let target_frame = eval
        .frame_manager_mut()
        .create_frame("target", 800, 600, target);
    assert!(eval.frame_manager_mut().select_frame(caller_frame));
    assert!(eval.buffer_manager_mut().switch_current_unrecorded(caller));
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Option<()> = eval.with_frame_display_context(target_frame, |eval| {
            assert_eq!(eval.buffer_manager().current_buffer_id(), Some(target));
            panic!("frame display callback panic");
        });
    }));
    assert!(failed.is_err());
    assert_eq!(eval.buffer_manager().current_buffer_id(), Some(caller));
    assert_eq!(
        eval.frame_manager().selected_frame().map(|frame| frame.id),
        Some(caller_frame)
    );
}

/// `finish` unwinds what the operation pushed inside the frame context, like
/// GNU `unbind_to`; a watcher that panics during that cleanup falls through
/// to the guard's Drop, which retires the remaining suffix storage-only and
/// still restores the caller's frame and buffer (P4.11).
#[test]
fn frame_display_context_keeps_window_recovery_armed_during_cleanup_panic() {
    use crate::emacs_core::Value;
    use crate::emacs_core::error::EvalResult;
    use crate::emacs_core::intern::intern;
    use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

    fn watcher(eval: &mut Context, args: Vec<Value>) -> EvalResult {
        if args.get(2) == Some(&Value::symbol("unlet")) {
            let frame = eval
                .obarray()
                .symbol_value_id_or_nil(intern("display-finish-panic-frame"))
                .as_frame_id()
                .expect("native fixture frame");
            assert!(
                eval.frame_manager_mut()
                    .select_frame(crate::window::FrameId(frame))
            );
            panic!("frame display cleanup watcher panic");
        }
        Ok(Value::NIL)
    }
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let caller = eval
        .buffer_manager_mut()
        .create_buffer("display-finish-caller");
    let target = eval
        .buffer_manager_mut()
        .create_buffer("display-finish-target");
    let caller_frame = eval
        .frame_manager_mut()
        .create_frame("caller", 800, 600, caller);
    let target_frame = eval
        .frame_manager_mut()
        .create_frame("target", 800, 600, target);
    let panic_frame = eval
        .frame_manager_mut()
        .create_frame("watcher", 800, 600, target);
    assert!(eval.frame_manager_mut().select_frame(caller_frame));
    assert!(eval.buffer_manager_mut().switch_current_unrecorded(caller));
    let symbol = intern("display-finish-panic-value");
    eval.obarray_mut()
        .set_symbol_value_id(symbol, Value::fixnum(10));
    eval.obarray_mut().set_symbol_value_id(
        intern("display-finish-panic-frame"),
        Value::make_frame(panic_frame.0),
    );
    eval.register_subr(SubrSpec::new(
        "display-finish-panic-watcher",
        NativeFn::ContextVec(watcher),
        SubrArity::new(4, Some(4)),
    ));
    let depth = eval.specpdl.len();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = eval.with_frame_display_context(target_frame, |eval| {
            eval.try_specbind(symbol, Value::fixnum(20)).expect("bind");
            crate::emacs_core::advice::builtin_add_variable_watcher(
                eval,
                vec![
                    Value::from_sym_id(symbol),
                    Value::symbol("display-finish-panic-watcher"),
                ],
            )
            .expect("watcher");
            Value::NIL
        });
    }));
    assert!(
        caught.is_err(),
        "normal finish must reach the native watcher"
    );
    assert_eq!(eval.buffer_manager().current_buffer_id(), Some(caller));
    assert_eq!(
        eval.frame_manager().selected_frame().map(|frame| frame.id),
        Some(caller_frame)
    );
    assert_eq!(eval.specpdl.len(), depth);
    assert_eq!(
        eval.obarray().symbol_value_id_or_nil(symbol),
        Value::fixnum(10)
    );
}
