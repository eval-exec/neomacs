//! Window/redisplay hook-running machinery kept in the core command layer
//! (stage-4 E7 R0).
//!
//! The window/frame hook plan, the caller-context save/restore, the redisplay
//! window-change runners and the scroll-function entries bind live window and
//! frame state and Lisp hook values.  Under R0 they stay core-side.  The
//! window-configuration snapshot DATA and its registry stay with the evaluator
//! data layer (S1) and are reached through the evaluator's accessors.
//!
//! Vocabulary follows the E2 display move: frame/window ids and plain records
//! are named through `neovm_value::display_vocab` and the evaluator's
//! `eval::data::window_state` modules, exactly as the live hooks bodies do.

use crate::emacs_core::hook_runtime;
use crate::emacs_core::intern::WellKnown;
use neovm_value::error::{EvalResult, Flow, LispCondition, expect_max_args, signal};
use neovm_value::value::*;

mod redisplay_gnu;

fn expect_optional_live_frame_designator(
    value: &Value,
    eval: &crate::emacs_core::eval::Context,
) -> Result<(), Flow> {
    expect_optional_live_frame_designator_in_state(value, &eval.frames)
}

pub(crate) fn expect_optional_live_frame_designator_in_state(
    value: &Value,
    frames: &crate::window::FrameManager,
) -> Result<(), Flow> {
    if value.is_nil() {
        return Ok(());
    }
    if value.is_frame()
        && let Some(fid) = value.as_frame_id()
        && frames
            .get(neovm_value::display_vocab::FrameId(fid))
            .is_some()
    {
        return Ok(());
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("frame-live-p"), *value],
    ))
}

fn save_hook_caller_context(eval: &crate::emacs_core::eval::Context) -> HookCallerContextState {
    let selected_frame_id = eval.frames.selected_frame().map(|frame| frame.id);
    let selected_window_id = selected_frame_id
        .and_then(|frame_id| eval.frames.get(frame_id).map(|frame| frame.selected_window));
    HookCallerContextState {
        selected_frame_id,
        selected_window_id,
        current_buffer_id: eval.edit.buffers.current_buffer_id(),
    }
}

fn window_buffer_id_in_state(
    eval: &crate::emacs_core::eval::Context,
    frame_id: neovm_value::display_vocab::FrameId,
    window_id: neovm_edit::emacs_core::display_metrics::WindowId,
) -> Option<crate::buffer::BufferId> {
    eval.frames
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.buffer_id())
}

fn select_frame_window_for_hook_context(
    eval: &mut crate::emacs_core::eval::Context,
    frame_id: neovm_value::display_vocab::FrameId,
    window_id: neovm_edit::emacs_core::display_metrics::WindowId,
) -> EvalResult {
    let _ = eval.frames.select_frame(frame_id);
    eval.sync_keyboard_terminal_owner();
    if let Some(frame) = eval.frames.get_mut(frame_id) {
        let _ = frame.select_window(window_id);
    }
    if let Some(buffer_id) = window_buffer_id_in_state(eval, frame_id, window_id) {
        let _ = eval.switch_current_buffer(buffer_id);
    }
    crate::emacs_core::frame::sync_gui_frame_focus_redirects(eval)
}

fn restore_hook_caller_context(
    eval: &mut crate::emacs_core::eval::Context,
    saved: HookCallerContextState,
) -> EvalResult {
    if let Some(frame_id) = saved
        .selected_frame_id
        .filter(|frame_id| eval.frames.get(*frame_id).is_some())
    {
        let _ = eval.frames.select_frame(frame_id);
        eval.sync_keyboard_terminal_owner();
        if let Some(window_id) = saved.selected_window_id
            && let Some(frame) = eval.frames.get_mut(frame_id)
        {
            let _ = frame.select_window(window_id);
        }
    }
    if let Some(buffer_id) = saved.current_buffer_id {
        eval.restore_current_buffer_if_live(buffer_id);
    }
    crate::emacs_core::frame::sync_gui_frame_focus_redirects(eval)
}

fn push_unique_window(
    windows: &mut Vec<neovm_edit::emacs_core::display_metrics::WindowId>,
    window_id: neovm_edit::emacs_core::display_metrics::WindowId,
) {
    if !windows.contains(&window_id) {
        windows.push(window_id);
    }
}

