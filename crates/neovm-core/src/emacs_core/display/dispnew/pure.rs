//! Dispnew builtins extracted from display.rs and builtins.rs.
//!
//! Provides cursor visibility state, window designator helpers,
//! and all dispnew-related builtins (redraw, ding, termscript,
//! send-string-to-terminal, internal-show-cursor, force-window-update).

use crate::emacs_core::display::live_frame_designator_p;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{EvalResult, Flow, signal};
use crate::emacs_core::error::{expect_args, expect_args_range};
use crate::emacs_core::terminal::pure::expect_terminal_designator_eval;
use crate::emacs_core::value::*;
use crate::emacs_core::value::{ValueKind, VecLikeType};
use crate::window::WindowId;

/// Reset cursor visibility state (called from `reset_display_thread_locals`).
///
/// Cursor visibility now lives on `WindowDisplayState::cursor_off_p`, so
/// there is no longer any dispnew-specific thread-local state to clear.
pub(crate) fn reset_dispnew_thread_locals() {}

// ---------------------------------------------------------------------------
// Argument helpers (local copies — originals are pub(crate) in display.rs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Window designator helpers
// ---------------------------------------------------------------------------

fn live_window_designator_p(eval: &mut crate::emacs_core::eval::Context, value: &Value) -> bool {
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Window) => eval
            .frames
            .find_window_frame_id(WindowId(value.as_window_id().unwrap()))
            .is_some(),
        ValueKind::Fixnum(id) if id >= 0 => eval
            .frames
            .find_window_frame_id(WindowId(id as u64))
            .is_some(),
        _ => false,
    }
}

fn expect_window_designator_eval(
    eval: &mut crate::emacs_core::eval::Context,
    value: &Value,
) -> Result<(), Flow> {
    if value.is_nil() || live_window_designator_p(eval, value) {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("windowp"), *value],
        ))
    }
}

fn window_id_from_window_designator(value: &Value) -> Option<WindowId> {
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Window) => Some(WindowId(value.as_window_id().unwrap())),
        ValueKind::Fixnum(id) if id >= 0 => Some(WindowId(id as u64)),
        _ => None,
    }
}

fn selected_window_id(eval: &mut crate::emacs_core::eval::Context) -> Option<WindowId> {
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(eval);
    eval.frames.get(frame_id).map(|frame| frame.selected_window)
}

fn resolve_internal_show_cursor_window_id(
    eval: &mut crate::emacs_core::eval::Context,
    value: &Value,
) -> Option<WindowId> {
    if value.is_nil() {
        selected_window_id(eval)
    } else {
        window_id_from_window_designator(value)
    }
}

// ---------------------------------------------------------------------------
// Dispnew builtins
// ---------------------------------------------------------------------------

/// Context-aware variant of `redraw-frame`.
///
/// Accepts live frame designators in addition to nil.
pub(crate) fn builtin_redraw_frame(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("redraw-frame", &args, 0, 1)?;
    if let Some(frame) = args.first()
        && !frame.is_nil()
        && !live_frame_designator_p(eval, frame)
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *frame],
        ));
    }
    Ok(Value::NIL)
}

/// (redraw-display) -> nil
pub(crate) fn builtin_redraw_display(args: Vec<Value>) -> EvalResult {
    expect_args("redraw-display", &args, 0)?;
    Ok(Value::NIL)
}

/// (open-termscript FILE) -> error
///
/// NeoVM does not support terminal script logging.
pub(crate) fn builtin_open_termscript(args: Vec<Value>) -> EvalResult {
    expect_args("open-termscript", &args, 1)?;
    Err(signal(
        "error",
        vec![Value::string("Current frame is not on a tty device")],
    ))
}

/// (ding &optional ARG) -> nil
pub(crate) fn builtin_ding(args: Vec<Value>) -> EvalResult {
    expect_args_range("ding", &args, 0, 1)?;
    Ok(Value::NIL)
}

/// Context-aware variant of `send-string-to-terminal`.
///
/// Accepts live frame designators for the optional TERMINAL argument.
pub(crate) fn builtin_send_string_to_terminal(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("send-string-to-terminal", &args, 1, 2)?;
    match args[0].kind() {
        ValueKind::String => {
            if let Some(terminal) = args.get(1) {
                expect_terminal_designator_eval(eval, terminal)?;
            }
            Ok(Value::NIL)
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )),
    }
}

/// Context-aware variant of `internal-show-cursor`.
///
/// Accepts live window designators in addition to nil.
pub(crate) fn builtin_internal_show_cursor(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-show-cursor", &args, 2)?;
    expect_window_designator_eval(eval, &args[0])?;
    let visible = !args[1].is_nil();
    if let Some(window_id) = resolve_internal_show_cursor_window_id(eval, &args[0]) {
        eval.frames.set_window_cursor_visible(window_id, visible);
    }
    Ok(Value::NIL)
}

/// Context-aware variant of `internal-show-cursor-p`.
///
/// Accepts live window designators in addition to nil.
pub(crate) fn builtin_internal_show_cursor_p(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("internal-show-cursor-p", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_designator_eval(eval, window)?;
    }
    let query_window = args.first().unwrap_or(&Value::NIL);
    if let Some(window_id) = resolve_internal_show_cursor_window_id(eval, query_window) {
        return Ok(Value::bool_val(
            eval.frames.window_cursor_visible(window_id),
        ));
    }
    Ok(Value::T)
}

/// (frame--z-order-lessp A B) -> t/nil
///
/// Internal frame sorting predicate.  In NeoVM all frames have equal
/// z-order so this always returns nil.
pub(crate) fn builtin_frame_z_order_lessp(args: Vec<Value>) -> EvalResult {
    expect_args("frame--z-order-lessp", &args, 2)?;
    Ok(Value::NIL)
}
