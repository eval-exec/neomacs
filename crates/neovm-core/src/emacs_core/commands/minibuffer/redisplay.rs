//! Exact publication owners for the reader's split-borrow minibuffer paths.
//!
//! These cold transitions own an exclusive Context. Temporary frame/window
//! IDs and numeric snapshots carry no Lisp references and are never shared
//! across mutators. All Lisp executes through the Context after releasing
//! window/buffer borrows; no TLS or process-shared Lisp cache is introduced.
use super::*;

use crate::emacs_core::intern::intern;
use crate::emacs_core::reader::ActiveMinibufferWindowState;

#[cold]
#[inline(never)]
pub(super) fn activate(
    eval: &mut crate::emacs_core::eval::Context,
    minibuf_id: crate::buffer::BufferId,
    entry_level: neovm_value::minibuffer_state::MinibufferEntryLevel,
) -> Option<ActiveMinibufferWindowState> {
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    let caller = crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(frames, buffers);
    let frame = eval.frame_manager().get(caller)?;
    let old = frame.selected_window;
    let window = frame.minibuffer_window?;
    let owner = eval.frame_manager().find_window_frame_id(window)?;
    eval.frame_manager().get(owner)?.find_window(window)?;
    // GNU set_window_buffer first; selecting an already selected window
    // records it but publishes no new selection target.
    eval.gnu_mark_window_mode_line(window);
    if old != window {
        eval.gnu_mark_selection(Some(old), window, true);
    }
    publish_tty_top_change(eval, owner);
    let (frames, buffers, minibuffer_selected_window, active_minibuffer_window) =
        eval.frame_manager_buffers_and_minibuffer_selection_mut();
    let saved = activate_minibuffer_window_in_state(
        frames,
        buffers,
        minibuffer_selected_window,
        active_minibuffer_window,
        minibuf_id,
        entry_level,
    )?;
    if let Some(window) = eval
        .frame_manager_mut()
        .get_mut(owner)
        .and_then(|frame| frame.find_window_mut(window))
    {
        if let crate::window::Window::Leaf { hscroll, .. } = window {
            *hscroll = 0;
        }
        window.set_suspend_auto_hscroll(false);
    }
    Some(saved)
}

/// GNU do_switch_frame's NORECORD=t path: SOME without per-window marks.
#[cold]
#[inline(never)]
pub(crate) fn publish_frame_switch(
    eval: &mut crate::emacs_core::eval::Context,
    frame: neovm_value::display_vocab::FrameId,
) {
    if !crate::emacs_core::eval::gnu_redisplay_hooks_enabled()
        || eval
            .frame_manager()
            .selected_frame()
            .is_some_and(|selected| selected.id == frame)
    {
        return;
    }
    if let Some(new) = eval
        .frame_manager()
        .get(frame)
        .map(|frame| frame.selected_window)
    {
        let old = eval
            .frame_manager()
            .selected_frame()
            .map(|frame| frame.selected_window);
        if old != Some(new) {
            eval.gnu_mark_selection(old, new, false);
        }
        publish_tty_top_change(eval, frame);
    }
}

/// A new terminal top owns frame repaint obligations for the selected frame
/// and each ancestor (GNU frame.c:1985-2010); no topology/global ALL event.
#[cold]
#[inline(never)]
fn publish_tty_top_change(
    eval: &mut crate::emacs_core::eval::Context,
    frame: neovm_value::display_vocab::FrameId,
) {
    let Some(target) = eval.frame_manager().get(frame) else {
        return;
    };
    if target.effective_window_system().is_some() {
        return;
    }
    let terminal = target.terminal_id;
    let Some(root) = eval.frame_manager().root_frame_id(frame) else {
        return;
    };
    if eval.frame_manager().top_frame_on_terminal(terminal) == Some(root) {
        return;
    }
    if let Some(root) = eval.frame_manager_mut().get_mut(root) {
        root.visibility = neovm_value::display_vocab::FrameVisibility::Visible;
    }
    let mut current = Some(frame);
    let mut seen = std::collections::HashSet::new();
    while let Some(frame) = current {
        if !seen.insert(frame) {
            break;
        }
        eval.gnu_mark_frame_redisplay(frame);
        eval.gnu_request_frame_redraw(frame);
        current = eval.frame_manager().frame_parent_id(frame);
    }
    eval.request_menu_bar_rebuild(crate::emacs_core::eval::MenuBarRebuildReason::FullFrameRedraw);
}

