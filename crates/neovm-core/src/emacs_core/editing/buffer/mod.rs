use super::builtins::*;
use crate::emacs_core::error::{
    expect_args, expect_args_range, expect_fixnum, expect_max_args, expect_min_args,
};

// ===========================================================================
// Buffer operations (require evaluator for BufferManager access)
// ===========================================================================

use crate::buffer::{
    Buffer, BufferId, BufferManager, CharLen, CharPos0, CharRange, EmacsByteLen, EmacsBytePos,
    EmacsByteRange, LispBytePos1, LispCharPos1, TextChange, TextEditRange, TextExtent,
    TextPositionAnchor,
};
use crate::emacs_core::filelock;
use crate::emacs_core::misc;
use crate::emacs_core::value::{
    ValueKind, VecLikeType, equal_value, get_string_text_properties_table_for_value,
    set_string_text_properties_table_for_value,
};
use crate::window::FrameManager;

#[derive(Clone, Copy)]
pub(crate) struct MakeIndirectBufferPlan {
    pub(crate) id: BufferId,
    pub(crate) saved_current: Option<BufferId>,
    pub(crate) run_clone_hook: bool,
}

pub(crate) fn expect_buffer_id(value: &Value) -> Result<BufferId, Flow> {
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => Ok(value.as_buffer_id().unwrap()),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), *value],
        )),
    }
}

fn expect_buffer_name_string(value: &Value) -> Result<String, Flow> {
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), *value],
            )
        })
}

fn find_buffer_by_name_arg(
    buffers: &BufferManager,
    value: &Value,
) -> Result<Option<BufferId>, Flow> {
    let name = expect_buffer_name_string(value)?;
    Ok(buffers.find_buffer_by_name(&name))
}

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
                                        && items[0].as_symbol_name() == Some("window")
                                        && items[1].as_symbol_name() == Some("window")
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

fn sync_current_buffer_to_selected_window(eval: &mut super::eval::Context) {
    let Some(frame_id) = eval.frames.selected_frame().map(|frame| frame.id) else {
        return;
    };
    let selected_buffer_id = eval
        .frames
        .get(frame_id)
        .and_then(|frame| frame.find_window(frame.selected_window))
        .and_then(|window| window.buffer_id());
    if let Some(buffer_id) = selected_buffer_id {
        let _ = eval.buffers.switch_current_unrecorded(buffer_id);
    }
}

pub(crate) fn point_char_pos(buf: &crate::buffer::Buffer, byte_pos: EmacsBytePos) -> i64 {
    buf.emacs_byte_pos_to_lisp_char_pos(byte_pos).as_i64()
}

fn char_pos_to_buffer_emacs_byte_pos(
    buf: &crate::buffer::Buffer,
    char_pos: CharPos0,
) -> EmacsBytePos {
    buf.char_pos_to_emacs_byte_pos_clamped(char_pos)
}

fn clamped_lisp_char_pos_to_char_pos(pos: LispCharPos1, max_chars: CharLen) -> CharPos0 {
    pos.to_char_pos().min(CharPos0::ZERO.add_len(max_chars))
}

pub(crate) fn normalize_narrow_region_in_buffers(
    buffers: &BufferManager,
    current_id: BufferId,
    start: LispCharPos1,
    end: LispCharPos1,
    start_arg: Value,
    end_arg: Value,
) -> Result<EmacsByteRange, Flow> {
    let buf = buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let mut s = start.as_i64();
    let mut e = end.as_i64();
    if e < s {
        std::mem::swap(&mut s, &mut e);
    }
    let full_min = 1_i64;
    let full_max = buf.z_lisp_char_pos().as_i64();
    if s < full_min || s > full_max || e < full_min || e > full_max {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![start_arg, end_arg],
        ));
    }
    if let Some(labeled) = buffers.current_labeled_restriction_char_bounds(current_id) {
        let labeled_min = labeled.start_lisp().as_i64();
        let labeled_max = labeled.end_lisp().as_i64();
        s = s.clamp(labeled_min, labeled_max);
        e = e.clamp(labeled_min, labeled_max);
    }
    let start_char = clamped_lisp_char_pos_to_char_pos(LispCharPos1::new(s), buf.total_char_len());
    let end_char = clamped_lisp_char_pos_to_char_pos(LispCharPos1::new(e), buf.total_char_len());
    Ok(EmacsByteRange::new(
        buf.char_pos_to_emacs_byte_pos_clamped(start_char),
        buf.char_pos_to_emacs_byte_pos_clamped(end_char),
    ))
}

pub(crate) fn expect_integer_or_marker_in_buffers(
    buffers: &BufferManager,
    value: &Value,
) -> Result<i64, Flow> {
    crate::emacs_core::position::fix_position_with_buffers(buffers, value)
}

fn canonicalize_or_self(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn run_buffer_list_update_hook(eval: &mut super::eval::Context) -> EvalResult {
    builtin_run_hooks(eval, vec![Value::symbol("buffer-list-update-hook")])
}

/// GNU `Fkill_all_local_variables` runs the normal hook
/// `change-major-mode-hook` via `run_hook` as its first action.  This is the
/// shared entry point used by every major-mode switch.
pub(crate) fn run_buffer_change_major_mode_hook(eval: &mut super::eval::Context) -> EvalResult {
    builtin_run_hooks(eval, vec![Value::symbol("change-major-mode-hook")])
}

pub(crate) fn builtin_get_buffer_create(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("get-buffer-create", &args, 1)?;
    expect_max_args("get-buffer-create", &args, 2)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => Ok(args[0]),
        _ => {
            let name = expect_string_lossy(&args[0])?;
            if let Some(id) = eval.buffers.find_buffer_by_name(&name) {
                Ok(Value::make_buffer(id))
            } else {
                let inhibit_buffer_hooks = args.get(1).is_some_and(|value| !value.is_nil());
                let id = eval
                    .buffers
                    .create_buffer_with_hook_inhibition(&name, inhibit_buffer_hooks);
                if !inhibit_buffer_hooks {
                    run_buffer_list_update_hook(eval)?;
                }
                Ok(Value::make_buffer(id))
            }
        }
    }
}

/// (make-indirect-buffer BASE-BUFFER NAME &optional CLONE INHIBIT-BUFFER-HOOKS) → buffer
pub(crate) fn builtin_make_indirect_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let plan = prepare_make_indirect_buffer_in_manager(&mut eval.buffers, args)?;
    finish_make_indirect_buffer_hooks(eval, plan)
}

pub(crate) fn prepare_make_indirect_buffer_in_manager(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> Result<MakeIndirectBufferPlan, Flow> {
    expect_args_range("make-indirect-buffer", &args, 2, 4)?;

    let base_id = match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = args[0].as_buffer_id().unwrap();
            if buffers.get(id).is_none() {
                return Err(signal(
                    "error",
                    vec![Value::string("Base buffer has been killed")],
                ));
            }
            id
        }
        ValueKind::String => {
            let name = expect_buffer_name_string(&args[0])?;
            buffers.find_buffer_by_name(&name).ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string(format!("No such buffer: `{name}`"))],
                )
            })?
        }
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };

    let name = expect_string_lossy(&args[1])?;
    if name.is_empty() {
        return Err(signal(
            "error",
            vec![Value::string("Empty string for buffer name is not allowed")],
        ));
    }
    if buffers.find_buffer_by_name(&name).is_some() {
        return Err(signal(
            "error",
            vec![Value::string(format!("Buffer name `{name}` is in use"))],
        ));
    }

    let clone = args.get(2).is_some_and(|value| !value.is_nil());
    let inhibit_buffer_hooks = args.get(3).is_some_and(|value| !value.is_nil());
    let id = buffers
        .create_indirect_buffer_with_hook_inhibition(base_id, &name, clone, inhibit_buffer_hooks)
        .ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Failed to create indirect buffer")],
            )
        })?;

    Ok(MakeIndirectBufferPlan {
        id,
        saved_current: buffers.current_buffer_id(),
        run_clone_hook: clone,
    })
}

pub(crate) fn finish_make_indirect_buffer_hooks(
    eval: &mut super::eval::Context,
    plan: MakeIndirectBufferPlan,
) -> EvalResult {
    if plan.run_clone_hook {
        eval.switch_current_buffer(plan.id)?;
        let clone_result =
            builtin_run_hooks(eval, vec![Value::symbol("clone-indirect-buffer-hook")]);
        if let Some(saved_id) = plan.saved_current {
            eval.restore_current_buffer_if_live(saved_id);
        }
        clone_result?;
    }
    if !eval.buffers.buffer_hooks_inhibited(plan.id) {
        run_buffer_list_update_hook(eval)?;
    }
    Ok(Value::make_buffer(plan.id))
}

pub(crate) fn builtin_get_buffer(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let buffers = &eval.buffers;
    expect_args("get-buffer", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => Ok(args[0]),
        ValueKind::String => Ok(find_buffer_by_name_arg(buffers, &args[0])?
            .map(Value::make_buffer)
            .unwrap_or(Value::NIL)),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )),
    }
}

pub(crate) fn builtin_find_buffer(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let obarray = eval.obarray();
    let dynamic: &[OrderedRuntimeBindingMap] = &[];
    let buffers = &eval.buffers;
    expect_args("find-buffer", &args, 2)?;
    let name = args[0].as_symbol_name().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        )
    })?;
    let target_value = args[1];

    let name_id = intern(name);
    let fallback_value = dynamic
        .iter()
        .rev()
        .find_map(|frame| frame.get(&name_id).cloned())
        .or_else(|| obarray.symbol_value(name).cloned())
        .ok_or_else(|| signal(LispCondition::VoidVariable, vec![Value::symbol(name)]))?;

    let mut scan_order = Vec::new();
    let current_id = buffers.current_buffer().map(|buf| buf.id);
    if let Some(id) = current_id {
        scan_order.push(id);
    }
    for id in buffers.buffer_list() {
        if Some(id) != current_id {
            scan_order.push(id);
        }
    }

    let key = Value::from_sym_id(name_id);
    for id in scan_order {
        let Some(buf) = buffers.get(id) else {
            continue;
        };
        // Phase 10E: prefer the buffer's local_var_alist (LOCALIZED
        // per-buffer storage), then fall back to the legacy
        // get_buffer_local lookup (slot or lisp_bindings), then to
        // the global default. Mirrors GNU `find_buffer` (`buffer.c`)
        // walking the alist directly.
        let observed = buf
            .find_in_local_var_alist(key)
            .or_else(|| buf.get_buffer_local(name))
            .unwrap_or(fallback_value);
        if equal_value(&observed, &target_value, 0) {
            return Ok(Value::make_buffer(id));
        }
    }

    Ok(Value::NIL)
}

pub(crate) fn builtin_delete_all_overlays(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = &mut eval.buffers;
    expect_max_args("delete-all-overlays", &args, 1)?;
    let target = if args.is_empty() || args[0].is_nil() {
        buffers.current_buffer().map(|buf| buf.id)
    } else {
        Some(expect_buffer_id(&args[0])?)
    };

    let Some(target_id) = target else {
        return Ok(Value::NIL);
    };
    if buffers.get(target_id).is_none() {
        // GNU Emacs treats dead buffers as a no-op.
        return Ok(Value::NIL);
    }
    let _ = buffers.delete_all_buffer_overlays(target_id);
    Ok(Value::NIL)
}

pub(crate) fn builtin_buffer_live_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = &eval.buffers;
    expect_args("buffer-live-p", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = args[0].as_buffer_id().unwrap();
            Ok(Value::bool_val(buffers.get(id).is_some()))
        }
        _ => Ok(Value::NIL),
    }
}

/// GNU `Fget_truename_buffer` (src/buffer.c:524-539): the first live buffer
/// whose `buffer-file-truename` is `string-equal` to FILENAME, else nil.
///
/// GNU compares with `Fstring_equal`, which never expands or canonicalizes
/// either side — `find-file` has already stored the truename — so this is a
/// plain byte comparison, unlike `get-file-buffer`'s resolving search.
pub(crate) fn builtin_get_truename_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("get-truename-buffer", &args, 1)?;
    for id in eval.buffers.buffer_list() {
        let Some(buf) = eval.buffers.get(id) else {
            continue;
        };
        // GNU skips buffers whose file_truename is not a string BEFORE
        // calling Fstring_equal, so a non-string FILENAME only signals once
        // some live buffer visits a file.
        let Some(truename_value) = buf.buffer_local_value("buffer-file-truename") else {
            continue;
        };
        let Some(truename) = eval.lisp_string(truename_value) else {
            continue;
        };
        let filename = eval.expect_lisp_string(args[0])?;
        if truename.schars() == filename.schars()
            && truename.sbytes() == filename.sbytes()
            && truename.as_bytes() == filename.as_bytes()
        {
            return Ok(Value::make_buffer(id));
        }
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_get_file_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("get-file-buffer", &args, 1)?;
    let resolved = super::fileio::resolve_filename_lisp_in_state(
        &eval.obarray,
        &[],
        &eval.buffers,
        expect_lisp_string(&args[0])?,
    );
    let resolved_path = super::fileio::lisp_file_name_to_path_buf(&resolved);
    let resolved_true = canonicalize_or_self(&resolved_path);

    for id in eval.buffers.buffer_list() {
        let Some(buf) = eval.buffers.get(id) else {
            continue;
        };
        let Some(file_name) = buf.file_name_lisp_string() else {
            continue;
        };

        let candidate = super::fileio::resolve_filename_lisp_in_state(
            &eval.obarray,
            &[],
            &eval.buffers,
            file_name,
        );
        if candidate == resolved {
            return Ok(Value::make_buffer(id));
        }
        let candidate_path = super::fileio::lisp_file_name_to_path_buf(&candidate);
        if canonicalize_or_self(&candidate_path) == resolved_true {
            return Ok(Value::make_buffer(id));
        }
    }

    Ok(Value::NIL)
}

pub(crate) fn builtin_kill_buffer(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("kill-buffer", &args, 1)?;
    let id = match args.first() {
        None => match eval.buffers.current_buffer() {
            Some(buf) => buf.id,
            None => return Ok(Value::NIL),
        },
        Some(arg) => match arg.kind() {
            ValueKind::Nil => match eval.buffers.current_buffer() {
                Some(buf) => buf.id,
                None => return Ok(Value::NIL),
            },
            ValueKind::Veclike(VecLikeType::Buffer) => {
                let bid = arg.as_buffer_id().unwrap();
                if eval.buffers.get(bid).is_none() {
                    return Ok(Value::NIL);
                }
                bid
            }
            ValueKind::String => {
                let name = expect_buffer_name_string(arg)?;
                match eval.buffers.find_buffer_by_name(&name) {
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

    let saved_current = eval.buffers.current_buffer_id();
    let inhibit_buffer_hooks = eval.buffers.buffer_hooks_inhibited(id);
    // GNU `Fkill_buffer` runs query functions and `kill-buffer-hook` after
    // `set_buffer_internal`/`Fset_buffer`, not `record_buffer`; killing or
    // querying a buffer must not make it the head of `buffer-list`.
    eval.set_current_buffer_unrecorded(id)?;
    let query_result = if inhibit_buffer_hooks {
        Value::T
    } else {
        let query_sym = crate::emacs_core::hook_runtime::hook_symbol_by_name(
            eval,
            "kill-buffer-query-functions",
        );
        let query_value = crate::emacs_core::hook_runtime::hook_value_by_id(eval, query_sym)
            .unwrap_or(Value::NIL);
        crate::emacs_core::hook_runtime::run_hook_value_until_failure(
            eval,
            query_sym,
            query_value,
            &[],
            true,
        )?
    };
    if let Some(buffer_id) = saved_current {
        eval.restore_current_buffer_if_live(buffer_id);
    }
    if query_result.is_nil() {
        return Ok(Value::NIL);
    }
    if eval.buffers.get(id).is_none() {
        return Ok(Value::T);
    }

    eval.set_current_buffer_unrecorded(id)?;
    if !inhibit_buffer_hooks {
        let hook_sym =
            crate::emacs_core::hook_runtime::hook_symbol_by_name(eval, "kill-buffer-hook");
        let hook_value =
            crate::emacs_core::hook_runtime::hook_value_by_id(eval, hook_sym).unwrap_or(Value::NIL);
        crate::emacs_core::hook_runtime::run_hook_value(eval, hook_sym, hook_value, &[], true)?;
    }
    if let Some(buffer_id) = saved_current {
        eval.restore_current_buffer_if_live(buffer_id);
    }
    if eval.buffers.get(id).is_none() {
        return Ok(Value::T);
    }

    if eval
        .visible_variable_value_or_nil("kill-buffer-quit-windows")
        .is_truthy()
    {
        if delete_quit_restore_popup_windows_showing_buffer(&mut eval.frames, id) {
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
        if eval.buffers.get(id).is_none() {
            return Ok(Value::T);
        }
    }

    let current_before = eval.buffers.current_buffer().map(|buf| buf.id);
    let killed_ids = eval
        .buffers
        .collect_killed_buffer_ids(id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    let killed_set = killed_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let current_will_die = current_before.is_some_and(|current| killed_set.contains(&current));
    let replacement = if current_will_die {
        let other = other_buffer_impl_in_state(
            &mut eval.frames,
            &mut eval.buffers,
            vec![Value::make_buffer(current_before.expect("current buffer"))],
        )?;
        match other.as_buffer_id() {
            Some(next) if next != id => Some(next),
            _ => None,
        }
    } else {
        None
    };

    let killed_ids_to_signal = eval
        .buffers
        .collect_killed_buffer_ids(id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    for killed_id in killed_ids_to_signal {
        eval.kill_buffer_processes(killed_id)?;
    }
    if eval.buffers.get(id).is_none() {
        return Ok(Value::T);
    }

    let buffer_defaults = eval.buffers.buffer_defaults;
    let killed_ids_to_reset = eval
        .buffers
        .collect_killed_buffer_ids(id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    {
        let buffers = &mut eval.buffers;
        let obarray = &mut eval.obarray;
        for killed_id in &killed_ids_to_reset {
            if let Some(buffer) = buffers.get_mut(*killed_id) {
                buffer.kill_all_local_variables(obarray, true, &buffer_defaults);
            }
        }
    }

    let killed_ids = eval
        .buffers
        .kill_buffer_collect(id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    // Ensure dead-buffer windows continue to point at a live fallback buffer.
    let scratch = if let Some(scratch) = eval.buffers.find_buffer_by_name("*scratch*") {
        scratch
    } else {
        eval.buffers.create_buffer("*scratch*")
    };
    for killed_id in &killed_ids {
        eval.frames.replace_buffer_in_windows(*killed_id, scratch);
    }

    // Discard killed buffers from every frame's buffer list and buried
    // buffer list (GNU frame.c:3757-3769 discards from each frame).
    for killed_id in &killed_ids {
        for fid in eval.frames.frame_list() {
            if let Some(frame) = eval.frames.get_mut(fid) {
                frame.buffer_list.retain(|bid| *bid != *killed_id);
                frame.buried_buffer_list.retain(|bid| *bid != *killed_id);
            }
        }
    }

    if current_will_die {
        if let Some(next) = replacement
            && eval.buffers.get(next).is_some()
        {
            eval.set_current_buffer_unrecorded(next)?;
        }
        if eval.buffers.current_buffer().is_none() {
            if let Some(next) = eval.buffers.buffer_list().into_iter().next() {
                eval.set_current_buffer_unrecorded(next)?;
            } else {
                eval.set_current_buffer_unrecorded(scratch)?;
            }
        }
    }

    if !inhibit_buffer_hooks {
        run_buffer_list_update_hook(eval)?;
    }

    Ok(Value::T)
}

pub(crate) fn builtin_set_buffer(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("set-buffer", &args, 1)?;
    let id = match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let bid = args[0].as_buffer_id().unwrap();
            if eval.buffers.get(bid).is_none() {
                return Err(signal(
                    "error",
                    vec![Value::string("Selecting deleted buffer")],
                ));
            }
            bid
        }
        ValueKind::String => {
            let s = expect_buffer_name_string(&args[0])?;
            eval.buffers.find_buffer_by_name(&s).ok_or_else(|| {
                signal("error", vec![Value::string(format!("No buffer named {s}"))])
            })?
        }
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };
    eval.set_current_buffer_unrecorded(id)?;
    Ok(Value::make_buffer(id))
}

pub(crate) fn builtin_current_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = &eval.buffers;
    expect_args("current-buffer", &args, 0)?;
    match buffers.current_buffer() {
        Some(buf) => Ok(Value::make_buffer(buf.id)),
        None => Ok(Value::NIL),
    }
}

pub(crate) fn builtin_buffer_name(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let buffers = &eval.buffers;
    expect_max_args("buffer-name", &args, 1)?;
    let id = if args.is_empty() || args[0].is_nil() {
        match buffers.current_buffer() {
            Some(b) => b.id,
            None => return Ok(Value::NIL),
        }
    } else {
        expect_buffer_id(&args[0])?
    };
    match buffers.get(id) {
        Some(buf) => Ok(buf.name_value()),
        None => Ok(Value::NIL),
    }
}

pub(crate) fn builtin_buffer_file_name(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = &eval.buffers;
    expect_max_args("buffer-file-name", &args, 1)?;
    let id = if args.is_empty() || args[0].is_nil() {
        match buffers.current_buffer() {
            Some(b) => b.id,
            None => return Ok(Value::NIL),
        }
    } else {
        expect_buffer_id(&args[0])?
    };
    Ok(buffers
        .get_any(id)
        .and_then(|buf| buf.buffer_local_value("buffer-file-name"))
        .unwrap_or(Value::NIL))
}

pub(crate) fn builtin_buffer_base_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = &eval.buffers;
    expect_max_args("buffer-base-buffer", &args, 1)?;
    let target = if args.is_empty() || args[0].is_nil() {
        match buffers.current_buffer() {
            Some(buf) => buf.id,
            None => return Ok(Value::NIL),
        }
    } else {
        expect_buffer_id(&args[0])?
    };

    Ok(buffers
        .get_any(target)
        .and_then(|buf| buf.base_buffer)
        .map(Value::make_buffer)
        .unwrap_or(Value::NIL))
}

pub(crate) fn builtin_buffer_last_name(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = &eval.buffers;
    expect_max_args("buffer-last-name", &args, 1)?;
    let target = if args.is_empty() || args[0].is_nil() {
        match buffers.current_buffer() {
            Some(buf) => buf.id,
            None => return Ok(Value::NIL),
        }
    } else {
        expect_buffer_id(&args[0])?
    };

    if let Some(buf) = buffers.get_any(target) {
        return Ok(buf.last_name_value());
    }
    Ok(Value::NIL)
}

/// Interned-once ids for the buffer-access property gate — it runs on
/// every buffer-text read primitive and re-hashed both names per call.
#[inline(always)]
fn buffer_access_fontify_functions_sym() -> crate::emacs_core::intern::SymId {
    static SYMBOL: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
        std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| crate::emacs_core::intern::intern("buffer-access-fontify-functions"))
}

#[inline(always)]
fn buffer_access_fontified_property_sym() -> crate::emacs_core::intern::SymId {
    static SYMBOL: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
        std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| crate::emacs_core::intern::intern("buffer-access-fontified-property"))
}

/// (buffer-substring START END) → string
fn update_buffer_properties_for_access(
    eval: &mut super::eval::Context,
    start: LispCharPos1,
    end: LispCharPos1,
) -> Result<(), Flow> {
    if eval
        .visible_variable_value_or_nil_by_id(buffer_access_fontify_functions_sym())
        .is_nil()
    {
        return Ok(());
    }

    let fontified_property =
        eval.visible_variable_value_or_nil_by_id(buffer_access_fontified_property_sym());
    if !fontified_property.is_nil() {
        let needs_fontification = super::textprop::builtin_text_property_any(
            eval,
            vec![
                Value::fixnum(start.as_i64()),
                Value::fixnum(end.as_i64()),
                fontified_property,
                Value::NIL,
            ],
        )?;
        if needs_fontification.is_nil() {
            return Ok(());
        }
    }

    builtin_run_hook_with_args(
        eval,
        vec![
            Value::symbol("buffer-access-fontify-functions"),
            Value::fixnum(start.as_i64()),
            Value::fixnum(end.as_i64()),
        ],
    )?;
    Ok(())
}

pub(crate) fn builtin_buffer_substring(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("buffer-substring", &args, 2)?;
    let start = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(&eval.buffers, &args[1])?;
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let (start, end) = {
        let buf = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        checked_accessible_lisp_range_from_raw(
            buf,
            start,
            end,
            vec![Value::make_buffer(buf.id), args[0], args[1]],
        )?
    };
    update_buffer_properties_for_access(eval, start, end)?;
    let buf = eval
        .buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let byte_range = accessible_lisp_range_to_byte_range(buf, start, end);
    Ok(buffer_slice_value_range(buf, byte_range))
}

pub(crate) fn builtin_buffer_string(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("buffer-string", &args, 0)?;
    let (current_id, start, end) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            buf.id,
            buf.point_min_lisp_char_pos(),
            buf.point_max_lisp_char_pos(),
        )
    };
    update_buffer_properties_for_access(eval, start, end)?;
    let buf = eval
        .buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(buffer_slice_value_range(
        buf,
        buf.accessible_emacs_byte_range(),
    ))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn resolve_buffer_designator_allow_nil_current(
    eval: &mut super::eval::Context,
    arg: &Value,
) -> Result<Option<BufferId>, Flow> {
    match arg.kind() {
        ValueKind::Nil => eval
            .buffers
            .current_buffer()
            .map(|buf| Some(buf.id))
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = arg.as_buffer_id().unwrap();
            if eval.buffers.get(id).is_some() {
                Ok(Some(id))
            } else {
                Err(signal(
                    "error",
                    vec![Value::string("Selecting deleted buffer")],
                ))
            }
        }
        ValueKind::String => {
            let name = expect_buffer_name_string(arg)?;
            eval.buffers
                .find_buffer_by_name(&name)
                .map(Some)
                .ok_or_else(|| {
                    signal(
                        "error",
                        vec![Value::string(format!("No buffer named {name}"))],
                    )
                })
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *arg],
        )),
    }
}

