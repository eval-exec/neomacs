//! Minibuffer window session behaviour kept in the core command layer
//! (stage-4 E7 R0).
//!
//! The activation/restore/teardown machinery and the redisplay publication
//! bind minibuffer windows, frame selection and buffer state.  Under R0 they
//! stay core-side.  The unwind STATE types remain in the evaluator reader
//! module because the evaluator's specpdl unwind actions hold them by value;
//! this module names them downward through `crate::emacs_core::reader`.

use crate::buffer::LispCharPos1;
use crate::emacs_core::minibuffer::MinibufferManagerExt;
use crate::emacs_core::reader::{
    ActiveMinibufferWindowState, CallingFrame, MinibufferBufferUnwind, MinibufferExitDisposition,
    MinibufferOwnerFrame, MinibufferSessionUnwind,
};
use crate::window::DisplayFrameManagerExt;
use neovm_value::error::{EvalResult, Flow, FlowKind, FlowResultExt};
use neovm_value::value::*;

#[must_use = "a restored minibuffer window must invalidate its display chrome"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MinibufferWindowRestoreEffect {
    NoBufferRestored,
    BufferRestored(neovm_edit::emacs_core::display_metrics::WindowId),
}

impl MinibufferWindowRestoreEffect {
    pub(crate) fn apply(self, eval: &mut crate::emacs_core::eval::Context) {
        if let Self::BufferRestored(window) = self {
            // GNU `set_window_buffer` calls `wset_update_mode_line`; after
            // `read_minibuf_unwind` has restored the caller's selection this
            // mini-window is normally nonselected, so `wset_redisplay` also
            // raises `windows_or_buffers_changed` and rebuilds the menu bar.
            eval.gnu_mark_window_mode_line(window);
            eval.mark_chrome_dirty_window(window);
        }
    }
}

#[must_use = "minibuffer teardown effects must be applied before returning"]
pub(crate) struct MinibufferTeardownOutcome {
    pub(crate) inactive_mode_result: EvalResult,
    pub(crate) window_restore: MinibufferWindowRestoreEffect,
}

pub(crate) use crate::emacs_core::eval::data::minibuffer::MinibufferSelectedWindowUpdate;

pub(crate) fn activate_minibuffer_window_in_state(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    minibuffer_selected_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    active_minibuffer_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    minibuf_id: crate::buffer::BufferId,
    entry_level: neovm_value::minibuffer_state::MinibufferEntryLevel,
) -> Option<ActiveMinibufferWindowState> {
    let calling_frame = CallingFrame(
        crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(frames, buffers),
    );
    let calling_frame_state = frames.get(calling_frame.0)?;
    let minibuffer_window_id = calling_frame_state.minibuffer_window?;
    let calling_selected_window = calling_frame_state.selected_window;
    let minibuffer_frame = MinibufferOwnerFrame(frames.find_window_frame_id(minibuffer_window_id)?);
    let minibuffer_frame_state = frames.get(minibuffer_frame.0)?;
    let previous_minibuffer_frame_selected_window = minibuffer_frame_state.selected_window;
    let mut previous_minibuffer_buffer = None;
    let mut previous_minibuffer_window_start = LispCharPos1::ONE;
    let mut previous_minibuffer_point = LispCharPos1::ONE;
    if let Some(crate::window::Window::Leaf {
        buffer_id,
        window_start,
        point,
        ..
    }) = minibuffer_frame_state.find_window(minibuffer_window_id)
    {
        previous_minibuffer_buffer = Some(*buffer_id);
        previous_minibuffer_window_start = *window_start;
        previous_minibuffer_point = *point;
    }

    let saved = ActiveMinibufferWindowState {
        minibuffer_frame,
        minibuffer_window_id,
        calling_frame,
        calling_selected_window,
        previous_minibuffer_frame_selected_window,
        previous_minibuffer_buffer,
        previous_minibuffer_window_start,
        previous_minibuffer_point,
        previous_minibuffer_selected_window: *minibuffer_selected_window,
        previous_active_minibuffer_window: *active_minibuffer_window,
    };

    // GNU saves the caller's live BUF_PT into its window point marker before
    // selecting the minibuffer.  The caller becomes non-selected during the
    // read, so redisplay and mode-line `%l` must read that saved point rather
    // than the window's stale construction-time marker.
    crate::emacs_core::window_cmds::remember_selected_window_point_in_state(
        frames,
        buffers,
        calling_frame.0,
    );
    if let Some(frame) = frames.get_mut(minibuffer_frame.0) {
        if let Some(window) = frame.find_window_mut(minibuffer_window_id) {
            window.set_buffer(minibuf_id);
            debug_assert_eq!(window.buffer_id(), Some(minibuf_id));
            crate::window::window_markers::attach_window_position_markers(buffers, window);
        }
        let _ = frame.select_window(minibuffer_window_id);
    }
    let _ = frames.select_frame(minibuffer_frame.0);
    buffers.switch_current(minibuf_id);
    MinibufferSelectedWindowUpdate::for_entry(
        entry_level,
        calling_selected_window,
        minibuffer_window_id,
    )
    .apply(minibuffer_selected_window);
    *active_minibuffer_window = Some(minibuffer_window_id);
    Some(saved)
}

