//! Shutdown hook walks requiring the canonical Core runtime.

use crate::emacs_core::error::EvalResult;
use crate::emacs_core::eval::keyboard::{FatalSignal, FatalSignalShutdown};
use crate::emacs_core::eval::{Context, ShutdownPhase, ShutdownRequest};
use crate::emacs_core::hook_runtime::{hook_symbol_by_name, hook_value_by_id, run_hook_value};
use crate::emacs_core::{SymId, Value};

/// Lisp variable on the Context that ran the dispatch. Each Context has
/// its own binding, so two Contexts do not share a log.
fn dispatch_log_symbol(ctx: &Context) -> SymId {
    hook_symbol_by_name(ctx, "neomacs--shutdown-dispatch-log")
}

fn cons_list(items: Vec<Value>) -> Value {
    items
        .into_iter()
        .rev()
        .fold(Value::NIL, |cdr, car| Value::cons(car, cdr))
}

fn list_items(mut list: Value) -> Vec<Value> {
    let mut items = Vec::new();
    while !list.is_nil() {
        assert!(list.is_cons(), "dispatch list");
        items.push(list.cons_car());
        list = list.cons_cdr();
    }
    items
}

/// Records the auto-save dispatch through [`Context::register_subr`].
/// The record is a Lisp list on this Context:
/// `(args phase-fixnum . stored-flag)`, consed onto
/// `neomacs--shutdown-dispatch-log`.
fn recording_shutdown_subr(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    let phase = ctx.shutdown_phase;
    let request_stored = ctx.shutdown_request().is_some();
    let stored = if request_stored { Value::T } else { Value::NIL };
    let record = Value::cons(
        cons_list(args),
        Value::cons(Value::fixnum(phase as u8 as i64), stored),
    );
    let sym = dispatch_log_symbol(ctx);
    let prev = hook_value_by_id(ctx, sym).unwrap_or(Value::NIL);
    ctx.assign_by_id(sym, Value::cons(record, prev));
    Ok(Value::NIL)
}

fn install_recording_shutdown_subr(ctx: &mut Context) {
    ctx.assign_by_id(dispatch_log_symbol(ctx), Value::NIL);
    ctx.register_subr(crate::emacs_core::subr::SubrSpec::new(
        "do-auto-save",
        crate::emacs_core::subr::NativeFn::ContextVec(recording_shutdown_subr),
        crate::emacs_core::subr::SubrArity::new(0, Some(2)),
    ));
}

fn assert_shutdown_dispatches(ctx: &Context, count: usize) {
    let list = hook_value_by_id(ctx, dispatch_log_symbol(ctx)).unwrap_or(Value::NIL);
    let mut rows = list_items(list);
    rows.reverse();
    assert_eq!(rows.len(), count, "shutdown dispatch count");
    for (index, record) in rows.iter().enumerate() {
        assert!(record.is_cons(), "dispatch record");
        let args = list_items(record.cons_car());
        let tail = record.cons_cdr();
        assert!(tail.is_cons(), "dispatch phase");
        assert_eq!(
            args,
            vec![Value::T, Value::NIL],
            "dispatch {index} arguments"
        );
        assert_eq!(
            tail.cons_car().as_fixnum(),
            Some(ShutdownPhase::Dying as u8 as i64),
            "dispatch {index} phase"
        );
        assert!(
            tail.cons_cdr().is_nil(),
            "dispatch {index} saw a stored request"
        );
    }
}

#[test]
fn kill_emacs_hook_runs_once_before_dying_and_later_walks_stay_silent() {
    let mut ctx = crate::test_utils::new_context();
    ctx.eval_str(
        r#"(progn
  (setq n 0)
  (setq during nil)
  (setq probe-ran nil)
  (setq kill-emacs-hook (lambda ()
    (setq during inhibit-redisplay)
    (setq n (1+ n))))
  (setq probe-hook (lambda () (setq probe-ran t))))"#,
    )
    .expect("install hooks");
    install_recording_shutdown_subr(&mut ctx);
    let flow = ctx
        .shutdown_with_hooks(ShutdownRequest {
            exit_code: 0,
            restart: false,
        })
        .expect_err("shutdown");
    assert!(flow.is_shutdown());
    assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
    assert!(ctx.eval_str("during").expect("during").is_nil());
    assert_eq!(ctx.eval_str("n").expect("n"), Value::fixnum(1));
    assert!(ctx.eval_str("inhibit-redisplay").expect("inhibit").is_t());
    assert!(
        ctx.eval_str("(run-hooks 'probe-hook)")
            .expect("run-hooks")
            .is_nil()
    );
    assert!(
        ctx.eval_str("(run-hook-with-args-until-success 'probe-hook)")
            .expect("until-success")
            .is_nil()
    );
    assert!(
        ctx.eval_str("(run-hook-with-args-until-failure 'probe-hook)")
            .expect("until-failure")
            .is_t()
    );
    assert!(ctx.eval_str("probe-ran").expect("probe-ran").is_nil());
    assert_shutdown_dispatches(&ctx, 1);
}