fn accessible_lisp_range_to_byte_range(
    buf: &Buffer,
    start: LispCharPos1,
    end: LispCharPos1,
) -> EmacsByteRange {
    let (from, to) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    EmacsByteRange::new(
        buf.lisp_pos_to_accessible_emacs_byte_pos(from),
        buf.lisp_pos_to_accessible_emacs_byte_pos(to),
    )
}

fn validate_accessible_lisp_range(
    buf: &Buffer,
    start: LispCharPos1,
    end: LispCharPos1,
    error_values: Vec<Value>,
) -> Result<(), Flow> {
    let point_min = buf.point_min_lisp_char_pos();
    let point_max = buf.point_max_lisp_char_pos();
    if start < point_min || start > point_max || end < point_min || end > point_max {
        return Err(signal(LispCondition::ArgsOutOfRange, error_values));
    }
    Ok(())
}

fn checked_accessible_lisp_range_from_raw(
    buf: &Buffer,
    start: i64,
    end: i64,
    error_values: Vec<Value>,
) -> Result<(LispCharPos1, LispCharPos1), Flow> {
    let start = LispCharPos1::new(start);
    let end = LispCharPos1::new(end);
    validate_accessible_lisp_range(buf, start, end, error_values)?;
    Ok((start, end))
}

fn resolve_lisp_range_with_buffer_defaults(
    buffers: &BufferManager,
    buffer_id: Option<BufferId>,
    start_arg: &Value,
    end_arg: &Value,
) -> Result<(LispCharPos1, LispCharPos1), Flow> {
    let default_start = || {
        buffer_id
            .and_then(|id| {
                buffers
                    .get(id)
                    .map(|buf| buf.point_min_lisp_char_pos().as_i64())
            })
            .unwrap_or(1)
    };
    let default_end = || {
        buffer_id
            .and_then(|id| {
                buffers
                    .get(id)
                    .map(|buf| buf.point_max_lisp_char_pos().as_i64())
            })
            .unwrap_or(1)
    };

    let start = if start_arg.is_nil() {
        default_start()
    } else {
        expect_integer_or_marker_in_buffers(buffers, start_arg)?
    };
    let end = if end_arg.is_nil() {
        default_end()
    } else {
        expect_integer_or_marker_in_buffers(buffers, end_arg)?
    };
    Ok((LispCharPos1::new(start), LispCharPos1::new(end)))
}

pub(crate) fn resolve_buffer_designator_allow_nil_current_in_manager(
    buffers: &BufferManager,
    arg: &Value,
) -> Result<Option<BufferId>, Flow> {
    match arg.kind() {
        ValueKind::Nil => buffers
            .current_buffer()
            .map(|buf| Some(buf.id))
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = arg.as_buffer_id().unwrap();
            if buffers.get(id).is_some() {
                Ok(Some(id))
            } else {
                Err(signal(
                    "error",
                    vec![Value::string("Selecting deleted buffer")],
                ))
            }
        }
        ValueKind::String => {
            let name = expect_buffer_name_string(arg)?;
            buffers.find_buffer_by_name(&name).map(Some).ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string(format!("No buffer named {name}"))],
                )
            })
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *arg],
        )),
    }
}

fn checked_buffer_slice_for_char_region_in_manager(
    buffers: &BufferManager,
    buffer_id: Option<BufferId>,
    start: LispCharPos1,
    end: LispCharPos1,
    start_arg: Value,
    end_arg: Value,
) -> Result<crate::heap_types::LispString, Flow> {
    let Some(buffer_id) = buffer_id else {
        return Ok(crate::heap_types::LispString::from_utf8(""));
    };
    let Some(buf) = buffers.get(buffer_id) else {
        return Ok(crate::heap_types::LispString::from_utf8(""));
    };

    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    if start.as_i64() < point_min
        || start.as_i64() > point_max
        || end.as_i64() < point_min
        || end.as_i64() > point_max
    {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![start_arg, end_arg],
        ));
    }

    // Issue #131: return the byte-faithful buffer substring as a `LispString`
    // so `compare-buffer-substrings` can compare real Emacs characters (eight-bit
    // and PUA glyphs stay distinct) instead of routing through a lossy/storage
    // Rust string.
    let byte_range = accessible_lisp_range_to_byte_range(buf, start, end);
    Ok(buf.buffer_substring_lisp_string_range(byte_range))
}

fn checked_buffer_substring_for_char_region_in_manager(
    buffers: &BufferManager,
    buffer_id: Option<BufferId>,
    start: LispCharPos1,
    end: LispCharPos1,
    start_arg: Value,
    end_arg: Value,
) -> Result<Value, Flow> {
    let Some(buffer_id) = buffer_id else {
        return Ok(Value::heap_string(
            crate::heap_types::LispString::from_emacs_bytes(Vec::new()),
        ));
    };
    let Some(buf) = buffers.get(buffer_id) else {
        return Ok(Value::heap_string(
            crate::heap_types::LispString::from_emacs_bytes(Vec::new()),
        ));
    };

    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    if start.as_i64() < point_min
        || start.as_i64() > point_max
        || end.as_i64() < point_min
        || end.as_i64() > point_max
    {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![start_arg, end_arg],
        ));
    }

    let byte_range = accessible_lisp_range_to_byte_range(buf, start, end);
    Ok(buffer_slice_value(
        buf,
        byte_range.start().get(),
        byte_range.end().get(),
    ))
}

fn compare_buffer_substring_strings(
    left: &crate::heap_types::LispString,
    right: &crate::heap_types::LispString,
    case_fold: bool,
) -> i64 {
    // Issue #131: compare the two substrings character-by-character over their
    // exact Emacs bytes (GNU `Fcompare_buffer_substrings` returns the 1-based
    // char index of the first difference). Eight-bit raw bytes and Private-Use
    // glyphs stay distinct because each Emacs char is decoded faithfully.
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let mut lp = 0usize;
    let mut rp = 0usize;
    let mut pos = 1i64;

    loop {
        match (lp < left_bytes.len(), rp < right_bytes.len()) {
            (true, true) => {
                let (a_code, a_len) = crate::emacs_core::emacs_char::string_char(&left_bytes[lp..]);
                let (b_code, b_len) =
                    crate::emacs_core::emacs_char::string_char(&right_bytes[rp..]);
                lp += a_len;
                rp += b_len;
                let a = fold_emacs_char_code(a_code, case_fold);
                let b = fold_emacs_char_code(b_code, case_fold);
                if a != b {
                    return if a < b { -pos } else { pos };
                }
                pos += 1;
            }
            (true, false) => return pos,
            (false, true) => return -pos,
            (false, false) => return 0,
        }
    }
}

/// Issue #131: lowercase an Emacs character code when `case_fold` is set,
/// preserving non-Unicode/eight-bit codes (which have no Rust `char`) verbatim.
fn fold_emacs_char_code(code: u32, case_fold: bool) -> u32 {
    if !case_fold {
        return code;
    }
    match char::from_u32(code) {
        Some(ch) => ch.to_lowercase().next().map(|c| c as u32).unwrap_or(code),
        None => code,
    }
}

pub(crate) fn builtin_buffer_line_statistics(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = &eval.buffers;
    expect_max_args("buffer-line-statistics", &args, 1)?;
    let buffer_id = if args.is_empty() {
        resolve_buffer_designator_allow_nil_current_in_manager(buffers, &Value::NIL)?
    } else {
        resolve_buffer_designator_allow_nil_current_in_manager(buffers, &args[0])?
    };

    let text = buffer_id
        .and_then(|id| {
            buffers.get(id).map(|buf| {
                let mut bytes = Vec::new();
                buf.copy_emacs_byte_range_to(buf.accessible_emacs_byte_range(), &mut bytes);
                bytes
            })
        })
        .unwrap_or_default();

    if text.is_empty() {
        return Ok(Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::make_float(0.0),
        ]));
    }

    let mut line_count = 0usize;
    let mut max_len = 0usize;
    let mut mean = 0.0f64;
    let mut start = 0usize;
    while start < text.len() {
        if let Some(rel_nl) = text[start..].iter().position(|&b| b == b'\n') {
            let width = rel_nl;
            line_count += 1;
            max_len = max_len.max(width);
            mean += (width as f64 - mean) / line_count as f64;
            start += rel_nl + 1;
        } else {
            let width = text.len() - start;
            line_count += 1;
            max_len = max_len.max(width);
            mean += (width as f64 - mean) / line_count as f64;
            break;
        }
    }

    if line_count == 0 {
        return Ok(Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::make_float(0.0),
        ]));
    }

    Ok(Value::list(vec![
        Value::fixnum(line_count as i64),
        Value::fixnum(max_len as i64),
        Value::make_float(mean),
    ]))
}

fn replace_region_contents_type_predicate() -> Value {
    Value::list(vec![
        Value::symbol("or"),
        Value::symbol("stringp"),
        Value::symbol("bufferp"),
        Value::symbol("vectorp"),
    ])
}

fn replace_region_source_value_in_state(
    buffers: &BufferManager,
    source: &Value,
    current_id: BufferId,
) -> Result<Value, Flow> {
    match source.kind() {
        ValueKind::String => Ok(*source),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = source.as_buffer_id().unwrap();
            if id == current_id {
                return Err(signal(
                    "error",
                    vec![Value::string("Cannot replace a buffer with itself")],
                ));
            }
            let Some(buf) = buffers.get(id) else {
                return Err(signal(
                    "error",
                    vec![Value::string("Selecting deleted buffer")],
                ));
            };
            let start = buf.point_min_lisp_char_pos();
            let end = buf.point_max_lisp_char_pos();
            checked_buffer_substring_for_char_region_in_manager(
                buffers,
                Some(id),
                start,
                end,
                Value::fixnum(start.as_i64()),
                Value::fixnum(end.as_i64()),
            )
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = source.as_vector_data().unwrap().clone();
            if items.len() != 3 {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![replace_region_contents_type_predicate(), *source],
                ));
            }
            let buffer_id = expect_buffer_id(&items[0])?;
            if buffer_id == current_id {
                return Err(signal(
                    "error",
                    vec![Value::string("Cannot replace a buffer with itself")],
                ));
            }
            let start = expect_integer_or_marker_in_buffers(buffers, &items[1])?;
            let end = expect_integer_or_marker_in_buffers(buffers, &items[2])?;
            let start = LispCharPos1::new(start);
            let end = LispCharPos1::new(end);
            checked_buffer_substring_for_char_region_in_manager(
                buffers,
                Some(buffer_id),
                start,
                end,
                items[1],
                items[2],
            )
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![replace_region_contents_type_predicate(), *source],
        )),
    }
}

pub(crate) fn builtin_buffer_swap_text(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = &mut eval.buffers;
    expect_args("buffer-swap-text", &args, 1)?;
    let other_id = expect_buffer_id(&args[0])?;
    let current_id = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .id;

    buffers
        .swap_buffer_text(current_id, other_id)
        .map(|()| Value::NIL)
        .map_err(|err| signal("error", vec![Value::string(err.message())]))
}

pub(crate) fn builtin_insert_buffer_substring(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("insert-buffer-substring", &args, 1, 3)?;
    let buffer_id =
        resolve_buffer_designator_allow_nil_current_in_manager(&eval.buffers, &args[0])?;
    let (default_start, default_end) = buffer_id
        .and_then(|id| {
            eval.buffers.get(id).map(|buf| {
                (
                    buf.point_min_lisp_char_pos().as_i64(),
                    buf.point_max_lisp_char_pos().as_i64(),
                )
            })
        })
        .unwrap_or((1, 1));
    let start = if args.len() > 1 && !args[1].is_nil() {
        expect_integer_or_marker_in_buffers(&eval.buffers, &args[1])?
    } else {
        default_start
    };
    let end = if args.len() > 2 && !args[2].is_nil() {
        expect_integer_or_marker_in_buffers(&eval.buffers, &args[2])?
    } else {
        default_end
    };

    let text = checked_buffer_substring_for_char_region_in_manager(
        &eval.buffers,
        buffer_id,
        LispCharPos1::new(start),
        LispCharPos1::new(end),
        Value::fixnum(start),
        Value::fixnum(end),
    )?;
    builtin_insert(eval, vec![text])
}

pub(crate) fn builtin_kill_all_local_variables(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("kill-all-local-variables", &args, 0, 1)?;
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let kill_permanent = args.first().copied().unwrap_or(Value::NIL).is_truthy();
    // GNU `Fkill_all_local_variables` calls `bset_update_mode_line`
    // (buffer.c:3046, "Force mode-line redisplay.  Useful here because all
    // major mode commands call this function."). `%m` and any buffer-local
    // mode-line-format are the reason.
    eval.mark_chrome_dirty_all();

    // GNU `Fkill_all_local_variables` (buffer.c) runs the normal hook
    // `change-major-mode-hook` as its very first action, *before* any local
    // bindings are eliminated.  Because every command that selects a new major
    // mode (`fundamental-mode', `text-mode', derived modes, `set-auto-mode',
    // `normal-mode', `set-buffer-major-mode', ...) starts by calling this
    // function, this is the single shared chokepoint where
    // `change-major-mode-hook' must fire.  Running it here -- with the current
    // buffer's local variables and local hook value still in effect, and with
    // `major-mode' still naming the *previous* mode -- matches GNU exactly.
    run_buffer_change_major_mode_hook(eval)?;

    // GNU buffer.c reset_buffer_local_variables:
    // - preserves most always-local slots
    // - resets only a small fixed reset-on-kill-all subset
    // - clears conditional slot locals unless they are permanent-local
    // - walks local_var_alist for LOCALIZED entries (Phase 10E)
    let _ =
        eval.buffers
            .clear_buffer_local_properties(current_id, &mut eval.obarray, kill_permanent);
    Ok(Value::NIL)
}

/// `(ntake N LIST)` -> LIST
pub(crate) fn builtin_ntake(args: Vec<Value>) -> EvalResult {
    expect_args("ntake", &args, 2)?;
    let n = expect_int(&args[0])?;
    if n <= 0 {
        return Ok(Value::NIL);
    }

    let head = args[1];
    if head.is_nil() {
        return Ok(Value::NIL);
    }
    if !head.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), head],
        ));
    }

    let mut cursor = head;
    for _ in 1..n {
        match cursor.kind() {
            ValueKind::Cons => {
                let next = cursor.cons_cdr();
                match next.kind() {
                    ValueKind::Cons => cursor = next,
                    ValueKind::Nil => return Ok(head),
                    _other => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("listp"), next],
                        ));
                    }
                }
            }
            ValueKind::Nil => return Ok(head),
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), cursor],
                ));
            }
        }
    }

    match cursor.kind() {
        ValueKind::Cons => {
            cursor.set_cdr(Value::NIL);
            Ok(head)
        }
        ValueKind::Nil => Ok(head),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), cursor],
        )),
    }
}