fn live_windows_for_hook_plan(frame: &crate::window::Frame) -> Vec<LiveWindowHookState> {
    let mut windows = Vec::new();
    for window_id in frame.window_list() {
        let Some(window) = frame.find_window(window_id) else {
            continue;
        };
        let Some(buffer_id) = window.buffer_id() else {
            continue;
        };
        windows.push(LiveWindowHookState {
            window_id,
            buffer_id,
            bounds: *window.bounds(),
        });
    }
    if let Some(minibuffer_window) = frame.minibuffer_window
        && let Some(window) = frame.find_window(minibuffer_window)
        && let Some(buffer_id) = window.buffer_id()
    {
        windows.push(LiveWindowHookState {
            window_id: minibuffer_window,
            buffer_id,
            bounds: *window.bounds(),
        });
    }
    windows
}

fn frame_window_hook_record_from_live_state(
    frame: &crate::window::Frame,
    was_selected_frame: bool,
) -> crate::emacs_core::eval::data::window_state::FrameWindowHookRecord {
    let windows = live_windows_for_hook_plan(frame)
        .into_iter()
        .map(|window| {
            (
                window.window_id,
                crate::emacs_core::eval::data::window_state::WindowHookSnapshot {
                    buffer_id: window.buffer_id,
                    bounds: window.bounds,
                },
            )
        })
        .collect();
    crate::emacs_core::eval::data::window_state::FrameWindowHookRecord {
        windows,
        selected_window: Some(frame.selected_window),
        was_selected_frame,
    }
}

fn run_window_local_hook_values(
    eval: &mut crate::emacs_core::eval::Context,
    frame_id: neovm_value::display_vocab::FrameId,
    window_ids: &[neovm_edit::emacs_core::display_metrics::WindowId],
    hook_name: &str,
    hook_sym: crate::emacs_core::intern::SymId,
) -> EvalResult {
    if window_ids.is_empty() {
        return Ok(Value::NIL);
    }

    let saved = save_hook_caller_context(eval);
    let result = (|| -> EvalResult {
        for window_id in window_ids {
            let Some(buffer_id) = window_buffer_id_in_state(eval, frame_id, *window_id) else {
                continue;
            };
            let has_local_hook = eval
                .edit
                .buffers
                .get(buffer_id)
                .and_then(|buffer| buffer.get_buffer_local_binding(hook_name))
                .is_some();
            if !has_local_hook {
                continue;
            }
            select_frame_window_for_hook_context(eval, frame_id, *window_id)?;
            let Some(local_hook_value) = eval
                .edit
                .buffers
                .current_buffer()
                .and_then(|buffer| buffer.buffer_local_value(hook_name))
            else {
                continue;
            };
            let _ = hook_runtime::safe_run_hook_value(
                eval,
                hook_sym,
                local_hook_value,
                &[Value::make_window(window_id.0)],
                false,
            )?;
        }
        Ok(Value::NIL)
    })();
    let restore_result = restore_hook_caller_context(eval, saved);
    result?;
    restore_result
}

fn run_window_default_hook_value(
    eval: &mut crate::emacs_core::eval::Context,
    frame_id: neovm_value::display_vocab::FrameId,
    run_hook: bool,
    hook_sym: crate::emacs_core::intern::SymId,
) -> EvalResult {
    if !run_hook {
        return Ok(Value::NIL);
    }
    let global_hook_value = eval
        .variables()
        .default_value(hook_sym)
        .unwrap_or(Value::NIL);
    if global_hook_value.is_nil() {
        return Ok(Value::NIL);
    }

    let saved = save_hook_caller_context(eval);
    let result = (|| -> EvalResult {
        let selected_window = eval.frames.get(frame_id).map(|frame| frame.selected_window);
        if let Some(selected_window) = selected_window {
            select_frame_window_for_hook_context(eval, frame_id, selected_window)?;
        } else {
            let _ = eval.frames.select_frame(frame_id);
            eval.sync_keyboard_terminal_owner();
            crate::emacs_core::frame::sync_gui_frame_focus_redirects(eval)?;
        }
        let _ = hook_runtime::safe_run_hook_value(
            eval,
            hook_sym,
            global_hook_value,
            &[Value::make_frame(frame_id.0)],
            false,
        )?;
        Ok(Value::NIL)
    })();
    let restore_result = restore_hook_caller_context(eval, saved);
    result?;
    restore_result
}

