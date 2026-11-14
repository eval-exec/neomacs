//! `kill-buffer` and its hook and window-repair phases.
//!
//! This Lisp entry binds green-thread scheduling, the minibuffer window,
//! frame/window state and the caller's excursion, so it stays in the core
//! command layer under the R0 per-function placement rule; the buffer bodies
//! it drives stay in the editor layer.

use crate::buffer::BufferId;
use crate::emacs_core::buffer::{
    expect_buffer_name_string, other_buffer_safely_in_state, run_buffer_list_update_hook,
};
use crate::emacs_core::buffer_ops::other_buffer_impl_in_state;
use crate::emacs_core::error::{EvalResult, Flow, LispCondition, expect_max_args, signal};
use crate::emacs_core::process::ProcessContextExt;
use crate::emacs_core::value::{Value, ValueKind, VecLikeType, eq_value};
use crate::window::DisplayFrameExt;
use crate::window::DisplayFrameManagerExt;
use crate::window::FrameManager;
use neovm_edit::emacs_core::edit_host::EditHost;

fn delete_quit_restore_popup_windows_showing_buffer(
    frames: &mut FrameManager,
    buffer_id: BufferId,
) -> bool {
    let mut deleted_any = false;
    let quit_restore_key = Value::symbol("quit-restore");
    let buffer_value = Value::make_buffer(buffer_id);

    for frame_id in frames.frame_list() {
        let Some(window_ids) = frames.get(frame_id).map(|frame| frame.window_list()) else {
            continue;
        };

        for window_id in window_ids {
            let should_delete = {
                let Some(frame) = frames.get(frame_id) else {
                    continue;
                };
                if frame.minibuffer_window == Some(window_id)
                    || frame.window_list().len() <= 1
                    || frame
                        .find_window(window_id)
                        .and_then(|window| window.buffer_id())
                        != Some(buffer_id)
                {
                    false
                } else {
                    match frames.window_parameter(window_id, &quit_restore_key) {
                        Some(quit_restore) => {
                            match crate::emacs_core::value::list_to_vec(&quit_restore) {
                                Some(items) => {
                                    items.len() >= 4
                                        && items[0].as_bare_symbol_name() == Some("window")
                                        && items[1].as_bare_symbol_name() == Some("window")
                                        && eq_value(&items[3], &buffer_value)
                                }
                                None => false,
                            }
                        }
                        None => false,
                    }
                }
            };

            if should_delete && frames.delete_window(frame_id, window_id) {
                deleted_any = true;
            }
        }
    }

    deleted_any
}