/// A single change run from the minimal diff used by
/// `replace-region-contents`: replace characters `[a_start, a_end)` of the
/// destination region with characters `[b_start, b_end)` of the source.
/// All indices are character indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReplaceRegionChangeRun {
    pub(crate) a_start: usize,
    pub(crate) a_end: usize,
    pub(crate) b_start: usize,
    pub(crate) b_end: usize,
}

/// Decode an Emacs-byte buffer/string slice into a vector of `(codepoint,
/// byte_offset)` pairs.  `byte_offset` is the offset (within `bytes`) of each
/// character; a final sentinel entry `(0, bytes.len())` is appended so callers
/// can map a character index to its byte offset for `0..=len`.
fn decode_chars_with_byte_offsets(bytes: &[u8]) -> Vec<(u32, usize)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let (code, len) = crate::emacs_core::emacs_char::string_char(&bytes[pos..]);
        out.push((code, pos));
        pos += len.max(1);
    }
    out.push((0, bytes.len()));
    out
}

/// Compute the minimal sequence of change runs transforming `a` into `b`,
/// mirroring GNU's use of the Myers O(ND) difference algorithm (lib/diffseq.h,
/// referenced from `Freplace_region_contents`).
///
/// `a` and `b` are the codepoint sequences of the destination region and the
/// source.  The returned runs are in ascending order and cover only the
/// differing portions; common prefix/suffix and matched interior characters are
/// left untouched, so markers, point and overlays within them are preserved
/// exactly as GNU does by issuing a `replace_range` only per changed run.
pub(crate) fn replace_region_minimal_change_runs(
    a: &[u32],
    b: &[u32],
) -> Vec<ReplaceRegionChangeRun> {
    // Trim the common prefix.
    let mut lo = 0usize;
    while lo < a.len() && lo < b.len() && a[lo] == b[lo] {
        lo += 1;
    }
    // Trim the common suffix.
    let mut a_hi = a.len();
    let mut b_hi = b.len();
    while a_hi > lo && b_hi > lo && a[a_hi - 1] == b[b_hi - 1] {
        a_hi -= 1;
        b_hi -= 1;
    }

    let sub_a = &a[lo..a_hi];
    let sub_b = &b[lo..b_hi];

    // Myers O(ND) shortest-edit-script over the differing middle.  Walking the
    // backtraced path yields the same minimal edit (and therefore the same
    // change-run boundaries) as GNU's `compareseq` (lib/diffseq.h).
    let raw = myers_edit_runs(sub_a, sub_b, lo, lo);

    // Coalesce a deletion immediately followed by an insertion at the same
    // point (and vice-versa) into a single replacement, matching GNU's
    // back-to-front merge of contiguous deletion/insertion bits.
    let mut runs: Vec<ReplaceRegionChangeRun> = Vec::with_capacity(raw.len());
    for run in raw {
        match runs.last_mut() {
            Some(prev) if prev.a_end == run.a_start && prev.b_end == run.b_start => {
                prev.a_end = run.a_end;
                prev.b_end = run.b_end;
            }
            _ => runs.push(run),
        }
    }
    runs
}

/// Compute the minimal edit runs transforming `a` into `b` using Myers' O(ND)
/// algorithm with a recorded trace.  `a_off`/`b_off` are added to every emitted
/// index so callers can diff a sub-slice while reporting absolute positions.
///
/// Runs are returned in ascending position order; pure deletions have
/// `b_start == b_end`, pure insertions have `a_start == a_end`.  Adjacent
/// single-character edits are emitted as separate runs (the caller coalesces
/// truly-contiguous delete+insert pairs).
fn myers_edit_runs(
    a: &[u32],
    b: &[u32],
    a_off: usize,
    b_off: usize,
) -> Vec<ReplaceRegionChangeRun> {
    let n = a.len() as isize;
    let m = b.len() as isize;

    if n == 0 && m == 0 {
        return Vec::new();
    }

    // `v[k]` holds the furthest-reaching x on diagonal k for the current d.
    // We snapshot `v` after each d so we can backtrack the path afterwards.
    let max_d = (n + m) as usize;
    let offset = n + m; // index shift so k in [-(n+m), n+m] is >= 0.
    let vsize = (2 * (n + m) + 1) as usize;
    let mut v = vec![0isize; vsize];
    let mut trace: Vec<Vec<isize>> = Vec::with_capacity(max_d + 1);

    let mut found_d = 0usize;
    'outer: for d in 0..=max_d as isize {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            // Decide whether we arrived here via an insertion (down) or a
            // deletion (right).  Ties prefer deletion-from-the-left, matching
            // the canonical Myers backtrack.
            let mut x = if k == -d
                || (k != d && v[(k - 1 + offset) as usize] < v[(k + 1 + offset) as usize])
            {
                v[(k + 1 + offset) as usize] // down (insertion)
            } else {
                v[(k - 1 + offset) as usize] + 1 // right (deletion)
            };
            let mut y = x - k;
            // Follow the diagonal (snake) of equal elements.
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[(k + offset) as usize] = x;
            if x >= n && y >= m {
                found_d = d as usize;
                break 'outer;
            }
            k += 2;
        }
    }

    // Backtrack through the recorded traces to recover the edit runs.
    let mut runs: Vec<ReplaceRegionChangeRun> = Vec::new();
    let mut x = n;
    let mut y = m;
    for d in (1..=found_d as isize).rev() {
        let vprev = &trace[d as usize];
        let k = x - y;
        let prev_k = if k == -d
            || (k != d && vprev[(k - 1 + offset) as usize] < vprev[(k + 1 + offset) as usize])
        {
            k + 1 // came from a down move (insertion)
        } else {
            k - 1 // came from a right move (deletion)
        };
        let prev_x = vprev[(prev_k + offset) as usize];
        let prev_y = prev_x - prev_k;

        // Skip the trailing snake (matched diagonal) — those characters are
        // unchanged and must not be part of any run.
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
        }

        // The single edit step between (prev_x, prev_y) and (x, y).
        if x == prev_x {
            // Insertion of b[prev_y] (down move): a stays, b advances.
            runs.push(ReplaceRegionChangeRun {
                a_start: a_off + x as usize,
                a_end: a_off + x as usize,
                b_start: b_off + prev_y as usize,
                b_end: b_off + y as usize,
            });
        } else {
            // Deletion of a[prev_x] (right move): a advances, b stays.
            runs.push(ReplaceRegionChangeRun {
                a_start: a_off + prev_x as usize,
                a_end: a_off + x as usize,
                b_start: b_off + y as usize,
                b_end: b_off + y as usize,
            });
        }
        x = prev_x;
        y = prev_y;
    }

    runs.reverse();
    runs
}

/// `(replace-buffer-contents SOURCE &optional MAX-SECS MAX-COSTS)` -> t
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_replace_buffer_contents(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("replace-buffer-contents", &args, 1, 3)?;
    // GNU 31's `replace-buffer-contents` (subr.el) is a thin wrapper:
    //   (replace-region-contents (point-min) (point-max) (get-buffer source)
    //                            max-secs max-costs)
    // Delegating keeps the minimal (Myers-diff) non-destructive replacement,
    // so markers, point, properties and overlays in the accessible portion
    // outside the changed runs are preserved exactly as in GNU.
    let source_id = resolve_buffer_designator_allow_nil_current(eval, &args[0])?;
    let source_buffer = match source_id {
        Some(id) => Value::make_buffer(id),
        None => Value::NIL,
    };

    let (point_min, point_max) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            buf.point_min_lisp_char_pos().as_i64(),
            buf.point_max_lisp_char_pos().as_i64(),
        )
    };

    let mut region_args = vec![
        Value::fixnum(point_min),
        Value::fixnum(point_max),
        source_buffer,
    ];
    // Forward MAX-SECS / MAX-COSTS positionally so the optional behavior
    // (immediate delete/insert fallback) matches GNU.
    if let Some(max_secs) = args.get(1) {
        region_args.push(*max_secs);
        if let Some(max_costs) = args.get(2) {
            region_args.push(*max_costs);
        }
    }

    builtin_replace_region_contents(eval, region_args)
}

pub(crate) fn builtin_replace_region_contents(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("replace-region-contents", &args, 3, 6)?;
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let start = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(&eval.buffers, &args[1])?;

    // GNU editfns.c `Freplace_region_contents`: if SOURCE is a function, call
    // it with no arguments and the current buffer narrowed to BEG..END, and use
    // its return value (a buffer or string) as the actual source.  This is
    // wrapped in `save-excursion` + `save-restriction` so the narrowing and
    // point are restored afterwards.  (This SOURCE form is deprecated in GNU.)
    let mut source = args[2];
    if super::builtins::types::builtin_functionp_1(eval, source)?.is_truthy() {
        let count = eval.specpdl.len();
        eval.record_save_excursion();
        if let Some(state) = eval.buffers.save_current_restriction_state() {
            eval.specpdl
                .push(super::eval::SpecBinding::save_restriction(state));
        }
        let narrow_result =
            builtin_narrow_to_region(eval, vec![Value::fixnum(start), Value::fixnum(end)])
                .and_then(|_| eval.funcall_general(source, Vec::<Value>::new()));
        let narrow_result = eval.unbind_to_with_result(count, narrow_result);
        source = narrow_result?;
    }

    let source_value = replace_region_source_value_in_state(&eval.buffers, &source, current_id)?;
    // When SOURCE is a buffer, source_value is a FRESH substring held only
    // in this Rust local while modification hooks run arbitrary Lisp during
    // the replacement below; root it for the rest of the function. Normal
    // exits restore explicitly; error unwinds pop the GcRoot with the frame.
    let source_root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(source_value);

    let read_only_buffer_name = eval.buffers.current_buffer().and_then(|buf| {
        if super::editfns::buffer_read_only_active_in_state(&eval.obarray, &[], buf) {
            Some(buf.name_value())
        } else {
            None
        }
    });
    if let Some(name) = read_only_buffer_name {
        return Err(signal(LispCondition::BufferReadOnly, vec![name]));
    }

    let byte_range = {
        let buf = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let start_byte = buf.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(start));
        let end_byte = buf.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(end));

        if start_byte <= end_byte {
            EmacsByteRange::new(start_byte, end_byte)
        } else {
            EmacsByteRange::new(end_byte, start_byte)
        }
    };
    let old_range = super::editfns::buffer_edit_range_for_byte_range_in_manager(
        &eval.buffers,
        current_id,
        byte_range,
    )?;
    let target_multibyte = current_buffer_multibyte(&eval.buffers)?;
    // GNU 31's Freplace_region_contents currently passes its Lisp INHERIT
    // argument to replace_range's adjust-match-data parameter and hard-codes
    // the distinct property-inheritance parameter to false (editfns.c).
    // Neomacs does not yet expose that internal match-data switch, but it must
    // keep the two semantic axes separate: replacement SOURCE properties are
    // copied, while adjoining destination properties are not inherited.
    let _adjust_match_data = args.get(5).is_some_and(|value| value.is_truthy());

    // GNU `Freplace_region_contents` falls back to a plain `delete-region` +
    // `insert` when MAX-SECS or MAX-COSTS is exactly 0 (the comparison step is
    // disabled).  Detect that here so callers retain the escape hatch.
    let comparison_disabled = matches!(args.get(3), Some(v) if eq_fixnum_zero(v))
        || matches!(args.get(4), Some(v) if eq_fixnum_zero(v));

    // Decode the destination region (A) and the source (B) into codepoint
    // sequences so we can compute a minimal edit.  The source is first
    // converted into the destination buffer's representation so both sides use
    // the same encoding.
    let region_bytes = eval
        .buffers
        .get(current_id)
        .map(|buf| buf.buffer_substring_bytes_range(byte_range))
        .unwrap_or_default();
    let source_string = source_value
        .as_lisp_string()
        .map(|ls| buffer_insert_lisp_string_from_lisp_string(ls, target_multibyte))
        .unwrap_or_else(|| lisp_string_from_buffer_bytes(Vec::new(), target_multibyte));

    let a_decoded = decode_chars_with_byte_offsets(&region_bytes);
    let b_decoded = decode_chars_with_byte_offsets(source_string.as_bytes());
    // Strip the trailing sentinel for the codepoint comparison.
    let a_codes: Vec<u32> = a_decoded[..a_decoded.len() - 1]
        .iter()
        .map(|&(c, _)| c)
        .collect();
    let b_codes: Vec<u32> = b_decoded[..b_decoded.len() - 1]
        .iter()
        .map(|&(c, _)| c)
        .collect();

    // The diff machinery is not prepared for an empty side: just delete or
    // insert wholesale.  This also covers the trivial cases GNU handles up
    // front (both empty -> nothing to do).
    if comparison_disabled || a_codes.is_empty() || b_codes.is_empty() {
        let replacement = buffer_insert_piece_from_string(source_value, target_multibyte)?
            .into_replacement_text();
        let new_extent = super::editfns::lisp_string_text_extent(&replacement);
        let change = TextChange::new(old_range, new_extent);
        super::editfns::signal_before_text_change(eval, change)?;
        eval.buffers
            .replace_buffer_measured_region_lisp_string(current_id, old_range, &replacement)
            .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
        super::editfns::signal_after_text_change(eval, change)?;
        eval.restore_specpdl_roots(source_root_scope);
        return Ok(Value::T);
    }

    // Compute the minimal change runs (Myers O(ND), like GNU's compareseq).
    let runs = replace_region_minimal_change_runs(&a_codes, &b_codes);

    // GNU `Freplace_region_contents` (editfns.c) calls `Fundo_boundary` once
    // compareseq has succeeded, before walking the change runs — including
    // when there is nothing to change.  Besides the boundary itself this sets
    // `point_before_last_command_or_undo`, which the per-run `record_point`
    // then conses ahead of the first change record.  The trivial
    // empty-side/comparison-disabled paths above return before reaching it,
    // matching GNU.
    // GNU's `Freplace_buffer_contents' calls `Fundo_boundary' (src/editfns.c:2139),
    // so it sets `undo-auto--last-boundary-cause' by exactly the same route.
    if eval
        .buffers
        .add_undo_boundary(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?
        == crate::buffer::buffer::UndoBoundaryOutcome::Recorded
    {
        crate::emacs_core::undo::set_last_boundary_cause_explicit(eval)?;
    }

    if runs.is_empty() {
        // Buffers already identical within the region: nothing to do, and no
        // markers/point are disturbed.  GNU returns t in this case.
        eval.restore_specpdl_roots(source_root_scope);
        return Ok(Value::T);
    }

    // Announce a single modification spanning the whole region, exactly like
    // GNU which calls `prepare_to_modify_buffer` once and binds
    // `inhibit-modification-hooks` while issuing the per-run replacements.
    let new_extent = super::editfns::lisp_string_text_extent(&source_string);
    let change = TextChange::new(old_range, new_extent);
    super::editfns::signal_before_text_change(eval, change)?;

    let region_start = byte_range.start().get();
    // Apply the change runs back-to-front so that earlier byte positions stay
    // valid as we edit (mirrors GNU walking the change lists backwards).
    for run in runs.iter().rev() {
        let del_start = EmacsBytePos::new(region_start + a_decoded[run.a_start].1);
        let del_end = EmacsBytePos::new(region_start + a_decoded[run.a_end].1);
        let del_range = EmacsByteRange::new(del_start, del_end);

        // Replacement text for this run: characters [b_start, b_end) of the
        // source, sliced from the original SOURCE value so text properties are
        // preserved.
        let replacement = if run.b_start == run.b_end {
            Value::string("")
        } else {
            super::builtins::strings::builtin_substring(vec![
                source_value,
                Value::fixnum(run.b_start as i64),
                Value::fixnum(run.b_end as i64),
            ])?
        };
        let replacement =
            buffer_insert_piece_from_string(replacement, target_multibyte)?.into_replacement_text();
        eval.buffers
            .replace_buffer_emacs_byte_range_lisp_string(current_id, del_range, &replacement)
            .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    }

    super::editfns::signal_after_text_change(eval, change)?;

    eval.restore_specpdl_roots(source_root_scope);
    Ok(Value::T)
}

/// True when `value` is the fixnum 0 (used for GNU's MAX-SECS/MAX-COSTS == 0
/// "disable comparison" fallback).
fn eq_fixnum_zero(value: &Value) -> bool {
    value.as_fixnum() == Some(0)
}