pub(crate) fn run_redisplay_window_change_hooks(
    eval: &mut crate::emacs_core::eval::Context,
) -> EvalResult {
    if crate::emacs_core::eval::gnu_redisplay_hooks_enabled() {
        return redisplay_gnu::run(eval);
    }
    // Mirrors GNU `run_window_change_functions` (window.c:4116):
    //   specbind (Qinhibit_redisplay, Qt);
    // Any hook function that calls `redisplay` (directly or indirectly)
    // would otherwise re-enter here and infinitely recurse. The specpdl
    // entry is popped when we return, restoring the previous value.
    let specpdl_count = eval.specpdl.len();
    eval.try_specbind_or_unwind_to(specpdl_count, INHIBIT_REDISPLAY.id(), Value::T)?;

    let result = run_redisplay_window_change_hooks_inner(eval);

    eval.unbind_to_with_result(specpdl_count, result)
}

fn run_redisplay_window_change_hooks_inner(
    eval: &mut crate::emacs_core::eval::Context,
) -> EvalResult {
    let frame_ids = eval.frames.frame_list();
    let selected_frame_id = eval.frames.selected_frame().map(|frame| frame.id);
    let mut plans = Vec::new();

    for frame_id in &frame_ids {
        let Some(frame) = eval.frames.get(*frame_id) else {
            continue;
        };
        let previous_record = frame.window_hook_record.clone();
        let current_windows = live_windows_for_hook_plan(frame);
        let selected_window = Some(frame.selected_window);
        let frame_selected_window_change = previous_record.selected_window != selected_window;
        let frame_selected_change =
            previous_record.was_selected_frame != (selected_frame_id == Some(*frame_id));
        let window_deleted = previous_record.windows.keys().any(|window_id| {
            !current_windows
                .iter()
                .any(|window| window.window_id == *window_id)
        });

        let mut local_buffer_windows = Vec::new();
        let mut local_size_windows = Vec::new();
        let mut local_selection_windows = Vec::new();
        let mut local_state_windows = Vec::new();

        for window in &current_windows {
            let previous = previous_record.windows.get(&window.window_id);
            let buffer_changed = previous.is_none()
                || previous.is_some_and(|entry| entry.buffer_id != window.buffer_id);
            let size_changed =
                previous.is_none() || previous.is_some_and(|entry| entry.bounds != window.bounds);
            let selection_changed = frame_selected_window_change
                && (previous_record.selected_window == Some(window.window_id)
                    || selected_window == Some(window.window_id));

            if buffer_changed {
                push_unique_window(&mut local_buffer_windows, window.window_id);
                push_unique_window(&mut local_size_windows, window.window_id);
                push_unique_window(&mut local_state_windows, window.window_id);
            }
            if size_changed {
                push_unique_window(&mut local_size_windows, window.window_id);
                push_unique_window(&mut local_state_windows, window.window_id);
            }
            if selection_changed {
                push_unique_window(&mut local_selection_windows, window.window_id);
                push_unique_window(&mut local_state_windows, window.window_id);
            }
        }

        let frame_buffer_change = !local_buffer_windows.is_empty();
        let frame_size_change = !local_size_windows.is_empty();
        let frame_state_change = frame.window_state_change
            || frame_selected_change
            || frame_selected_window_change
            || frame_buffer_change
            || frame_size_change
            || window_deleted;

        plans.push(FrameWindowHookPlan {
            frame_id: *frame_id,
            frame_buffer_change,
            frame_size_change,
            frame_selected_window_change,
            frame_state_change,
            frame_configuration_change: frame_size_change || window_deleted,
            local_buffer_windows,
            local_size_windows,
            local_selection_windows,
            local_state_windows,
        });
    }

    let window_buffer_change_functions =
        hook_runtime::hook_symbol_by_id(eval, WINDOW_BUFFER_CHANGE_FUNCTIONS.id());
    let window_size_change_functions =
        hook_runtime::hook_symbol_by_id(eval, WINDOW_SIZE_CHANGE_FUNCTIONS.id());
    let window_selection_change_functions =
        hook_runtime::hook_symbol_by_id(eval, WINDOW_SELECTION_CHANGE_FUNCTIONS.id());
    let window_state_change_functions =
        hook_runtime::hook_symbol_by_id(eval, WINDOW_STATE_CHANGE_FUNCTIONS.id());
    let window_state_change_hook =
        hook_runtime::hook_symbol_by_id(eval, WINDOW_STATE_CHANGE_HOOK.id());

    let mut run_window_state_change_hook = false;
    for plan in &plans {
        if eval.frames.get(plan.frame_id).is_none() {
            continue;
        }
        run_window_local_hook_values(
            eval,
            plan.frame_id,
            &plan.local_buffer_windows,
            "window-buffer-change-functions",
            window_buffer_change_functions,
        )?;
        run_window_default_hook_value(
            eval,
            plan.frame_id,
            plan.frame_buffer_change,
            window_buffer_change_functions,
        )?;

        run_window_local_hook_values(
            eval,
            plan.frame_id,
            &plan.local_size_windows,
            "window-size-change-functions",
            window_size_change_functions,
        )?;
        run_window_default_hook_value(
            eval,
            plan.frame_id,
            plan.frame_size_change || plan.frame_buffer_change,
            window_size_change_functions,
        )?;

        run_window_local_hook_values(
            eval,
            plan.frame_id,
            &plan.local_selection_windows,
            "window-selection-change-functions",
            window_selection_change_functions,
        )?;
        run_window_default_hook_value(
            eval,
            plan.frame_id,
            plan.frame_selected_window_change,
            window_selection_change_functions,
        )?;

        run_window_local_hook_values(
            eval,
            plan.frame_id,
            &plan.local_state_windows,
            "window-state-change-functions",
            window_state_change_functions,
        )?;
        run_window_default_hook_value(
            eval,
            plan.frame_id,
            plan.frame_state_change,
            window_state_change_functions,
        )?;
        run_window_state_change_hook |= plan.frame_state_change;

        // GNU `run_window_change_functions` (window.c:4308-4312) runs
        // `window-configuration-change-hook' (via
        // `run_window_configuration_change_hook') when the frame's window
        // configuration changed (a window changed size or was deleted).  This
        // is the redisplay-driven home of the hook -- so in batch (no
        // redisplay) it never fires from bare `split-window'/`delete-window',
        // matching GNU.
        if plan.frame_configuration_change && eval.frames.get(plan.frame_id).is_some() {
            let _ = builtin_run_window_configuration_change_hook(
                eval,
                vec![Value::make_frame(plan.frame_id.0)],
            )?;
        }
    }

    if run_window_state_change_hook {
        let _ = hook_runtime::safe_run_named_hook(eval, window_state_change_hook, &[])?;
    }

    let selected_frame_id = eval.frames.selected_frame().map(|frame| frame.id);
    for frame_id in frame_ids {
        let was_selected_frame = selected_frame_id == Some(frame_id);
        if let Some(frame) = eval.frames.get_mut(frame_id) {
            // GNU `window_change_record` (`src/window.c:3954-3990`)
            // records the current selected_window into
            // `frame->old_selected_window` exactly here, after
            // running the change hooks. neomacs mirrors that
            // step. Window audit Critical 8 in
            // `drafts/window-system-audit.md`.
            frame.old_selected_window = Some(frame.selected_window);
            frame.window_hook_record =
                frame_window_hook_record_from_live_state(frame, was_selected_frame);
            // GNU closes the same block by opening a new epoch
            // (`window_change_record`, `src/window.c`):
            //
            //     f->change_stamp += 1;
            //     if (f->change_stamp == 0) f->change_stamp = 1;
            //     f->number_of_windows
            //       = window_change_record_windows (f->root_window, f->change_stamp, 0);
            //
            // The bump and the per-window record belong together: a window
            // stamped with the epoch its frame is leaving would report a stale
            // record forever.  `ChangeStamp::next` carries GNU's wrap guard, so
            // the reserved zero cannot reappear here.
            frame.change_stamp = frame.change_stamp.next();
            let stamp = frame.change_stamp;
            frame.record_window_change_epoch(stamp);
            frame.window_state_change = false;
        }
    }

    eval.record_redisplay_selection();
    Ok(Value::NIL)
}