pub(crate) fn record_active_minibuffer_selection(
    eval: &mut crate::emacs_core::eval::Context,
    active: ActiveMinibufferWindowState,
    minibuf_id: crate::buffer::BufferId,
) -> EvalResult {
    let _ = eval
        .frame_manager_mut()
        .note_window_selected(active.minibuffer_window_id);
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    crate::emacs_core::window_cmds::record_buffer_in_state(
        frames,
        buffers,
        minibuf_id,
        active.minibuffer_frame.0,
    )?;
    if !eval.edit.buffers.buffer_hooks_inhibited(minibuf_id) {
        crate::emacs_core::builtins::run_buffer_list_update_hook(eval)?;
    }
    Ok(Value::NIL)
}

fn record_restored_calling_window_selection(
    eval: &mut crate::emacs_core::eval::Context,
    active: ActiveMinibufferWindowState,
) -> EvalResult {
    let Some(buffer_id) = eval
        .frame_manager()
        .get(active.calling_frame.0)
        .and_then(|frame| frame.find_window(active.calling_selected_window))
        .and_then(crate::window::Window::buffer_id)
    else {
        return Ok(Value::NIL);
    };
    let _ = eval
        .frame_manager_mut()
        .note_window_selected(active.calling_selected_window);
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    crate::emacs_core::window_cmds::record_buffer_in_state(
        frames,
        buffers,
        buffer_id,
        active.calling_frame.0,
    )?;
    if !eval.edit.buffers.buffer_hooks_inhibited(buffer_id) {
        crate::emacs_core::builtins::run_buffer_list_update_hook(eval)?;
    }
    Ok(Value::NIL)
}

pub(crate) fn activate_minibuffer_window(
    eval: &mut crate::emacs_core::eval::Context,
    minibuf_id: crate::buffer::BufferId,
    entry_level: neovm_value::minibuffer_state::MinibufferEntryLevel,
) -> Option<ActiveMinibufferWindowState> {
    if crate::emacs_core::eval::gnu_redisplay_hooks_enabled() {
        return super::redisplay::activate(eval, minibuf_id, entry_level);
    }
    let (frames, buffers, minibuffer_selected_window, active_minibuffer_window) =
        eval.frame_manager_buffers_and_minibuffer_selection_mut();
    activate_minibuffer_window_in_state(
        frames,
        buffers,
        minibuffer_selected_window,
        active_minibuffer_window,
        minibuf_id,
        entry_level,
    )
}

#[cold]
#[inline(never)]
pub(crate) fn unwind_minibuffer_buffer(
    eval: &mut crate::emacs_core::eval::Context,
    state: MinibufferBufferUnwind,
) -> EvalResult {
    if let Some(saved) = state.saved {
        super::redisplay::restore_buffer(eval, saved).apply(eval);
    }
    Ok(Value::NIL)
}