pub(crate) fn builtin_set_buffer_multibyte(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-buffer-multibyte", &args, 1)?;
    let flag = args[0];
    let target_multibyte = !flag.is_nil();
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let (already_multibyte, narrowed, base_buffer, shared_ids) = {
        let current = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            current.get_multibyte(),
            current.is_narrowed(),
            current.base_buffer,
            eval.buffers.shared_text_buffer_ids(current_id),
        )
    };
    let old_undo_list = eval
        .buffers
        .get(current_id)
        .map(|buffer| buffer.get_undo_list())
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if base_buffer.is_some() {
        return Err(signal(
            "error",
            vec![Value::string(
                "Cannot do `set-buffer-multibyte' on an indirect buffer",
            )],
        ));
    }

    if already_multibyte == target_multibyte {
        return Ok(flag);
    }

    if narrowed {
        return Err(signal(
            "error",
            vec![Value::string("Changing multibyteness in a narrowed buffer")],
        ));
    }

    #[derive(Clone, Copy)]
    struct OverlaySnapshot {
        overlay: Value,
        start_old_emacs_byte: EmacsBytePos,
        end_old_emacs_byte: EmacsBytePos,
    }

    struct BufferSnapshot {
        id: BufferId,
        pt_old_emacs_byte: EmacsBytePos,
        begv_old_emacs_byte: EmacsBytePos,
        zv_old_emacs_byte: EmacsBytePos,
        mark_old_emacs_byte: Option<EmacsBytePos>,
        last_window_start_old_emacs_byte: EmacsBytePos,
        overlays: Vec<OverlaySnapshot>,
    }

    let snapshots = {
        let mut snapshots = Vec::with_capacity(shared_ids.len());
        for id in &shared_ids {
            let buffer = eval
                .buffers
                .get(*id)
                .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
            let overlays = buffer
                .overlays
                .dump_overlays()
                .into_iter()
                .filter_map(|overlay| {
                    let data = overlay.as_overlay_data()?;
                    let (start, end) = data.current_range();
                    let total_end = buffer.total_emacs_byte_end_pos();
                    Some(OverlaySnapshot {
                        overlay,
                        start_old_emacs_byte: EmacsBytePos::new(start).min(total_end),
                        end_old_emacs_byte: EmacsBytePos::new(end).min(total_end),
                    })
                })
                .collect();
            let last_window_start = buffer.last_window_start;
            let total_end = buffer.total_emacs_byte_end_pos();
            snapshots.push(BufferSnapshot {
                id: *id,
                pt_old_emacs_byte: buffer.point_emacs_byte_pos().min(total_end),
                begv_old_emacs_byte: buffer.point_min_emacs_byte_pos().min(total_end),
                zv_old_emacs_byte: buffer.point_max_emacs_byte_pos().min(total_end),
                mark_old_emacs_byte: buffer.mark_emacs_byte_pos().map(|mark| mark.min(total_end)),
                last_window_start_old_emacs_byte: buffer
                    .lisp_pos_to_full_buffer_emacs_byte_pos(last_window_start),
                overlays,
            });
        }
        snapshots
    };

    let source_value = {
        let buffer = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buffer_slice_value_range(buffer, buffer.full_emacs_byte_range())
    };
    let old_bytes: Vec<u8> = source_value
        .as_lisp_string()
        .map(|s| s.as_bytes().to_vec())
        .unwrap_or_default();
    let old_byte_len = old_bytes.len();
    let (converted_value, mode) = convert_buffer_string_for_multibyte(source_value, flag)?;
    let piece = buffer_insert_piece_from_string(converted_value, target_multibyte)?;
    let new_storage = piece.text;
    let new_total_bytes = new_storage.sbytes();
    let conversion_byte_map = build_multibyte_conversion_byte_map(&old_bytes, mode);

    let new_props = {
        let buffer = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let old_props = buffer.text_props_snapshot();
        match mode {
            BufferMultibyteConversionMode::ToMultibyte => old_props,
            BufferMultibyteConversionMode::AsUnibyte
            | BufferMultibyteConversionMode::AsMultibyte => {
                remap_text_property_table(&old_props, |char_pos| {
                    let logical_byte = buffer
                        .char_pos_to_emacs_byte_pos_clamped(CharPos0::new(char_pos))
                        .get();
                    let boundary = remap_old_byte_to_new_boundary(
                        &conversion_byte_map,
                        old_byte_len,
                        &new_storage,
                        new_total_bytes,
                        logical_byte,
                    );
                    lisp_string_byte_to_char(&new_storage, boundary)
                })
            }
        }
    };

    // Walk the intrusive marker chain and remap each logical byte position
    // through the same boundary arithmetic used for point, narrowing and
    // overlays.
    {
        let buffer = eval
            .buffers
            .get_mut(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buffer.remap_text_marker_anchors(|old_position| {
            let old_byte = old_position.emacs_byte_pos().get();
            let boundary = remap_old_byte_to_new_boundary(
                &conversion_byte_map,
                old_byte_len,
                &new_storage,
                new_total_bytes,
                old_byte,
            );
            let new_char = lisp_string_byte_to_char(&new_storage, boundary);
            TextPositionAnchor::new(CharPos0::new(new_char), EmacsBytePos::new(boundary))
        });
        buffer.replace_lisp_string_with_text_props(&new_storage, new_props);
    }

    // Set the multibyte flag BEFORE remapping positions. GNU does
    // `bset_enable_multibyte_characters` first ("so that chars_in_text asks the
    // right question"): the buffer's byte<->char conversion used by point /
    // narrowing / marker remapping must see the new multibyteness, otherwise it
    // counts every byte as a character.
    for id in &shared_ids {
        eval.buffers
            .set_buffer_multibyte_flag(*id, target_multibyte)
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
    }

    for snapshot in snapshots {
        let map_boundary = |logical_byte: usize| {
            remap_old_byte_to_new_boundary(
                &conversion_byte_map,
                old_byte_len,
                &new_storage,
                new_total_bytes,
                logical_byte,
            )
        };

        let pt_byte = map_boundary(snapshot.pt_old_emacs_byte.get());
        let begv_byte = map_boundary(snapshot.begv_old_emacs_byte.get());
        let zv_byte = map_boundary(snapshot.zv_old_emacs_byte.get());
        let mark_byte = snapshot
            .mark_old_emacs_byte
            .map(|old_byte| map_boundary(old_byte.get()));
        let last_window_start_byte = map_boundary(snapshot.last_window_start_old_emacs_byte.get());

        eval.buffers
            .restore_buffer_emacs_byte_restriction(
                snapshot.id,
                EmacsByteRange::new(EmacsBytePos::new(begv_byte), EmacsBytePos::new(zv_byte)),
            )
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
        eval.buffers
            .goto_buffer_emacs_byte_pos(snapshot.id, EmacsBytePos::new(pt_byte))
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;

        if let Some(mark_byte) = mark_byte {
            eval.buffers
                .set_buffer_mark_emacs_byte_pos(snapshot.id, EmacsBytePos::new(mark_byte))
                .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
        } else {
            eval.buffers
                .clear_buffer_mark(snapshot.id)
                .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
        }

        eval.buffers
            .set_buffer_last_window_start(
                snapshot.id,
                LispCharPos1::from_one_based_usize(
                    lisp_string_byte_to_char(&new_storage, last_window_start_byte) + 1,
                ),
            )
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;

        for overlay in snapshot.overlays {
            let start_byte = map_boundary(overlay.start_old_emacs_byte.get());
            let end_byte = map_boundary(overlay.end_old_emacs_byte.get());
            eval.buffers
                .move_buffer_overlay_to_emacs_byte_range(
                    snapshot.id,
                    overlay.overlay,
                    EmacsByteRange::new(EmacsBytePos::new(start_byte), EmacsBytePos::new(end_byte)),
                )
                .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
        }

        eval.buffers
            .set_buffer_multibyte_flag(snapshot.id, target_multibyte)
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
    }

    if !old_undo_list.is_t() {
        let restore_flag = if flag.is_nil() { Value::T } else { Value::NIL };
        let undo_entry = Value::list(vec![
            Value::symbol("apply"),
            Value::symbol("set-buffer-multibyte"),
            restore_flag,
        ]);
        let _ = eval
            .buffers
            .configure_buffer_undo_list(current_id, Value::cons(undo_entry, old_undo_list));
    }
    Ok(flag)
}

/// `(split-window-internal OLD PIXEL-SIZE SIDE NORMAL-SIZE &optional REFER)`
///
/// GNU `src/window.c::Fsplit_window_internal` honors all five
/// arguments. The fourth argument NORMAL-SIZE seeds the new
/// window's `normal_lines`/`normal_cols` slot so future
/// proportional resizes preserve the requested ratio. The fifth
/// argument REFER lets `set-window-configuration` revive a
/// previously-deleted window by id, restoring its parameters,
/// dedication, and history alists.
///
/// Window audit Critical 5 in `drafts/window-system-audit.md`:
/// neomacs accepts both arguments for arity compatibility but
/// drops them on the floor. NORMAL-SIZE is observable as soon as
/// audit Critical 7 lands the per-window normal-size fields; REFER
/// is observable when window.el's `display-buffer` falls back to
/// reviving a deleted window inside `set-window-configuration`.
///
/// Both fixes are deferred until the structural prereqs land
/// (per-window normal_lines/cols storage and a deleted-window
/// revival registry).
pub(crate) fn builtin_split_window_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("split-window-internal", &args, 4, 5)?;
    if !args[1].is_nil() {
        let _ = expect_fixnum(&args[1])?;
    }
    if !args[2].is_nil() && !args[2].is_symbol() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[2]],
        ));
    }

    // REFER is accepted for arity compatibility and ignored.  NORMAL-SIZE is
    // NOT ignored: GNU stages it as the new window's `new_normal'
    // (`src/window.c:5650') for `window_resize_apply' to commit, and
    // `lisp/window.el' relies on that to give the new window its proportional
    // share -- especially under `window-combination-resize', where every
    // sibling's fraction is restaged too.
    if let Some(refer) = args.get(4) {
        let _ = refer;
    }
    // GNU `Fsplit_window_internal` reads the dynamic variable
    // `window-combination-limit' to decide whether to interpose a new parent
    // window (`src/window.c:5426').  Only the symbol `t' forces one; `lisp/window.el'
    // binds it to t for, among others, a split whose target has a side-window
    // sibling, which is what keeps a frame's main area in its own combination.
    let combination_limit = crate::window::CombinationLimit::from_is_t(
        super::builtins::misc_eval::dynamic_or_global_symbol_value(
            eval,
            "window-combination-limit",
        )
        .is_some_and(|value| value.is_t()),
    );
    let result = super::window_cmds::split_window_internal_impl_in_state_with_normal(
        &mut eval.frames,
        &mut eval.buffers,
        args[0],
        args[1],
        args[2],
        args[3],
        combination_limit,
    )?;
    // GNU does NOT run `window-configuration-change-hook' eagerly from
    // `split-window-internal'.  It is deferred to `run_window_change_functions'
    // during redisplay (window.c:4308-4312); neomacs mirrors this in
    // `run_redisplay_window_change_hooks'.  Firing it here diverged from GNU in
    // batch (no redisplay), where the hook must not run.
    Ok(result)
}

/// `(compare-buffer-substrings BUF1 START1 END1 BUF2 START2 END2)` -> integer
pub(crate) fn builtin_compare_buffer_substrings(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let case_fold =
        super::builtins::misc_eval::dynamic_or_global_symbol_value(eval, "case-fold-search")
            .map(|value| !value.is_nil())
            .unwrap_or(true);
    builtin_compare_buffer_substrings_with_case_fold(case_fold, &eval.buffers, args)
}

pub(crate) fn builtin_compare_buffer_substrings_with_case_fold(
    case_fold: bool,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("compare-buffer-substrings", &args, 6)?;

    let left_buffer = resolve_buffer_designator_allow_nil_current_in_manager(buffers, &args[0])?;
    let right_buffer = resolve_buffer_designator_allow_nil_current_in_manager(buffers, &args[3])?;

    let (left_start, left_end) =
        resolve_lisp_range_with_buffer_defaults(buffers, left_buffer, &args[1], &args[2])?;
    let (right_start, right_end) =
        resolve_lisp_range_with_buffer_defaults(buffers, right_buffer, &args[4], &args[5])?;

    let left = checked_buffer_slice_for_char_region_in_manager(
        buffers,
        left_buffer,
        left_start,
        left_end,
        args[1],
        args[2],
    )?;
    let right = checked_buffer_slice_for_char_region_in_manager(
        buffers,
        right_buffer,
        right_start,
        right_end,
        args[4],
        args[5],
    )?;
    Ok(Value::fixnum(compare_buffer_substring_strings(
        &left, &right, case_fold,
    )))
}

/// Extract two fixnums from a cons cell (CAR . CDR).
pub(crate) fn extract_cons_fixnums(val: Value) -> Result<(i64, i64), Flow> {
    match val.kind() {
        ValueKind::Cons => {
            let car = val.cons_car();
            let cdr = val.cons_cdr();
            let a = match car.kind() {
                ValueKind::Fixnum(n) => n,
                _ => {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("fixnump"), car],
                    ));
                }
            };
            let b = match cdr.kind() {
                ValueKind::Fixnum(n) => n,
                _ => {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("fixnump"), cdr],
                    ));
                }
            };
            Ok((a, b))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), val],
        )),
    }
}

pub(crate) fn builtin_coordinates_in_window_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let frames = &mut eval.frames;
    let buffers = &mut eval.buffers;
    expect_args("coordinates-in-window-p", &args, 2)?;

    let (x, y) = if args[0].is_cons() {
        let car = args[0].cons_car();
        let cdr = args[0].cons_cdr();
        let x = match car.kind() {
            ValueKind::Fixnum(n) => n as f64,
            ValueKind::Float => car.xfloat(),
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("numberp"), car],
                ));
            }
        };
        let y = match cdr.kind() {
            ValueKind::Fixnum(n) => n as f64,
            ValueKind::Float => cdr.xfloat(),
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("numberp"), cdr],
                ));
            }
        };
        (x, y)
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), args[0]],
        ));
    };

    let window_arg = args[1];
    let width = match super::window_cmds::window_total_width_impl(
        frames,
        buffers,
        vec![window_arg],
    )?
    .kind()
    {
        ValueKind::Fixnum(n) => n as f64,
        _ => 0.0,
    };
    let height =
        match super::window_cmds::window_total_height_impl(frames, buffers, vec![window_arg])?
            .kind()
        {
            ValueKind::Fixnum(n) => n as f64,
            _ => 0.0,
        };

    if x >= 0.0 && y >= 0.0 && x < width && y < height {
        Ok(args[0])
    } else {
        Ok(Value::NIL)
    }
}

struct ConstrainToFieldSyms {
    field: crate::emacs_core::intern::SymId,
    category: crate::emacs_core::intern::SymId,
    inhibit_field_text_motion: crate::emacs_core::intern::SymId,
    char_property_alias_alist: crate::emacs_core::intern::SymId,
    default_text_properties: crate::emacs_core::intern::SymId,
}

fn constrain_to_field_syms() -> &'static ConstrainToFieldSyms {
    static SYMS: std::sync::OnceLock<ConstrainToFieldSyms> = std::sync::OnceLock::new();
    SYMS.get_or_init(|| ConstrainToFieldSyms {
        field: crate::emacs_core::intern::intern("field"),
        category: crate::emacs_core::intern::intern("category"),
        inhibit_field_text_motion: crate::emacs_core::intern::intern("inhibit-field-text-motion"),
        char_property_alias_alist: crate::emacs_core::intern::intern("char-property-alias-alist"),
        default_text_properties: crate::emacs_core::intern::intern("default-text-properties"),
    })
}

/// True when no position in the current buffer can yield a `field` char
/// property: no overlays, neither `field` nor a `category` symbol (whose
/// plist GNU `textget` would consult) ever assigned as a text property, and
/// no alias alist or `default-text-properties` that could supply one.
fn current_buffer_cannot_have_fields(eval: &super::eval::Context) -> bool {
    use crate::buffer::text_props::PropertyNamePresence::DefinitelyAbsent;
    let syms = constrain_to_field_syms();
    let Some(buf) = eval.buffers.current_buffer() else {
        return false;
    };
    let nil_var = |sym: crate::emacs_core::intern::SymId| {
        eval.eval_symbol_by_id(sym)
            .is_ok_and(|value| value.is_nil())
    };
    buf.overlays.is_empty()
        && buf.text_props_property_name_presence(Value::from_sym_id(syms.field)) == DefinitelyAbsent
        && buf.text_props_property_name_presence(Value::from_sym_id(syms.category))
            == DefinitelyAbsent
        && nil_var(syms.char_property_alias_alist)
        && nil_var(syms.default_text_properties)
}

pub(crate) fn builtin_constrain_to_field(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("constrain-to-field", &args, 2, 5)?;
    let current = &mut eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let point_min = current.point_min_lisp_char_pos().as_i64();
    let orig_point = if args[0].is_nil() {
        Some(current.point_lisp_char_pos().as_i64())
    } else {
        None
    };
    let mut new_pos = if let Some(point) = orig_point {
        point
    } else {
        expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?
    };
    let old_pos = expect_integer_or_marker_in_buffers(&eval.buffers, &args[1])?;
    let escape_from_edge = args.get(2).is_some_and(|value| value.is_truthy());
    let only_in_line = args.get(3).is_some_and(|value| value.is_truthy());

    // GNU evaluates these probes lazily inside ONE short-circuiting `&&`
    // (editfns.c `Fconstrain_to_field`): `inhibit-field-text-motion` and
    // `new_pos == old_pos` are checked first, so a buffer with no field
    // properties never pays for a single `get-char-property` lookup.
    //
    // Computing them eagerly cost ~5x GNU on `line-beginning-position`, which
    // dired's font-lock calls once per line: 6 property lookups plus 4 `field`
    // symbol interns on every call, all of them thrown away.
    let inhibit_field_text_motion = eval
        .eval_symbol_by_id(constrain_to_field_syms().inhibit_field_text_motion)
        .is_ok_and(|value| !value.is_nil());

    let mut constrain = !inhibit_field_text_motion && new_pos != old_pos;
    if constrain && current_buffer_cannot_have_fields(eval) {
        // GNU would now run up to four `Fget_char_property` probes; when the
        // buffer cannot hold a `field` anywhere they all answer nil, and
        // `line-beginning-position` calls this once per line (~1.8K Ir).
        constrain = false;
    }

    if constrain {
        let field = Value::from_sym_id(constrain_to_field_syms().field);
        let has_field = |eval: &super::eval::Context, pos: i64| -> Result<bool, Flow> {
            Ok(
                !char_property_in_current_buffer(&eval.obarray, &eval.buffers, pos, field)?
                    .is_nil(),
            )
        };
        constrain = has_field(eval, new_pos)?
            || has_field(eval, old_pos)?
            || (new_pos > point_min && has_field(eval, new_pos - 1)?)
            || (old_pos > point_min && has_field(eval, old_pos - 1)?);
    }

    if constrain && let Some(capture_prop) = args.get(4).filter(|value| !value.is_nil()) {
        let old_capture = crate::emacs_core::builtins::misc_eval::builtin_get_pos_property_impl(
            &eval.obarray,
            &[],
            None,
            &eval.buffers,
            vec![Value::fixnum(old_pos), *capture_prop],
        )?;
        constrain = old_capture.is_nil()
            && (old_pos <= point_min
                || char_property_in_current_buffer(
                    &eval.obarray,
                    &eval.buffers,
                    old_pos,
                    *capture_prop,
                )?
                .is_nil()
                || char_property_in_current_buffer(
                    &eval.obarray,
                    &eval.buffers,
                    old_pos - 1,
                    *capture_prop,
                )?
                .is_nil());
    }

    if constrain {
        let forward = new_pos > old_pos;
        let field_bound = if forward {
            expect_int(&builtin_field_end(
                eval,
                vec![
                    Value::fixnum(old_pos),
                    Value::bool_val(escape_from_edge),
                    Value::fixnum(new_pos),
                ],
            )?)?
        } else {
            expect_int(&builtin_field_beginning(
                eval,
                vec![
                    Value::fixnum(old_pos),
                    Value::bool_val(escape_from_edge),
                    Value::fixnum(new_pos),
                ],
            )?)?
        };

        let should_constrain = if field_bound < new_pos {
            forward
        } else {
            !forward
        };
        let same_line = !only_in_line
            || !current_buffer_has_newline_between_positions(&eval.buffers, new_pos, field_bound)?;
        if should_constrain && same_line {
            new_pos = field_bound;
        }
    }

    if let Some(orig_point) = orig_point
        && new_pos != orig_point
    {
        let current_id = eval
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let buf = &mut eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let byte_pos = buf.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(new_pos));
        let _ = eval
            .buffers
            .goto_buffer_emacs_byte_pos(current_id, byte_pos);
    }

    Ok(Value::fixnum(new_pos))
}

fn char_property_in_current_buffer(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &BufferManager,
    pos: i64,
    property: Value,
) -> Result<Value, Flow> {
    crate::emacs_core::textprop::builtin_get_char_property_in_state(
        obarray,
        buffers,
        vec![Value::fixnum(pos), property],
    )
}

fn current_buffer_has_newline_between_positions(
    buffers: &BufferManager,
    left: i64,
    right: i64,
) -> Result<bool, Flow> {
    let Some(current_id) = buffers.current_buffer_id() else {
        return Err(signal("error", vec![Value::string("No current buffer")]));
    };
    let text = checked_buffer_slice_for_char_region_in_manager(
        buffers,
        Some(current_id),
        LispCharPos1::new(left.min(right)),
        LispCharPos1::new(left.max(right)),
        Value::fixnum(left.min(right)),
        Value::fixnum(left.max(right)),
    )?;
    // A newline is the single ASCII byte 0x0A in Emacs' internal encoding.
    Ok(text.as_bytes().contains(&b'\n'))
}

fn resolve_field_position_in_buffers(
    buffers: &BufferManager,
    position_value: Option<&Value>,
) -> Result<(i64, i64, i64), Flow> {
    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    let pos = match position_value {
        None => point_char_pos(buf, buf.point_emacs_byte_pos()),
        Some(value) if value.is_nil() => point_char_pos(buf, buf.point_emacs_byte_pos()),
        Some(value) => expect_integer_or_marker_in_buffers(buffers, value)?,
    };
    if pos < point_min || pos > point_max {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::fixnum(pos)],
        ));
    }
    Ok((pos, point_min, point_max))
}

fn field_property_after_char_in_buffers(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &BufferManager,
    pos: i64,
) -> Result<Value, Flow> {
    let value = crate::emacs_core::textprop::builtin_get_char_property_and_overlay_in_state(
        obarray,
        buffers,
        vec![Value::fixnum(pos), Value::symbol("field")],
    )?;
    match value.kind() {
        ValueKind::Cons => Ok(value.cons_car()),
        _other => Err(signal("error", vec![value])),
    }
}

fn field_property_at_position_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &BufferManager,
    pos: i64,
) -> Result<Value, Flow> {
    crate::emacs_core::builtins::misc_eval::builtin_get_pos_property_impl(
        obarray,
        dynamic,
        None,
        buffers,
        vec![Value::fixnum(pos), Value::symbol("field")],
    )
}

fn previous_field_change_in_buffers(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &BufferManager,
    pos: i64,
    limit: Option<i64>,
) -> Result<i64, Flow> {
    let mut args = vec![Value::fixnum(pos), Value::symbol("field")];
    if let Some(limit) = limit {
        args.push(Value::NIL);
        args.push(Value::fixnum(limit));
    }
    expect_int(
        &crate::emacs_core::builtins::misc_eval::builtin_previous_single_char_property_change_in_buffers(
            obarray, None, buffers, args,
        )?,
    )
}

fn next_field_change_in_buffers(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &BufferManager,
    pos: i64,
    limit: Option<i64>,
) -> Result<i64, Flow> {
    let mut args = vec![Value::fixnum(pos), Value::symbol("field")];
    if let Some(limit) = limit {
        args.push(Value::NIL);
        args.push(Value::fixnum(limit));
    }
    expect_int(
        &crate::emacs_core::builtins::misc_eval::builtin_next_single_char_property_change_in_buffers(
            obarray, None, buffers, args,
        )?,
    )
}