fn sync_current_buffer_to_selected_window(eval: &mut crate::emacs_core::eval::Context) {
    let Some(frame_id) = eval.frame_manager().selected_frame().map(|frame| frame.id) else {
        return;
    };
    let selected_buffer_id = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(frame.selected_window))
        .and_then(|window| window.buffer_id());
    if let Some(buffer_id) = selected_buffer_id {
        let _ = eval.edit.buffers.switch_current_unrecorded(buffer_id);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KillBufferHooks {
    /// Go on killing the buffer (unless a hook already killed it).
    Proceed,
    /// A `kill-buffer-query-functions` member returned nil.
    Refused,
}

/// The hook phase of GNU `Fkill_buffer` (buffer.c:1936-1986), run with BUFFER
/// selected; the caller owns the excursion that restores the caller's state.
fn run_kill_buffer_hooks(
    eval: &mut crate::emacs_core::eval::Context,
    buffer: BufferId,
    inhibit_buffer_hooks: bool,
) -> Result<KillBufferHooks, Flow> {
    // GNU selects with `set_buffer_internal`, not `record_buffer`: killing
    // or querying a buffer must not make it the head of `buffer-list`.
    eval.set_current_buffer_unrecorded(buffer)?;
    if !inhibit_buffer_hooks {
        let query_sym = crate::emacs_core::hook_runtime::hook_symbol_by_name(
            eval,
            "kill-buffer-query-functions",
        );
        let query_value = crate::emacs_core::hook_runtime::hook_value_by_id(eval, query_sym)
            .unwrap_or(Value::NIL);
        let answer = crate::emacs_core::hook_runtime::run_hook_value_until_failure(
            eval,
            query_sym,
            query_value,
            &[],
            true,
        )?;
        if answer.is_nil() {
            return Ok(KillBufferHooks::Refused);
        }
    }
    // "If the hooks have killed the buffer, exit now."
    if eval.edit.buffers.get(buffer).is_none() {
        return Ok(KillBufferHooks::Proceed);
    }
    if !inhibit_buffer_hooks {
        let hook_sym =
            crate::emacs_core::hook_runtime::hook_symbol_by_name(eval, "kill-buffer-hook");
        let hook_value =
            crate::emacs_core::hook_runtime::hook_value_by_id(eval, hook_sym).unwrap_or(Value::NIL);
        crate::emacs_core::hook_runtime::run_hook_value(eval, hook_sym, hook_value, &[], true)?;
    }
    Ok(KillBufferHooks::Proceed)
}

/// Dispatch a modified buffer's string truename without selecting it.
/// The caller passes the string borrowed from the original rooted Value.
#[cold]
#[inline(never)]
fn unlock_buffer_file_before_retirement(
    eval: &mut crate::emacs_core::eval::Context,
    filename: Value,
    string: &neovm_value::heap_types::LispString,
) -> Result<(), Flow> {
    // No buffer/manager borrow crosses the callback. Keep the original
    // Lisp string (including interval Values) rooted until dispatch ends.
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(filename);
    let result = neovm_eval::emacs_core::eval::lifecycle::unlock_file(eval, string);
    eval.restore_specpdl_roots(roots);
    result.map(|_| ())
}

pub(crate) fn builtin_kill_buffer(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("kill-buffer", &args, 1)?;
    let id = match args.first() {
        None => match eval.edit.buffers.current_buffer() {
            Some(buf) => buf.id(),
            None => return Ok(Value::NIL),
        },
        Some(arg) => match arg.kind() {
            ValueKind::Nil => match eval.edit.buffers.current_buffer() {
                Some(buf) => buf.id(),
                None => return Ok(Value::NIL),
            },
            ValueKind::Veclike(VecLikeType::Buffer) => {
                let bid = arg.as_buffer_id().unwrap();
                if eval.edit.buffers.get(bid).is_none() {
                    return Ok(Value::NIL);
                }
                bid
            }
            ValueKind::String => {
                let name = expect_buffer_name_string(arg)?;
                match eval.edit.buffers.find_buffer_by_name(&name) {
                    Some(id) => id,
                    None => {
                        return Err(signal(
                            "error",
                            vec![Value::string(format!("No buffer named {name}"))],
                        ));
                    }
                }
            }
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), *arg],
                ));
            }
        },
    };

    // GNU buffer.c:1927: another thread's current buffer that it keeps.
    if eval.green_buffer_held(id) {
        return Ok(Value::NIL);
    }
    let inhibit_buffer_hooks = eval.edit.buffers.buffer_hooks_inhibited(id);
    // GNU `Fkill_buffer` (buffer.c:1931-1987) runs the query functions and
    // `kill-buffer-hook` with the dying buffer current inside ONE
    // `record_unwind_protect_excursion`, so the caller's buffer and its
    // point come back on every exit, a hook signal included.
    let mut hooks = KillBufferHooks::Proceed;
    let mut excursion = crate::emacs_core::eval::ExcursionScope::enter(eval);
    let result =
        run_kill_buffer_hooks(excursion.context(), id, inhibit_buffer_hooks).map(|outcome| {
            hooks = outcome;
            Value::NIL
        });
    excursion.finish(result)?;
    match hooks {
        KillBufferHooks::Refused => return Ok(Value::NIL),
        KillBufferHooks::Proceed => {}
    }
    if eval.edit.buffers.get(id).is_none() {
        return Ok(Value::T);
    }

    // GNU buffer.c:1998-2000: "Don't kill the minibuffer now current."
    // Killing the buffer displayed in `minibuf_window` would leave an
    // ordinary buffer there; GNU refuses after the hooks have run.
    if eval
        .minibuf_window_id()
        .and_then(|window| eval.frame_manager().window_buffer_id(window))
        == Some(id)
    {
        return Ok(Value::NIL);
    }

    if eval
        .visible_variable_value_or_nil("kill-buffer-quit-windows")
        .is_truthy()
    {
        if delete_quit_restore_popup_windows_showing_buffer(eval.frame_manager_mut(), id) {
            sync_current_buffer_to_selected_window(eval);
        }
    } else {
        // GNU `Fkill_buffer` (buffer.c:2030) calls `replace-buffer-in-windows`
        // (window.el) while the buffer is still live.  For the default
        // `kill-buffer-quit-windows` = nil path that function deletes a
        // dedicated window showing the buffer when no previous buffer is
        // available (and deletes side windows likewise), rather than leaving
        // it live showing `*scratch*`.  Delegate to the Lisp function instead
        // of reimplementing it; the `*scratch*` swap below remains as the
        // safety fallback (GNU `replace_buffer_in_windows_safely`).
        if eval.obarray().fboundp("replace-buffer-in-windows") {
            eval.funcall_general(
                Value::symbol("replace-buffer-in-windows"),
                vec![Value::make_buffer(id)],
            )?;
        }
        if eval.edit.buffers.get(id).is_none() {
            return Ok(Value::T);
        }
    }

    // GNU buffer.c:2046-2051 selects away from the target before thread
    // cleanup, unlocking and process sentinels. Do not defer this selection
    // until retirement: an escaping unlock handler observes the live session.
    if eval.edit.buffers.current_buffer_id() == Some(id) {
        let other = {
            let (frames, buffers) = eval.frame_manager_and_buffers_mut();
            other_buffer_impl_in_state(frames, buffers, vec![Value::make_buffer(id)])?
        };
        if let Some(next) = other.as_buffer_id() {
            eval.set_current_buffer_unrecorded(next)?;
        }
        if eval.edit.buffers.current_buffer_id() == Some(id) {
            return Ok(Value::NIL);
        }
    }

    // GNU buffer.c:2055-2059: check the threads again, in case a hook
    // changed something, then move the other threads out of the buffers.
    if eval.green_threads_running() {
        if eval.green_buffer_held(id) {
            return Ok(Value::NIL);
        }
        for dying in eval
            .edit
            .buffers
            .collect_killed_buffer_ids(id)
            .unwrap_or_default()
        {
            eval.green_before_buffer_killed(dying)?;
        }
    }
    // GNU buffer.c:2063-2067: after the thread recheck, refuse when the
    // current buffer is shown in the minibuffer and target is its sole other.
    let current = eval.edit.buffers.current_buffer_id();
    let minibuffer = eval
        .minibuf_window_id()
        .and_then(|window| eval.frame_manager().window_buffer_id(window));
    if current.is_some() && current == minibuffer {
        let other = {
            let (frames, buffers) = eval.frame_manager_and_buffers_mut();
            other_buffer_impl_in_state(frames, buffers, vec![Value::make_buffer(id)])?
        };
        if other.as_buffer_id() == Some(id) {
            return Ok(Value::NIL);
        }
    }

    // GNU buffer.c:2072-2075: direct handler-aware unlock of the requested
    // buffer, before deleting its processes or resetting its local state.
    // This bypasses a replaced Lisp unlock-file/unlock-buffer function cell.
    // Keep the ordinary unmodified path inline: one semantic tick
    // comparison, without a helper call, root management or Result branch.
    let filename = eval.edit.buffers.get(id).and_then(|buffer| {
        (buffer.save_modified_tick() < buffer.modified_tick())
            .then(|| buffer.buffer_local_value("buffer-file-truename"))
            .flatten()
    });
    if let Some(filename) = filename
        && let Some(string) = filename.as_lisp_string()
    {
        unlock_buffer_file_before_retirement(eval, filename, string)?;
    }
    if eval.edit.buffers.get(id).is_none() {
        // GNU still runs kill_buffer_processes after a reentrant unlock
        // handler kills the target (buffer.c:2073-2081). Real processes keep
        // their buffer association after HUP, so the outer call must send
        // its own HUP too; do not skip it or collect a dead retirement set.
        eval.kill_buffer_processes(id)?;
        return Ok(Value::T);
    }

    let killed_ids_to_signal = eval
        .edit
        .buffers
        .collect_killed_buffer_ids(id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    for killed_id in killed_ids_to_signal {
        eval.kill_buffer_processes(killed_id)?;
    }
    if eval.edit.buffers.get(id).is_none() {
        return Ok(Value::T);
    }

    let buffer_defaults = eval.edit.buffers.buffer_defaults;
    let killed_ids_to_reset = eval
        .edit
        .buffers
        .collect_killed_buffer_ids(id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    {
        let (buffers, mut obarray_view) = eval.buffers_and_obarray_view_mut();
        // SAFETY: killed-buffer retirement only resets buffer/BLV value
        // caches and reads properties. All process/Lisp callbacks finished
        // above; no function or canonical descriptor writer runs in this phase.
        unsafe {
            obarray_view.with_legacy_value_write(|obarray| {
                for killed_id in &killed_ids_to_reset {
                    if let Some(buffer) = buffers.get_mut(*killed_id) {
                        buffer.kill_all_local_variables(obarray, true, &buffer_defaults);
                    }
                }
            });
        }
    }

    // All callback mutations are complete. Use the actual retirement result
    // below, not a set predicted before unlock or process sentinels ran.
    let current_before_retirement = eval.edit.buffers.current_buffer_id();
    let killed_ids = eval
        .edit
        .buffers
        .kill_buffer_collect(id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    if !eval.owner().is_dormant() {
        eval.owner_buffers_killed(&killed_ids);
    }

    // GNU `replace_buffer_in_windows_safely` (window.c:3742) repairs only
    // windows that still show the dead buffer after the Lisp
    // `replace-buffer-in-windows` pass above.  Do not materialize a fallback
    // buffer just because a buffer died: killing an unshown temporary buffer
    // must not create `*scratch*` (it would reappear in `list-buffers`).
    let mut dead_window_fallback = None;
    let mut retired_selected_current_replacement = None;
    for killed_id in &killed_ids {
        if eval.frame_manager().window_count_showing_buffer(*killed_id) == 0 {
            continue;
        }
        let fallback = match dead_window_fallback {
            Some(fallback) => fallback,
            None => {
                // GNU picks `other_buffer_safely` per repaired window; with
                // no hooks run by the raw binding replacement, one safe
                // fallback serves every such window.
                let fallback =
                    other_buffer_safely_in_state(&mut eval.edit.buffers, Some(*killed_id));
                dead_window_fallback = Some(fallback);
                fallback
            }
        };
        // TODO(peer-5728): this bulk-retired current indirect would have
        // been selected away during GNU's recursive child kill. Preserve the
        // actual selected-window replacement only when that window showed it;
        // an unrelated selected window must not override frame preferences.
        let replaces_selected_current = current_before_retirement == Some(*killed_id)
            && *killed_id != id
            && eval
                .frame_manager()
                .selected_window()
                .and_then(|window| window.buffer_id())
                == Some(*killed_id);
        eval.frame_manager_mut()
            .replace_buffer_in_windows(*killed_id, fallback);
        if replaces_selected_current {
            retired_selected_current_replacement = eval
                .frame_manager()
                .selected_window()
                .and_then(|window| window.buffer_id());
        }
    }

    // Discard killed buffers from every frame's buffer list and buried
    // buffer list (GNU frame.c:3757-3769 discards from each frame).
    for killed_id in &killed_ids {
        for fid in eval.frame_manager().frame_list() {
            if let Some(frame) = eval.frame_manager_mut().get_mut(fid) {
                frame.buffer_list.retain(|bid| *bid != *killed_id);
                frame.buried_buffer_list.retain(|bid| *bid != *killed_id);
            }
        }
    }

    if eval.edit.buffers.current_buffer_id().is_none() {
        match current_before_retirement {
            // TODO(peer-5728): remove this transitional repair when recursive
            // indirect kills and target-only mechanical retirement land.
            // The deferred bulk path can retire an indirect that a callback
            // selected. Only that actual non-target retirement may be repaired.
            Some(previous) if previous != id && killed_ids.contains(&previous) => {
                let fallback = match retired_selected_current_replacement {
                    Some(replacement) => replacement,
                    None => {
                        let other = {
                            let (frames, buffers) = eval.frame_manager_and_buffers_mut();
                            other_buffer_impl_in_state(
                                frames,
                                buffers,
                                vec![Value::make_buffer(previous)],
                            )?
                        };
                        other.as_buffer_id().unwrap_or_else(|| {
                            other_buffer_safely_in_state(&mut eval.edit.buffers, Some(previous))
                        })
                    }
                };
                eval.set_current_buffer_unrecorded(fallback)?;
            }
            previous => {
                tracing::error!(
                    target: "neomacs::kill_buffer",
                    buffer = ?id,
                    current_before_retirement = ?previous,
                    killed_ids = ?killed_ids,
                    "kill-buffer lost current selection outside deferred indirect retirement"
                );
                debug_assert!(
                    eval.edit.buffers.current_buffer_id().is_some(),
                    "kill-buffer lost current selection outside deferred indirect retirement"
                );
            }
        }
    }

    if !inhibit_buffer_hooks {
        run_buffer_list_update_hook(eval)?;
    }

    Ok(Value::T)
}