pub(crate) fn restore_minibuffer_window_in_state(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    minibuffer_selected_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    active_minibuffer_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    saved: ActiveMinibufferWindowState,
) -> MinibufferWindowRestoreEffect {
    let mut window_restore = MinibufferWindowRestoreEffect::NoBufferRestored;
    if let Some(frame) = frames.get_mut(saved.minibuffer_frame.0) {
        if let Some(window) = frame.find_window_mut(saved.minibuffer_window_id)
            && let Some(prev_buffer_id) = saved.previous_minibuffer_buffer
        {
            window.set_buffer(prev_buffer_id);
            window_restore =
                MinibufferWindowRestoreEffect::BufferRestored(saved.minibuffer_window_id);
            debug_assert_eq!(window.buffer_id(), Some(prev_buffer_id));
            crate::window::window_markers::attach_window_position_markers(buffers, window);
            crate::window::window_markers::set_window_start_with_marker(
                buffers,
                window,
                saved.previous_minibuffer_window_start,
            );
            crate::window::window_markers::set_window_point_with_marker(
                buffers,
                window,
                saved.previous_minibuffer_point,
            );
        }
    }
    restore_minibuffer_selection_in_state(
        frames,
        minibuffer_selected_window,
        active_minibuffer_window,
        saved,
    );
    window_restore
}

pub(crate) fn restore_minibuffer_selection_in_state(
    frames: &mut crate::window::FrameManager,
    minibuffer_selected_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    active_minibuffer_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    saved: ActiveMinibufferWindowState,
) {
    if let Some(frame) = frames.get_mut(saved.minibuffer_frame.0) {
        let _ = frame.select_window(saved.previous_minibuffer_frame_selected_window);
    }
    if let Some(frame) = frames.get_mut(saved.calling_frame.0) {
        let _ = frame.select_window(saved.calling_selected_window);
    }
    if frames.get(saved.calling_frame.0).is_some()
        && frames
            .selected_frame()
            .is_none_or(|frame| frame.id != saved.calling_frame.0)
    {
        let _ = frames.select_frame(saved.calling_frame.0);
    }
    *minibuffer_selected_window = saved.previous_minibuffer_selected_window;
    *active_minibuffer_window = saved.previous_active_minibuffer_window;
}

pub(crate) fn erase_expired_minibuffer_buffer_in_state(
    buffers: &mut crate::buffer::BufferManager,
    minibuf_id: crate::buffer::BufferId,
) {
    // GNU `read_minibuf_unwind` (minibuf.c:1181) erases the expired buffer's
    // text, and its companion `get_minibuffer` reuse path (minibuf.c:1062-1063)
    // drops the buffer's overlays. neomacs previously erased text only, so a
    // vertico candidate `after-string` overlay anchored on ` *Minibuf-N*`
    // survived teardown and kept the mini-window measuring as multi-line. Delete
    // the overlays here so the expired buffer is fully reset (text + overlays).
    let _ = buffers.delete_all_buffer_overlays(minibuf_id);
    let _ = buffers.replace_buffer_contents(minibuf_id, "");
}

#[allow(clippy::too_many_arguments)]
fn teardown_minibuffer_level_in_state(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    minibuffer_selected_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    active_minibuffer_window: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    minibuf_id: crate::buffer::BufferId,
    depth_after_pop: usize,
    saved: ActiveMinibufferWindowState,
    run_inactive_mode: impl FnOnce() -> EvalResult,
) -> MinibufferTeardownOutcome {
    let teardown_frame_id = saved.minibuffer_frame.0;

    // (R1) Completely reset the expired *Minibuf-N* (overlays + text), then run
    // minibuffer-inactive-mode.
    erase_expired_minibuffer_buffer_in_state(buffers, minibuf_id);
    let inactive_mode_result = run_inactive_mode();

    // (R2) Restore the mini-window's buffer to *Minibuf-0* / the prev buffer.
    let window_restore = restore_minibuffer_window_in_state(
        frames,
        buffers,
        minibuffer_selected_window,
        active_minibuffer_window,
        saved,
    );

    // (R3 + R4) At the outermost level, force the mini-window back to one line
    // and invalidate its cached matrix so the engine cannot reuse the stale
    // row count. Guarded by depth==0 so a nested minibuffer popping back to an
    // outer (still active) one does not collapse the outer minibuffer's window.
    if depth_after_pop == 0 {
        frames.force_resize_mini_window_to_one_line(teardown_frame_id);
    }

    MinibufferTeardownOutcome {
        inactive_mode_result,
        window_restore,
    }
}