pub(crate) fn find_field_bounds_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &BufferManager,
    position_value: Option<&Value>,
    merge_at_boundary: bool,
    beg_limit: Option<i64>,
    end_limit: Option<i64>,
) -> Result<(i64, i64), Flow> {
    let (pos, point_min, _point_max) = resolve_field_position_in_buffers(buffers, position_value)?;
    let after_field = field_property_after_char_in_buffers(obarray, buffers, pos)?;
    let before_field = if pos > point_min {
        field_property_after_char_in_buffers(obarray, buffers, pos - 1)?
    } else {
        after_field
    };

    let mut at_field_start = false;
    let mut at_field_end = false;
    if !merge_at_boundary {
        let field = field_property_at_position_in_state(obarray, dynamic, buffers, pos)?;
        if !eq_value(&field, &after_field) {
            at_field_end = true;
        }
        if !eq_value(&field, &before_field) {
            at_field_start = true;
        }
        if field.is_nil() && at_field_start && at_field_end {
            at_field_start = false;
            at_field_end = false;
        }
    }

    let boundary = Value::symbol("boundary");
    let beg = if at_field_start {
        pos
    } else {
        let mut cursor = pos;
        if merge_at_boundary && eq_value(&before_field, &boundary) {
            cursor = previous_field_change_in_buffers(obarray, buffers, cursor, beg_limit)?;
        }
        previous_field_change_in_buffers(obarray, buffers, cursor, beg_limit)?
    };
    let end = if at_field_end {
        pos
    } else {
        let mut cursor = pos;
        if merge_at_boundary && eq_value(&after_field, &boundary) {
            cursor = next_field_change_in_buffers(obarray, buffers, cursor, end_limit)?;
        }
        next_field_change_in_buffers(obarray, buffers, cursor, end_limit)?
    };

    Ok((beg, end))
}

pub(crate) fn builtin_field_beginning(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("field-beginning", &args, 3)?;
    let limit = match args.get(2) {
        Some(limit_value) if !limit_value.is_nil() => {
            let limit = expect_integer_or_marker_in_buffers(&eval.buffers, limit_value)?;
            if limit <= 0 {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![Value::fixnum(limit)],
                ));
            }
            Some(limit)
        }
        _ => None,
    };
    let (beg, _) = find_field_bounds_in_state(
        &eval.obarray,
        &[],
        &eval.buffers,
        args.first(),
        args.get(1).is_some_and(|value| value.is_truthy()),
        limit,
        None,
    )?;
    Ok(Value::fixnum(beg))
}

pub(crate) fn builtin_field_end(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("field-end", &args, 3)?;
    let limit = match args.get(2) {
        Some(limit_value) if !limit_value.is_nil() => Some(expect_integer_or_marker_in_buffers(
            &eval.buffers,
            limit_value,
        )?),
        _ => None,
    };
    let (_, end) = find_field_bounds_in_state(
        &eval.obarray,
        &[],
        &eval.buffers,
        args.first(),
        args.get(1).is_some_and(|value| value.is_truthy()),
        None,
        limit,
    )?;
    Ok(Value::fixnum(end))
}

pub(crate) fn builtin_field_string(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("field-string", &args, 1)?;
    let (beg, end) = find_field_bounds_in_state(
        &eval.obarray,
        &[],
        &eval.buffers,
        args.first(),
        false,
        None,
        None,
    )?;
    builtin_buffer_substring(eval, vec![Value::fixnum(beg), Value::fixnum(end)])
}

pub(crate) fn builtin_field_string_no_properties(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("field-string-no-properties", &args, 1)?;
    let (beg, end) = find_field_bounds_in_state(
        &eval.obarray,
        &[],
        &eval.buffers,
        args.first(),
        false,
        None,
        None,
    )?;
    super::editfns::builtin_buffer_substring_no_properties(
        eval,
        vec![Value::fixnum(beg), Value::fixnum(end)],
    )
}

pub(crate) fn builtin_delete_field(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("delete-field", &args, 1)?;
    let (beg, end) = find_field_bounds_in_state(
        &eval.obarray,
        &[],
        &eval.buffers,
        args.first(),
        false,
        None,
        None,
    )?;
    super::editfns::builtin_delete_region(eval, vec![Value::fixnum(beg), Value::fixnum(end)])
}

/// `(clear-string STRING)` -> nil
/// Zeroes every byte, makes STRING unibyte, and removes text properties.
pub(crate) fn builtin_clear_string(args: Vec<Value>) -> EvalResult {
    expect_args("clear-string", &args, 1)?;
    let _ = expect_lisp_string(&args[0])?;
    if args[0].is_string() {
        let _ = args[0].with_lisp_string_mut(|lisp_str| {
            let len = lisp_str.sbytes();
            *lisp_str = crate::heap_types::LispString::from_unibyte(vec![0; len]);
        });
    }
    Ok(Value::NIL)
}

/// `(command-error-default-function DATA CONTEXT CALLER)` -> nil
///
/// GNU keyboard.c:1049-1101. Batch and pre-display sessions print the
/// diagnostic to stderr and exit with status 255; a live session messages it.
/// help.el's `help-command-error-confusable-suggestions` delegates here, so
/// this is what every unhandled command/filter/sentinel error is reported by.
pub(crate) fn builtin_command_error_default_function(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("command-error-default-function", &args, 3)?;
    eval.command_error_default_report(args[0], args[1])?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_point(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("point", &args, 0)?;
    builtin_point_0(eval)
}

pub(crate) fn builtin_point_0(eval: &mut super::eval::Context) -> EvalResult {
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::fixnum(buf.point_lisp_char_pos().as_i64()))
}

pub(crate) fn builtin_point_min_0(eval: &mut super::eval::Context) -> EvalResult {
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::fixnum(buf.point_min_lisp_char_pos().as_i64()))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_point_max(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("point-max", &args, 0)?;
    builtin_point_max_0(eval)
}

pub(crate) fn builtin_point_max_0(eval: &mut super::eval::Context) -> EvalResult {
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::fixnum(buf.point_max_lisp_char_pos().as_i64()))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_goto_char(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("goto-char", &args, 1)?;
    builtin_goto_char_1(eval, args[0])
}

pub(crate) fn builtin_goto_char_1(eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    // GNU returns POSITION itself (a marker stays a marker), so the raw
    // arg survives extraction; the typed position carries the coordinate.
    let pos = LispCharPos1::from_value(eval, arg)?;
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let (old_byte, byte_pos) = {
        let buf = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            buf.point_emacs_byte_pos(),
            buf.lisp_pos_to_accessible_emacs_byte_pos(pos),
        )
    };
    // Adjust for intangible text property
    let direction = if byte_pos >= old_byte { 1 } else { -1 };
    let adjusted = super::navigation::adjust_for_intangible(eval, byte_pos, direction);
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(current_id, adjusted);
    // Run point motion hooks
    super::navigation::check_point_motion_hooks(eval, old_byte, adjusted)?;
    Ok(arg)
}

struct InsertPiece {
    text: crate::heap_types::LispString,
    text_props: Option<crate::buffer::text_props::TextPropertyTable>,
}

impl InsertPiece {
    /// Reassemble converted text and properties for the typed replacement
    /// pipeline. Lisp string properties normally live beside a tagged Value;
    /// `ReplaceTextPlan` deliberately owns both in one `LispString`.
    fn into_replacement_text(mut self) -> crate::heap_types::LispString {
        if let Some(properties) = self.text_props {
            *self.text.intervals_mut() = properties;
        }
        self.text
    }
}

fn insert_pieces_extent(pieces: &[InsertPiece]) -> TextExtent {
    TextExtent::new(
        CharLen::new(pieces.iter().map(|piece| piece.text.schars()).sum()),
        EmacsByteLen::new(pieces.iter().map(|piece| piece.text.sbytes()).sum()),
    )
}

fn current_empty_text_change_at_emacs_byte_pos(
    buffers: &BufferManager,
    current_id: BufferId,
    byte_pos: EmacsBytePos,
    new_extent: TextExtent,
) -> Result<TextChange, Flow> {
    super::editfns::text_change_for_empty_insertion_at_emacs_byte_pos(
        buffers, current_id, byte_pos, new_extent,
    )
}

fn current_buffer_multibyte(buffers: &BufferManager) -> Result<bool, Flow> {
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buffers
        .get(current_id)
        .map(|buf| buf.get_multibyte())
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))
}

fn lisp_string_byte_to_char(string: &crate::heap_types::LispString, byte_pos: usize) -> usize {
    let clamped = byte_pos.min(string.sbytes());
    if string.is_multibyte() {
        crate::emacs_core::emacs_char::byte_to_char_pos(string.as_bytes(), clamped)
    } else {
        clamped
    }
}

fn lisp_string_char_to_byte(string: &crate::heap_types::LispString, char_pos: usize) -> usize {
    let clamped = char_pos.min(string.schars());
    if string.is_multibyte() {
        crate::emacs_core::emacs_char::char_to_byte_pos(string.as_bytes(), clamped)
    } else {
        clamped
    }
}

fn lisp_string_advance_byte_to_boundary(
    string: &crate::heap_types::LispString,
    byte_pos: usize,
) -> usize {
    let clamped = byte_pos.min(string.sbytes());
    if !string.is_multibyte() {
        return clamped;
    }

    let bytes = string.as_bytes();
    let mut pos = 0usize;
    while pos < clamped && pos < bytes.len() {
        let (_, len) = crate::emacs_core::emacs_char::string_char(&bytes[pos..]);
        if pos + len >= clamped {
            return pos + len;
        }
        pos += len;
    }
    clamped
}

fn remap_text_property_table(
    table: &crate::buffer::text_props::TextPropertyTable,
    char_map: impl Fn(usize) -> usize,
) -> crate::buffer::text_props::TextPropertyTable {
    let intervals = table
        .intervals_snapshot()
        .into_iter()
        .filter_map(|interval| {
            let start = char_map(interval.start);
            let end = char_map(interval.end);
            (start < end).then_some(crate::buffer::text_props::PropertyInterval {
                start,
                end,
                properties: interval.properties,
                key_order: interval.key_order,
            })
        })
        .collect();
    crate::buffer::text_props::TextPropertyTable::from_dump(intervals)
}

fn buffer_insert_char_codes(
    string: &crate::heap_types::LispString,
    target_multibyte: bool,
) -> Vec<u32> {
    let mut codes = super::builtins::lisp_string_char_codes(string);
    if target_multibyte {
        if !string.is_multibyte() {
            for code in &mut codes {
                if *code > 0x7F {
                    *code = crate::emacs_core::emacs_char::unibyte_to_char(*code as u8);
                }
            }
        }
    } else {
        for code in &mut codes {
            *code &= 0xFF;
        }
    }
    codes
}

fn encode_char_code_for_buffer_bytes(code: u32, multibyte: bool) -> Option<Vec<u8>> {
    if code > crate::emacs_core::emacs_char::MAX_CHAR {
        return None;
    }
    if multibyte {
        let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
        Some(buf[..len].to_vec())
    } else {
        Some(vec![(code & 0xFF) as u8])
    }
}

fn buffer_insert_lisp_string_from_lisp_string(
    string: &crate::heap_types::LispString,
    target_multibyte: bool,
) -> crate::heap_types::LispString {
    // GNU `insert_from_string_1`/`copy_text`: when source and target share
    // multibyteness the text copies verbatim — and GNU does NOT normalize
    // the bytes, so a byte copy is also the faithful behavior. The previous
    // decode-all-chars → re-encode-each-char round trip was an identity
    // transform costing one Vec per character.
    if string.is_multibyte() == target_multibyte {
        return string.clone();
    }
    if target_multibyte {
        let bytes = string.as_bytes();
        // Unibyte ASCII is already valid canonical multibyte content. Copy
        // with one spare slot so the constructor's trailing-NUL push cannot
        // force a realloc + full re-copy of an exact-capacity Vec.
        if bytes.iter().all(|&byte| byte < 0x80) {
            let mut owned = Vec::with_capacity(bytes.len() + 1);
            owned.extend_from_slice(bytes);
            return crate::heap_types::LispString::from_emacs_bytes(owned);
        }
        // Real conversion: raw 128-255 bytes become their (byte8) chars.
        // Encode through one reused stack buffer, not a Vec per char.
        let codes = buffer_insert_char_codes(string, true);
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len() * 2);
        let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
        for code in codes {
            assert!(
                code <= crate::emacs_core::emacs_char::MAX_CHAR,
                "valid Emacs character code must encode into buffer bytes"
            );
            let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
            out.extend_from_slice(&buf[..len]);
        }
        lisp_string_from_buffer_bytes(out, true)
    } else {
        let codes = buffer_insert_char_codes(string, false);
        let bytes: Vec<u8> = codes
            .into_iter()
            .map(|code| {
                assert!(
                    code <= 0xFF,
                    "unibyte insertion produced non-byte character code {code:#X}"
                );
                code as u8
            })
            .collect();
        lisp_string_from_buffer_bytes(bytes, false)
    }
}

fn buffer_insert_piece_from_string(
    value: Value,
    target_multibyte: bool,
) -> Result<InsertPiece, Flow> {
    let source = value.as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), value],
        )
    })?;
    let text = buffer_insert_lisp_string_from_lisp_string(source, target_multibyte);
    let text_props = get_string_text_properties_table_for_value(value).and_then(|table| {
        if table.is_empty() {
            return None;
        }
        Some(table)
    });
    Ok(InsertPiece { text, text_props })
}

pub(crate) fn lisp_string_from_buffer_bytes(
    bytes: Vec<u8>,
    multibyte: bool,
) -> crate::heap_types::LispString {
    if multibyte {
        crate::heap_types::LispString::from_emacs_bytes(bytes)
    } else {
        crate::heap_types::LispString::from_unibyte(bytes)
    }
}

pub(crate) fn buffer_slice_value(
    buf: &crate::buffer::Buffer,
    start_byte: usize,
    end_byte: usize,
) -> Value {
    buffer_slice_value_range(
        buf,
        EmacsByteRange::new(EmacsBytePos::new(start_byte), EmacsBytePos::new(end_byte)),
    )
}

pub(crate) fn buffer_slice_value_range(
    buf: &crate::buffer::Buffer,
    byte_range: EmacsByteRange,
) -> Value {
    let mut bytes = Vec::new();
    buf.copy_emacs_byte_range_to(byte_range, &mut bytes);
    let string = lisp_string_from_buffer_bytes(bytes, buf.get_multibyte());
    let value = Value::heap_string(string);
    if !buf.text_props_is_empty() {
        let sliced = buf.text_props_slice_emacs_byte_range(byte_range);
        if !sliced.is_empty() {
            set_string_text_properties_table_for_value(value, sliced);
        }
    }
    value
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum BufferMultibyteConversionMode {
    AsUnibyte,
    AsMultibyte,
    ToMultibyte,
}

fn remap_string_text_props_for_conversion(
    source: Value,
    target: Value,
    mode: BufferMultibyteConversionMode,
) {
    let Some(table) = get_string_text_properties_table_for_value(source) else {
        return;
    };
    if table.is_empty() {
        return;
    }
    let remapped = match mode {
        BufferMultibyteConversionMode::ToMultibyte => table,
        BufferMultibyteConversionMode::AsUnibyte | BufferMultibyteConversionMode::AsMultibyte => {
            let source_string = source.as_lisp_string().expect("source string");
            let target_string = target.as_lisp_string().expect("target string");
            remap_text_property_table(&table, |char_pos| {
                let source_byte = lisp_string_char_to_byte(source_string, char_pos);
                let boundary = lisp_string_advance_byte_to_boundary(target_string, source_byte);
                lisp_string_byte_to_char(target_string, boundary)
            })
        }
    };
    if !remapped.is_empty() {
        set_string_text_properties_table_for_value(target, remapped);
    }
}

fn convert_buffer_string_for_multibyte(
    source: Value,
    flag: Value,
) -> Result<(Value, BufferMultibyteConversionMode), Flow> {
    let (converted, mode) = if flag.is_nil() {
        (
            misc::builtin_string_as_unibyte(vec![source])?,
            BufferMultibyteConversionMode::AsUnibyte,
        )
    } else if flag.as_symbol_name() == Some("to") {
        (
            misc::builtin_string_to_multibyte(vec![source])?,
            BufferMultibyteConversionMode::ToMultibyte,
        )
    } else {
        (
            misc::builtin_string_as_multibyte(vec![source])?,
            BufferMultibyteConversionMode::AsMultibyte,
        )
    };
    if converted != source {
        remap_string_text_props_for_conversion(source, converted, mode);
    }
    Ok((converted, mode))
}

/// Map each OLD content byte offset (into the pre-conversion buffer storage) to
/// the corresponding byte offset in the converted storage, by replaying the
/// conversion's exact byte consumption. A unibyte byte and a multibyte byte are
/// not the same position: converting unibyte `[0xFF 0xFE]` to multibyte yields 4
/// bytes (two eight-bit chars), so old byte 2 maps to new byte 4 — without this,
/// the old end position maps short and the buffer is wrongly narrowed. `map[k]`
/// is the new offset of old byte `k`; `map[old_len]` is the total new length.
fn build_multibyte_conversion_byte_map(
    old_bytes: &[u8],
    mode: BufferMultibyteConversionMode,
) -> Vec<usize> {
    use crate::emacs_core::emacs_char::{bytes_by_char_head, char_byte8_head_p, multibyte_length};
    let mut map = Vec::with_capacity(old_bytes.len() + 1);
    let mut new_pos = 0usize;
    let mut p = 0usize;
    match mode {
        // string-as-multibyte: a valid multibyte sequence is kept as-is (N bytes
        // -> N bytes); an invalid (eight-bit) byte becomes a 2-byte char.
        BufferMultibyteConversionMode::AsMultibyte => {
            while p < old_bytes.len() {
                match multibyte_length(&old_bytes[p..], true) {
                    Some(n) if n > 0 => {
                        for i in 0..n {
                            map.push(new_pos + i);
                        }
                        new_pos += n;
                        p += n;
                    }
                    _ => {
                        map.push(new_pos);
                        new_pos += 2;
                        p += 1;
                    }
                }
            }
        }
        // string-to-multibyte: every byte becomes a character (1 byte ASCII, a
        // 2-byte eight-bit char for a high byte).
        BufferMultibyteConversionMode::ToMultibyte => {
            for &b in old_bytes {
                map.push(new_pos);
                new_pos += if b < 0x80 { 1 } else { 2 };
            }
        }
        // string-as-unibyte: an eight-bit char collapses to its single raw byte;
        // any other multibyte char keeps its bytes.
        BufferMultibyteConversionMode::AsUnibyte => {
            while p < old_bytes.len() {
                let lead = old_bytes[p];
                let len = bytes_by_char_head(lead).max(1).min(old_bytes.len() - p);
                if char_byte8_head_p(lead) {
                    for _ in 0..len {
                        map.push(new_pos);
                    }
                    new_pos += 1;
                } else {
                    for i in 0..len {
                        map.push(new_pos + i);
                    }
                    new_pos += len;
                }
                p += len;
            }
        }
    }
    map.push(new_pos);
    map
}

/// Remap an old buffer byte offset to a character boundary in the converted
/// storage, via the conversion byte map (then round to a char boundary for any
/// position that fell inside a multibyte sequence).
fn remap_old_byte_to_new_boundary(
    byte_map: &[usize],
    old_byte_len: usize,
    new_storage: &crate::heap_types::LispString,
    new_total_bytes: usize,
    old_byte: usize,
) -> usize {
    let new_byte = byte_map
        .get(old_byte.min(old_byte_len))
        .copied()
        .unwrap_or(new_total_bytes)
        .min(new_total_bytes);
    lisp_string_advance_byte_to_boundary(new_storage, new_byte)
}

/// Convert the CHARACTER arm of one `insert` argument into buffer text, as
/// GNU's `general_insert_function` does with `CHAR_STRING (c, str)` /
/// `CHAR_TO_BYTE8 (c)` (src/editfns.c:1320-1333).
///
/// GNU converts and inserts one argument at a time, so this deliberately takes
/// a single argument: hoisting the conversion of the whole argument vector
/// above the first insertion is exactly what made a valid prefix disappear when
/// a later argument was neither a character nor a string.
///
/// This is the only arm converted before the change hook, and the signature
/// says why: a `Fixnum` code point is a value, not a heap object, so there is
/// nothing here for `before-change-functions` to mutate.  The string arm is
/// `PendingInsert::materialize`, deliberately after the hook.
fn insert_piece_from_char_arg(
    code_point: i64,
    arg: Value,
    target_multibyte: bool,
) -> Result<InsertPiece, Flow> {
    let text = u32::try_from(code_point)
        .ok()
        .and_then(|code| encode_char_code_for_buffer_bytes(code, target_multibyte))
        .map(|bytes| lisp_string_from_buffer_bytes(bytes, target_multibyte))
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("char-or-string-p"), arg],
            )
        })?;
    Ok(InsertPiece {
        text,
        text_props: None,
    })
}

