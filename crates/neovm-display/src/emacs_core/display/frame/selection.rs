//! Logical frame selection, mirroring GNU `frame.c::do_switch_frame`.
//!
//! Native focus only produces input events. Selection belongs to the VM and
//! must update its selected window and current buffer before calling Lisp.

use crate::window::DisplayFrameManagerExt;
use crate::window::{FrameFocusTracking, FrameId};
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::hook_runtime::{hook_symbol_by_name, run_named_hook};
use neovm_value::error::{EvalResult, LispCondition, expect_max_args, expect_min_args, signal};
use neovm_value::value::Value;

/// `(select-frame FRAME &optional NORECORD)` -> frame.
#[doc(hidden)]
pub fn builtin_select_frame(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("select-frame", &args, 1)?;
    expect_max_args("select-frame", &args, 2)?;
    let frame = args[0];
    let id = frame
        .as_frame_id()
        .map(FrameId)
        .filter(|id| eval.frame_manager().get(*id).is_some())
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("frame-live-p"), frame],
            )
        })?;
    switch_frame(
        eval,
        id,
        args.get(1).copied().unwrap_or(Value::NIL),
        FrameFocusTracking::FollowSelection,
    )
}

/// `(handle-switch-frame EVENT)` -> selected frame, or nil for a dead frame.
/// The fixed-arity registration supplies exactly one event argument.
#[doc(hidden)]
pub fn builtin_handle_switch_frame(eval: &mut Context, event: Value) -> EvalResult {
    // GNU performs both effects even when the event is invalid or stale.
    let prefix = eval.eval_symbol("current-prefix-arg").unwrap_or(Value::NIL);
    eval.assign("prefix-arg", prefix);
    let hook = hook_symbol_by_name(eval, "mouse-leave-buffer-hook");
    run_named_hook(eval, hook, &[])?;

    let frame = if event.is_cons()
        && event.cons_car() == Value::symbol("switch-frame")
        && event.cons_cdr().is_cons()
    {
        event.cons_cdr().cons_car()
    } else {
        event
    };
    let id = frame.as_frame_id().map(FrameId).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("framep"), frame],
        )
    })?;
    // A queued switch may outlive its frame. Never turn that into an error or
    // silently substitute whichever frame happens to be selected now.
    switch_frame(eval, id, Value::NIL, FrameFocusTracking::Preserve)
}

fn switch_frame(
    eval: &mut Context,
    id: FrameId,
    norecord: Value,
    focus_tracking: FrameFocusTracking,
) -> EvalResult {
    let Some(frame) = eval.frame_manager().get(id) else {
        return Ok(Value::NIL);
    };
    let previous = eval.frame_manager().selected_frame().map(|frame| frame.id);
    if previous == Some(id) {
        return Ok(Value::make_frame(id.0));
    }
    let window = frame.selected_window;
    crate::emacs_core::window_cmds::select_window(eval, window, norecord, focus_tracking)?;
    Ok(Value::make_frame(id.0))
}