#[test]
fn hook_callback_kill_emacs_does_not_run_the_remaining_callbacks() {
    // GNU-observable shape: `Fkill_emacs` does not return, so the rest of
    // the walk does not run. Here `Flow::shutdown` propagates through the
    // walker and is not swallowed.
    let mut ctx = crate::test_utils::new_context();
    ctx.eval_str(
        r#"(progn
  (setq first nil)
  (setq second nil)
  (setq probe-hook (list
    (lambda () (setq first t) (kill-emacs 0))
    (lambda () (setq second t)))))"#,
    )
    .expect("install probe");
    install_recording_shutdown_subr(&mut ctx);
    let sym = hook_symbol_by_name(&ctx, "probe-hook");
    let value = hook_value_by_id(&ctx, sym).expect("probe value");
    let flow = run_hook_value(&mut ctx, sym, value, &[], true).expect_err("shutdown flow");
    assert!(flow.is_shutdown());
    assert_eq!(ctx.eval_str("first").expect("first"), Value::T);
    assert!(ctx.eval_str("second").expect("second").is_nil());
    assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
    assert_shutdown_dispatches(&ctx, 1);
}

/// These liveness observers answer from this Context's frame table,
/// the same live-frame rule as
/// `frame-live-p`, and `t` for a terminal object while a frame is live
/// (TTY `terminal-live-p`).
fn install_display_liveness_standins(ctx: &mut Context) {
    use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};
    use neovm_value::value::{ValueKind, VecLikeType};

    fn selected_frame(ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
        Ok(match ctx.frame_manager().selected_frame() {
            Some(frame) => Value::make_frame(frame.id.0),
            None => Value::NIL,
        })
    }
    fn frame_live_p(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        let live = args
            .first()
            .and_then(|value| value.as_frame_id())
            .and_then(|id| {
                ctx.frame_manager()
                    .get(neovm_value::display_vocab::FrameId(id))
            })
            .is_some();
        Ok(Value::bool_val(live))
    }
    fn frame_terminal(ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
        Ok(match ctx.frame_manager().selected_frame() {
            Some(frame) => Value::make_terminal(frame.terminal_id),
            None => Value::NIL,
        })
    }
    fn terminal_live_p(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
        let terminal = matches!(
            args.first().copied().map(Value::kind),
            Some(ValueKind::Veclike(VecLikeType::Terminal))
        );
        let frame_live = ctx
            .frame_manager()
            .frame_list()
            .into_iter()
            .any(|id| ctx.frame_manager().get(id).is_some());
        Ok(Value::bool_val(terminal && frame_live))
    }
    ctx.register_subr(SubrSpec::new(
        "selected-frame",
        NativeFn::ContextVec(selected_frame),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-live-p",
        NativeFn::ContextVec(frame_live_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-terminal",
        NativeFn::ContextVec(frame_terminal),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "terminal-live-p",
        NativeFn::ContextVec(terminal_live_p),
        SubrArity::new(1, Some(1)),
    ));
}

fn insert_live_tty_frame(ctx: &mut Context) {
    use crate::emacs_core::eval::data::window_state::{Frame, Window};
    use neovm_edit::emacs_core::display_metrics::{Rect, WindowId};

    let scratch = ctx.buffer_manager_mut().create_buffer("*scratch*");
    let frames = ctx.frame_manager_mut();
    let frame_id = neovm_value::display_vocab::FrameId(frames.next_frame_id);
    frames.next_frame_id += 1;
    let window_id = WindowId(frames.next_window_id);
    frames.next_window_id += 1;
    let minibuffer_id = WindowId(frames.next_window_id);
    frames.next_window_id += 1;
    let root = Window::new_leaf(window_id, scratch, Rect::new(0.0, 0.0, 80.0, 24.0));
    let frame = Frame::new(
        frame_id,
        Value::string("F1"),
        crate::emacs_core::eval::protocol::tty_host::TERMINAL_ID,
        80,
        24,
        root,
        minibuffer_id,
    );
    frames.frames.insert(frame_id, frame);
    frames.selected = Some(frame_id);
    frames.terminal_top_frames.insert(
        crate::emacs_core::eval::protocol::tty_host::TERMINAL_ID,
        frame_id,
    );
}

fn install_last_terminal_hook(ctx: &mut Context) {
    insert_live_tty_frame(ctx);
    install_display_liveness_standins(ctx);
    ctx.eval_str(
        r#"(progn
  (setq n 0)
  (setq during nil)
  (setq frame-was-live nil)
  (setq terminal-was-live nil)
  (setq kill-emacs-hook (lambda ()
    (setq during inhibit-redisplay)
    (setq n (1+ n))
    (setq frame-was-live (frame-live-p (selected-frame)))
    (setq terminal-was-live (terminal-live-p (frame-terminal))))))"#,
    )
    .expect("install hook");
    install_recording_shutdown_subr(ctx);
}