pub(crate) fn unwind_minibuffer_session(
    shared: &mut crate::emacs_core::eval::Context,
    state: MinibufferSessionUnwind,
) -> EvalResult {
    let restored_calling_selection = state.active_window_state;
    let exit_hook_result = match shared.run_hook_if_bound("minibuffer-exit-hook").kinded() {
        Ok(value) => Ok(value),
        Err(FlowKind::Signal(_)) => Ok(Value::NIL),
        Err(flow) => Err(Flow::from_kind(flow)),
    };

    if shared.minibuffers().depth() > state.depth_before_entry {
        match state.disposition {
            MinibufferExitDisposition::Accepted => {
                let _ = shared.minibuffers_mut().exit_minibuffer();
            }
            MinibufferExitDisposition::Pending | MinibufferExitDisposition::Aborted => {
                shared.minibuffers_mut().abort_minibuffer();
            }
        }
    }
    // Defensive recovery for a future nested setup path: specpdl actions
    // unwind LIFO, so normally the loop executes zero times after the pop
    // above.  The invariant on return is the captured entry depth.
    while shared.minibuffers().depth() > state.depth_before_entry {
        shared.minibuffers_mut().abort_minibuffer();
    }

    let teardown_outcome = if let Some(saved) = state.active_window_state {
        let _ = shared
            .edit
            .buffers
            .switch_current_unrecorded(state.minibuf_id);
        if crate::emacs_core::eval::gnu_redisplay_hooks_enabled() {
            let depth_after_pop = shared.minibuffers().depth();
            super::redisplay::teardown(shared, state.minibuf_id, depth_after_pop, saved)
        } else {
            let depth_after_pop = shared.minibuffers().depth();
            let shared_ptr = std::ptr::NonNull::from(&mut *shared);
            let (frames, buffers, minibuffer_selected_window, active_minibuffer_window) =
                shared.frame_manager_buffers_and_minibuffer_selection_mut();
            teardown_minibuffer_level_in_state(
                frames,
                buffers,
                minibuffer_selected_window,
                active_minibuffer_window,
                state.minibuf_id,
                depth_after_pop,
                saved,
                move || unsafe {
                    run_minibuffer_mode_if_bound(
                        shared_ptr.as_ptr().as_mut().unwrap(),
                        "minibuffer-inactive-mode",
                    )
                },
            )
        }
    } else {
        super::redisplay::publish_expired_buffer(shared, state.minibuf_id);
        erase_expired_minibuffer_buffer_in_state(&mut shared.edit.buffers, state.minibuf_id);
        MinibufferTeardownOutcome {
            inactive_mode_result: run_minibuffer_mode_if_bound(shared, "minibuffer-inactive-mode"),
            window_restore: MinibufferWindowRestoreEffect::NoBufferRestored,
        }
    };
    let redirect_result = crate::emacs_core::frame::sync_gui_frame_focus_redirects(shared);
    teardown_outcome.window_restore.apply(shared);
    let inactive_mode_result = teardown_outcome.inactive_mode_result;

    if let Some(buffer_id) = state.saved_buffer_id
        && shared.edit.buffers.get(buffer_id).is_some()
    {
        shared.edit.buffers.switch_current(buffer_id);
    }
    // A sizing/inactive-mode transfer skips the remaining read_minibuf_unwind
    // statements. Separate configuration and minibuffer restoration still run.
    // A failed focus-redirect sync likewise skips the selection record.
    let selection_record_result = if redirect_result.is_err()
        || (crate::emacs_core::eval::gnu_redisplay_hooks_enabled() && inactive_mode_result.is_err())
    {
        Ok(Value::NIL)
    } else {
        restored_calling_selection
            .map(|active| record_restored_calling_window_selection(shared, active))
            .unwrap_or(Ok(Value::NIL))
    };
    shared.store_variable_by_name(
        "minibuffer-depth",
        Value::fixnum(shared.minibuffers().depth() as i64),
    );
    shared.store_variable_by_name("current-prefix-arg", state.saved_current_prefix_arg);
    shared.store_variable_by_name(
        "minibuffer-history-variable",
        state.saved_minibuffer_history_variable,
    );
    shared.store_variable_by_name(
        "minibuffer-history-position",
        state.saved_minibuffer_history_position,
    );
    shared.set_command_key_sequences(state.saved_command_keys, state.saved_raw_command_keys);

    tracing::debug!(
        "read-from-minibuffer: unwound current_buffer={:?} depth={} active_window={:?} selected_window={:?}",
        shared.edit.buffers.current_buffer_id(),
        shared.minibuffers().depth(),
        shared.active_minibuffer_window(),
        shared
            .frame_manager()
            .selected_frame()
            .map(|frame| frame.selected_window)
    );

    exit_hook_result?;
    inactive_mode_result?;
    redirect_result?;
    selection_record_result?;
    Ok(Value::NIL)
}