pub(crate) fn expect_optional_live_window_designator(
    value: &Value,
    eval: &crate::emacs_core::eval::Context,
) -> Result<(), Flow> {
    if value.is_nil() {
        return Ok(());
    }
    if value.is_window()
        && let Some(wid) = value.as_window_id()
        && eval
            .frames
            .is_live_window_id(neovm_edit::emacs_core::display_metrics::WindowId(wid))
    {
        return Ok(());
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("window-live-p"), *value],
    ))
}

pub(crate) fn builtin_run_window_configuration_change_hook(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("run-window-configuration-change-hook", &args, 1)?;
    // NOTE: GNU's `run_window_configuration_change_hook` (window.c) does NOT
    // gate on `window--sides-inhibit-check`. That variable is let-bound to t
    // only to suppress `window--check` (the side-window consistency validator,
    // window.el), never the hook. It routinely sits at t in real configs (e.g.
    // Doom), so gating the hook on it wrongly silenced
    // `window-configuration-change-hook' entirely -- winum never renumbered a
    // freshly-opened popup/compile window until an unrelated command forced it
    // (#191). The redisplay driver (run_window_change_functions) is the sole
    // caller and only fires post-op, so there is no re-entrancy to guard.
    if let Some(frame) = args.first() {
        expect_optional_live_frame_designator(frame, eval)?;
    }
    let frame = match args.first().copied().unwrap_or(Value::NIL).kind() {
        ValueKind::Nil => crate::emacs_core::window_cmds::selected_frame_impl(
            &mut eval.frames,
            &mut eval.edit.buffers,
            vec![],
        )?,
        _value => args.first().copied().unwrap_or(Value::NIL),
    };
    let Some(fid) = frame.as_frame_id() else {
        return Ok(Value::NIL);
    };
    let frame_id = neovm_value::display_vocab::FrameId(fid);
    if crate::emacs_core::eval::gnu_redisplay_hooks_enabled() {
        return redisplay_gnu::run_configuration(eval, frame_id);
    }
    let Some(frame_state) = eval.frames.get(frame_id) else {
        return Ok(Value::NIL);
    };

    let hook_sym = hook_runtime::hook_symbol_by_id(eval, WINDOW_CONFIGURATION_CHANGE_HOOK.id());
    let global_hook_value = eval
        .variables()
        .default_value(hook_sym)
        .unwrap_or(Value::NIL);
    let selected_window = frame_state.selected_window;
    let window_ids = frame_state.window_list();
    let hook_name = crate::emacs_core::intern::resolve_sym(hook_sym);
    let saved = save_hook_caller_context(eval);

    // The snapshot of the GLOBAL hook list is held in a Rust local across
    // the per-window buffer-local hook runs (arbitrary Lisp); a local hook
    // that setq-defaults or globally remove-hooks the variable unlinks the
    // snapshot from its obarray root, and a GC frees the conses before the
    // final global run walks them. One root for the snapshot preserves the
    // GNU behavior of running the pre-local-hooks snapshot.
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(global_hook_value);

    let result = (|| -> EvalResult {
        select_frame_window_for_hook_context(eval, frame_id, selected_window)?;
        for window_id in &window_ids {
            let Some(buffer_id) = window_buffer_id_in_state(eval, frame_id, *window_id) else {
                continue;
            };
            let has_local_hook = eval
                .edit
                .buffers
                .get(buffer_id)
                .and_then(|buffer| buffer.get_buffer_local_binding(hook_name))
                .is_some();
            if !has_local_hook {
                continue;
            }
            select_frame_window_for_hook_context(eval, frame_id, *window_id)?;
            let Some(local_hook_value) = eval
                .edit
                .buffers
                .current_buffer()
                .and_then(|buffer| buffer.buffer_local_value(hook_name))
            else {
                continue;
            };
            let _ = hook_runtime::run_hook_value(eval, hook_sym, local_hook_value, &[], false)?;
            select_frame_window_for_hook_context(eval, frame_id, selected_window)?;
        }
        let _ = hook_runtime::run_hook_value(eval, hook_sym, global_hook_value, &[], false)?;
        Ok(Value::NIL)
    })();

    eval.restore_specpdl_roots(root_scope);
    let restore_result = restore_hook_caller_context(eval, saved);
    result?;
    restore_result
}