fn assert_last_terminal_exit(ctx: &mut Context) {
    assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
    assert!(ctx.eval_str("during").expect("during").is_nil());
    assert_eq!(ctx.eval_str("n").expect("n"), Value::fixnum(1));
    assert!(ctx.eval_str("frame-was-live").expect("frame").is_t());
    assert!(ctx.eval_str("terminal-was-live").expect("terminal").is_t());
    assert!(ctx.eval_str("inhibit-redisplay").expect("inhibit").is_t());
    assert_eq!(
        ctx.shutdown_request().expect("request"),
        ShutdownRequest::fatal_signal(FatalSignal::Hangup)
    );
    assert_shutdown_dispatches(ctx, 1);
}

#[test]
fn last_terminal_disconnect_enters_dying_after_kill_emacs_hook() {
    // GNU Emacs 31.1 a360712c9d, keyboard.c:8113-8124 and emacs.c:441-462.
    // Last-terminal hangup exits with SIGHUP (code 1) after one
    // kill-emacs-hook call that still sees a live frame and terminal.
    // Delete hooks do not run. Accepted pty evidence:
    // escape-gnu-tty-hangup-2821-artifacts-20261110T210907Z
    // Unattested GNU, not the attested oracle. Source contract at a360712c9d,
    // kill.log "KILL frame=t terminal=t", delete.log absent, exit 1.
    let mut ctx = crate::test_utils::new_context();
    install_last_terminal_hook(&mut ctx);
    ctx.handle_display_terminal_disconnect();
    assert_last_terminal_exit(&mut ctx);

    // GNU's SIGHUP path calls `Fkill_emacs` again while the first call
    // is still on the stack (`emacs.c:453-460`). A disconnect during
    // InProgress re-enters the hook instead of returning nil.
    let mut during = crate::test_utils::new_context();
    install_last_terminal_hook(&mut during);
    during.shutdown_phase = ShutdownPhase::InProgress;
    during.handle_display_terminal_disconnect();
    assert_last_terminal_exit(&mut during);
}

#[test]
fn nested_kill_emacs_inside_the_hook_reenters_and_keeps_the_inner_code() {
    // GNU probe: hook A calls `(kill-emacs 5)` on its first run, hook B
    // is later on the list, outer `(kill-emacs 0)`. The inner call runs
    // the list again and the process exits 5. Log shape: HOOK 1, HOOK 2,
    // SECOND-HOOK. B runs once, inside the inner run.
    let mut ctx = crate::test_utils::new_context();
    ctx.eval_str(
        r#"(progn
  (setq n 0)
  (setq second 0)
  (setq kill-emacs-hook (list
    (lambda ()
      (setq n (1+ n))
      (if (= n 1) (progn (kill-emacs 5))))
    (lambda ()
      (setq second (1+ second))))))"#,
    )
    .expect("install hooks");
    install_recording_shutdown_subr(&mut ctx);
    ctx.eval_str("(kill-emacs 0)")
        .expect_err("inner shutdown propagates");
    assert_eq!(ctx.shutdown_request().expect("inner").exit_code, 5);
    assert_eq!(ctx.eval_str("n").expect("n"), Value::fixnum(2));
    assert_eq!(ctx.eval_str("second").expect("second"), Value::fixnum(1));
    assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
    assert!(ctx.eval_str("inhibit-redisplay").expect("inhibit").is_t());
    assert_shutdown_dispatches(&ctx, 1);
}

#[test]
fn dying_retry_skips_the_hook_and_keeps_a_published_request() {
    let mut ctx = crate::test_utils::new_context();
    ctx.eval_str(
        r#"(progn
  (setq n 0)
  (setq kill-emacs-hook (lambda () (setq n (1+ n)))))"#,
    )
    .expect("install hook");
    install_recording_shutdown_subr(&mut ctx);
    ctx.shutdown_phase = ShutdownPhase::Dying;
    let flow = ctx
        .shutdown_with_hooks(ShutdownRequest {
            exit_code: 3,
            restart: false,
        })
        .expect_err("retry");
    assert!(flow.is_shutdown());
    assert_eq!(ctx.shutdown_request().expect("published").exit_code, 3);
    assert_eq!(ctx.eval_str("n").expect("n"), Value::fixnum(0));
    assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
    let again = ctx
        .shutdown_with_hooks(ShutdownRequest {
            exit_code: 0,
            restart: false,
        })
        .expect_err("published request stays");
    assert!(again.is_shutdown());
    assert_eq!(ctx.shutdown_request().expect("still 3").exit_code, 3);
    assert_eq!(ctx.eval_str("n").expect("n"), Value::fixnum(0));
    assert_eq!(ctx.shutdown_phase, ShutdownPhase::Dying);
    assert_shutdown_dispatches(&ctx, 1);
}