pub(crate) fn find_or_create_minibuffer_buffer_in_state(
    buffers: &mut crate::buffer::BufferManager,
    depth: usize,
) -> crate::buffer::BufferId {
    let minibuf_name = format!(" *Minibuf-{depth}*");
    let minibuf_id = match buffers.find_buffer_by_name(&minibuf_name) {
        Some(existing) => {
            // GNU `get_minibuffer` (minibuf.c:1062-1063) resets every reused
            // minibuffer pool buffer with `delete_all_overlays + reset_buffer`
            // so a new activation never inherits stale overlays or text from a
            // prior (possibly aborted) session. Mirror that defense-in-depth
            // here on the reuse branch: even if a teardown was skipped, the
            // buffer starts clean.
            let _ = buffers.delete_all_buffer_overlays(existing);
            let _ = buffers.replace_buffer_contents(existing, "");
            existing
        }
        None => buffers.create_buffer(&minibuf_name),
    };
    let _ = buffers.configure_buffer_undo_list(minibuf_id, Value::NIL);
    let _ = buffers.set_buffer_local_property(minibuf_id, "truncate-lines", Value::NIL);
    minibuf_id
}

pub(crate) fn minibuffer_ambient_directory_in_state(
    buffers: &crate::buffer::BufferManager,
) -> Option<Value> {
    let current = buffers.current_buffer_id();
    current
        .and_then(|id| buffers.get(id))
        .and_then(|buffer| buffer.buffer_local_value("default-directory"))
        .filter(|value| value.is_string())
        .or_else(|| {
            buffers.buffer_list().into_iter().find_map(|id| {
                buffers
                    .get(id)
                    .and_then(|buffer| buffer.buffer_local_value("default-directory"))
                    .filter(|value| value.is_string())
            })
        })
}

pub(crate) fn install_minibuffer_ambient_directory_in_state(
    buffers: &mut crate::buffer::BufferManager,
    minibuf_id: crate::buffer::BufferId,
    ambient_directory: Option<Value>,
) {
    if let Some(directory) = ambient_directory {
        let _ = buffers.set_buffer_local_property(minibuf_id, "default-directory", directory);
    }
}

pub(crate) fn run_minibuffer_mode_if_bound(
    eval: &mut crate::emacs_core::eval::Context,
    mode: &str,
) -> EvalResult {
    if eval.obarray().symbol_function(mode).is_some() {
        eval.apply0(Value::symbol(mode))
    } else {
        Ok(Value::NIL)
    }
}