pub(crate) fn apply_inherited_text_properties(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &mut BufferManager,
    current_id: BufferId,
    old_pt: usize,
    text_len: usize,
) {
    if text_len == 0 {
        return;
    }

    let props = buffers
        .get(current_id)
        .map(|buf| {
            super::builtins::misc_eval::inherited_text_properties_for_inserted_range_in_state(
                obarray, dynamic, buffers, buf, old_pt, text_len,
            )
        })
        .unwrap_or_default();
    if props.is_empty() {
        return;
    }

    // put_property prepends new properties to interval order, so apply the
    // merged GNU plist in reverse to preserve the final plist shape.
    let byte_range =
        EmacsByteRange::from_start_len(EmacsBytePos::new(old_pt), EmacsByteLen::new(text_len));
    for (name, value) in props.iter().rev() {
        let _ = buffers
            .put_buffer_text_property_in_emacs_byte_range(current_id, byte_range, *name, *value);
    }
}

/// Where markers exactly at an insertion site are placed.
///
/// GNU threads this decision through its insertion core as a boolean.  A
/// dedicated type prevents it from being confused with the independent text
/// property inheritance decision at call sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InsertPieceMarkerPlacement {
    AfterMarkers,
    BeforeMarkers,
}

/// How an inserted piece obtains text properties.
///
/// SourceOnly copies only properties carried by the inserted string.
/// InheritAdjoining additionally merges sticky properties from the destination
/// text, matching insert-and-inherit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InsertPiecePropertyMode {
    SourceOnly,
    InheritAdjoining,
}

/// GNU's `general_insert_function` (src/editfns.c:1307-1345): the single loop
/// behind `insert`, `insert-before-markers`, `insert-and-inherit`, and
/// `insert-before-markers-and-inherit`.
///
/// Each argument is converted and inserted in turn.  Two GNU-visible
/// consequences follow from the loop shape and are the reason this is one
/// function rather than four hoisted copies:
///
///  * a `wrong-type-argument` for argument N leaves arguments 0..N already in
///    the buffer, with point, `buffer-modified-p`, and the undo list advanced;
///  * the buffer's multibyteness is re-read per argument, because a
///    change hook run by an earlier argument may have changed it.
///
/// The two behavioral axes GNU passes as separate parameters stay separate
/// here, so no call site can conflate marker placement with property
/// inheritance.
fn general_insert_function(
    eval: &mut super::eval::Context,
    args: &[Value],
    marker_placement: InsertPieceMarkerPlacement,
    property_mode: InsertPiecePropertyMode,
) -> EvalResult {
    for arg in args {
        let target_multibyte = current_buffer_multibyte(&eval.buffers)?;
        let Some(pending) = PendingInsert::classify(*arg, target_multibyte)? else {
            continue;
        };
        insert_one_pending(
            eval,
            pending,
            target_multibyte,
            marker_placement,
            property_mode,
        )?;
    }
    Ok(Value::NIL)
}

/// One `insert` argument, decided but not yet read.
///
/// GNU settles exactly two things about an argument before any hook can run:
/// whether it is a character or a string (`general_insert_function`,
/// src/editfns.c:1320-1343, which signals `char-or-string-p` here), and, for a
/// string, whether it is empty (`insert_from_string`, src/insdel.c:986-987,
/// returns for `SCHARS (string) == 0` before `insert_from_string_1` is
/// entered).  Everything else about a string -- its bytes and its intervals --
/// GNU reads AFTER `before-change-functions`: `prepare_to_modify_buffer (PT,
/// PT, NULL)` (src/insdel.c:1043) sits between the caller's `SCHARS`/`SBYTES`
/// snapshot and both `copy_text (SDATA (string) + pos_byte, ...)` (:1053) and
/// `intervals = string_intervals (string)` (:1093).
///
/// Reading late is sound in GNU for a reason that is easy to miss and is the
/// whole argument for reproducing the shape rather than working around it:
///
///  * the object is ROOTED.  `string` is a `Lisp_Object` in a C frame, which
///    `mark_stack` scans conservatively, so the hook cannot collect it.
///  * the pointer is RE-READ.  `SDATA` is a macro over
///    `XSTRING (string)->u.s.data`, evaluated at each use, so a GC that
///    relocated the payload (`compact_small_strings`, src/alloc.c) is
///    invisible to the caller.
///  * the LENGTH cannot go stale.  `Faset` on a string (src/data.c:2658-2681)
///    is strictly length-preserving in chars and in bytes -- multibyte strings
///    take ASCII-for-ASCII in place, unibyte strings take `SSET`, and every
///    other case is an `error` -- so no Lisp operation can invalidate the
///    pre-hook `nchars`/`nbytes`.
///
/// The `Str` arm therefore holds a `Value`, which can be rooted, and never a
/// `&LispString`, which cannot.  That is the type-level statement of
/// DIVERGENCES.md 163's thesis: the `&'static LispString` seam was survivable
/// only because this path copied early, and copying early is what made it
/// disagree with GNU.  Deferring the borrow to `materialize`, past the
/// safepoint, is what lets both properties hold at once.
enum PendingInsert {
    /// A character argument, converted eagerly exactly as GNU does
    /// (`CHAR_STRING (c, str)`, src/editfns.c:1327).  A fixnum has no bytes a
    /// hook could reach, so there is nothing to defer.
    Char(InsertPiece),
    /// A string argument, held the way GNU holds it: as the Lisp object, to be
    /// read after the hook.
    Str(Value),
}

impl PendingInsert {
    /// The pre-hook half of GNU's protocol: dispatch on type, reject anything
    /// that is neither, and report an empty string as "nothing to do".
    ///
    /// `Ok(None)` is GNU's `SCHARS (string) == 0` early return, which happens
    /// before `prepare_to_modify_buffer`, so an empty argument runs no hook at
    /// all.  A character always yields at least one byte, so it is never
    /// `None`.
    fn classify(arg: Value, target_multibyte: bool) -> Result<Option<Self>, Flow> {
        match arg.kind() {
            ValueKind::String => {
                let empty = arg
                    .as_lisp_string()
                    .is_none_or(|string| string.schars() == 0);
                if empty {
                    return Ok(None);
                }
                Ok(Some(Self::Str(arg)))
            }
            ValueKind::Fixnum(code_point) => Ok(Some(Self::Char(insert_piece_from_char_arg(
                code_point,
                arg,
                target_multibyte,
            )?))),
            _other => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("char-or-string-p"), arg],
            )),
        }
    }

    /// The Lisp object that must stay reachable across the change hook, which
    /// is what stands in for GNU's conservatively-scanned C frame.
    fn source_root(&self) -> Value {
        match self {
            Self::Char(_) => Value::NIL,
            Self::Str(value) => *value,
        }
    }

    /// The post-hook half: read the bytes and the intervals now, the way GNU's
    /// `copy_text` and `string_intervals (string)` do.
    fn materialize(self, target_multibyte: bool) -> Result<InsertPiece, Flow> {
        match self {
            Self::Char(piece) => Ok(piece),
            Self::Str(value) => buffer_insert_piece_from_string(value, target_multibyte),
        }
    }
}

/// Insert exactly one argument, with its own before/after change signals,
/// mirroring one `insert`/`insert_from_string` call inside GNU's
/// `general_insert_function` loop.
///
/// The ordering here is GNU's and is load-bearing: signal first, read the
/// source second.  See `PendingInsert` for why reading second is safe.
fn insert_one_pending(
    eval: &mut super::eval::Context,
    pending: PendingInsert,
    target_multibyte: bool,
    marker_placement: InsertPieceMarkerPlacement,
    property_mode: InsertPiecePropertyMode,
) -> EvalResult {
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let insert_pos = eval
        .buffers
        .get(current_id)
        .map(Buffer::point_emacs_byte_pos)
        .unwrap_or(EmacsBytePos::ZERO);

    // Root the SOURCE, which is the only thing that has to survive the hook,
    // and which subsumes everything derived from it.
    //
    // GNU's `string` survives `prepare_to_modify_buffer` because it is a
    // rooted `Lisp_Object`; this states the same guarantee explicitly instead
    // of inheriting it from a conservative stack scan.  It also replaces the
    // list of interval plists this function used to cons up before signalling:
    // the piece's `TextPropertyTable` clone shares the source string's plist
    // spines, and while the source is rooted those spines are reachable
    // through it, exactly as GNU reaches them through `string_intervals
    // (string)` (src/insdel.c:1093).  Rooting them separately was only
    // necessary while the clone was taken BEFORE the hook, where a
    // `set-text-properties` on the source could unlink a spine the clone still
    // pointed at.  Materializing after the hook closes that window, so one
    // root replaces a per-property cons chain on every propertized insert.
    let piece_root_scope = eval.save_specpdl_roots();
    let source_root = pending.source_root();
    if source_root.is_heap_object() {
        eval.push_specpdl_root(source_root);
    }
    super::editfns::signal_before_insertion_at_emacs_byte_pos(eval, insert_pos)?;

    // Past the safepoint: now read the bytes and the intervals.
    let pieces = vec![pending.materialize(target_multibyte)?];
    let insert_extent = insert_pieces_extent(&pieces);
    if insert_extent.is_empty() {
        // GNU cannot reach this -- `Faset` cannot empty a string -- but a
        // conversion that produced no bytes must not be handed to the after
        // signal as a zero-width insertion that already ran a before signal.
        eval.restore_specpdl_roots(piece_root_scope);
        return Ok(Value::NIL);
    }
    let change = current_empty_text_change_at_emacs_byte_pos(
        &eval.buffers,
        current_id,
        insert_pos,
        insert_extent,
    )?;
    insert_pieces_in_state(
        &eval.obarray,
        &[],
        &mut eval.buffers,
        pieces,
        marker_placement,
        property_mode,
    )?;
    super::editfns::signal_after_text_change(eval, change)?;
    eval.restore_specpdl_roots(piece_root_scope);
    Ok(Value::NIL)
}

pub(crate) fn builtin_insert(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    general_insert_function(
        eval,
        &args,
        InsertPieceMarkerPlacement::AfterMarkers,
        InsertPiecePropertyMode::SourceOnly,
    )
}

pub(crate) fn builtin_insert_before_markers(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    general_insert_function(
        eval,
        &args,
        InsertPieceMarkerPlacement::BeforeMarkers,
        InsertPiecePropertyMode::SourceOnly,
    )
}

pub(crate) fn builtin_insert_and_inherit(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    general_insert_function(
        eval,
        &args,
        InsertPieceMarkerPlacement::AfterMarkers,
        InsertPiecePropertyMode::InheritAdjoining,
    )
}

pub(crate) fn builtin_insert_before_markers_and_inherit(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    general_insert_function(
        eval,
        &args,
        InsertPieceMarkerPlacement::BeforeMarkers,
        InsertPiecePropertyMode::InheritAdjoining,
    )
}

fn insert_pieces_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &mut BufferManager,
    pieces: Vec<InsertPiece>,
    marker_placement: InsertPieceMarkerPlacement,
    property_mode: InsertPiecePropertyMode,
) -> EvalResult {
    if pieces.iter().all(|piece| piece.text.is_empty()) {
        return Ok(Value::NIL);
    }

    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if buffers
        .get(current_id)
        .is_some_and(|buf| super::editfns::buffer_read_only_active_in_state(obarray, dynamic, buf))
    {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(current_id)],
        ));
    }

    for piece in pieces {
        if piece.text.is_empty() {
            continue;
        }
        let (insert_pos, insert_char_pos) = buffers
            .get(current_id)
            .map(|buf| (buf.point_emacs_byte_pos(), buf.point_char_pos()))
            .unwrap_or((EmacsBytePos::ZERO, CharPos0::ZERO));
        match marker_placement {
            InsertPieceMarkerPlacement::AfterMarkers => {
                let _ = buffers.insert_lisp_string_into_buffer(current_id, &piece.text);
            }
            InsertPieceMarkerPlacement::BeforeMarkers => {
                let _ =
                    buffers.insert_lisp_string_into_buffer_before_markers(current_id, &piece.text);
            }
        }
        let inserted_end = insert_pos.add_len(EmacsByteLen::new(piece.text.sbytes()));
        if property_mode == InsertPiecePropertyMode::SourceOnly && piece.text_props.is_none() {
            // The inserted text occupies char range [insert_char_pos,
            // insert_char_pos + schars); use it directly to avoid the
            // byte->char reconversion of [insert_pos, inserted_end].
            let _ = buffers.clear_inserted_plain_text_properties_in_char_range(
                current_id,
                CharRange::from_start_len(insert_char_pos, CharLen::new(piece.text.schars())),
            );
        }
        if property_mode == InsertPiecePropertyMode::InheritAdjoining {
            apply_inherited_text_properties(
                obarray,
                dynamic,
                buffers,
                current_id,
                insert_pos.get(),
                piece.text.sbytes(),
            );
            if piece.text_props.is_none() {
                let _ = buffers.merge_adjacent_equal_buffer_text_properties(
                    current_id,
                    EmacsByteRange::new(insert_pos, inserted_end),
                );
            }
        }
        if let Some(str_table) = piece.text_props {
            if property_mode == InsertPiecePropertyMode::InheritAdjoining {
                let _ = buffers
                    .merge_missing_buffer_text_properties(current_id, &str_table, insert_pos);
            } else {
                let _ = buffers.append_buffer_text_properties(current_id, &str_table, insert_pos);
            }
        }
    }
    Ok(Value::NIL)
}

pub(crate) fn insert_string_value_in_current_buffer(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &mut BufferManager,
    value: Value,
    marker_placement: InsertPieceMarkerPlacement,
    property_mode: InsertPiecePropertyMode,
) -> EvalResult {
    let target_multibyte = current_buffer_multibyte(buffers)?;
    let piece = buffer_insert_piece_from_string(value, target_multibyte)?;
    insert_pieces_in_state(
        obarray,
        dynamic,
        buffers,
        vec![piece],
        marker_placement,
        property_mode,
    )
}

pub(crate) fn insert_char_code_from_value(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(c) => Ok(c),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *value],
        )),
    }
}

pub(crate) fn builtin_insert_char(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("insert-char", &args, 1, 3)?;
    let char_code = insert_char_code_from_value(&args[0])?;
    let count = match args.get(1) {
        None => 1,
        Some(value) if value.is_nil() => 1,
        Some(value) => expect_fixnum(value)?,
    };

    if count <= 0 {
        return Ok(Value::NIL);
    }

    let multibyte = current_buffer_multibyte(&eval.buffers)?;
    let unit = if let Some(bytes) = encode_char_code_for_buffer_bytes(char_code as u32, multibyte) {
        bytes
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), args[0]],
        ));
    };
    let mut bytes = Vec::with_capacity(unit.len() * count as usize);
    for _ in 0..count {
        bytes.extend_from_slice(&unit);
    }
    let to_insert = lisp_string_from_buffer_bytes(bytes, multibyte);
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if eval.buffers.get(current_id).is_some_and(|buf| {
        super::editfns::buffer_read_only_active_in_state(&eval.obarray, &[], buf)
    }) {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(current_id)],
        ));
    }

    let insert_pos = eval
        .buffers
        .get(current_id)
        .map(Buffer::point_emacs_byte_pos)
        .unwrap_or(EmacsBytePos::ZERO);
    let text_extent = super::editfns::lisp_string_text_extent(&to_insert);
    let text_len = text_extent.emacs_bytes();
    let change = current_empty_text_change_at_emacs_byte_pos(
        &eval.buffers,
        current_id,
        insert_pos,
        text_extent,
    )?;
    super::editfns::signal_before_text_change(eval, change)?;
    let _ = eval
        .buffers
        .insert_lisp_string_into_buffer(current_id, &to_insert);
    if args.get(2).is_some_and(|value| value.is_truthy()) {
        apply_inherited_text_properties(
            &eval.obarray,
            &[],
            &mut eval.buffers,
            current_id,
            insert_pos.get(),
            text_len.get(),
        );
        let _ = eval.buffers.merge_adjacent_equal_buffer_text_properties(
            current_id,
            EmacsByteRange::from_start_len(insert_pos, text_len),
        );
    }
    super::editfns::signal_after_text_change(eval, change)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_insert_byte(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("insert-byte", &args, 2, 3)?;
    let byte = expect_fixnum(&args[0])?;
    if !(0..=255).contains(&byte) {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::fixnum(byte), Value::fixnum(0), Value::fixnum(255)],
        ));
    }
    let count = expect_fixnum(&args[1])?;
    if count <= 0 {
        return Ok(Value::NIL);
    }

    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let multibyte = eval
        .buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .get_multibyte();
    if eval.buffers.get(current_id).is_some_and(|buf| {
        super::editfns::buffer_read_only_active_in_state(&eval.obarray, &[], buf)
    }) {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(current_id)],
        ));
    }

    let unit = encode_char_code_for_buffer_bytes(
        if multibyte {
            crate::emacs_core::emacs_char::unibyte_to_char(byte as u8)
        } else {
            byte as u32
        },
        multibyte,
    )
    .expect("insert-byte must produce a valid buffer encoding");
    let mut bytes = Vec::with_capacity(unit.len() * count as usize);
    for _ in 0..count {
        bytes.extend_from_slice(&unit);
    }
    let to_insert = lisp_string_from_buffer_bytes(bytes, multibyte);
    let insert_pos = eval
        .buffers
        .get(current_id)
        .map(Buffer::point_emacs_byte_pos)
        .unwrap_or(EmacsBytePos::ZERO);
    let text_extent = super::editfns::lisp_string_text_extent(&to_insert);
    let text_len = text_extent.emacs_bytes();
    let change = current_empty_text_change_at_emacs_byte_pos(
        &eval.buffers,
        current_id,
        insert_pos,
        text_extent,
    )?;
    super::editfns::signal_before_text_change(eval, change)?;
    let _ = eval
        .buffers
        .insert_lisp_string_into_buffer(current_id, &to_insert);
    if args.get(2).is_some_and(|value| value.is_truthy()) {
        apply_inherited_text_properties(
            &eval.obarray,
            &[],
            &mut eval.buffers,
            current_id,
            insert_pos.get(),
            text_len.get(),
        );
        let _ = eval.buffers.merge_adjacent_equal_buffer_text_properties(
            current_id,
            EmacsByteRange::from_start_len(insert_pos, text_len),
        );
    }
    super::editfns::signal_after_text_change(eval, change)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_subst_char_in_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("subst-char-in-region", &args, 4, 5)?;

    let start = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(&eval.buffers, &args[1])?;
    let from_code = expect_character_code(&args[2])?;
    let to_code = expect_character_code(&args[3])?;
    let noundo = args.get(4).is_some_and(|value| !value.is_nil());

    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let target_multibyte = eval
        .buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .get_multibyte();
    let from_bytes = encode_char_code_for_buffer_bytes(from_code as u32, target_multibyte)
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), args[2]],
            )
        })?;
    let to_bytes =
        encode_char_code_for_buffer_bytes(to_code as u32, target_multibyte).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), args[3]],
            )
        })?;

    // GNU editfns.c:3051+ uses CHAR_BYTES (Emacs internal encoding length)
    // for this check, not storage-form length. The two agree for standard
    // Unicode but diverge for raw bytes (C0/C1 overlong vs PUA sentinel)
    // and nonunicode codepoints.
    if from_bytes.len() != to_bytes.len() {
        return Err(signal(
            "error",
            vec![Value::string(
                "Characters in `subst-char-in-region' have different byte-lengths",
            )],
        ));
    }

    let Some((range, changed_range)) = subst_char_in_region_scan(
        eval, current_id, start, end, from_code, to_code, &to_bytes, &args,
    )?
    else {
        return Ok(Value::NIL);
    };

    if eval.buffers.get(current_id).is_some_and(|buf| {
        super::editfns::buffer_read_only_active_in_state(&eval.obarray, &[], buf)
    }) {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(current_id)],
        ));
    }

    // subst-char-in-region replaces characters of the same byte length,
    // so changed bytes keep their old extent.  GNU calls modify_text from the
    // first changed character through the original END, but after-change only
    // reports through the last changed character.
    let before_range =
        TextEditRange::from_start_end(changed_range.start_anchor(), range.end_anchor());
    let change = TextChange::unchanged_extent_with_after_range(before_range, changed_range);
    super::editfns::signal_before_text_change(eval, change)?;

    // GNU `subst-char-in-region` restarts after `modify_text` because
    // `before-change-functions` may move the gap or alter the buffer.  It does
    // not run before-change hooks a second time; it simply rescans the same
    // Lisp range in the current buffer and either performs the substitutions
    // found there or returns without an after-change signal.
    let Some((range, changed_range)) = subst_char_in_region_scan(
        eval, current_id, start, end, from_code, to_code, &to_bytes, &args,
    )?
    else {
        return Ok(Value::NIL);
    };
    let changed_range_through_end =
        TextEditRange::from_start_end(changed_range.start_anchor(), range.end_anchor());
    let after_change =
        TextChange::unchanged_extent_with_after_range(changed_range_through_end, changed_range);
    let changed = eval.buffers.subst_char_in_buffer_region(
        current_id,
        range,
        changed_range_through_end,
        from_code as u32,
        &to_bytes,
        noundo,
    );
    if changed == Some(true) {
        super::editfns::signal_after_text_change(eval, after_change)?;
    }
    Ok(Value::NIL)
}

