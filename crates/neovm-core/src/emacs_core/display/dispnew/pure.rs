//! Dispnew builtins extracted from display.rs and builtins.rs.
//!
//! Provides cursor visibility state, window designator helpers,
//! and all dispnew-related builtins (redraw, ding, termscript,
//! send-string-to-terminal, internal-show-cursor, force-window-update).

use crate::emacs_core::display::live_frame_designator_p;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{EvalResult, Flow, signal};
use crate::emacs_core::error::{expect_args, expect_args_range};
use crate::emacs_core::terminal::pure::decode_terminal_id_eval;
use crate::emacs_core::terminal::pure::expect_terminal_designator_eval;
use crate::emacs_core::value::ValueKind;
use crate::emacs_core::value::*;
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

/// GNU's `decode_any_window`: nil is the selected window, and everything else
/// need only be a WINDOW -- `windowp`, the widest of the three window
/// predicates.  An INTERNAL window and a DELETED one both pass.
///
/// This used to require a LIVE window while still reporting `windowp`: the
/// predicate named `decode_any_window`'s contract, the check enforced
/// `decode_live_window`'s.  `internal-show-cursor-p` is one line in GNU --
/// `return decode_any_window (window)->cursor_off_p ? Qnil : Qt;`
/// (`src/dispnew.c`) -- and it answers `t` for windows this rejected.
fn expect_window_designator_eval(
    eval: &mut crate::emacs_core::eval::Context,
    value: &Value,
) -> Result<(), Flow> {
    let _ = &eval;
    if value.is_nil() || window_id_from_window_designator(value).is_some() {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("windowp"), *value],
        ))
    }
}

/// A third verbatim copy of this decoder lived here, `Fixnum(id) => WindowId(id)`
/// and all -- the arm GNU has no counterpart for.  It defers to the one in
/// `window_cmds`, the mirror of GNU `src/window.c`, so the window type contract
/// has a single definition.
fn window_id_from_window_designator(value: &Value) -> Option<WindowId> {
    crate::emacs_core::window_cmds::window_id_from_designator(value)
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
    // GNU `redraw_frame` clears the current matrices and marks every window
    // inaccurate, even when the Lisp-visible display state did not change.
    eval.request_menu_bar_rebuild(crate::emacs_core::eval::MenuBarRebuildReason::FullFrameRedraw);
    if crate::emacs_core::eval::gnu_redisplay_hooks_enabled() {
        let frame = crate::emacs_core::window_cmds::resolve_frame_id(
            eval,
            args.first(),
            crate::emacs_core::window_cmds::FrameDomain::Live,
        )?;
        publish_gnu_frame_redraw(eval, frame);
    }
    Ok(Value::NIL)
}

/// (redraw-display) -> nil
pub(crate) fn builtin_redraw_display(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("redraw-display", &args, 0)?;
    eval.request_menu_bar_rebuild(crate::emacs_core::eval::MenuBarRebuildReason::FullFrameRedraw);
    Ok(Value::NIL)
}

/// Context dispatch for GNU redraw-display. The baseline pure entry and its
/// argument/error behavior stay available when the policy is disabled.
#[cold]
#[inline(never)]
pub(crate) fn builtin_redraw_display_in_context(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let result = builtin_redraw_display(eval, args)?;
    if crate::emacs_core::eval::gnu_redisplay_hooks_enabled() {
        crate::emacs_core::window_cmds::ensure_selected_frame_id(eval);
        let mut frames = eval.frames.frame_list();
        // GNU frame creation conses onto Vframe_list. IDs increase at creation.
        frames.sort_unstable_by_key(|frame| std::cmp::Reverse(frame.0));
        for frame in frames {
            if gnu_frame_redisplay_p(&eval.frames, frame) {
                publish_gnu_frame_redraw(eval, frame);
            }
        }
    }
    Ok(result)
}

/// GNU frame_redisplay_p (frame.c:452-491), using borrowed frame IDs only.
/// The owning Context is exclusively borrowed by the caller. Temporary cycle
/// tracking contains no Lisp state and is not shared across mutator threads.
#[cold]
#[inline(never)]
fn gnu_frame_redisplay_p(
    frames: &crate::window::FrameManager,
    frame: crate::window::FrameId,
) -> bool {
    let Some(target) = frames.get(frame) else {
        return false;
    };
    if target.effective_window_system().is_some() {
        return target.visibility.is_visible();
    }
    let terminal = target.terminal_id;
    let mut current = frame;
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current) {
            return false;
        }
        let Some(target) = frames.get(current) else {
            return false;
        };
        if !target.visibility.is_visible() {
            return false;
        }
        if let Some(parent) = frames.frame_parent_id(current) {
            current = parent;
        } else {
            return frames.top_frame_on_terminal(terminal) == Some(current);
        }
    }
}

/// Publish one actual GNU redraw (dispnew.c:3213-3241), including the terminal
/// repaint obligation. IDs belong to this exclusive Context; no Lisp values,
/// thread-local cache, or process-shared mutable state is introduced here.
#[cold]
#[inline(never)]
pub(crate) fn publish_gnu_frame_redraw(
    eval: &mut crate::emacs_core::eval::Context,
    frame: crate::window::FrameId,
) {
    if !crate::emacs_core::eval::gnu_redisplay_hooks_enabled() {
        return;
    }
    let Some((windows, is_tty)) = eval.frames.get(frame).map(|target| {
        let windows = target
            .window_list()
            .into_iter()
            .chain(target.minibuffer_window)
            .filter(|window| {
                target
                    .find_window(*window)
                    .and_then(crate::window::Window::buffer_id)
                    .is_some()
            })
            .collect::<Vec<_>>();
        (windows, target.effective_window_system().is_none())
    }) else {
        return;
    };
    // GNU fset_redisplay is SOME. Redraw never raises global windows ALL or
    // frame-window-change merely because every glyph must be repainted.
    eval.gnu_mark_frame_redisplay(frame);
    for window in windows {
        // GNU marks every live leaf inaccurate and must_be_updated. A general
        // redisplay generation drops retained-body reuse; chrome is targeted
        // at these leaves without inventing update_mode_lines ALL.
        eval.gnu_mark_window_redisplay(window);
        eval.mark_chrome_dirty_window(window);
    }
    if is_tty {
        eval.gnu_request_frame_redraw(frame);
    }
    eval.request_menu_bar_rebuild(crate::emacs_core::eval::MenuBarRebuildReason::FullFrameRedraw);
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
    // GNU writes SBYTES raw, "without alteration" -- internal encoding bytes,
    // never a lossy UTF-8 re-encoding (src/dispnew.c:6820-6821).
    let string = match args[0].kind() {
        ValueKind::String => args[0]
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload"),
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };
    if let Some(terminal) = args.get(1) {
        expect_terminal_designator_eval(eval, terminal)?;
    }
    // GNU `decode_live_terminal': nil is the selected frame's terminal
    // (src/terminal.c:238-245); a garbage or deleted designator already
    // failed `terminal-live-p' above.
    let designator = args.get(1).copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    crate::emacs_core::terminal::pure::write_bytes_to_terminal(terminal_id, string.as_bytes())?;
    Ok(Value::NIL)
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
