use super::*;

/// Recovery must release the reporter's inhibition before reading the next
/// command. Reading C-g alone misses this: the leak occurs in cmd_error.
#[test]
fn command_loop_error_recovery_releases_reporter_quit_inhibition() {
    for condition in ["quit", "error"] {
        let (mut ev, global_map) = command_loop_error_test_context();
        ev.eval_str(&format!(
            r#"(progn
                 (setq neo-report-inhibited nil neo-following-inhibited :unset)
                 (setq command-error-function
                       (lambda (_data _context _caller)
                         (setq neo-report-inhibited inhibit-quit)
                         (let ((inhibit-quit t)) (garbage-collect))
                         (setq quit-flag t)))
                 (fset 'neo-quit-command
                       (lambda () (interactive) (signal '{condition} nil)))
                 (fset 'neo-following-command
                       (lambda () (interactive)
                         (setq neo-following-inhibited inhibit-quit))))"#
        ))
        .expect("install quit recovery probe");
        crate::emacs_core::keymap::list_keymap_define_seq(
            global_map,
            &[Value::fixnum(7)],
            Value::symbol("neo-quit-command"),
        )
        .expect("bind C-g");
        ev.command_loop.unread_event(Value::fixnum(7));
        ev.quit_requested.request();
        let depth = ev.specpdl.len();
        run_command_loop_error_commands(&mut ev, global_map, &[("f9", "neo-following-command")]);
        assert_eq!(ev.eval_symbol("neo-report-inhibited").unwrap(), Value::T);
        assert_eq!(
            ev.eval_symbol("neo-following-inhibited").unwrap(),
            Value::NIL,
            "{condition}: the next command must not inherit reporter inhibition"
        );
        assert!(ev.inhibit_quit.is_nil());
        assert!(ev.quit_flag_value().is_nil());
        assert_eq!(ev.specpdl.len(), depth);
        ev.quit_requested.request();
        assert!(
            ev.maybe_quit().is_err_and(|flow| matches!(flow.kind(), crate::emacs_core::error::FlowRef::Signal(sig) if sig.symbol == intern("quit"))),
            "{condition}: a subsequent out-of-band C-g must interrupt"
        );
    }
}

#[test]
fn startup_error_recovery_releases_reporter_quit_inhibition() {
    let (mut ev, _) = command_loop_error_test_context();
    ev.set_variable("noninteractive", Value::NIL);
    ev.eval_str(
        r#"(progn
        (setq top-level '(signal 'error nil))
        (setq command-error-function
              (lambda (_data _context _caller) (setq quit-flag t))))"#,
    )
    .unwrap();
    ev.command_loop_top_level_1()
        .expect("startup report returns normally");
    assert!(ev.inhibit_quit.is_nil());
    assert!(ev.quit_flag_value().is_nil());
}

#[test]
fn command_loop_error_reporter_throw_propagates_without_restarting() {
    let (mut ev, global_map) = command_loop_error_test_context();
    ev.eval_str(
        r#"(progn
        (setq command-error-function
              (lambda (_data _context _caller)
                (unwind-protect (throw 'report-exit 42)
                  (let ((inhibit-quit t)) (garbage-collect)))))
        (fset 'neo-quit-command
              (lambda () (interactive) (signal 'quit nil))))"#,
    )
    .unwrap();
    crate::emacs_core::keymap::list_keymap_define_seq(
        global_map,
        &[Value::fixnum(7)],
        Value::symbol("neo-quit-command"),
    )
    .unwrap();
    ev.command_loop.unread_event(Value::fixnum(7));
    ev.command_loop.running = true;
    let depth = ev.specpdl.len();
    let result = ev
        .eval_str("(catch 'report-exit (recursive-edit))")
        .expect("reporter throw must reach its outer catch");
    assert_eq!(result, Value::fixnum(42));
    assert_eq!(ev.specpdl.len(), depth);
    assert_eq!(
        ev.inhibit_quit,
        Value::T,
        "GNU cmd_error's epilogue is not reached after reporter throw"
    );
}

#[test]
fn command_loop_preserves_deliberate_inhibition_without_error_recovery() {
    let (mut ev, global_map) = command_loop_error_test_context();
    ev.eval_str(
        r#"(progn
        (setq inhibit-quit t neo-command-inhibited nil)
        (fset 'neo-inhibited-command
              (lambda () (interactive)
                (setq neo-command-inhibited inhibit-quit))))"#,
    )
    .unwrap();
    run_command_loop_error_commands(&mut ev, global_map, &[("f9", "neo-inhibited-command")]);
    assert_eq!(ev.eval_symbol("neo-command-inhibited").unwrap(), Value::T);
    assert_eq!(
        ev.inhibit_quit,
        Value::T,
        "ordinary command boundaries must not reset deliberate inhibition"
    );
}

#[test]
fn deliberate_quit_inhibition_preserves_pending_quit_until_unbound() {
    let mut ev = Context::new();
    let depth = ev.specpdl.len();
    ev.try_specbind_or_unwind_to(depth, intern("inhibit-quit"), Value::T)
        .unwrap();
    ev.quit_requested.request();
    ev.maybe_quit().expect("protected code must not quit");
    assert_eq!(ev.quit_flag_value(), Value::T);
    assert_eq!(ev.inhibit_quit, Value::T);
    ev.unbind_to_with_result(depth, Ok(Value::NIL)).unwrap();
    assert!(ev.inhibit_quit.is_nil());
    assert!(ev.maybe_quit().is_err_and(|flow| matches!(flow.kind(), crate::emacs_core::error::FlowRef::Signal(sig) if sig.symbol == intern("quit"))));
}