/// Publish only a real erasure/overlay deletion, before mutating shared text.
#[cold]
#[inline(never)]
pub(crate) fn publish_expired_buffer(
    eval: &mut crate::emacs_core::eval::Context,
    buffer: crate::buffer::BufferId,
) {
    if crate::emacs_core::eval::gnu_redisplay_hooks_enabled()
        && eval.edit.buffers.get(buffer).is_some_and(|buffer| {
            buffer.total_char_len().get() != 0 || !buffer.overlays().is_empty()
        })
    {
        eval.gnu_mark_buffer_redisplay(buffer);
    }
}

#[cold]
#[inline(never)]
fn restore_selection(
    eval: &mut crate::emacs_core::eval::Context,
    saved: ActiveMinibufferWindowState,
) {
    // Nonselected frame-selected-window assignments are pure fset operations;
    // only assignments on the globally selected frame invoke Fselect_window.
    let selected = eval
        .frame_manager()
        .selected_frame()
        .map(|frame| (frame.id, frame.selected_window));
    let mut old = selected.map(|(_, window)| window);
    if selected.is_some_and(|(frame, _)| frame == saved.minibuffer_frame.0)
        && eval
            .frame_manager()
            .get(saved.minibuffer_frame.0)
            .and_then(|frame| frame.find_window(saved.previous_minibuffer_frame_selected_window))
            .is_some()
    {
        let new = saved.previous_minibuffer_frame_selected_window;
        if old != Some(new) {
            eval.gnu_mark_selection(old, new, true);
        }
        old = Some(new);
    }
    if selected.is_some_and(|(frame, _)| frame == saved.calling_frame.0)
        && eval
            .frame_manager()
            .get(saved.calling_frame.0)
            .and_then(|frame| frame.find_window(saved.calling_selected_window))
            .is_some()
        && old != Some(saved.calling_selected_window)
    {
        eval.gnu_mark_selection(old, saved.calling_selected_window, true);
    }
    if selected.is_none_or(|(frame, _)| frame != saved.calling_frame.0)
        && eval.frame_manager().get(saved.calling_frame.0).is_some()
    {
        eval.gnu_mark_selection(old, saved.calling_selected_window, false);
        publish_tty_top_change(eval, saved.calling_frame.0);
    }
    let (frames, minibuffer_selected_window, active_minibuffer_window) =
        eval.frame_manager_and_minibuffer_selection_mut();
    restore_minibuffer_selection_in_state(
        frames,
        minibuffer_selected_window,
        active_minibuffer_window,
        saved,
    );
}

/// GNU's separately registered minibuffer_unwind restores buffer/start/point
/// after configuration restoration. It also runs when sizing/inactive-mode
/// transfers before the read-unwind selection statements.
#[cold]
#[inline(never)]
pub(super) fn restore_buffer(
    eval: &mut crate::emacs_core::eval::Context,
    saved: ActiveMinibufferWindowState,
) -> MinibufferWindowRestoreEffect {
    let mut effect = MinibufferWindowRestoreEffect::NoBufferRestored;
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    if let Some(window) = frames
        .get_mut(saved.minibuffer_frame.0)
        .and_then(|frame| frame.find_window_mut(saved.minibuffer_window_id))
        && let Some(buffer) = saved.previous_minibuffer_buffer
    {
        window.set_buffer(buffer);
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
        effect = MinibufferWindowRestoreEffect::BufferRestored(saved.minibuffer_window_id);
    }
    *eval.minibuffer_selected_window_mut() = saved.previous_minibuffer_selected_window;
    *eval.active_minibuffer_window_mut() = saved.previous_active_minibuffer_window;
    effect
}