pub(crate) fn run_gnu_committed_scroll_functions(
    eval: &mut crate::emacs_core::eval::Context,
    window: neovm_edit::emacs_core::display_metrics::WindowId,
) -> EvalResult {
    redisplay_gnu::run_committed_scroll(eval, window)
}

pub(crate) fn builtin_run_window_scroll_functions(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("run-window-scroll-functions", &args, 1)?;
    if let Some(window) = args.first() {
        expect_optional_live_window_designator(window, eval)?;
    }

    let window_arg = match args.first().copied().unwrap_or(Value::NIL).kind() {
        ValueKind::Nil => crate::emacs_core::window_cmds::builtin_selected_window(eval, &[])?,
        _value => args.first().copied().unwrap_or(Value::NIL),
    };
    let Some(wid) = window_arg.as_window_id() else {
        return Ok(Value::NIL);
    };
    let window_id = neovm_edit::emacs_core::display_metrics::WindowId(wid);
    if crate::emacs_core::eval::gnu_redisplay_hooks_enabled() {
        return redisplay_gnu::run_eager_scroll(eval, window_id);
    }
    let frame_id = eval.frames.find_window_frame_id(window_id).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), window_arg],
        )
    })?;
    let window_start = crate::emacs_core::window_cmds::builtin_window_start(eval, &[window_arg])?;
    let hook_sym = hook_runtime::hook_symbol_by_id(eval, WINDOW_SCROLL_FUNCTIONS.id());
    let saved_buffer_id = eval.edit.buffers.current_buffer_id();
    if let Some(buffer_id) = window_buffer_id_in_state(eval, frame_id, window_id) {
        // GNU `set_window_buffer` temporarily enters the displayed buffer
        // with `Fset_buffer` before running `window-scroll-functions`
        // (window.c), which does not call `record_buffer` or change
        // `buffer-list` recency.
        let _ = eval.set_current_buffer_unrecorded(buffer_id);
    }
    let hook_value = hook_runtime::hook_value_by_id(eval, hook_sym).unwrap_or(Value::NIL);
    let result = hook_runtime::run_hook_value(
        eval,
        hook_sym,
        hook_value,
        &[window_arg, window_start],
        true,
    );
    if let Some(buffer_id) = saved_buffer_id {
        eval.restore_current_buffer_if_live(buffer_id);
    }
    result
}