#[allow(clippy::too_many_arguments)] // keeps the scan's validated region and replacement state explicit
fn subst_char_in_region_scan(
    eval: &super::eval::Context,
    current_id: BufferId,
    start: i64,
    end: i64,
    from_code: i64,
    to_code: i64,
    to_bytes: &[u8],
    args: &[Value],
) -> Result<Option<(TextEditRange, TextEditRange)>, Flow> {
    let buf = eval
        .buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    if start < point_min || start > point_max || end < point_min || end > point_max {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::make_buffer(buf.id), args[0], args[1]],
        ));
    }

    let lo = LispCharPos1::new(start.min(end));
    let hi = LispCharPos1::new(start.max(end));
    let range = buf.edit_range_for_char_range(CharRange::new(lo.to_char_pos(), hi.to_char_pos()));
    if from_code == to_code {
        return Ok(None);
    }
    Ok(buf
        .subst_char_changed_range(range, from_code as u32, to_bytes)
        .map(|changed_range| (range, changed_range)))
}

pub(crate) fn builtin_buffer_enable_undo(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("buffer-enable-undo"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let id = if args.is_empty() || args[0].is_nil() {
        eval.buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
            .id
    } else {
        match args[0].kind() {
            ValueKind::Veclike(VecLikeType::Buffer) => {
                let bid = args[0].as_buffer_id().unwrap();
                if eval.buffers.get(bid).is_none() {
                    return Ok(Value::NIL);
                }
                bid
            }
            ValueKind::String => {
                let name = expect_buffer_name_string(&args[0])?;
                eval.buffers.find_buffer_by_name(&name).ok_or_else(|| {
                    signal(
                        "error",
                        vec![Value::string(format!("No buffer named {name}"))],
                    )
                })?
            }
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), args[0]],
                ));
            }
        }
    };
    eval.buffers
        .enable_buffer_undo(id)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    Ok(Value::NIL)
}

// No `buffer-disable-undo' here.  GNU DEFUNs `buffer-enable-undo'
// (src/buffer.c:1829, above) but NOT its partner: `buffer-disable-undo' is
// (defun buffer-disable-undo (&optional buffer) ...) at lisp/simple.el:3591,
// three lines of `with-current-buffer' + `setq' over `get-buffer'.
// DIVERGENCES.md 150.

pub(crate) fn builtin_buffer_size(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("buffer-size", &args, 1)?;
    if args.is_empty() || args[0].is_nil() {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        return Ok(Value::fixnum(buf.total_char_len().get() as i64));
    }

    let id = expect_buffer_id(&args[0])?;
    if let Some(buf) = eval.buffers.get(id) {
        Ok(Value::fixnum(buf.total_char_len().get() as i64))
    } else {
        Ok(Value::fixnum(0))
    }
}

pub(crate) fn builtin_narrow_to_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("narrow-to-region", &args, 2)?;
    let start = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(&eval.buffers, &args[1])?;
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let byte_range = normalize_narrow_region_in_buffers(
        &eval.buffers,
        current_id,
        LispCharPos1::new(start),
        LispCharPos1::new(end),
        args[0],
        args[1],
    )?;
    let before = accessible_bounds(eval, current_id);
    let _ = eval
        .buffers
        .narrow_buffer_to_emacs_byte_range(current_id, byte_range);
    // GNU carries narrowing into the mode line through `clip_changed`: it is a
    // term of the update_mode_line expression (xdisp.c:20471-20475) and
    // redisplay_internal escalates it to the buffer flag outright
    // (`if (current_buffer->clip_changed) bset_update_mode_line`, :17498).
    // `%n` ("Narrow") and `%p`/`%o` all change here.
    //
    // Only when the bounds ACTUALLY move, which is how GNU guards it
    // (`buf->clip_changed = 1` sits inside a bounds-changed test,
    // editfns.c:3104 and :3121). Marking unconditionally is not a
    // conservative-but-safe choice: `widen` is called about twice per
    // keystroke by font-lock's `save-restriction` + `widen`, so an
    // unconditional mark leaves chrome permanently dirty and silently
    // disables every chrome skip. Measured: 440 marks over 200 keystrokes,
    // and a chrome skip rate of exactly zero.
    if accessible_bounds(eval, current_id) != before {
        eval.mark_chrome_dirty_all();
    }
    Ok(Value::NIL)
}

/// The buffer's accessible bounds, for GNU's `clip_changed` bounds-changed
/// test. A missing buffer reports an impossible range so a disappearing buffer
/// never reads as "unchanged".
fn accessible_bounds(
    eval: &super::eval::Context,
    buffer: crate::buffer::BufferId,
) -> Option<(usize, usize)> {
    eval.buffers.get(buffer).map(|buf| {
        let range = buf.accessible_emacs_byte_region().range();
        (range.start().get(), range.end().get())
    })
}

pub(crate) fn builtin_widen(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("widen", &args, 0)?;
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let before = accessible_bounds(eval, current_id);
    let _ = eval.buffers.widen_buffer(current_id);
    // The other half of GNU's clip_changed, and the same bounds-changed guard
    // (GNU: `if (BEGV != s || ZV != e) current_buffer->clip_changed = 1;`,
    // editfns.c:2990). This is the call that made the guard load-bearing —
    // font-lock widens constantly on an already-widened buffer.
    if accessible_bounds(eval, current_id) != before {
        eval.mark_chrome_dirty_all();
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_buffer_modified_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("buffer-modified-p", &args, 1)?;
    if args.is_empty() || args[0].is_nil() {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        return Ok(buf.modified_state_value());
    }

    let id = expect_buffer_id(&args[0])?;
    if let Some(buf) = eval.buffers.get(id) {
        Ok(buf.modified_state_value())
    } else {
        Ok(Value::NIL)
    }
}

pub(crate) fn builtin_set_buffer_modified_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-buffer-modified-p", &args, 1)?;
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let was_modified = eval
        .buffers
        .modified_state_root_id(current_id)
        .and_then(|root_id| eval.buffers.get(root_id))
        .is_some_and(|buffer| buffer.modified_state_value().is_truthy());
    filelock::sync_modified_buffer_file_lock(eval, current_id, was_modified, args[0])?;
    let _ = eval
        .buffers
        .restore_buffer_modified_state(current_id, args[0]);
    super::builtins::misc_pure::builtin_force_mode_line_update(eval, vec![Value::NIL])
}

pub(crate) fn builtin_restore_buffer_modified_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("restore-buffer-modified-p", &args, 1)?;
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let was_modified = eval
        .buffers
        .modified_state_root_id(current_id)
        .and_then(|root_id| eval.buffers.get(root_id))
        .is_some_and(|buffer| buffer.modified_state_value().is_truthy());
    filelock::sync_modified_buffer_file_lock(eval, current_id, was_modified, args[0])?;
    eval.buffers
        .restore_buffer_modified_state(current_id, args[0])
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))
}

fn optional_buffer_tick_target_in_manager(
    buffers: &BufferManager,
    name: &str,
    args: &[Value],
) -> Result<Option<BufferId>, Flow> {
    expect_max_args(name, args, 1)?;
    if args.is_empty() || args[0].is_nil() {
        Ok(buffers.current_buffer().map(|buf| buf.id))
    } else {
        Ok(Some(expect_buffer_id(&args[0])?))
    }
}

pub(crate) fn builtin_buffer_modified_tick(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let target =
        optional_buffer_tick_target_in_manager(&eval.buffers, "buffer-modified-tick", &args)?;
    if let Some(id) = target
        && let Some(buf) = eval.buffers.get(id)
    {
        return Ok(Value::fixnum(buf.modified_tick()));
    }
    Ok(Value::fixnum(1))
}

pub(crate) fn builtin_buffer_chars_modified_tick(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let target =
        optional_buffer_tick_target_in_manager(&eval.buffers, "buffer-chars-modified-tick", &args)?;
    if let Some(id) = target
        && let Some(buf) = eval.buffers.get(id)
    {
        return Ok(Value::fixnum(buf.chars_modified_tick()));
    }
    Ok(Value::fixnum(1))
}

pub(crate) fn builtin_internal_set_buffer_modified_tick(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("internal--set-buffer-modified-tick", &args, 1, 2)?;
    let tick = expect_fixnum(&args[0])?;
    let target = if let Some(buffer) = args.get(1) {
        if buffer.is_nil() {
            eval.buffers.current_buffer_id()
        } else {
            Some(expect_buffer_id(buffer)?)
        }
    } else {
        eval.buffers.current_buffer_id()
    }
    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    eval.buffers
        .set_buffer_modified_tick(target, tick)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_recent_auto_save_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("recent-auto-save-p", &args, 0)?;
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::bool_val(buf.recent_auto_save_p()))
}

pub(crate) fn builtin_set_buffer_auto_saved(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-buffer-auto-saved", &args, 0)?;
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    eval.buffers
        .set_buffer_auto_saved(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_buffer_list(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("buffer-list", &args, 1)?;
    let ids = args
        .first()
        .and_then(|value| match value.kind() {
            ValueKind::Veclike(VecLikeType::Frame) => {
                let fid = crate::window::FrameId(value.as_frame_id().unwrap());
                let frame = eval.frames.get(fid)?;

                // Global buffer list (Vbuffer_alist equivalent).
                let mut general = eval.buffers.buffer_list();

                // Frame-specific lists (GNU buffer.c:438-460).
                // f->buffer_list: most-recently-shown first.
                let framelist: Vec<BufferId> = frame.buffer_list.clone();
                // f->buried_buffer_list: most-recently-buried first;
                // GNU reverses it so most-recently-buried comes last.
                let mut prevlist: Vec<BufferId> = frame.buried_buffer_list.clone();
                prevlist.reverse();

                // Remove duplicates from general.
                general.retain(|bid| !framelist.contains(bid) && !prevlist.contains(bid));

                // GNU buffer.c:457: CALLN(Fnconc, framelist, general, prevlist).
                // Framelist first, then remaining (deduped) general, then
                // reversed buried list last so buried buffers are at the
                // absolute end.
                let mut ids = Vec::with_capacity(framelist.len() + prevlist.len() + general.len());
                ids.extend(framelist);
                ids.extend(general);
                ids.extend(prevlist);
                Some(ids)
            }
            _ => None,
        })
        .unwrap_or_else(|| eval.buffers.buffer_list());
    let vals: Vec<Value> = ids.into_iter().map(Value::make_buffer).collect();
    Ok(Value::list(vals))
}

fn other_buffer_designator(
    buffers: &crate::buffer::BufferManager,
    value: Option<&Value>,
) -> Option<crate::buffer::BufferId> {
    let v = value?;
    match v.kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = v.as_buffer_id().unwrap();
            if buffers.get(id).is_some() {
                Some(id)
            } else {
                None
            }
        }
        ValueKind::String => {
            let name = v
                .as_lisp_string()
                .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
                .expect("ValueKind::String must carry LispString payload");
            buffers.find_buffer_by_name(&name)
        }
        _ => None,
    }
}

fn is_hidden_buffer(buffers: &crate::buffer::BufferManager, id: crate::buffer::BufferId) -> bool {
    buffers
        .get(id)
        .map(|buf| buf.name_starts_with_space())
        .unwrap_or(true)
}

pub(crate) fn builtin_other_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    other_buffer_impl_in_state(&mut eval.frames, &mut eval.buffers, args)
}

pub(crate) fn other_buffer_impl(
    buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("other-buffer", &args, 3)?;

    let current_id = buffers.current_buffer_id();
    let avoid_id = other_buffer_designator(buffers, args.first());
    let visible_ok = args.get(1).is_some_and(|arg| !arg.is_nil());
    let mut notsogood = None;

    for id in buffers.buffer_list() {
        if Some(id) == avoid_id || is_hidden_buffer(buffers, id) {
            continue;
        }
        if visible_ok || Some(id) != current_id {
            return Ok(Value::make_buffer(id));
        }
        if notsogood.is_none() {
            notsogood = Some(id);
        }
    }

    if let Some(id) = notsogood {
        return Ok(Value::make_buffer(id));
    }

    let scratch = buffers
        .find_buffer_by_name("*scratch*")
        .unwrap_or_else(|| buffers.create_buffer("*scratch*"));
    Ok(Value::make_buffer(scratch))
}

fn buffer_visible_in_visible_frame(
    frames: &crate::window::FrameManager,
    buffer_id: crate::buffer::BufferId,
) -> bool {
    frames.frame_list().into_iter().any(|fid| {
        let Some(frame) = frames.get(fid) else {
            return false;
        };
        frame.visible
            && frame.window_list().into_iter().any(|wid| {
                frame
                    .find_window(wid)
                    .and_then(crate::window::Window::buffer_id)
                    == Some(buffer_id)
            })
    })
}

fn other_buffer_candidate(
    frames: &crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    buffer_id: crate::buffer::BufferId,
    avoid_id: Option<crate::buffer::BufferId>,
    visible_ok: bool,
    notsogood: &mut Option<crate::buffer::BufferId>,
) -> Option<crate::buffer::BufferId> {
    if Some(buffer_id) == avoid_id || is_hidden_buffer(buffers, buffer_id) {
        return None;
    }
    if visible_ok || !buffer_visible_in_visible_frame(frames, buffer_id) {
        Some(buffer_id)
    } else {
        if notsogood.is_none() {
            *notsogood = Some(buffer_id);
        }
        None
    }
}

pub(crate) fn other_buffer_impl_in_state(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("other-buffer", &args, 3)?;

    let frame_id = super::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        args.get(2),
        "frame-live-p",
    )?;
    let frame_buffer_list = frames
        .get(frame_id)
        .map(|frame| frame.buffer_list.clone())
        .unwrap_or_default();
    let avoid_id = other_buffer_designator(buffers, args.first());
    let visible_ok = args.get(1).is_some_and(|arg| !arg.is_nil());
    let mut notsogood = None;

    for id in frame_buffer_list {
        if let Some(candidate) =
            other_buffer_candidate(frames, buffers, id, avoid_id, visible_ok, &mut notsogood)
        {
            return Ok(Value::make_buffer(candidate));
        }
    }

    for id in buffers.buffer_list() {
        if let Some(candidate) =
            other_buffer_candidate(frames, buffers, id, avoid_id, visible_ok, &mut notsogood)
        {
            return Ok(Value::make_buffer(candidate));
        }
    }

    if let Some(id) = notsogood {
        return Ok(Value::make_buffer(id));
    }

    let scratch = buffers
        .find_buffer_by_name("*scratch*")
        .unwrap_or_else(|| buffers.create_buffer("*scratch*"));
    Ok(Value::make_buffer(scratch))
}

pub(crate) fn builtin_generate_new_buffer_name(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("generate-new-buffer-name", &args, 1)?;
    expect_max_args("generate-new-buffer-name", &args, 2)?;
    if args.len() == 2
        && !(args[1].is_nil()
            || args[1].is_t()
            || args[1].is_string()
            || args[1].is_symbol()
            || args[1].as_keyword_id().is_some())
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[1]],
        ));
    }
    let ignore = args.get(1).and_then(|v| v.as_utf8_str());
    generate_new_buffer_name_value_in_state(&eval.buffers, args[0], ignore)
}

/// Return GNU `generate-new-buffer-name`'s Lisp string result.
///
/// Buffer identity is indexed by the decoded name text, but the public name is
/// a Lisp string object whose identity and text properties are observable.
/// Keep those two concerns separate: the manager chooses the unique text;
/// this boundary reuses BASE when possible and appends only the generated
/// suffix when allocation is necessary, just like GNU's `concat2` path.
pub(crate) fn generate_new_buffer_name_value_in_state(
    buffers: &crate::buffer::BufferManager,
    base: Value,
    ignore: Option<&str>,
) -> EvalResult {
    let base_text = expect_string_lossy(&base)?;
    let generated = buffers.generate_new_buffer_name_ignoring(&base_text, ignore);
    if generated == base_text {
        return Ok(base);
    }

    let suffix = generated
        .strip_prefix(&base_text)
        .expect("BufferManager must generate a name by appending to its base");
    super::builtins::strings::builtin_concat_slice(&[base, Value::string(suffix)])
}

/// (bufferp OBJECT) → t or nil
pub(crate) fn builtin_bufferp(args: Vec<Value>) -> EvalResult {
    expect_args("bufferp", &args, 1)?;
    Ok(Value::bool_val(args[0].is_buffer()))
}

pub(crate) fn builtin_char_after(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("char-after", &args, 1)?;
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let accessible = buf.accessible_emacs_byte_region();
    let byte_pos = if args.is_empty() || args[0].is_nil() {
        let point = buf.point_emacs_byte_pos();
        accessible.contains(point).then_some(point)
    } else {
        let pos = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
        if pos <= 0 {
            return Ok(Value::NIL);
        }
        let point_min = point_char_pos(buf, accessible.start());
        let point_max = point_char_pos(buf, accessible.end());
        if pos < point_min || pos >= point_max {
            return Ok(Value::NIL);
        }
        Some(buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(pos)))
    };
    match byte_pos.and_then(|pos| buf.char_code_after_emacs_byte_pos(pos)) {
        Some(code) => Ok(Value::fixnum(code as i64)),
        None => Ok(Value::NIL),
    }
}

pub(crate) fn builtin_char_before(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("char-before", &args, 1)?;
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let accessible = buf.accessible_emacs_byte_region();
    let byte_pos = if args.is_empty() || args[0].is_nil() {
        let point = buf.point_emacs_byte_pos();
        accessible
            .contains_preceding_char_boundary(point)
            .then_some(point)
    } else {
        let pos = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
        if pos <= 0 {
            return Ok(Value::NIL);
        }
        let point_min = point_char_pos(buf, accessible.start());
        let point_max = point_char_pos(buf, accessible.end());
        if pos <= point_min || pos > point_max {
            return Ok(Value::NIL);
        }
        Some(buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(pos)))
    };
    match byte_pos.and_then(|pos| buf.char_code_before_emacs_byte_pos(pos)) {
        Some(code) => Ok(Value::fixnum(code as i64)),
        None => Ok(Value::NIL),
    }
}

fn get_byte_from_multibyte_char_code(code: u32) -> EvalResult {
    if code <= 0x7F {
        return Ok(Value::fixnum(code as i64));
    }
    if (0x3FFF80..=0x3FFFFF).contains(&code) {
        return Ok(Value::fixnum((code - 0x3FFF00) as i64));
    }
    Err(signal(
        "error",
        vec![Value::string(format!(
            "Not an ASCII nor an 8-bit character: {code}"
        ))],
    ))
}