/// GNU read_minibuf_unwind erases, conditionally resizes, then calls inactive
/// mode. A captured Flow skips that call and read-unwind selection recording;
/// independent buffer/configuration cleanup finishes before Flow propagates.
#[cold]
#[inline(never)]
pub(super) fn teardown(
    eval: &mut crate::emacs_core::eval::Context,
    buffer: crate::buffer::BufferId,
    depth_after_pop: usize,
    saved: ActiveMinibufferWindowState,
) -> MinibufferTeardownOutcome {
    // Exit hooks may have selected another frame. GNU locates the expired
    // minibuffer and switches back to its owner before resetting its buffer
    // (minibuf.c:1131-1140), with NORECORD=t publication.
    if eval.frame_manager().get(saved.minibuffer_frame.0).is_some()
        && eval
            .frame_manager()
            .selected_frame()
            .is_none_or(|frame| frame.id != saved.minibuffer_frame.0)
    {
        publish_frame_switch(eval, saved.minibuffer_frame.0);
        let _ = eval
            .frame_manager_mut()
            .select_frame(saved.minibuffer_frame.0);
    }
    publish_expired_buffer(eval, buffer);
    erase_expired_minibuffer_buffer_in_state(&mut eval.edit.buffers, buffer);
    let inactive_mode_result = resize_empty(eval, buffer, depth_after_pop, saved)
        .and_then(|_| run_minibuffer_mode_if_bound(eval, "minibuffer-inactive-mode"));
    if inactive_mode_result.is_ok() {
        restore_selection(eval, saved);
    } else {
        // A transfer skips the remaining read-unwind selection statements.
        // The independent final buffer action and configurations still run.
        *eval.minibuffer_selected_window_mut() = saved.previous_minibuffer_selected_window;
        *eval.active_minibuffer_window_mut() = saved.previous_active_minibuffer_window;
    }
    let window_restore = MinibufferWindowRestoreEffect::NoBufferRestored;
    MinibufferTeardownOutcome {
        inactive_mode_result,
        window_restore,
    }
}

#[cold]
#[inline(never)]
fn resize_empty(
    eval: &mut crate::emacs_core::eval::Context,
    buffer: crate::buffer::BufferId,
    depth_after_pop: usize,
    saved: ActiveMinibufferWindowState,
) -> EvalResult {
    let future_owner = saved
        .previous_active_minibuffer_window
        .and_then(|window| eval.frame_manager().find_window_frame_id(window))
        .unwrap_or(saved.minibuffer_frame.0);
    if (depth_after_pop != 0
        && eval
            .frame_manager()
            .selected_frame()
            .is_some_and(|frame| frame.id == future_owner))
        || eval
            .special_variable_value_by_id(intern("inhibit-redisplay"))
            .is_some_and(|value| value.is_truthy())
    {
        return Ok(Value::NIL);
    }
    let Some(frame) = eval.frame_manager().get(saved.minibuffer_frame.0) else {
        return Ok(Value::NIL);
    };
    let mini_only = frame.minibuffer_window == Some(frame.root_window().id());
    let height = frame.char_height.max(1.0);
    if eval
        .special_variable_value_by_id(intern("redisplay-adhoc-scroll-in-resize-mini-windows"))
        .is_some_and(|value| value.is_truthy())
        && let Some(start) = eval
            .edit
            .buffers
            .get(buffer)
            .map(|buffer| buffer.point_min_lisp_char_pos())
    {
        eval.gnu_set_prepared_minibuffer_start(
            saved.minibuffer_frame.0,
            saved.minibuffer_window_id,
            start,
            false,
        );
    }
    if mini_only {
        return eval.gnu_resize_prepared_mini_frame(saved.minibuffer_frame.0);
    }
    if !eval
        .special_variable_value_by_id(intern("resize-mini-windows"))
        .is_some_and(|value| value.is_truthy())
    {
        return Ok(Value::NIL);
    }
    // The ordinary-window empty-source branch sets BEGV even when the
    // custom-tailored scrolling flag is nil (xdisp.c:13395-13404).
    if let Some(start) = eval
        .edit
        .buffers
        .get(buffer)
        .map(|buffer| buffer.point_min_lisp_char_pos())
    {
        eval.gnu_set_prepared_minibuffer_start(
            saved.minibuffer_frame.0,
            saved.minibuffer_window_id,
            start,
            false,
        );
    }
    eval.gnu_apply_prepared_minibuffer_resize(
        saved.minibuffer_frame.0,
        saved.minibuffer_window_id,
        height,
    )
}