#[derive(Clone, Copy)]
struct HookCallerContextState {
    selected_frame_id: Option<neovm_value::display_vocab::FrameId>,
    selected_window_id: Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    current_buffer_id: Option<crate::buffer::BufferId>,
}

#[derive(Clone, Copy)]
struct LiveWindowHookState {
    window_id: neovm_edit::emacs_core::display_metrics::WindowId,
    buffer_id: crate::buffer::BufferId,
    bounds: neovm_edit::emacs_core::display_metrics::Rect,
}

#[derive(Clone)]
struct FrameWindowHookPlan {
    frame_id: neovm_value::display_vocab::FrameId,
    frame_buffer_change: bool,
    frame_size_change: bool,
    frame_selected_window_change: bool,
    frame_state_change: bool,
    /// GNU `run_window_change_functions` runs
    /// `run_window_configuration_change_hook' when a frame's configuration
    /// changed, i.e. `frame_size_change || window_deleted'
    /// (`src/window.c:4308-4312`).
    frame_configuration_change: bool,
    local_buffer_windows: Vec<neovm_edit::emacs_core::display_metrics::WindowId>,
    local_size_windows: Vec<neovm_edit::emacs_core::display_metrics::WindowId>,
    local_selection_windows: Vec<neovm_edit::emacs_core::display_metrics::WindowId>,
    local_state_windows: Vec<neovm_edit::emacs_core::display_metrics::WindowId>,
}

static WINDOW_BUFFER_CHANGE_FUNCTIONS: WellKnown = WellKnown::WindowBufferChangeFunctions;

static WINDOW_SIZE_CHANGE_FUNCTIONS: WellKnown = WellKnown::WindowSizeChangeFunctions;

static WINDOW_SELECTION_CHANGE_FUNCTIONS: WellKnown = WellKnown::WindowSelectionChangeFunctions;

static WINDOW_STATE_CHANGE_FUNCTIONS: WellKnown = WellKnown::WindowStateChangeFunctions;

static WINDOW_STATE_CHANGE_HOOK: WellKnown = WellKnown::WindowStateChangeHook;

static WINDOW_CONFIGURATION_CHANGE_HOOK: WellKnown = WellKnown::WindowConfigurationChangeHook;

static WINDOW_SCROLL_FUNCTIONS: WellKnown = WellKnown::WindowScrollFunctions;

static INHIBIT_REDISPLAY: WellKnown = WellKnown::InhibitRedisplay;