pub(crate) fn builtin_byte_to_position(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("byte-to-position", &args, 1)?;
    let byte_pos = expect_fixnum(&args[0])?;
    if byte_pos <= 0 {
        return Ok(Value::NIL);
    }

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let byte_len = buf.total_emacs_byte_len();
    let byte_pos0 = LispBytePos1::new(byte_pos).to_emacs_byte_pos();
    let byte_end = buf.total_emacs_byte_end_pos();
    if byte_pos0 > byte_end {
        return Ok(Value::NIL);
    }

    let mut boundary = byte_pos0.get();
    if buf.get_multibyte() && boundary < byte_len.get() {
        while boundary > 0
            && buf
                .emacs_byte_at_pos(EmacsBytePos::new(boundary))
                .is_some_and(|byte| (byte & 0xC0) == 0x80)
        {
            boundary -= 1;
        }
    }

    Ok(Value::fixnum(point_char_pos(
        buf,
        EmacsBytePos::new(boundary),
    )))
}

pub(crate) fn builtin_position_bytes(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("position-bytes", &args, 1)?;
    let pos = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let max_char_pos = buf.z_lisp_char_pos().as_i64();
    if pos <= 0 || pos > max_char_pos {
        return Ok(Value::NIL);
    }

    let byte_pos = char_pos_to_buffer_emacs_byte_pos(buf, LispCharPos1::new(pos).to_char_pos());
    Ok(Value::fixnum(byte_pos.to_lisp_byte_pos().as_i64()))
}

pub(crate) fn builtin_get_byte(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("get-byte", &args, 2)?;

    // STRING path: POSITION is a zero-based character index.
    if args.get(1).is_some_and(|v| !v.is_nil()) {
        let string_value = args[1];
        // Validate that arg is a string (without extracting as &str, which
        // would fail for non-UTF-8 unibyte strings).
        if !args[1].is_string() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[1]],
            ));
        }
        let pos = if args.is_empty() || args[0].is_nil() {
            0usize
        } else {
            expect_wholenump(&args[0])? as usize
        };

        let string = eval.lisp_string(args[1]).expect("string");
        let char_len = string.schars();
        if pos >= char_len && !args[0].is_nil() {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![string_value, Value::fixnum(pos as i64)],
            ));
        }

        // GNU returns the terminating NUL for (get-byte nil "") after the
        // explicit-position path has already been range-checked.
        if char_len == 0 {
            return Ok(Value::fixnum(0));
        }

        if !string.is_multibyte() {
            // Unibyte: direct byte access
            return Ok(Value::fixnum(string.as_bytes()[pos] as i64));
        }
        // Use lisp_string_char_codes which handles sentinel translation
        let codes = super::builtins::lisp_string_char_codes(string);
        let code = codes[pos];
        return get_byte_from_multibyte_char_code(code);
    }

    // Buffer path: POSITION is a 1-based character position.
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let byte_pos = if args.is_empty() || args[0].is_nil() {
        buf.point_emacs_byte_pos()
    } else {
        let pos = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
        let point_min = buf.point_min_lisp_char_pos().as_i64();
        let point_max = buf.point_max_lisp_char_pos().as_i64();
        if pos < point_min || pos >= point_max {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![args[0], Value::fixnum(point_min), Value::fixnum(point_max)],
            ));
        }
        buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(pos))
    };

    if byte_pos >= buf.total_emacs_byte_end_pos() {
        return Ok(Value::fixnum(0));
    }

    if !buf.get_multibyte() {
        let code = match buf.char_code_after_emacs_byte_pos(byte_pos) {
            Some(code) => code,
            None => return Ok(Value::fixnum(0)),
        };
        assert!(
            code <= 0xFF,
            "unibyte buffer storage contained non-byte character code {code:#X}"
        );
        return Ok(Value::fixnum(code as i64));
    }

    let code = match buf.char_code_after_emacs_byte_pos(byte_pos) {
        Some(code) => code,
        None => return Ok(Value::fixnum(0)),
    };

    get_byte_from_multibyte_char_code(code)
}

pub(crate) fn builtin_buffer_local_value(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    use crate::emacs_core::intern::{intern, resolve_sym};
    use crate::emacs_core::symbol::SymbolRedirect;

    expect_args("buffer-local-value", &args, 2)?;
    let original_arg = args[0];
    let symbol = match args[0].kind() {
        ValueKind::Symbol(id) => id,
        ValueKind::Nil => intern("nil"),
        ValueKind::T => intern("t"),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[0]],
            ));
        }
    };
    let resolved_id = crate::emacs_core::builtins::symbols::resolve_variable_alias_id_in_obarray(
        eval.obarray(),
        symbol,
    )?;
    let id = expect_buffer_id(&args[1])?;
    let buf = eval
        .buffers
        .get_any(id)
        .ok_or_else(|| signal("error", vec![Value::string("No such buffer")]))?;

    // Phase 10E: route LOCALIZED reads through the BLV machinery
    // (immutable walker — buffer-local-value never swaps the cache).
    // Mirrors GNU `Fbuffer_local_value` SYMBOL_LOCALIZED arm at
    // `data.c:1696-1740` which uses `blv_value` (returning the
    // already-loaded valcell.cdr if `where == buf`, else walks
    // `BVAR(buf, local_var_alist)`), then signals void-variable if
    // the result is `Qunbound`.
    if let Some(sym_slot) = eval.obarray().get_by_id(resolved_id)
        && sym_slot.redirect() == SymbolRedirect::Localized
    {
        let target_buf = Value::make_buffer(buf.id);
        if let Some(value) =
            eval.obarray()
                .read_localized(resolved_id, target_buf, buf.local_var_alist_value())
        {
            if value.is_unbound() {
                return Err(signal(LispCondition::VoidVariable, vec![original_arg]));
            }
            return Ok(value);
        }
    }

    match buf.get_buffer_local_binding_by_sym_id(resolved_id) {
        Some(binding) => binding
            .as_value()
            .ok_or_else(|| signal(LispCondition::VoidVariable, vec![original_arg])),
        None if let Some(info) =
            crate::buffer::buffer::lookup_buffer_slot_by_sym_id(resolved_id) =>
        {
            Ok(buf.slots[info.offset.index()])
        }
        None if resolved_id == intern("nil") => Ok(Value::NIL),
        None if resolved_id == intern("t") => Ok(Value::T),
        None if resolve_sym(resolved_id).starts_with(':') => Ok(Value::from_sym_id(resolved_id)),
        None => eval
            .obarray()
            .find_symbol_value(resolved_id)
            .ok_or_else(|| signal(LispCondition::VoidVariable, vec![original_arg])),
    }
}

// ===========================================================================
// Overlay builtins (GNU buffer.c hosts the overlay DEFUN family; the
// position-coercion helpers stay with the interval machinery in textprop.rs)
// ===========================================================================
use super::textprop::{
    byte_to_elisp_pos, current_buffer_id_in_buffers, elisp_pos_to_byte_clipped_full,
    elisp_range_to_byte_clipped_full, ensure_marker_points_into_buffer, expect_overlay,
    lookup_overlay_property, resolve_buffer_id_in_buffers, resolve_overlay_buffer_id,
};

/// (next-overlay-change POS)
pub(crate) fn builtin_next_overlay_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_next_overlay_change_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_next_overlay_change_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("next-overlay-change", &args, 1)?;
    let pos = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let buf_id = current_buffer_id_in_buffers(buffers)?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let byte_pos = elisp_pos_to_byte_clipped_full(buf, LispCharPos1::new(pos));
    let accessible = buf.accessible_emacs_byte_region();
    match buf
        .overlays
        .next_boundary_after_until_emacs_byte_pos(byte_pos, accessible.end())
    {
        Some(next) => Ok(Value::fixnum(byte_to_elisp_pos(buf, next))),
        None => Ok(Value::fixnum(byte_to_elisp_pos(buf, accessible.end()))),
    }
}

/// (previous-overlay-change POS)
pub(crate) fn builtin_previous_overlay_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_previous_overlay_change_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_previous_overlay_change_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("previous-overlay-change", &args, 1)?;
    let pos = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let buf_id = current_buffer_id_in_buffers(buffers)?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let byte_pos = elisp_pos_to_byte_clipped_full(buf, LispCharPos1::new(pos));
    let accessible = buf.accessible_emacs_byte_region();
    match buf
        .overlays
        .previous_boundary_before_since_emacs_byte_pos(byte_pos, accessible.start())
    {
        Some(prev) => Ok(Value::fixnum(byte_to_elisp_pos(buf, prev))),
        None => Ok(Value::fixnum(byte_to_elisp_pos(buf, accessible.start()))),
    }
}

/// (make-overlay BEG END &optional BUFFER FRONT-ADVANCE REAR-ADVANCE)
pub(crate) fn builtin_make_overlay(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_make_overlay_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_make_overlay_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("make-overlay", &args, 2)?;
    expect_max_args("make-overlay", &args, 5)?;
    let buf_id = resolve_buffer_id_in_buffers(buffers, args.get(2))?;
    ensure_marker_points_into_buffer(buffers, &args[0], buf_id)?;
    ensure_marker_points_into_buffer(buffers, &args[1], buf_id)?;
    let beg = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    let front_advance = args.get(3).is_some_and(|v| v.is_truthy());
    let rear_advance = args.get(4).is_some_and(|v| v.is_truthy());

    let buf = buffers
        .get_mut(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let byte_range = elisp_range_to_byte_clipped_full(buf, beg, end);
    let overlay = Value::make_overlay(crate::heap_types::OverlayData {
        serial: 0,
        plist: Value::NIL,
        buffer: Some(buf_id),
        start: byte_range.start().get(),
        end: byte_range.end().get(),
        position_handle: None,
        front_advance,
        rear_advance,
    });
    buf.overlays.insert_overlay(overlay);
    // Creating an overlay changes what redisplay must consider (it can carry a
    // face/display/before-string the moment a property is attached), so bump
    // the modification tick here — matching move/put/delete, which already do.
    buf.increment_overlay_modified_tick();
    Ok(overlay)
}

/// (delete-overlay OVERLAY)
pub(crate) fn builtin_delete_overlay(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_delete_overlay_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_delete_overlay_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("delete-overlay", &args, 1)?;
    let overlay = expect_overlay(&args[0])?;
    if let Some(buf_id) = resolve_overlay_buffer_id(overlay) {
        let _ = buffers.delete_buffer_overlay(buf_id, overlay);
    }
    Ok(Value::NIL)
}

/// (overlay-put OVERLAY PROP VAL)
pub(crate) fn builtin_overlay_put(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_overlay_put_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_overlay_put_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("overlay-put", &args, 3)?;
    let overlay = expect_overlay(&args[0])?;
    let val = args[2];
    let changed = if let Some(buf_id) = resolve_overlay_buffer_id(overlay) {
        buffers
            .get_mut(buf_id)
            .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?
            .overlays
            .overlay_put(overlay, args[1], val)?
    } else {
        overlay
            .with_overlay_data_mut(|object| {
                let (plist, changed) = super::plist::plist_put(object.plist, args[1], val)?;
                object.plist = plist;
                Ok::<bool, crate::emacs_core::error::Flow>(changed)
            })
            .unwrap()?
    };
    if let Some(buf_id) = resolve_overlay_buffer_id(overlay)
        && changed
    {
        if let Some(buf) = buffers.get_mut(buf_id) {
            buf.increment_overlay_modified_tick();
        }
        let evaporate = args[1].is_symbol_named("evaporate") && val.is_truthy();
        let is_empty = buffers
            .get(buf_id)
            .and_then(|buf| {
                let start = buf.overlays.overlay_start_emacs_byte_pos(overlay)?;
                let end = buf.overlays.overlay_end_emacs_byte_pos(overlay)?;
                Some(start == end)
            })
            .unwrap_or(false);
        if evaporate && is_empty {
            let _ = buffers.delete_buffer_overlay(buf_id, overlay);
        }
    }
    Ok(val)
}

/// (overlay-get OVERLAY PROP)
pub(crate) fn builtin_overlay_get(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("overlay-get", &args, 2)?;
    let overlay = expect_overlay(&args[0])?;
    Ok(lookup_overlay_property(
        &eval.obarray,
        &eval.buffers,
        overlay,
        args[1],
    ))
}

/// (overlayp OBJ)
pub(crate) fn builtin_overlayp(_eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_overlayp_pure(args)
}

pub(crate) fn builtin_overlayp_pure(args: Vec<Value>) -> EvalResult {
    expect_args("overlayp", &args, 1)?;
    if args[0].is_overlay() {
        return Ok(Value::T);
    }
    Ok(Value::NIL)
}

/// (overlays-at POS &optional SORTED)
pub(crate) fn builtin_overlays_at(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_overlays_at_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_overlays_at_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("overlays-at", &args, 1)?;
    expect_max_args("overlays-at", &args, 2)?;
    let pos = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let buf_id = current_buffer_id_in_buffers(buffers)?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let byte_pos = elisp_pos_to_byte_clipped_full(buf, LispCharPos1::new(pos));
    let mut ids = buf.overlays.overlays_at_emacs_byte_pos(byte_pos);
    if let Some(sorted) = args.get(1)
        && sorted.is_truthy()
    {
        // GNU `Foverlays_at` (buffer.c:3901): when SORTED is a window value,
        // `sort_overlays` filters via `overlay_matches_window` — overlays
        // whose `window` property is a window distinct from W are dropped.
        if let Some(target_window_id) = sorted.as_window_id() {
            let window_sym = Value::symbol("window");
            ids.retain(|ov| match buf.overlays.overlay_get_named(*ov, window_sym) {
                Some(prop) => prop
                    .as_window_id()
                    .is_none_or(|wid| wid == target_window_id),
                None => true,
            });
        }
        buf.overlays.sort_overlay_ids_by_priority_desc(&mut ids);
    }
    Ok(Value::list(ids))
}

/// (overlays-in BEG END)
pub(crate) fn builtin_overlays_in(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_overlays_in_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_overlays_in_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("overlays-in", &args, 2)?;
    let beg = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    // GNU `overlays_in` (buffer.c) treats BEG > END as an empty region: the
    // interval-tree walk `[beg, search_end)` is empty and the very first node
    // (whose `begin > end`) breaks the loop, so no overlays are returned.
    // Unlike `make-overlay`/`move-overlay`, `overlays-in` must NOT swap the
    // endpoints, so guard before the (swapping) clip helper.
    if beg > end {
        return Ok(Value::NIL);
    }
    let buf_id = current_buffer_id_in_buffers(buffers)?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let byte_range = elisp_range_to_byte_clipped_full(buf, beg, end);
    let accessible = buf.accessible_emacs_byte_region();
    let ids = buf
        .overlays
        .overlays_in_accessible_emacs_byte_range(byte_range, accessible.end());
    Ok(Value::list(ids))
}

/// (overlay-lists)
///
/// Mirrors GNU `Foverlay_lists` (buffer.c). Returns `(BEFORE . AFTER)`: the
/// car holds every overlay of the current buffer, the cdr is always empty.
/// GNU's docstring still describes the pair as the overlays before/after the
/// "overlay center", but since Emacs 29.1 (commit moving overlays to the
/// `itree` interval tree) that center no longer exists: `Foverlay_lists`
/// conses every node of `current_buffer->overlays` (walked `BEG..Z`
/// DESCENDING, which reverses back to ascending `begin` order) into a single
/// list and returns `(cons overlays Qnil)`. Even for an empty buffer GNU
/// returns `(nil)` (i.e. `(cons nil nil)`), never bare `nil`.
pub(crate) fn builtin_overlay_lists(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_overlay_lists_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_overlay_lists_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("overlay-lists", &args, 0)?;
    let buf_id = current_buffer_id_in_buffers(buffers)?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    let before = Value::list(buf.overlays.overlays_in_gnu_lists_order());
    Ok(Value::cons(before, Value::NIL))
}

/// (overlay-recenter POS)
///
/// Mirrors GNU `Foverlay_recenter` (buffer.c): since Emacs 29.1 this is a
/// no-op (overlay lookup is fast at any position with the `itree` store), but
/// it still type-checks POS as a fixnum-or-marker and returns nil.
pub(crate) fn builtin_overlay_recenter(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("overlay-recenter", &args, 1)?;
    let _ = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
    Ok(Value::NIL)
}

/// (move-overlay OVERLAY BEG END &optional BUFFER)
pub(crate) fn builtin_move_overlay(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_move_overlay_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_move_overlay_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("move-overlay", &args, 3)?;
    expect_max_args("move-overlay", &args, 4)?;
    let overlay = expect_overlay(&args[0])?;
    let old_buf_id = resolve_overlay_buffer_id(overlay);

    // Resolve target buffer: use BUFFER arg if given, otherwise same buffer.
    let new_buf_id = if let Some(buf_arg) = args.get(3) {
        if buf_arg.is_truthy() {
            resolve_buffer_id_in_buffers(buffers, Some(buf_arg))?
        } else {
            old_buf_id.unwrap_or_else(|| buffers.current_buffer_id().expect("current buffer"))
        }
    } else {
        old_buf_id.unwrap_or_else(|| buffers.current_buffer_id().expect("current buffer"))
    };

    ensure_marker_points_into_buffer(buffers, &args[1], new_buf_id)?;
    ensure_marker_points_into_buffer(buffers, &args[2], new_buf_id)?;
    let beg = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    let end = expect_integer_or_marker_in_buffers(buffers, &args[2])?;

    if old_buf_id == Some(new_buf_id) {
        // Same buffer: just move within the buffer.
        let buf = buffers
            .get_mut(new_buf_id)
            .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
        let byte_range = elisp_range_to_byte_clipped_full(buf, beg, end);
        buf.overlays
            .move_overlay_to_emacs_byte_range(overlay, byte_range);
        buf.increment_overlay_modified_tick();
        Ok(args[0])
    } else {
        if let Some(old_buf_id) = old_buf_id
            && let Some(buf) = buffers.get_mut(old_buf_id)
            && buf.overlays.detach_overlay(overlay)
        {
            buf.increment_overlay_modified_tick();
        }

        let new_buf = buffers
            .get_mut(new_buf_id)
            .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
        let byte_range = elisp_range_to_byte_clipped_full(new_buf, beg, end);
        let _ = overlay.with_overlay_data_mut(|object| {
            object.buffer = Some(new_buf_id);
            object.start = byte_range.start().get();
            object.end = byte_range.end().get();
        });
        new_buf.overlays.insert_overlay(overlay);
        new_buf.increment_overlay_modified_tick();
        if byte_range.is_empty()
            && new_buf
                .overlays
                .overlay_get_named(overlay, Value::symbol("evaporate"))
                .is_some_and(|value| value.is_truthy())
            && new_buf.overlays.delete_overlay(overlay)
        {
            new_buf.increment_overlay_modified_tick();
        }
        Ok(args[0])
    }
}

/// (overlay-start OVERLAY)
pub(crate) fn builtin_overlay_start(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_overlay_start_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_overlay_start_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("overlay-start", &args, 1)?;
    let overlay = expect_overlay(&args[0])?;
    let Some(buf_id) = resolve_overlay_buffer_id(overlay) else {
        return Ok(Value::NIL);
    };
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    match buf.overlays.overlay_start_emacs_byte_pos(overlay) {
        Some(byte_pos) => Ok(Value::fixnum(byte_to_elisp_pos(buf, byte_pos))),
        None => Ok(Value::NIL),
    }
}

/// (overlay-end OVERLAY)
pub(crate) fn builtin_overlay_end(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_overlay_end_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_overlay_end_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("overlay-end", &args, 1)?;
    let overlay = expect_overlay(&args[0])?;
    let Some(buf_id) = resolve_overlay_buffer_id(overlay) else {
        return Ok(Value::NIL);
    };
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    match buf.overlays.overlay_end_emacs_byte_pos(overlay) {
        Some(byte_pos) => Ok(Value::fixnum(byte_to_elisp_pos(buf, byte_pos))),
        None => Ok(Value::NIL),
    }
}

/// (overlay-buffer OVERLAY)
pub(crate) fn builtin_overlay_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_overlay_buffer_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_overlay_buffer_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("overlay-buffer", &args, 1)?;
    let overlay = expect_overlay(&args[0])?;
    if let Some(buf_id) = resolve_overlay_buffer_id(overlay)
        && buffers.get(buf_id).is_some()
    {
        return Ok(Value::make_buffer(buf_id));
    }
    Ok(Value::NIL)
}

/// (overlay-properties OVERLAY)
pub(crate) fn builtin_overlay_properties(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_overlay_properties_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_overlay_properties_in_buffers(
    _buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("overlay-properties", &args, 1)?;
    let overlay = expect_overlay(&args[0])?;
    builtin_copy_sequence(vec![
        overlay.as_overlay_data().map_or(Value::NIL, |d| d.plist),
    ])
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
