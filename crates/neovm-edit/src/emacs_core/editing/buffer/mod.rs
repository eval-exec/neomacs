use crate::emacs_core::edit_host::{EditHost, EditVariableRead};

use neovm_value::error::{
    EvalResult, Flow, LispCondition, TakeCount, expect_args, expect_args_range, expect_fixnum,
    expect_max_args, expect_min_args, signal,
};
#[cfg(any(test, feature = "test-support"))]
use std::time::Instant;

// ===========================================================================
// Buffer operations (require evaluator for BufferManager access)
// ===========================================================================

use crate::buffer::{
    Buffer, BufferId, BufferManager, CharLen, CharPos0, CharRange, EmacsByteLen, EmacsBytePos,
    EmacsByteRange, LispBytePos1, LispCharPos1, TextChange, TextEditRange, TextExtent,
};
use neovm_value::emacs_char::EmacsChar;
use neovm_value::tagged::value::SymbolMode;
use neovm_value::value::{
    OrderedRuntimeBindingMap, Value, ValueKind, VecLikeType, eq_value, equal_value,
    get_string_text_properties_table_for_value, set_string_text_properties_table_for_value,
};

#[derive(Clone, Copy)]
pub(crate) struct MakeIndirectBufferPlan {
    pub(crate) id: BufferId,
    pub(crate) saved_current: Option<BufferId>,
    pub(crate) run_clone_hook: bool,
}

#[doc(hidden)]
pub fn expect_buffer_id(value: &Value) -> Result<BufferId, Flow> {
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => Ok(value.as_buffer_id().unwrap()),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), *value],
        )),
    }
}

#[doc(hidden)]
pub fn expect_buffer_name_string(value: &Value) -> Result<String, Flow> {
    value
        .as_lisp_string()
        .map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), *value],
            )
        })
}

// Local copies of the lisp builtins layer's argument extractors: the
// bodies are pure `Value`-layer checks, and the per-module copy keeps this
// file free of the lisp builtins layer below the split (established
// practice: strings_core, casefiddle, chartable, network, and search each
// carry one).
fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

fn maybe_trace_characterp_nil(value: &Value, source: &str) {
    if !value.is_nil() {
        return;
    }
    if std::env::var("NEOVM_TRACE_CHARACTERP_NIL").unwrap_or_default() != "1" {
        return;
    }
    eprintln!(
        "NEOVM_TRACE_CHARACTERP_NIL source={source}\n{}",
        std::backtrace::Backtrace::force_capture()
    );
}

fn expect_character_code(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(c) if (0..=0x3F_FFFF).contains(&c) => Ok(c),
        _other => {
            maybe_trace_characterp_nil(value, "expect_character_code");
            Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), *value],
            ))
        }
    }
}

fn expect_wholenump(value: &Value) -> Result<i64, Flow> {
    let n = match value.kind() {
        ValueKind::Fixnum(n) => n,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("wholenump"), *value],
            ));
        }
    };
    if n < 0 {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), *value],
        ));
    }
    Ok(n)
}

fn expect_lisp_string(value: &Value) -> Result<&neovm_value::heap_types::LispString, Flow> {
    value.as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )
    })
}

/// Validate a string argument and decode it to a Rust `String` for text-only
/// processing (display strings, names, identifiers). Valid Unicode (including
/// real Private-Use glyphs) is preserved exactly; raw eight-bit bytes become
/// U+FFFD. Callers that must preserve raw bytes use `expect_lisp_string`.
fn expect_string_lossy(value: &Value) -> Result<String, Flow> {
    expect_lisp_string(value).map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

fn find_buffer_by_name_arg(
    buffers: &BufferManager,
    value: &Value,
) -> Result<Option<BufferId>, Flow> {
    let name = expect_buffer_name_string(value)?;
    Ok(buffers.find_buffer_by_name(&name))
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

#[doc(hidden)]
pub fn normalize_narrow_region_in_buffers(
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

#[doc(hidden)]
pub fn expect_integer_or_marker_in_buffers(
    buffers: &BufferManager,
    value: &Value,
) -> Result<i64, Flow> {
    crate::emacs_core::position::fix_position_with_buffers(buffers, value)
}

/// Local copy of the lisp builtins layer's `integer-or-marker-p` extractor
/// (per-module copies are established practice below the split). Strictly
/// fixnum-or-marker with the live-buffer marker read — NOT
/// `position::fix_position_eval`, which additionally clamps bignums and so
/// is not behavior-equivalent here.
fn canonicalize_or_self(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Run a named hook through the host seam — the C13 editfns idiom: resolve
/// the symbol by name (alias-resolving), read the buffer-local-aware hook
/// value, run with `inherit_global` as `Frun_hooks`/`run-hook-with-args`
/// do. A nil hook value is `run_hook_value`'s own empty-collection no-op
/// arm, and a cyclic alias that `resolve_hook_symbol` would signal on
/// falls back to the unresolved symbol, matching the host methods the
/// peer approved for editfns' before/after-change signals.
fn run_named_hook_via_host<H: EditHost>(
    host: &mut H,
    hook_name: &str,
    hook_args: &[Value],
) -> EvalResult {
    let hook_sym = host.hook_symbol_by_name(hook_name);
    let hook_value = host.hook_value_by_id(hook_sym).unwrap_or(Value::NIL);
    host.run_hook_value(hook_sym, hook_value, hook_args, true)
}

#[doc(hidden)]
pub fn run_buffer_list_update_hook<H: EditHost>(host: &mut H) -> EvalResult {
    run_named_hook_via_host(host, "buffer-list-update-hook", &[])
}

/// GNU `Fkill_all_local_variables` runs the normal hook
/// `change-major-mode-hook` via `run_hook` as its first action.  This is the
/// shared entry point used by every major-mode switch.
pub(crate) fn run_buffer_change_major_mode_hook<H: EditHost>(host: &mut H) -> EvalResult {
    run_named_hook_via_host(host, "change-major-mode-hook", &[])
}

#[doc(hidden)]
pub fn builtin_get_buffer_create<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_min_args("get-buffer-create", &args, 1)?;
    expect_max_args("get-buffer-create", &args, 2)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => Ok(args[0]),
        _ => {
            let name = expect_string_lossy(&args[0])?;
            let inhibit_buffer_hooks = args.get(1).is_some_and(|value| !value.is_nil());
            let (id, created) = {
                let (edit, _) = host.edit_and_vars();
                if let Some(id) = edit.buffers.find_buffer_by_name(&name) {
                    (id, false)
                } else {
                    let id = edit
                        .buffers
                        .create_buffer_with_hook_inhibition(&name, inhibit_buffer_hooks);
                    (id, true)
                }
            };
            if created && !inhibit_buffer_hooks {
                run_buffer_list_update_hook(host)?;
            }
            Ok(Value::make_buffer(id))
        }
    }
}

/// (make-indirect-buffer BASE-BUFFER NAME &optional CLONE INHIBIT-BUFFER-HOOKS) → buffer
#[doc(hidden)]
pub fn builtin_make_indirect_buffer<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    let plan = {
        let (edit, _) = host.edit_and_vars();
        prepare_make_indirect_buffer_in_manager(&mut edit.buffers, args)?
    };
    finish_make_indirect_buffer_hooks(host, plan)
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

pub(crate) fn finish_make_indirect_buffer_hooks<H: EditHost>(
    host: &mut H,
    plan: MakeIndirectBufferPlan,
) -> EvalResult {
    if plan.run_clone_hook {
        // GNU make-indirect-buffer (src/buffer.c:950) runs the clone setup
        // under `record_unwind_current_buffer` + `set_buffer_internal_1 (b)`.
        // set_buffer_internal_1 does NOT touch buffer-list recency: GNU only
        // records a buffer when it is displayed (`record_buffer`, window.c:582).
        // The unrecorded switch is therefore the GNU-faithful one here; the
        // recorded switch this used to call added a spurious recency bump.
        host.set_current_buffer_unrecorded(plan.id)?;
        let clone_result = run_named_hook_via_host(host, "clone-indirect-buffer-hook", &[]);
        if let Some(saved_id) = plan.saved_current {
            host.restore_current_buffer_if_live(saved_id);
        }
        clone_result?;
    }
    let hooks_inhibited = {
        let (edit, _) = host.edit_and_vars();
        edit.buffers.buffer_hooks_inhibited(plan.id)
    };
    if !hooks_inhibited {
        run_buffer_list_update_hook(host)?;
    }
    Ok(Value::make_buffer(plan.id))
}

#[doc(hidden)]
pub fn builtin_get_buffer(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = edit_buffers;
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

#[doc(hidden)]
pub fn builtin_find_buffer<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    let symbol_mode = host.symbol_mode();
    let dynamic: &[OrderedRuntimeBindingMap] = &[];
    expect_args("find-buffer", &args, 2)?;
    let name = args[0].as_symbol_name(symbol_mode).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        )
    })?;
    let target_value = args[1];

    let name_id = neovm_value::intern::intern(name);
    let fallback_value = dynamic
        .iter()
        .rev()
        .find_map(|frame| frame.get(&name_id).cloned())
        // GNU find-buffer reads VARIABLE per CANDIDATE buffer
        // (Fbuffer_local_value, src/buffer.c:1377ff): the buffer's own
        // binding, else the variable's DEFAULT (Fdefault_value at
        // buffer.c:1399/1408). That fallback is the explicitly-named
        // default read: GNU Fdefault_value returns the default cell, which a
        // dynamic `let` reaches when specbind stores in place (PLAINVAL /
        // forwarded) or through LET_DEFAULT, but a buffer-local binding is
        // not the default and must come from the candidate's own lookup.
        .or_else(|| host.default_value(neovm_value::intern::intern(name)))
        .ok_or_else(|| signal(LispCondition::VoidVariable, vec![Value::symbol(name)]))?;

    let (edit, _) = host.edit_and_vars();
    // GNU `Ffind_buffer` walks the live buffers in buffer-list order
    // (`FOR_EACH_LIVE_BUFFER`, src/buffer.c:544-555). It does NOT start with
    // the current buffer: when several buffers share the same effective value
    // (a setq-local in one and the default elsewhere), the first buffer in
    // buffer-list order wins, which is often *scratch* rather than the
    // selected buffer.
    let scan_order = edit.buffers.buffer_list();

    let key = Value::from_sym_id(name_id);
    for id in scan_order {
        let Some(buf) = edit.buffers.get(id) else {
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

#[doc(hidden)]
pub fn builtin_delete_all_overlays(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = edit_buffers;
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

#[doc(hidden)]
pub fn builtin_buffer_live_p(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: &[Value],
) -> EvalResult {
    let buffers = edit_buffers;
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
#[doc(hidden)]
pub fn builtin_get_truename_buffer<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("get-truename-buffer", &args, 1)?;
    let buffer_ids = {
        let (edit, _) = host.edit_and_vars();
        edit.buffers.buffer_list()
    };
    for id in buffer_ids {
        // GNU skips buffers whose file_truename is not a string BEFORE
        // calling Fstring_equal, so a non-string FILENAME only signals once
        // some live buffer visits a file.
        let truename_value = {
            let (edit, _) = host.edit_and_vars();
            edit.buffers
                .get(id)
                .and_then(|buf| buf.buffer_local_value("buffer-file-truename"))
        };
        let Some(truename_value) = truename_value else {
            continue;
        };
        let Some(truename) = host.lisp_string(truename_value) else {
            continue;
        };
        let filename = host.expect_lisp_string(args[0])?;
        if truename.schars() == filename.schars()
            && truename.sbytes() == filename.sbytes()
            && truename.as_bytes() == filename.as_bytes()
        {
            return Ok(Value::make_buffer(id));
        }
    }
    Ok(Value::NIL)
}

#[doc(hidden)]
pub fn builtin_get_file_buffer<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("get-file-buffer", &args, 1)?;
    let resolved = host.resolve_filename(expect_lisp_string(&args[0])?);
    let resolved_path = super::file_name::lisp_file_name_to_path_buf(&resolved);
    let resolved_true = canonicalize_or_self(&resolved_path);

    let buffer_ids = {
        let (edit, _) = host.edit_and_vars();
        edit.buffers.buffer_list()
    };
    for id in buffer_ids {
        let file_name = {
            let (edit, _) = host.edit_and_vars();
            match edit
                .buffers
                .get(id)
                .and_then(|buf| buf.file_name_lisp_string())
            {
                Some(name) => name.clone(),
                None => continue,
            }
        };

        let candidate = host.resolve_filename(&file_name);
        if candidate == resolved {
            return Ok(Value::make_buffer(id));
        }
        let candidate_path = super::file_name::lisp_file_name_to_path_buf(&candidate);
        if canonicalize_or_self(&candidate_path) == resolved_true {
            return Ok(Value::make_buffer(id));
        }
    }

    Ok(Value::NIL)
}

/// What `kill-buffer`'s hooks decided.

/// GNU `other_buffer_safely` (buffer.c:1802): the first live, non-hidden
/// buffer other than `avoid_id`, ignoring frame-local buffer lists, window
/// visibility and frame predicates.  Only when no other live buffer exists
/// does it fall back to `*scratch*`, matching GNU's
/// `get-scratch-buffer-create` fallback.
#[doc(hidden)]
pub fn other_buffer_safely_in_state(
    buffers: &mut crate::buffer::BufferManager,
    avoid_id: Option<crate::buffer::BufferId>,
) -> crate::buffer::BufferId {
    for id in buffers.buffer_list() {
        if Some(id) != avoid_id && !is_hidden_buffer(buffers, id) {
            return id;
        }
    }
    buffers
        .find_buffer_by_name("*scratch*")
        .unwrap_or_else(|| buffers.create_buffer("*scratch*"))
}

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_set_buffer<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    neovm_value::error::expect_args("set-buffer", &args, 1)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_set_buffer_1(host, arg(0))
}
/// `set-buffer` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_set_buffer_1<H: EditHost>(host: &mut H, buffer_or_name: Value) -> EvalResult {
    let args: [Value; 1] = [buffer_or_name];
    expect_args("set-buffer", &args, 1)?;
    let id = {
        let (edit, _) = host.edit_and_vars();
        match args[0].kind() {
            ValueKind::Veclike(VecLikeType::Buffer) => {
                let bid = args[0].as_buffer_id().unwrap();
                if edit.buffers.get(bid).is_none() {
                    return Err(signal(
                        "error",
                        vec![Value::string("Selecting deleted buffer")],
                    ));
                }
                bid
            }
            ValueKind::String => {
                let s = expect_buffer_name_string(&args[0])?;
                edit.buffers.find_buffer_by_name(&s).ok_or_else(|| {
                    signal("error", vec![Value::string(format!("No buffer named {s}"))])
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
    host.set_current_buffer_unrecorded(id)?;
    Ok(Value::make_buffer(id))
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn builtin_current_buffer(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-buffer", &args, 0)?;
    builtin_current_buffer_0(edit_buffers)
}
/// `current-buffer` as registered: fixed arity 0, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a0` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_current_buffer_0(edit_buffers: &mut crate::buffer::BufferManager) -> EvalResult {
    let args: [Value; 0] = [];
    let buffers = edit_buffers;
    expect_args("current-buffer", &args, 0)?;
    match buffers.current_buffer() {
        Some(buf) => Ok(Value::make_buffer(buf.id)),
        None => Ok(Value::NIL),
    }
}

#[doc(hidden)]
pub fn builtin_buffer_name(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: &[Value],
) -> EvalResult {
    let buffers = edit_buffers;
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

#[doc(hidden)]
pub fn builtin_buffer_file_name(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = edit_buffers;
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

#[doc(hidden)]
pub fn builtin_buffer_base_buffer(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = edit_buffers;
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

#[doc(hidden)]
pub fn builtin_buffer_last_name(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = edit_buffers;
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
fn buffer_access_fontify_functions_sym() -> neovm_value::intern::SymId {
    static SYMBOL: std::sync::OnceLock<neovm_value::intern::SymId> = std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| neovm_value::intern::intern("buffer-access-fontify-functions"))
}

#[inline(always)]
fn buffer_access_fontified_property_sym() -> neovm_value::intern::SymId {
    static SYMBOL: std::sync::OnceLock<neovm_value::intern::SymId> = std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| neovm_value::intern::intern("buffer-access-fontified-property"))
}

/// (buffer-substring START END) → string
fn update_buffer_properties_for_access<H: EditHost>(
    host: &mut H,
    start: LispCharPos1,
    end: LispCharPos1,
) -> Result<(), Flow> {
    if host
        .visible_variable_value_or_nil(buffer_access_fontify_functions_sym())
        .is_nil()
    {
        return Ok(());
    }

    let fontified_property =
        host.visible_variable_value_or_nil(buffer_access_fontified_property_sym());
    if !fontified_property.is_nil() {
        let needs_fontification = super::textprop::builtin_text_property_any(
            &mut *host,
            &[
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

    run_named_hook_via_host(
        &mut *host,
        "buffer-access-fontify-functions",
        &[Value::fixnum(start.as_i64()), Value::fixnum(end.as_i64())],
    )?;
    Ok(())
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_buffer_substring<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("buffer-substring", &args, 2)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_buffer_substring_2(host, arg(0), arg(1))
}
/// `buffer-substring` as registered: fixed arity 2, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a2` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_buffer_substring_2<H: EditHost>(
    host: &mut H,
    start: Value,
    end: Value,
) -> EvalResult {
    let args: [Value; 2] = [start, end];
    expect_args("buffer-substring", &args, 2)?;
    let (current_id, start, end) = {
        let (edit, _) = host.edit_and_vars();
        let start = expect_integer_or_marker_in_buffers(&edit.buffers, &args[0])?;
        let end = expect_integer_or_marker_in_buffers(&edit.buffers, &args[1])?;
        let current_id = edit
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let buf = edit
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let (start, end) = checked_accessible_lisp_range_from_raw(
            buf,
            start,
            end,
            vec![Value::make_buffer(buf.id), args[0], args[1]],
        )?;
        (current_id, start, end)
    };
    update_buffer_properties_for_access(host, start, end)?;
    let (edit, _) = host.edit_and_vars();
    let buf = edit
        .buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let byte_range = accessible_lisp_range_to_byte_range(buf, start, end);
    Ok(buffer_slice_value_range(buf, byte_range))
}

#[doc(hidden)]
pub fn builtin_buffer_string<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("buffer-string", &args, 0)?;
    let (current_id, start, end) = {
        let (edit, _) = host.edit_and_vars();
        let buf = edit
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            buf.id,
            buf.point_min_lisp_char_pos(),
            buf.point_max_lisp_char_pos(),
        )
    };
    update_buffer_properties_for_access(host, start, end)?;
    let (edit, _) = host.edit_and_vars();
    let buf = edit
        .buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(buffer_slice_value_range(
        buf,
        buf.accessible_emacs_byte_range(),
    ))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[doc(hidden)]
pub fn resolve_buffer_designator_allow_nil_current(
    edit_buffers: &mut crate::buffer::BufferManager,
    arg: &Value,
) -> Result<Option<BufferId>, Flow> {
    match arg.kind() {
        ValueKind::Nil => edit_buffers
            .current_buffer()
            .map(|buf| Some(buf.id))
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = arg.as_buffer_id().unwrap();
            if edit_buffers.get(id).is_some() {
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
            edit_buffers
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

/// A call-local argument policy; it holds no shared or mutator state.
#[derive(Clone, Copy, Debug)]
enum BufferDesignatorDefault {
    Current,
    Required,
}

#[doc(hidden)]
pub fn resolve_buffer_designator_allow_nil_current_in_manager(
    buffers: &BufferManager,
    arg: &Value,
) -> Result<Option<BufferId>, Flow> {
    resolve_buffer_designator_in_manager(buffers, arg, BufferDesignatorDefault::Current).map(Some)
}

fn resolve_buffer_designator_in_manager(
    buffers: &BufferManager,
    arg: &Value,
    default: BufferDesignatorDefault,
) -> Result<BufferId, Flow> {
    match arg.kind() {
        ValueKind::Nil => match default {
            BufferDesignatorDefault::Current => {
                buffers.current_buffer().map(|buf| buf.id).ok_or_else(|| {
                    signal(
                        LispCondition::Error,
                        vec![Value::string("No current buffer")],
                    )
                })
            }
            BufferDesignatorDefault::Required => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), *arg],
            )),
        },
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = arg.as_buffer_id().unwrap();
            if buffers.get(id).is_some() {
                Ok(id)
            } else {
                Err(signal(
                    LispCondition::Error,
                    vec![Value::string("Selecting deleted buffer")],
                ))
            }
        }
        ValueKind::String => {
            let name = expect_buffer_name_string(arg)?;
            buffers.find_buffer_by_name(&name).ok_or_else(|| {
                signal(
                    LispCondition::Error,
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
) -> Result<neovm_value::heap_types::LispString, Flow> {
    let Some(buffer_id) = buffer_id else {
        return Ok(neovm_value::heap_types::LispString::from_utf8(""));
    };
    let Some(buf) = buffers.get(buffer_id) else {
        return Ok(neovm_value::heap_types::LispString::from_utf8(""));
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
            neovm_value::heap_types::LispString::from_emacs_bytes(Vec::new()),
        ));
    };
    let Some(buf) = buffers.get(buffer_id) else {
        return Ok(Value::heap_string(
            neovm_value::heap_types::LispString::from_emacs_bytes(Vec::new()),
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
    left: &neovm_value::heap_types::LispString,
    right: &neovm_value::heap_types::LispString,
    fold: impl Fn(u32) -> u32,
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
                let (a_code, a_len) = if left.is_multibyte() {
                    let (c, n) = neovm_value::emacs_char::string_char(&left_bytes[lp..]);
                    (c.code(), n)
                } else {
                    (neovm_value::emacs_char::unibyte_to_char(left_bytes[lp]), 1)
                };
                let (b_code, b_len) = if right.is_multibyte() {
                    let (c, n) = neovm_value::emacs_char::string_char(&right_bytes[rp..]);
                    (c.code(), n)
                } else {
                    (neovm_value::emacs_char::unibyte_to_char(right_bytes[rp]), 1)
                };
                lp += a_len;
                rp += b_len;
                // GNU editfns.c:1860-1867 applies the same current canonical
                // table to both characters. Equal source codes therefore
                // remain equal; this callback-free comparison need not look
                // them up, even when an installed table changes their case.
                if a_code != b_code {
                    let a = fold(a_code);
                    let b = fold(b_code);
                    if a != b {
                        return if a < b { -pos } else { pos };
                    }
                }
                pos += 1;
            }
            (true, false) => return pos,
            (false, true) => return -pos,
            (false, false) => return 0,
        }
    }
}

#[doc(hidden)]
pub fn builtin_buffer_line_statistics(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    let buffers = edit_buffers;
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

#[doc(hidden)]
pub fn replace_region_source_value_in_state(
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

#[doc(hidden)]
pub fn builtin_buffer_swap_text<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("buffer-swap-text", &args, 1)?;
    let (other_id, current_id) = {
        let (edit, _vars) = host.edit_and_vars();
        let other_id = expect_buffer_id(&args[0])?;
        let current_id = edit
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
            .id;
        (other_id, current_id)
    };

    // The swapped result is owned, so the phase closes before the M3c
    // error-mapping host call reborrows `host` shared.
    let swapped = {
        let (edit, _vars) = host.edit_and_vars();
        edit.buffers.swap_buffer_text(current_id, other_id)
    };
    swapped
        .map(|()| Value::NIL)
        .map_err(|err| host.signal_c_error(neovm_value::errors::CErrorMessage::from(err.message())))
}

#[doc(hidden)]
pub fn builtin_insert_buffer_substring<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args_range("insert-buffer-substring", &args, 1, 3)?;
    // GNU editfns.c:1728 resolves a required BUFFER through get-buffer.
    let (buffer_id, default_start, default_end) = {
        let (edit, _) = host.edit_and_vars();
        let buffer_id = Some(resolve_buffer_designator_in_manager(
            &edit.buffers,
            &args[0],
            BufferDesignatorDefault::Required,
        )?);
        let (default_start, default_end) = buffer_id
            .and_then(|id| {
                edit.buffers.get(id).map(|buf| {
                    (
                        buf.point_min_lisp_char_pos().as_i64(),
                        buf.point_max_lisp_char_pos().as_i64(),
                    )
                })
            })
            .unwrap_or((1, 1));
        (buffer_id, default_start, default_end)
    };
    let (start, end) = {
        let (edit, _) = host.edit_and_vars();
        let start = if args.len() > 1 && !args[1].is_nil() {
            expect_integer_or_marker_in_buffers(&edit.buffers, &args[1])?
        } else {
            default_start
        };
        let end = if args.len() > 2 && !args[2].is_nil() {
            expect_integer_or_marker_in_buffers(&edit.buffers, &args[2])?
        } else {
            default_end
        };
        (start, end)
    };

    let text = {
        let (edit, _) = host.edit_and_vars();
        checked_buffer_substring_for_char_region_in_manager(
            &edit.buffers,
            buffer_id,
            LispCharPos1::new(start),
            LispCharPos1::new(end),
            Value::fixnum(start),
            Value::fixnum(end),
        )?
    };
    builtin_insert(host, vec![text])
}

#[doc(hidden)]
pub fn builtin_kill_all_local_variables<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args_range("kill-all-local-variables", &args, 0, 1)?;
    let (initial_id, kill_permanent) = {
        let (edit, _) = host.edit_and_vars();
        let current_id = edit
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let kill_permanent = args.first().copied().unwrap_or(Value::NIL).is_truthy();
        (current_id, kill_permanent)
    };
    // GNU `Fkill_all_local_variables` (buffer.c) runs the normal hook
    // `change-major-mode-hook` as its very first action, *before* any local
    // bindings are eliminated.  Because every command that selects a new major
    // mode (`fundamental-mode', `text-mode', derived modes, `set-auto-mode',
    // `normal-mode', `set-buffer-major-mode', ...) starts by calling this
    // function, this is the single shared chokepoint where
    // `change-major-mode-hook' must fire.  Running it here -- with the current
    // buffer's local variables and local hook value still in effect, and with
    // `major-mode' still naming the *previous* mode -- matches GNU exactly.
    run_buffer_change_major_mode_hook(host)?;
    let current_id = if super::redisplay_policy::gnu_redisplay_hooks_enabled() {
        let (edit, _) = host.edit_and_vars();
        edit.buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
    } else {
        initial_id
    };

    // GNU buffer.c reset_buffer_local_variables:
    // - preserves most always-local slots
    // - resets only a small fixed reset-on-kill-all subset
    // - clears conditional slot locals unless they are permanent-local
    // - walks local_var_alist for LOCALIZED entries (Phase 10E)
    host.reset_buffer_locals(current_id, kill_permanent);
    // GNU performs this after the hook and local reset. `bset_update_mode_line`
    // raises global `update_mode_lines` even for an offscreen buffer, ensuring
    // the menu and mode line are rebuilt if this major-mode transition is
    // immediately followed by displaying that buffer.
    if super::redisplay_policy::gnu_redisplay_hooks_enabled() {
        // GNU bset_update_mode_line is SOME. The typed helper marks only
        // windows sharing this buffer's text and requests the menu rebuild.
        // Broad chrome cache dirtiness would evaluate unrelated :eval forms.
        host.gnu_mark_buffer_mode_line(current_id);
    } else {
        host.request_global_mode_line_update();
    }
    Ok(Value::NIL)
}

/// `(ntake N LIST)` -> LIST
#[doc(hidden)]
pub fn builtin_ntake(args: Vec<Value>) -> EvalResult {
    expect_args("ntake", &args, 2)?;
    let TakeCount::Positive(n) = TakeCount::try_from(args[0])? else {
        return Ok(Value::NIL);
    };

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
    for _ in 1..n.get() {
        match cursor.kind() {
            ValueKind::Cons => {
                let next = cursor.cons_cdr();
                match next.kind() {
                    ValueKind::Cons => cursor = next,
                    ValueKind::Nil => return Ok(head),
                    _other => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("listp"), head],
                        ));
                    }
                }
            }
            ValueKind::Nil => return Ok(head),
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), head],
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
            vec![Value::symbol("listp"), head],
        )),
    }
}

/// A single change run from the diff used by
/// `replace-region-contents`: replace characters `[a_start, a_end)` of the
/// destination region with characters `[b_start, b_end)` of the source.
/// All indices are character indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct ReplaceRegionChangeRun {
    pub(crate) a_start: usize,
    pub(crate) a_end: usize,
    pub(crate) b_start: usize,
    pub(crate) b_end: usize,
}

/// Decode an Emacs-byte buffer/string slice into a vector of `(codepoint,
/// byte_offset)` pairs.  `byte_offset` is the offset (within `bytes`) of each
/// character; a final sentinel entry `(0, bytes.len())` is appended so callers
/// can map a character index to its byte offset for `0..=len`.
#[doc(hidden)]
pub fn decode_chars_with_byte_offsets(bytes: &[u8]) -> Vec<(u32, usize)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let (code, len) = neovm_value::emacs_char::string_char(&bytes[pos..]);
        out.push((code.code(), pos));
        pos += len.max(1);
    }
    out.push((0, bytes.len()));
    out
}

#[doc(hidden)]
pub mod replace_diff;

/// GNU diffseq change runs, with linear workspace and MAX-COSTS heuristics.
/// None means timeout or an unavailable comparison workspace; no edit has
/// occurred. Only tests use this default-cost wrapper; the builtin passes the
/// caller's MAX-COSTS and MAX-SECS directly.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn replace_region_minimal_change_runs(
    a: &[u32],
    b: &[u32],
    deadline: Option<Instant>,
) -> Option<Vec<ReplaceRegionChangeRun>> {
    replace_diff::change_runs(a, b, deadline, replace_diff::DEFAULT_MAX_COSTS)
}

#[doc(hidden)]
pub fn replace_region_max_costs(value: Option<&Value>) -> Result<usize, Flow> {
    match value.copied().unwrap_or(Value::NIL) {
        value if value.is_nil() => Ok(replace_diff::DEFAULT_MAX_COSTS),
        value if value.is_fixnum() => Ok(value.as_fixnum().unwrap().max(0) as usize),
        value if value.is_integer() => {
            let big = value.as_bignum().expect("non-fixnum integer");
            Ok(if *big < 0 {
                0
            } else {
                isize::try_from(big).unwrap_or(isize::MAX) as usize
            })
        }
        value => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), value],
        )),
    }
}

// `builtin_replace_buffer_contents` (the GNU subr.el `replace-buffer-contents`
// thin wrapper over `replace-region-contents`) moved up to
// `commands/buffer_ops` with the stage-3 C12 seam work: the edit layer must
// not call the lisp builtins layer, and the wrapper is evaluator-bound.

/// True when `value` is the fixnum 0 (used for GNU's MAX-SECS/MAX-COSTS == 0
/// "disable comparison" fallback).
#[doc(hidden)]
pub fn eq_fixnum_zero(value: &Value) -> bool {
    value.as_fixnum() == Some(0)
}

/// `(compare-buffer-substrings BUF1 START1 END1 BUF2 START2 END2)` -> integer
///
/// The case-fold policy (the dynamic variable and the current buffer's
/// canonical case table, GNU editfns.c:1769-1771) is computed by the upper
/// command wrapper and passed in; this editor-layer body only compares, so it
/// takes edit state (`BufferManager`) and stays monomorphic.
#[doc(hidden)]
pub fn compare_buffer_substrings_with_canon(
    buffers: &BufferManager,
    args: Vec<Value>,
    canon: Option<Value>,
) -> EvalResult {
    compare_buffer_substrings_with_translation(buffers, args, |code| {
        canon
            .and_then(|table| neovm_value::chartab::ct_lookup(&table, code as i64).ok())
            .and_then(|value| value.as_fixnum())
            .filter(|&mapped| (0..=neovm_value::emacs_char::MAX_CHAR as i64).contains(&mapped))
            .map_or(code, |mapped| mapped as u32)
    })
}

fn compare_buffer_substrings_with_translation(
    buffers: &BufferManager,
    args: Vec<Value>,
    fold: impl Fn(u32) -> u32,
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
        &left, &right, fold,
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

struct ConstrainToFieldSyms {
    field: neovm_value::intern::SymId,
    category: neovm_value::intern::SymId,
    inhibit_field_text_motion: neovm_value::intern::SymId,
    char_property_alias_alist: neovm_value::intern::SymId,
    default_text_properties: neovm_value::intern::SymId,
}

fn constrain_to_field_syms() -> &'static ConstrainToFieldSyms {
    static SYMS: std::sync::OnceLock<ConstrainToFieldSyms> = std::sync::OnceLock::new();
    SYMS.get_or_init(|| ConstrainToFieldSyms {
        field: neovm_value::intern::intern("field"),
        category: neovm_value::intern::intern("category"),
        inhibit_field_text_motion: neovm_value::intern::intern("inhibit-field-text-motion"),
        char_property_alias_alist: neovm_value::intern::intern("char-property-alias-alist"),
        default_text_properties: neovm_value::intern::intern("default-text-properties"),
    })
}

/// True when no position in the current buffer can yield a `field` char
/// property: no overlays, neither `field` nor a `category` symbol (whose
/// plist GNU `textget` would consult) ever assigned as a text property, and
/// no alias alist or `default-text-properties` that could supply one.
fn current_buffer_cannot_have_fields<'a, V: EditVariableRead<'a>>(
    vars: V,
    buffers: &BufferManager,
) -> bool {
    use crate::buffer::text_props::PropertyNamePresence::DefinitelyAbsent;
    let syms = constrain_to_field_syms();
    let Some(buf) = buffers.current_buffer() else {
        return false;
    };
    let nil_var =
        |sym: neovm_value::intern::SymId| vars.value(sym).is_some_and(|value| value.is_nil());
    buf.overlays.is_empty()
        && buf.text_props_property_name_presence(Value::from_sym_id(syms.field)) == DefinitelyAbsent
        && buf.text_props_property_name_presence(Value::from_sym_id(syms.category))
            == DefinitelyAbsent
        && nil_var(syms.char_property_alias_alist)
        && nil_var(syms.default_text_properties)
}

#[doc(hidden)]
pub fn builtin_constrain_to_field<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args_range("constrain-to-field", &args, 2, 5)?;
    builtin_constrain_to_field_5(host, &args)
}

/// GNU field motion first coerces markers and saturates bignums to fixnums.
/// This immutable scalar owns no heap pointers or mutable buffer state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FieldMotionPosition(neovm_value::tagged::value::Fixnum);

impl TryFrom<(&BufferManager, Value)> for FieldMotionPosition {
    type Error = Flow;

    fn try_from((buffers, value): (&BufferManager, Value)) -> Result<Self, Self::Error> {
        if let Some(position) = value.as_fixnum_value() {
            return Ok(Self(position));
        }
        let position = expect_integer_or_marker_in_buffers(buffers, &value)?;
        neovm_value::tagged::value::Fixnum::try_from(position)
            .map(Self)
            .map_err(|error| match error {
                neovm_value::tagged::value::FixnumRangeError::OutOfRange(_) => {
                    signal(LispCondition::OverflowError, vec![])
                }
            })
    }
}

impl From<crate::buffer::position::BufferLispPos> for FieldMotionPosition {
    fn from(position: crate::buffer::position::BufferLispPos) -> Self {
        Self(position.into())
    }
}

static_assertions::assert_impl_all!(FieldMotionPosition: Send, Sync);

/// The accessible region `[BEGV, ZV]` that field-motion probe positions must
/// lie in, read once from the current buffer before any Lisp can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FieldProbeBounds {
    begv: LispCharPos1,
    zv: LispCharPos1,
}

impl From<&Buffer> for FieldProbeBounds {
    fn from(buffer: &Buffer) -> Self {
        Self {
            begv: buffer.point_min_lisp_char_pos(),
            zv: buffer.point_max_lisp_char_pos(),
        }
    }
}

impl FieldProbeBounds {
    /// GNU's char-property probes signal `args-out-of-range` outside the
    /// accessible region.
    fn check_probe(self, position: FieldMotionPosition) -> Result<(), Flow> {
        let candidate = LispCharPos1::new(i64::from(position.0));
        if candidate < self.begv || candidate > self.zv {
            return Err(field_probe_out_of_range(position));
        }
        Ok(())
    }
}

#[cold]
#[inline(never)]
fn field_probe_out_of_range(position: FieldMotionPosition) -> Flow {
    signal(
        LispCondition::ArgsOutOfRange,
        vec![Value::from_fixnum(position.0)],
    )
}

/// `constrain-to-field` on an argument slice (2..=5 values, already
/// arity-checked): the internal callers (`line-beginning-position`,
/// `line-end-position`, `forward-word`) pass a stack array instead of
/// consing a `Vec` per call.
#[doc(hidden)]
pub fn builtin_constrain_to_field_5<H: EditHost>(host: &mut H, args: &[Value]) -> EvalResult {
    let syms = constrain_to_field_syms();
    let (bounds, point_min, orig_point, new_argument, old_argument, inhibit_field_text_motion) = {
        let (edit, vars) = host.edit_and_vars();
        let current = edit
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let bounds = FieldProbeBounds::from(&*current);
        let point_min = bounds.begv.as_i64();
        let orig_point = if args[0].is_nil() {
            Some(FieldMotionPosition::from(current.point_position()))
        } else {
            None
        };
        let new_argument = if let Some(point) = orig_point {
            point
        } else {
            FieldMotionPosition::try_from((&edit.buffers, args[0]))?
        };
        let old_argument = FieldMotionPosition::try_from((&edit.buffers, args[1]))?;
        let inhibit_field_text_motion = vars
            .value(syms.inhibit_field_text_motion)
            .is_some_and(|value| !value.is_nil());
        (
            bounds,
            point_min,
            orig_point,
            new_argument,
            old_argument,
            inhibit_field_text_motion,
        )
    };
    let mut result = new_argument;
    let new_pos = i64::from(new_argument.0);
    let old_pos = i64::from(old_argument.0);
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
    let mut constrain = !inhibit_field_text_motion && new_pos != old_pos;
    if constrain {
        let cannot_have_fields = {
            let (edit, vars) = host.edit_and_vars();
            current_buffer_cannot_have_fields(vars, &edit.buffers)
        };
        if cannot_have_fields {
            // These two probes would both return nil before GNU's OR completes.
            // Retain their accessible-range checks when skipping the lookups.
            // Buffers with fields keep the original short-circuit probe ordering.
            bounds.check_probe(new_argument)?;
            bounds.check_probe(old_argument)?;
            // GNU would now run up to four `Fget_char_property` probes; when the
            // buffer cannot hold a `field` anywhere they all answer nil, and
            // `line-beginning-position` calls this once per line (~1.8K Ir).
            constrain = false;
        }
    }

    if constrain {
        let field = Value::from_sym_id(syms.field);
        constrain = {
            let (edit, vars) = host.edit_and_vars();
            let has_field = |pos: i64| -> Result<bool, Flow> {
                Ok(!char_property_in_current_buffer(vars, &edit.buffers, pos, field)?.is_nil())
            };
            has_field(new_pos)?
                || has_field(old_pos)?
                || (new_pos > point_min && has_field(new_pos - 1)?)
                || (old_pos > point_min && has_field(old_pos - 1)?)
        };
    }

    if constrain && let Some(capture_prop) = args.get(4).filter(|value| !value.is_nil()) {
        let capture = {
            let (edit, vars) = host.edit_and_vars();
            let old_capture = super::textprop::builtin_get_pos_property_impl(
                vars,
                &[],
                |_| super::textprop::WindowBufferLookup::NotAvailable,
                &edit.buffers,
                &[Value::fixnum(old_pos), *capture_prop],
            )?;
            let still_constrained = old_capture.is_nil()
                && (old_pos <= point_min
                    || char_property_in_current_buffer(
                        vars,
                        &edit.buffers,
                        old_pos,
                        *capture_prop,
                    )?
                    .is_nil()
                    || char_property_in_current_buffer(
                        vars,
                        &edit.buffers,
                        old_pos - 1,
                        *capture_prop,
                    )?
                    .is_nil());
            still_constrained
        };
        constrain = capture;
    }

    if constrain {
        let forward = new_pos > old_pos;
        let field_bound = if forward {
            builtin_field_end(
                host,
                vec![
                    Value::from_fixnum(old_argument.0),
                    Value::bool_val(escape_from_edge),
                    Value::from_fixnum(new_argument.0),
                ],
            )?
        } else {
            builtin_field_beginning(
                host,
                vec![
                    Value::from_fixnum(old_argument.0),
                    Value::bool_val(escape_from_edge),
                    Value::from_fixnum(new_argument.0),
                ],
            )?
        };
        expect_int(&field_bound)?;
        let (field_bound, same_line) = {
            let (edit, _) = host.edit_and_vars();
            let field_bound = FieldMotionPosition::try_from((&edit.buffers, field_bound))?;
            let bound = i64::from(field_bound.0);
            let should_constrain = if bound < new_pos { forward } else { !forward };
            let same_line = !only_in_line
                || !current_buffer_has_newline_between_positions(&edit.buffers, new_pos, bound)?;
            (field_bound, should_constrain && same_line)
        };
        if same_line {
            result = field_bound;
        }
    }

    let new_pos = i64::from(result.0);
    if let Some(orig_point) = orig_point.map(|point| i64::from(point.0))
        && new_pos != orig_point
    {
        let (edit, _) = host.edit_and_vars();
        let current_id = edit
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let byte_pos = {
            let buf = edit
                .buffers
                .get(current_id)
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            buf.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(new_pos))
        };
        let _ = edit
            .buffers
            .goto_buffer_emacs_byte_pos(current_id, byte_pos);
    }

    Ok(Value::from_fixnum(result.0))
}

fn char_property_in_current_buffer<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
    buffers: &BufferManager,
    pos: i64,
    property: Value,
) -> Result<Value, Flow> {
    crate::emacs_core::textprop::builtin_get_char_property_in_state(
        vars,
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

fn field_property_after_char_in_buffers<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
    buffers: &BufferManager,
    pos: i64,
) -> Result<Value, Flow> {
    let value = crate::emacs_core::textprop::builtin_get_char_property_and_overlay_in_state(
        vars,
        buffers,
        vec![Value::fixnum(pos), Value::symbol("field")],
    )?;
    match value.kind() {
        ValueKind::Cons => Ok(value.cons_car()),
        _other => Err(signal("error", vec![value])),
    }
}

fn field_property_at_position_in_state<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &BufferManager,
    pos: i64,
) -> Result<Value, Flow> {
    super::textprop::builtin_get_pos_property_impl(
        vars,
        dynamic,
        |_| super::textprop::WindowBufferLookup::NotAvailable,
        buffers,
        &[Value::fixnum(pos), Value::symbol("field")],
    )
}

fn previous_field_change_in_buffers<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
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
        &super::textprop::builtin_previous_single_char_property_change_in_buffers(
            vars,
            |_| super::textprop::WindowBufferLookup::NotAvailable,
            buffers,
            args,
        )?,
    )
}

fn next_field_change_in_buffers<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
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
        &super::textprop::builtin_next_single_char_property_change_in_buffers(
            vars,
            |_| super::textprop::WindowBufferLookup::NotAvailable,
            buffers,
            &args,
        )?,
    )
}

#[doc(hidden)]
pub fn find_field_bounds_in_state<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &BufferManager,
    position_value: Option<&Value>,
    merge_at_boundary: bool,
    beg_limit: Option<i64>,
    end_limit: Option<i64>,
) -> Result<(i64, i64), Flow> {
    let (pos, point_min, _point_max) = resolve_field_position_in_buffers(buffers, position_value)?;
    let after_field = field_property_after_char_in_buffers(vars, buffers, pos)?;
    let before_field = if pos > point_min {
        field_property_after_char_in_buffers(vars, buffers, pos - 1)?
    } else {
        after_field
    };

    let mut at_field_start = false;
    let mut at_field_end = false;
    if !merge_at_boundary {
        let field = field_property_at_position_in_state(vars, dynamic, buffers, pos)?;
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
            cursor = previous_field_change_in_buffers(vars, buffers, cursor, beg_limit)?;
        }
        previous_field_change_in_buffers(vars, buffers, cursor, beg_limit)?
    };
    let end = if at_field_end {
        pos
    } else {
        let mut cursor = pos;
        if merge_at_boundary && eq_value(&after_field, &boundary) {
            cursor = next_field_change_in_buffers(vars, buffers, cursor, end_limit)?;
        }
        next_field_change_in_buffers(vars, buffers, cursor, end_limit)?
    };

    Ok((beg, end))
}

#[doc(hidden)]
pub fn builtin_field_beginning<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_max_args("field-beginning", &args, 3)?;
    let (edit, vars) = host.edit_and_vars();
    let limit = match args.get(2) {
        Some(limit_value) if !limit_value.is_nil() => {
            let limit = expect_integer_or_marker_in_buffers(&edit.buffers, limit_value)?;
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
        vars,
        &[],
        &edit.buffers,
        args.first(),
        args.get(1).is_some_and(|value| value.is_truthy()),
        limit,
        None,
    )?;
    Ok(Value::fixnum(beg))
}

#[doc(hidden)]
pub fn builtin_field_end<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_max_args("field-end", &args, 3)?;
    let (edit, vars) = host.edit_and_vars();
    let limit = match args.get(2) {
        Some(limit_value) if !limit_value.is_nil() => Some(expect_integer_or_marker_in_buffers(
            &edit.buffers,
            limit_value,
        )?),
        _ => None,
    };
    let (_, end) = find_field_bounds_in_state(
        vars,
        &[],
        &edit.buffers,
        args.first(),
        args.get(1).is_some_and(|value| value.is_truthy()),
        None,
        limit,
    )?;
    Ok(Value::fixnum(end))
}

#[doc(hidden)]
pub fn builtin_field_string<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_max_args("field-string", &args, 1)?;
    let (beg, end) = {
        let (edit, vars) = host.edit_and_vars();
        find_field_bounds_in_state(vars, &[], &edit.buffers, args.first(), false, None, None)?
    };
    builtin_buffer_substring(host, vec![Value::fixnum(beg), Value::fixnum(end)])
}

#[doc(hidden)]
pub fn builtin_field_string_no_properties<H: EditHost>(
    host: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("field-string-no-properties", &args, 1)?;
    let (beg, end) = {
        let (edit, vars) = host.edit_and_vars();
        find_field_bounds_in_state(vars, &[], &edit.buffers, args.first(), false, None, None)?
    };
    super::editfns::builtin_buffer_substring_no_properties(
        host,
        vec![Value::fixnum(beg), Value::fixnum(end)],
    )
}

#[doc(hidden)]
pub fn builtin_delete_field<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_max_args("delete-field", &args, 1)?;
    let (beg, end) = {
        let (edit, vars) = host.edit_and_vars();
        find_field_bounds_in_state(vars, &[], &edit.buffers, args.first(), false, None, None)?
    };
    super::editfns::builtin_delete_region(host, vec![Value::fixnum(beg), Value::fixnum(end)])
}

/// `(clear-string STRING)` -> nil
/// Zeroes every byte, makes STRING unibyte, and removes text properties.
#[doc(hidden)]
pub fn builtin_clear_string(args: Vec<Value>) -> EvalResult {
    expect_args("clear-string", &args, 1)?;
    let _ = expect_lisp_string(&args[0])?;
    if args[0].is_string() {
        let _ = args[0].with_lisp_string_mut(|lisp_str| {
            lisp_str.clear_contents_to_unibyte();
        });
    }
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_point(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("point", &args, 0)?;
    builtin_point_0(edit_buffers)
}

#[doc(hidden)]
pub fn builtin_point_0(edit_buffers: &mut crate::buffer::BufferManager) -> EvalResult {
    let buf = edit_buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::from_fixnum(buf.point_position().into()))
}

#[doc(hidden)]
pub fn builtin_point_min_0(edit_buffers: &mut crate::buffer::BufferManager) -> EvalResult {
    let buf = edit_buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::from_fixnum(buf.point_min_position().into()))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_point_max(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("point-max", &args, 0)?;
    builtin_point_max_0(edit_buffers)
}

#[doc(hidden)]
pub fn builtin_point_max_0(edit_buffers: &mut crate::buffer::BufferManager) -> EvalResult {
    let buf = edit_buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::from_fixnum(buf.point_max_position().into()))
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_goto_char<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("goto-char", &args, 1)?;
    builtin_goto_char_1(host, args[0])
}

#[doc(hidden)]
pub fn builtin_goto_char_1<H: EditHost>(host: &mut H, arg: Value) -> EvalResult {
    // GNU `Fgoto_char` (editfns.c:217-222) dispatches on MARKERP/FIXNUMP
    // directly and refuses everything else, bignums included:
    //   if (MARKERP (position)) ... else if (FIXNUMP (position)) ...
    //   else wrong_type_argument (Qinteger_or_marker_p, position);
    // It does NOT go through `fix_position` (buffer.c:147-156), which is the
    // clamping coercion `char-after`/`position-bytes` use and which folds a
    // bignum to MOST_POSITIVE/NEGATIVE_FIXNUM.  Binding goto-char to that
    // clamping extractor made `(goto-char (1+ most-positive-fixnum))` succeed
    // where GNU signals wrong-type-argument integer-or-marker-p.
    if !arg.is_fixnum() && !arg.is_marker() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), arg],
        ));
    }
    // GNU returns POSITION itself (a marker stays a marker), so the raw
    // arg survives extraction; the typed position carries the coordinate.
    let pos = {
        let (edit, _) = host.edit_and_vars();
        expect_integer_or_marker_in_buffers(&edit.buffers, &arg).map(LispCharPos1::new)?
    };
    let (current_id, old_byte, byte_pos) = {
        let (edit, _) = host.edit_and_vars();
        let current_id = edit
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let buf = edit
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            current_id,
            buf.point_emacs_byte_pos(),
            buf.lisp_pos_to_accessible_emacs_byte_pos(pos),
        )
    };
    // Adjust for intangible text property
    let direction = if byte_pos >= old_byte { 1 } else { -1 };
    let adjusted = {
        let (edit, vars) = host.edit_and_vars();
        super::navigation::adjust_for_intangible(vars, &edit.buffers, byte_pos, direction)
    };
    {
        let (edit, _) = host.edit_and_vars();
        let _ = edit
            .buffers
            .goto_buffer_emacs_byte_pos(current_id, adjusted);
    }
    // Run point motion hooks
    super::navigation::check_point_motion_hooks(host, old_byte, adjusted)?;
    Ok(arg)
}

#[doc(hidden)]

pub struct InsertPiece {
    pub(crate) text: neovm_value::heap_types::LispString,
    text_props: Option<crate::buffer::text_props::TextPropertyTable>,
}

impl InsertPiece {
    /// Reassemble converted text and properties for the typed replacement
    /// pipeline. Lisp string properties normally live beside a tagged Value;
    /// `ReplaceTextPlan` deliberately owns both in one `LispString`.
    #[doc(hidden)]
    pub fn into_replacement_text(mut self) -> neovm_value::heap_types::LispString {
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

#[doc(hidden)]
pub fn current_buffer_multibyte(buffers: &BufferManager) -> Result<bool, Flow> {
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buffers
        .get(current_id)
        .map(|buf| buf.get_multibyte())
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))
}

#[doc(hidden)]
pub fn lisp_string_byte_to_char(
    string: &neovm_value::heap_types::LispString,
    byte_pos: usize,
) -> usize {
    let clamped = byte_pos.min(string.sbytes());
    if string.is_multibyte() {
        neovm_value::emacs_char::byte_to_char_pos(string.as_bytes(), clamped)
    } else {
        clamped
    }
}

fn lisp_string_char_to_byte(
    string: &neovm_value::heap_types::LispString,
    char_pos: usize,
) -> usize {
    let clamped = char_pos.min(string.schars());
    if string.is_multibyte() {
        neovm_value::emacs_char::char_to_byte_pos(string.as_bytes(), clamped)
    } else {
        clamped
    }
}

fn lisp_string_advance_byte_to_boundary(
    string: &neovm_value::heap_types::LispString,
    byte_pos: usize,
) -> usize {
    let clamped = byte_pos.min(string.sbytes());
    if !string.is_multibyte() {
        return clamped;
    }

    // These are valid Emacs internal multibyte bytes, including C0/C1 raw
    // bytes and five-byte characters. GNU buffer.c:1057-1064 advances only
    // across the current character's continuation bytes, never a prefix scan.
    let bytes = string.as_bytes();
    let mut pos = clamped;
    while pos < bytes.len() && !neovm_value::emacs_char::char_head_p(bytes[pos]) {
        pos += 1;
    }
    pos
}

#[doc(hidden)]
pub fn remap_text_property_table(
    table: &crate::buffer::text_props::TextPropertyTable,
    char_map: impl Fn(usize) -> usize,
) -> crate::buffer::text_props::TextPropertyTable {
    let intervals = table
        .intervals_snapshot()
        .into_iter()
        .filter_map(|interval| {
            let start = char_map(interval.start);
            let end = char_map(interval.end);
            let key_order = interval.key_order().clone();
            (start < end).then(|| {
                crate::buffer::text_props::PropertyInterval::from_parts(
                    CharRange::new(CharPos0::new(start), CharPos0::new(end)),
                    interval.properties,
                    key_order,
                )
            })
        })
        .collect();
    crate::buffer::text_props::TextPropertyTable::from_dump(intervals)
}

fn buffer_insert_char_codes(
    string: &neovm_value::heap_types::LispString,
    target_multibyte: bool,
) -> Vec<u32> {
    let mut codes = neovm_value::strings_core::lisp_string_char_codes(string);
    if target_multibyte {
        if !string.is_multibyte() {
            for code in &mut codes {
                if *code > 0x7F {
                    *code = neovm_value::emacs_char::unibyte_to_char(*code as u8);
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
    if code > neovm_value::emacs_char::MAX_CHAR {
        return None;
    }
    if multibyte {
        let mut buf = [0u8; neovm_value::emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = neovm_value::emacs_char::char_string(code, &mut buf);
        Some(buf[..len].to_vec())
    } else {
        Some(vec![(code & 0xFF) as u8])
    }
}

#[doc(hidden)]
pub fn buffer_insert_lisp_string_from_lisp_string(
    string: &neovm_value::heap_types::LispString,
    target_multibyte: bool,
) -> neovm_value::heap_types::LispString {
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
            return neovm_value::heap_types::LispString::from_emacs_bytes(owned);
        }
        // Real conversion: raw 128-255 bytes become their (byte8) chars.
        // Encode through one reused stack buffer, not a Vec per char.
        let codes = buffer_insert_char_codes(string, true);
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len() * 2);
        let mut buf = [0u8; neovm_value::emacs_char::MAX_MULTIBYTE_LENGTH];
        for code in codes {
            assert!(
                code <= neovm_value::emacs_char::MAX_CHAR,
                "valid Emacs character code must encode into buffer bytes"
            );
            let len = neovm_value::emacs_char::char_string(code, &mut buf);
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

#[doc(hidden)]
pub fn buffer_insert_piece_from_string(
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

#[doc(hidden)]
pub fn lisp_string_from_buffer_bytes(
    bytes: Vec<u8>,
    multibyte: bool,
) -> neovm_value::heap_types::LispString {
    if multibyte {
        neovm_value::heap_types::LispString::from_emacs_bytes(bytes)
    } else {
        neovm_value::heap_types::LispString::from_unibyte(bytes)
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

#[doc(hidden)]
pub fn buffer_slice_value_range(buf: &crate::buffer::Buffer, byte_range: EmacsByteRange) -> Value {
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
#[doc(hidden)]
pub enum BufferMultibyteConversionMode {
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

#[doc(hidden)]
pub fn convert_buffer_string_for_multibyte(
    source: Value,
    flag: Value,
) -> Result<(Value, BufferMultibyteConversionMode), Flow> {
    // GNU `Fset_buffer_multibyte` (`buffer.c:2881-2884`) keeps a valid
    // multibyte sequence only when FLAG is exactly Qt, and even then
    // `multibyte_length` is called with `allow_8bit` false. Every other
    // non-nil value — the symbol `to`, any other symbol, a number, a
    // string — expands each high byte the way `string-to-multibyte` does.
    // Whether the buffer's multibyteness changes at all is decided by the
    // caller, matching `NILP (flag) == NILP (enable-multibyte-characters)`.
    let (converted, mode) = if flag.is_nil() {
        (
            neovm_value::strings_core::builtin_string_as_unibyte(vec![source])?,
            BufferMultibyteConversionMode::AsUnibyte,
        )
    } else if flag.is_t() {
        (
            neovm_value::strings_core::builtin_string_as_multibyte(vec![source])?,
            BufferMultibyteConversionMode::AsMultibyte,
        )
    } else {
        (
            neovm_value::strings_core::builtin_string_to_multibyte(vec![source])?,
            BufferMultibyteConversionMode::ToMultibyte,
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
#[doc(hidden)]
pub fn build_multibyte_conversion_byte_map(
    old_bytes: &[u8],
    mode: BufferMultibyteConversionMode,
) -> Vec<usize> {
    use neovm_value::emacs_char::{bytes_by_char_head, char_byte8_head_p, str_as_multibyte_span};
    let mut map = Vec::with_capacity(old_bytes.len() + 1);
    let mut new_pos = 0usize;
    let mut p = 0usize;
    match mode {
        // string-as-multibyte (`character.c:543`): a valid non-eight-bit
        // sequence is kept byte-for-byte. `allow_8bit` is false, so a
        // `C0`/`C1` raw-byte pair is two eight-bit characters, not one.
        // The same span drives the converted text (`str_as_multibyte`);
        // point, markers, overlays and text properties must follow it.
        BufferMultibyteConversionMode::AsMultibyte => {
            while p < old_bytes.len() {
                let (input_bytes, output_bytes) = str_as_multibyte_span(&old_bytes[p..]);
                debug_assert!(input_bytes > 0);
                if output_bytes == input_bytes {
                    for i in 0..input_bytes {
                        map.push(new_pos + i);
                    }
                    new_pos += input_bytes;
                } else {
                    map.push(new_pos);
                    new_pos += output_bytes;
                }
                p += input_bytes;
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
#[doc(hidden)]
pub fn remap_old_byte_to_new_boundary(
    byte_map: &[usize],
    old_byte_len: usize,
    new_storage: &neovm_value::heap_types::LispString,
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

pub(crate) fn apply_inherited_text_properties<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &mut BufferManager,
    current_id: BufferId,
    old_pt: usize,
    text_len: usize,
    mode: SymbolMode,
) {
    if text_len == 0 {
        return;
    }

    let props = buffers
        .get(current_id)
        .map(|buf| {
            super::textprop::inherited_text_properties_for_inserted_range_in_state(
                vars, dynamic, buffers, buf, old_pt, text_len, mode,
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
fn general_insert_function<H: EditHost>(
    host: &mut H,
    args: &[Value],
    marker_placement: InsertPieceMarkerPlacement,
    property_mode: InsertPiecePropertyMode,
) -> EvalResult {
    for arg in args {
        let target_multibyte = {
            let (edit, _) = host.edit_and_vars();
            current_buffer_multibyte(&edit.buffers)?
        };
        let Some(pending) = PendingInsert::classify(*arg, target_multibyte)? else {
            continue;
        };
        insert_one_pending(
            host,
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
fn insert_one_pending<H: EditHost>(
    host: &mut H,
    pending: PendingInsert,
    target_multibyte: bool,
    marker_placement: InsertPieceMarkerPlacement,
    property_mode: InsertPiecePropertyMode,
) -> EvalResult {
    let (current_id, insert_pos) = {
        let (edit, _) = host.edit_and_vars();
        let current_id = edit
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let insert_pos = edit
            .buffers
            .get(current_id)
            .map(Buffer::point_emacs_byte_pos)
            .unwrap_or(EmacsBytePos::ZERO);
        (current_id, insert_pos)
    };

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
    let piece_root_scope = host.save_specpdl_roots();
    let source_root = pending.source_root();
    if source_root.is_heap_object() {
        host.push_specpdl_root(source_root);
    }
    super::editfns::signal_before_insertion_at_emacs_byte_pos(host, insert_pos)?;

    // Past the safepoint: now read the bytes and the intervals.
    let pieces = vec![pending.materialize(target_multibyte)?];
    let insert_extent = insert_pieces_extent(&pieces);
    if insert_extent.is_empty() {
        // GNU cannot reach this -- `Faset` cannot empty a string -- but a
        // conversion that produced no bytes must not be handed to the after
        // signal as a zero-width insertion that already ran a before signal.
        host.restore_specpdl_roots(piece_root_scope);
        return Ok(Value::NIL);
    }
    let change = {
        let (edit, _) = host.edit_and_vars();
        current_empty_text_change_at_emacs_byte_pos(
            &edit.buffers,
            current_id,
            insert_pos,
            insert_extent,
        )?
    };
    let mode = host.symbol_mode();
    // Edit phase: `mode` (Copy, captured above as before) and the moved
    // `pieces` cross the boundary; the rooted source stays rooted by the
    // specpdl mark restored below.
    {
        let (edit, vars) = host.edit_and_vars();
        insert_pieces_in_state(
            vars,
            &[],
            &mut edit.buffers,
            pieces,
            marker_placement,
            property_mode,
            mode,
        )?;
    }
    super::editfns::signal_after_text_change(host, change)?;
    host.restore_specpdl_roots(piece_root_scope);
    Ok(Value::NIL)
}

#[doc(hidden)]
pub fn builtin_insert<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    general_insert_function(
        host,
        &args,
        InsertPieceMarkerPlacement::AfterMarkers,
        InsertPiecePropertyMode::SourceOnly,
    )
}

#[doc(hidden)]
pub fn builtin_insert_before_markers<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    general_insert_function(
        host,
        &args,
        InsertPieceMarkerPlacement::BeforeMarkers,
        InsertPiecePropertyMode::SourceOnly,
    )
}

#[doc(hidden)]
pub fn builtin_insert_and_inherit<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    general_insert_function(
        host,
        &args,
        InsertPieceMarkerPlacement::AfterMarkers,
        InsertPiecePropertyMode::InheritAdjoining,
    )
}

#[doc(hidden)]
pub fn builtin_insert_before_markers_and_inherit<H: EditHost>(
    host: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    general_insert_function(
        host,
        &args,
        InsertPieceMarkerPlacement::BeforeMarkers,
        InsertPiecePropertyMode::InheritAdjoining,
    )
}

fn insert_pieces_in_state<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &mut BufferManager,
    pieces: Vec<InsertPiece>,
    marker_placement: InsertPieceMarkerPlacement,
    property_mode: InsertPiecePropertyMode,
    mode: SymbolMode,
) -> EvalResult {
    if pieces.iter().all(|piece| piece.text.is_empty()) {
        return Ok(Value::NIL);
    }

    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if buffers
        .get(current_id)
        .is_some_and(|buf| super::editfns::buffer_read_only_active_in_state(vars, dynamic, buf))
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
                vars,
                dynamic,
                buffers,
                current_id,
                insert_pos.get(),
                piece.text.sbytes(),
                mode,
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

pub(crate) fn insert_string_value_in_current_buffer<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &mut BufferManager,
    value: Value,
    marker_placement: InsertPieceMarkerPlacement,
    property_mode: InsertPiecePropertyMode,
    mode: SymbolMode,
) -> EvalResult {
    let target_multibyte = current_buffer_multibyte(buffers)?;
    let piece = buffer_insert_piece_from_string(value, target_multibyte)?;
    insert_pieces_in_state(
        vars,
        dynamic,
        buffers,
        vec![piece],
        marker_placement,
        property_mode,
        mode,
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

#[doc(hidden)]
pub fn builtin_insert_char<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
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

    let multibyte = {
        let (edit, _) = host.edit_and_vars();
        current_buffer_multibyte(&edit.buffers)?
    };
    let unit = if let Some(bytes) = encode_char_code_for_buffer_bytes(char_code as u32, multibyte) {
        bytes
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), args[0]],
        ));
    };
    let existing = {
        let (edit, _) = host.edit_and_vars();
        edit.buffers
            .current_buffer()
            .map_or(0, |buffer| buffer.total_emacs_byte_len().get())
    };
    let length = neovm_value::alloc::BufferByteLen::repeated(
        unit.len(),
        neovm_value::alloc::RepeatCount::try_from(count).map_err(
            |neovm_value::alloc::RepeatCountError::OutOfRange| {
                neovm_value::alloc::buffer_overflow()
            },
        )?,
        existing,
    )?;
    let mut bytes = length.reserved_bytes()?;
    for _ in 0..count {
        bytes.extend_from_slice(&unit);
    }
    let to_insert = lisp_string_from_buffer_bytes(bytes, multibyte);
    let (current_id, read_only) = {
        let (edit, vars) = host.edit_and_vars();
        let current_id = edit
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let read_only = edit
            .buffers
            .get(current_id)
            .is_some_and(|buf| super::editfns::buffer_read_only_active_in_state(vars, &[], buf));
        (current_id, read_only)
    };
    if read_only {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(current_id)],
        ));
    }

    let (insert_pos, change) = {
        let (edit, _) = host.edit_and_vars();
        let insert_pos = edit
            .buffers
            .get(current_id)
            .map(Buffer::point_emacs_byte_pos)
            .unwrap_or(EmacsBytePos::ZERO);
        let text_extent = super::editfns::lisp_string_text_extent(&to_insert);
        let change = current_empty_text_change_at_emacs_byte_pos(
            &edit.buffers,
            current_id,
            insert_pos,
            text_extent,
        )?;
        (insert_pos, change)
    };
    let text_extent = super::editfns::lisp_string_text_extent(&to_insert);
    let text_len = text_extent.emacs_bytes();
    super::editfns::signal_before_text_change(host, change)?;
    {
        let (edit, _) = host.edit_and_vars();
        let _ = edit
            .buffers
            .insert_lisp_string_into_buffer(current_id, &to_insert);
    }
    if args.get(2).is_some_and(|value| value.is_truthy()) {
        let mode = host.symbol_mode();
        // Edit phase: only Copy values captured above cross it.
        let (edit, vars) = host.edit_and_vars();
        apply_inherited_text_properties(
            vars,
            &[],
            &mut edit.buffers,
            current_id,
            insert_pos.get(),
            text_len.get(),
            mode,
        );
        let _ = edit.buffers.merge_adjacent_equal_buffer_text_properties(
            current_id,
            EmacsByteRange::from_start_len(insert_pos, text_len),
        );
    }
    super::editfns::signal_after_text_change(host, change)?;
    Ok(Value::NIL)
}

#[doc(hidden)]
pub fn builtin_insert_byte<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
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

    let (current_id, multibyte, read_only) = {
        let (edit, vars) = host.edit_and_vars();
        let current_id = edit
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let multibyte = edit
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
            .get_multibyte();
        let read_only = edit
            .buffers
            .get(current_id)
            .is_some_and(|buf| super::editfns::buffer_read_only_active_in_state(vars, &[], buf));
        (current_id, multibyte, read_only)
    };
    if read_only {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(current_id)],
        ));
    }

    let unit = encode_char_code_for_buffer_bytes(
        if multibyte {
            neovm_value::emacs_char::unibyte_to_char(byte as u8)
        } else {
            byte as u32
        },
        multibyte,
    )
    .expect("insert-byte must produce a valid buffer encoding");
    let existing = {
        let (edit, _) = host.edit_and_vars();
        edit.buffers
            .current_buffer()
            .map_or(0, |buffer| buffer.total_emacs_byte_len().get())
    };
    let length = neovm_value::alloc::BufferByteLen::repeated(
        unit.len(),
        neovm_value::alloc::RepeatCount::try_from(count).map_err(
            |neovm_value::alloc::RepeatCountError::OutOfRange| {
                neovm_value::alloc::buffer_overflow()
            },
        )?,
        existing,
    )?;
    let mut bytes = length.reserved_bytes()?;
    for _ in 0..count {
        bytes.extend_from_slice(&unit);
    }
    let to_insert = lisp_string_from_buffer_bytes(bytes, multibyte);
    let (insert_pos, change) = {
        let (edit, _) = host.edit_and_vars();
        let insert_pos = edit
            .buffers
            .get(current_id)
            .map(Buffer::point_emacs_byte_pos)
            .unwrap_or(EmacsBytePos::ZERO);
        let text_extent = super::editfns::lisp_string_text_extent(&to_insert);
        let change = current_empty_text_change_at_emacs_byte_pos(
            &edit.buffers,
            current_id,
            insert_pos,
            text_extent,
        )?;
        (insert_pos, change)
    };
    let text_extent = super::editfns::lisp_string_text_extent(&to_insert);
    let text_len = text_extent.emacs_bytes();
    super::editfns::signal_before_text_change(host, change)?;
    {
        let (edit, _) = host.edit_and_vars();
        let _ = edit
            .buffers
            .insert_lisp_string_into_buffer(current_id, &to_insert);
    }
    if args.get(2).is_some_and(|value| value.is_truthy()) {
        let mode = host.symbol_mode();
        // Edit phase: only Copy values captured above cross it.
        let (edit, vars) = host.edit_and_vars();
        apply_inherited_text_properties(
            vars,
            &[],
            &mut edit.buffers,
            current_id,
            insert_pos.get(),
            text_len.get(),
            mode,
        );
        let _ = edit.buffers.merge_adjacent_equal_buffer_text_properties(
            current_id,
            EmacsByteRange::from_start_len(insert_pos, text_len),
        );
    }
    super::editfns::signal_after_text_change(host, change)?;
    Ok(Value::NIL)
}

/// Validated substitution characters in the buffer's encoding domain.
/// Values are immutable and local to the active mutator's edit call.
#[derive(Clone, Copy, Debug)]
enum SubstitutionCharacters {
    Unibyte { from: u8, to: u8 },
    Multibyte { from: EmacsChar, to: EmacsChar },
}

static_assertions::assert_impl_all!(SubstitutionCharacters: Send, Sync);

impl SubstitutionCharacters {
    fn for_buffer(buf: &Buffer, from: EmacsChar, to: EmacsChar) -> Self {
        if buf.get_multibyte() {
            Self::Multibyte { from, to }
        } else {
            Self::Unibyte {
                from: from.code() as u8,
                to: to.code() as u8,
            }
        }
    }

    fn codes(self) -> (EmacsChar, EmacsChar) {
        match self {
            Self::Unibyte { from, to } => (
                EmacsChar::from_code_unchecked(u32::from(from)),
                EmacsChar::from_code_unchecked(u32::from(to)),
            ),
            Self::Multibyte { from, to } => (from, to),
        }
    }
}

#[doc(hidden)]
pub fn builtin_subst_char_in_region<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args_range("subst-char-in-region", &args, 4, 5)?;

    // One read phase carries the argument coercions, the current-buffer
    // lookup, and the byte encodings out as owned values; every one of them
    // is needed again after the host calls below, and none borrows the
    // manager.
    let (start, end, noundo, current_id, from_code, to_code, from_bytes, to_bytes) = {
        let (edit, _vars) = host.edit_and_vars();
        let buffers = &edit.buffers;
        let start = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
        let end = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
        // The Lisp boundary has checked both codes against EmacsChar::MAX.
        let from_code = EmacsChar::from_code_unchecked(expect_character_code(&args[2])? as u32);
        let to_code = EmacsChar::from_code_unchecked(expect_character_code(&args[3])? as u32);
        let noundo = args.get(4).is_some_and(|value| !value.is_nil());

        let current_id = buffers.current_buffer_id().ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("No current buffer")],
            )
        })?;
        let buffer = buffers.get(current_id).ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("No current buffer")],
            )
        })?;
        let target_multibyte = buffer.get_multibyte();
        // GNU editfns.c:2340-2345 assigns both character codes to unsigned
        // bytes in a unibyte buffer. Carry normalized codes through both scans
        // and the storage edit; encoding only TOCHAR leaves FROMCHAR unmatched.
        let (from_code, to_code) =
            SubstitutionCharacters::for_buffer(buffer, from_code, to_code).codes();
        let from_bytes = encode_char_code_for_buffer_bytes(from_code.code(), target_multibyte)
            .ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("characterp"), args[2]],
                )
            })?;
        let to_bytes = encode_char_code_for_buffer_bytes(to_code.code(), target_multibyte)
            .ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("characterp"), args[3]],
                )
            })?;
        (
            start, end, noundo, current_id, from_code, to_code, from_bytes, to_bytes,
        )
    };

    // GNU editfns.c:3051+ uses CHAR_BYTES (Emacs internal encoding length)
    // for this check, not storage-form length. The two agree for standard
    // Unicode but diverge for raw bytes (C0/C1 overlong vs PUA sentinel)
    // and nonunicode codepoints.
    if from_bytes.len() != to_bytes.len() {
        return Err(
            host.signal_c_error(neovm_value::errors::CErrorMessage::from(
                "Characters in `subst-char-in-region' have different byte-lengths",
            )),
        );
    }

    let scan = {
        let (edit, _vars) = host.edit_and_vars();
        subst_char_in_region_scan(
            &mut edit.buffers,
            current_id,
            start,
            end,
            from_code,
            to_code,
            &to_bytes,
            &args,
        )?
    };
    let Some((range, changed_range)) = scan else {
        return Ok(Value::NIL);
    };

    let read_only = {
        let (edit, vars) = host.edit_and_vars();
        edit.buffers
            .get(current_id)
            .is_some_and(|buf| super::editfns::buffer_read_only_active_in_state(vars, &[], buf))
    };
    if read_only {
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
    super::editfns::signal_before_text_change(&mut *host, change)?;

    // GNU `subst-char-in-region` restarts after `modify_text` because
    // `before-change-functions` may move the gap or alter the buffer.  It does
    // not run before-change hooks a second time; it simply rescans the same
    // Lisp range in the current buffer and either performs the substitutions
    // found there or returns without an after-change signal.
    let rescan = {
        let (edit, _vars) = host.edit_and_vars();
        subst_char_in_region_scan(
            &mut edit.buffers,
            current_id,
            start,
            end,
            from_code,
            to_code,
            &to_bytes,
            &args,
        )?
    };
    let Some((range, changed_range)) = rescan else {
        return Ok(Value::NIL);
    };
    let changed_range_through_end =
        TextEditRange::from_start_end(changed_range.start_anchor(), range.end_anchor());
    let after_change =
        TextChange::unchanged_extent_with_after_range(changed_range_through_end, changed_range);
    let changed = {
        let (edit, _vars) = host.edit_and_vars();
        edit.buffers.subst_char_in_buffer_region(
            current_id,
            range,
            changed_range_through_end,
            from_code.code(),
            &to_bytes,
            noundo,
        )
    };
    if changed == Some(true) {
        super::editfns::signal_after_text_change(&mut *host, after_change)?;
    }
    Ok(Value::NIL)
}

#[allow(clippy::too_many_arguments)] // keeps the scan's validated region and replacement state explicit
fn subst_char_in_region_scan(
    edit_buffers: &mut crate::buffer::BufferManager,
    current_id: BufferId,
    start: i64,
    end: i64,
    from_code: EmacsChar,
    to_code: EmacsChar,
    to_bytes: &[u8],
    args: &[Value],
) -> Result<Option<(TextEditRange, TextEditRange)>, Flow> {
    let buf = edit_buffers.get(current_id).ok_or_else(|| {
        signal(
            LispCondition::Error,
            vec![Value::string("No current buffer")],
        )
    })?;
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
        .subst_char_changed_range(range, from_code.code(), to_bytes)
        .map(|changed_range| (range, changed_range)))
}

#[doc(hidden)]
pub fn builtin_buffer_enable_undo(
    edit_buffers: &mut crate::buffer::BufferManager,
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
        edit_buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
            .id
    } else {
        match args[0].kind() {
            ValueKind::Veclike(VecLikeType::Buffer) => {
                let bid = args[0].as_buffer_id().unwrap();
                if edit_buffers.get(bid).is_none() {
                    return Ok(Value::NIL);
                }
                bid
            }
            ValueKind::String => {
                let name = expect_buffer_name_string(&args[0])?;
                edit_buffers.find_buffer_by_name(&name).ok_or_else(|| {
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
    edit_buffers
        .enable_buffer_undo(id)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    Ok(Value::NIL)
}

// No `buffer-disable-undo' here.  GNU DEFUNs `buffer-enable-undo'
// (src/buffer.c:1829, above) but NOT its partner: `buffer-disable-undo' is
// (defun buffer-disable-undo (&optional buffer) ...) at lisp/simple.el:3591,
// three lines of `with-current-buffer' + `setq' over `get-buffer'.
// DIVERGENCES.md 150.

#[doc(hidden)]
pub fn builtin_buffer_size(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("buffer-size", &args, 1)?;
    if args.is_empty() || args[0].is_nil() {
        let buf = edit_buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        return Ok(Value::fixnum(buf.total_char_len().get() as i64));
    }

    let id = expect_buffer_id(&args[0])?;
    if let Some(buf) = edit_buffers.get(id) {
        Ok(Value::fixnum(buf.total_char_len().get() as i64))
    } else {
        Ok(Value::fixnum(0))
    }
}

#[doc(hidden)]
pub fn builtin_narrow_to_region<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    neovm_value::error::expect_args("narrow-to-region", &args, 2)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_narrow_to_region_2(host, arg(0), arg(1))
}
/// `narrow-to-region` as registered: fixed arity 2, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a2` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_narrow_to_region_2<H: EditHost>(
    host: &mut H,
    start: Value,
    end: Value,
) -> EvalResult {
    let args: [Value; 2] = [start, end];
    expect_args("narrow-to-region", &args, 2)?;
    let bounds_changed = {
        let (edit, _) = host.edit_and_vars();
        let start = expect_integer_or_marker_in_buffers(&edit.buffers, &args[0])?;
        let end = expect_integer_or_marker_in_buffers(&edit.buffers, &args[1])?;
        let current_id = edit
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let byte_range = normalize_narrow_region_in_buffers(
            &edit.buffers,
            current_id,
            LispCharPos1::new(start),
            LispCharPos1::new(end),
            args[0],
            args[1],
        )?;
        let before = accessible_bounds(&mut edit.buffers, current_id);
        let _ = edit
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
        accessible_bounds(&mut edit.buffers, current_id) != before
    };
    if bounds_changed {
        host.mark_chrome_dirty_all();
    }
    Ok(Value::NIL)
}

/// The buffer's accessible bounds, for GNU's `clip_changed` bounds-changed
/// test. A missing buffer reports an impossible range so a disappearing buffer
/// never reads as "unchanged".
fn accessible_bounds(
    edit_buffers: &mut crate::buffer::BufferManager,
    buffer: crate::buffer::BufferId,
) -> Option<(usize, usize)> {
    edit_buffers.get(buffer).map(|buf| {
        let range = buf.accessible_emacs_byte_region().range();
        (range.start().get(), range.end().get())
    })
}

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_widen<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("widen", &args, 0)?;
    builtin_widen_0(host)
}
/// `widen` as registered: fixed arity 0, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a0` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_widen_0<H: EditHost>(host: &mut H) -> EvalResult {
    let args: [Value; 0] = [];
    expect_args("widen", &args, 0)?;
    let bounds_changed = {
        let (edit, _) = host.edit_and_vars();
        let current_id = edit
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let before = accessible_bounds(&mut edit.buffers, current_id);
        let _ = edit.buffers.widen_buffer(current_id);
        // The other half of GNU's clip_changed, and the same bounds-changed guard
        // (GNU: `if (BEGV != s || ZV != e) current_buffer->clip_changed = 1;`,
        // editfns.c:2990). This is the call that made the guard load-bearing —
        // font-lock widens constantly on an already-widened buffer.
        accessible_bounds(&mut edit.buffers, current_id) != before
    };
    if bounds_changed {
        host.mark_chrome_dirty_all();
    }
    Ok(Value::NIL)
}

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_buffer_modified_p(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    neovm_value::error::expect_args_range("buffer-modified-p", &args, 0, 1)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_buffer_modified_p_1(edit_buffers, arg(0))
}
/// `buffer-modified-p` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_buffer_modified_p_1(
    edit_buffers: &mut crate::buffer::BufferManager,
    buffer: Value,
) -> EvalResult {
    let args: [Value; 1] = [buffer];
    expect_max_args("buffer-modified-p", &args, 1)?;
    if args.is_empty() || args[0].is_nil() {
        let buf = edit_buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        return Ok(buf.modified_state_value());
    }

    let id = expect_buffer_id(&args[0])?;
    if let Some(buf) = edit_buffers.get(id) {
        Ok(buf.modified_state_value())
    } else {
        Ok(Value::NIL)
    }
}

#[doc(hidden)]
pub fn builtin_set_buffer_modified_p<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("set-buffer-modified-p", &args, 1)?;
    let (current_id, was_modified, lock_target) = {
        let (edit, _) = host.edit_and_vars();
        let current_id = edit
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let was_modified = edit
            .buffers
            .modified_state_root_id(current_id)
            .and_then(|root_id| edit.buffers.get(root_id))
            .is_some_and(|buffer| buffer.modified_state_value().is_truthy());
        let lock_target = super::file_name::buffer_file_lock_target(&edit.buffers, current_id);
        (current_id, was_modified, lock_target)
    };
    if let Some(filename) = lock_target {
        // No expansion here: GNU's restore_buffer_modified_p hands
        // BVAR (b, file_truename) to Flock_file / Funlock_file untouched.
        if !was_modified && !args[0].is_nil() {
            host.lock_file(&filename)?;
        } else if was_modified && args[0].is_nil() {
            host.unlock_file(&filename)?;
        }
    }
    {
        let (edit, _) = host.edit_and_vars();
        let _ = edit
            .buffers
            .restore_buffer_modified_state(current_id, args[0]);
    }
    host.force_mode_line_update()
}

#[doc(hidden)]
pub fn builtin_restore_buffer_modified_p<H: EditHost>(
    host: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("restore-buffer-modified-p", &args, 1)?;
    let (current_id, was_modified, lock_target) = {
        let (edit, _) = host.edit_and_vars();
        let current_id = edit
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let was_modified = edit
            .buffers
            .modified_state_root_id(current_id)
            .and_then(|root_id| edit.buffers.get(root_id))
            .is_some_and(|buffer| buffer.modified_state_value().is_truthy());
        let lock_target = super::file_name::buffer_file_lock_target(&edit.buffers, current_id);
        (current_id, was_modified, lock_target)
    };
    if let Some(filename) = lock_target {
        // No expansion here: GNU's restore_buffer_modified_p hands
        // BVAR (b, file_truename) to Flock_file / Funlock_file untouched.
        if !was_modified && !args[0].is_nil() {
            host.lock_file(&filename)?;
        } else if was_modified && args[0].is_nil() {
            host.unlock_file(&filename)?;
        }
    }
    let (edit, _) = host.edit_and_vars();
    edit.buffers
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

#[doc(hidden)]
pub fn builtin_buffer_modified_tick(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    let target =
        optional_buffer_tick_target_in_manager(edit_buffers, "buffer-modified-tick", &args)?;
    if let Some(id) = target
        && let Some(buf) = edit_buffers.get(id)
    {
        return Ok(Value::fixnum(buf.modified_tick()));
    }
    Ok(Value::fixnum(1))
}

#[doc(hidden)]
pub fn builtin_buffer_chars_modified_tick(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    let target =
        optional_buffer_tick_target_in_manager(edit_buffers, "buffer-chars-modified-tick", &args)?;
    if let Some(id) = target
        && let Some(buf) = edit_buffers.get(id)
    {
        return Ok(Value::fixnum(buf.chars_modified_tick()));
    }
    Ok(Value::fixnum(1))
}

#[doc(hidden)]
pub fn builtin_internal_set_buffer_modified_tick(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("internal--set-buffer-modified-tick", &args, 1, 2)?;
    let tick = expect_fixnum(&args[0])?;
    let target = if let Some(buffer) = args.get(1) {
        if buffer.is_nil() {
            edit_buffers.current_buffer_id()
        } else {
            Some(expect_buffer_id(buffer)?)
        }
    } else {
        edit_buffers.current_buffer_id()
    }
    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    edit_buffers
        .set_buffer_modified_tick(target, tick)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::NIL)
}

#[doc(hidden)]
pub fn builtin_recent_auto_save_p(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("recent-auto-save-p", &args, 0)?;
    let buf = edit_buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::bool_val(buf.recent_auto_save_p()))
}

#[doc(hidden)]
pub fn builtin_set_buffer_auto_saved(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-buffer-auto-saved", &args, 0)?;
    let current_id = edit_buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    // GNU `fileio.c:6526-6528`: autosave tick, `save_length = Z - BEG`
    // (characters, not bytes), then clear `auto_save_failure_time`. The tick
    // update is shared with `do-auto-save`, which must NOT clear the time,
    // so the clear stays here.
    let saved_chars = edit_buffers
        .get(current_id)
        .map(|buf| i64::try_from(buf.total_char_len().get()).unwrap_or(i64::MAX))
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    edit_buffers
        .set_buffer_auto_saved(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let buf = edit_buffers
        .get_mut(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.set_buffer_local("buffer-saved-size", Value::fixnum(saved_chars));
    buf.set_auto_save_failure_time(0);
    Ok(Value::NIL)
}

#[doc(hidden)]
pub fn other_buffer_designator(
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
                .map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
                .expect("ValueKind::String must carry LispString payload");
            buffers.find_buffer_by_name(&name)
        }
        _ => None,
    }
}

#[doc(hidden)]
pub fn is_hidden_buffer(
    buffers: &crate::buffer::BufferManager,
    id: crate::buffer::BufferId,
) -> bool {
    buffers
        .get(id)
        .map(|buf| buf.name_starts_with_space())
        .unwrap_or(true)
}

#[doc(hidden)]
pub fn other_buffer_impl(
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

#[doc(hidden)]
pub fn builtin_generate_new_buffer_name<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    let symbol_mode = host.symbol_mode();
    expect_min_args("generate-new-buffer-name", &args, 1)?;
    expect_max_args("generate-new-buffer-name", &args, 2)?;
    if args.len() == 2
        && !(args[1].is_nil()
            || args[1].is_t()
            || args[1].is_string()
            || args[1].is_symbol(symbol_mode)
            || args[1].as_keyword_id(symbol_mode).is_some())
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[1]],
        ));
    }
    let ignore = args.get(1).and_then(|v| v.as_utf8_str());
    let (edit, _) = host.edit_and_vars();
    generate_new_buffer_name_value_in_state(&edit.buffers, args[0], ignore)
}

/// Return GNU `generate-new-buffer-name`'s Lisp string result.
///
/// Buffer identity is indexed by the decoded name text, but the public name is
/// a Lisp string object whose identity and text properties are observable.
/// Keep those two concerns separate: the manager chooses the unique text;
/// this boundary reuses BASE when possible and appends only the generated
/// suffix when allocation is necessary, just like GNU's `concat2` path.
#[doc(hidden)]
pub fn generate_new_buffer_name_value_in_state(
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
    neovm_value::strings_core::concat_impl(&[base, Value::string(suffix)])
}

/// (bufferp OBJECT) → t or nil
#[doc(hidden)]
pub fn builtin_bufferp(args: Vec<Value>) -> EvalResult {
    expect_args("bufferp", &args, 1)?;
    Ok(Value::bool_val(args[0].is_buffer()))
}

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_char_after(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    neovm_value::error::expect_args_range("char-after", &args, 0, 1)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_char_after_1(edit_buffers, arg(0))
}
/// `char-after` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_char_after_1(
    edit_buffers: &mut crate::buffer::BufferManager,
    pos: Value,
) -> EvalResult {
    let args: [Value; 1] = [pos];
    expect_max_args("char-after", &args, 1)?;
    let buf = edit_buffers.current_buffer().ok_or_else(|| {
        signal(
            LispCondition::Error,
            vec![Value::string("No current buffer")],
        )
    })?;
    let accessible = buf.accessible_emacs_byte_region();
    let byte_pos = if args.is_empty() || args[0].is_nil() {
        let point = buf.point_emacs_byte_pos();
        accessible.contains(point).then_some(point)
    } else if pos.is_marker() {
        // GNU editfns.c:1052-1057/1085-1090 uses the marker's own byte
        // position even when its buffer differs from the current buffer.
        let location = super::marker::marker_location_or_signal(edit_buffers, pos)?;
        let byte_pos = location.byte_pos();
        if !accessible.contains(byte_pos) {
            return Ok(Value::NIL);
        }
        let code = if location.buffer() == buf.id {
            buf.char_code_after_emacs_byte_pos(byte_pos)
        } else {
            buf.char_code_after_foreign_marker_byte_pos(byte_pos)
        };
        return Ok(code.map_or(Value::NIL, |code| Value::fixnum(i64::from(code))));
    } else {
        let pos = expect_integer_or_marker_in_buffers(edit_buffers, &args[0])?;
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

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_char_before(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    neovm_value::error::expect_args_range("char-before", &args, 0, 1)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_char_before_1(edit_buffers, arg(0))
}
/// `char-before` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_char_before_1(
    edit_buffers: &mut crate::buffer::BufferManager,
    pos: Value,
) -> EvalResult {
    let args: [Value; 1] = [pos];
    expect_max_args("char-before", &args, 1)?;
    let buf = edit_buffers.current_buffer().ok_or_else(|| {
        signal(
            LispCondition::Error,
            vec![Value::string("No current buffer")],
        )
    })?;
    let accessible = buf.accessible_emacs_byte_region();
    let byte_pos = if args.is_empty() || args[0].is_nil() {
        let point = buf.point_emacs_byte_pos();
        accessible
            .contains_preceding_char_boundary(point)
            .then_some(point)
    } else if pos.is_marker() {
        // GNU editfns.c:1052-1057/1085-1090 uses the marker's own byte
        // position even when its buffer differs from the current buffer.
        let byte_pos = super::marker::marker_location_or_signal(edit_buffers, pos)?.byte_pos();
        accessible
            .contains_preceding_char_boundary(byte_pos)
            .then_some(byte_pos)
    } else {
        let pos = expect_integer_or_marker_in_buffers(edit_buffers, &args[0])?;
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

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_byte_to_position(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("byte-to-position", &args, 1)?;
    builtin_byte_to_position_1(edit_buffers, args[0])
}

#[doc(hidden)]
pub fn builtin_byte_to_position_1(
    edit_buffers: &mut crate::buffer::BufferManager,
    arg: Value,
) -> EvalResult {
    let byte_pos = expect_fixnum(&arg)?;
    if byte_pos <= 0 {
        return Ok(Value::NIL);
    }

    let buf = edit_buffers
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

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_position_bytes(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("position-bytes", &args, 1)?;
    builtin_position_bytes_1(edit_buffers, args[0])
}

#[doc(hidden)]
pub fn builtin_position_bytes_1(
    edit_buffers: &mut crate::buffer::BufferManager,
    arg: Value,
) -> EvalResult {
    let pos = expect_integer_or_marker_in_buffers(edit_buffers, &arg)?;

    let buf = edit_buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let max_char_pos = buf.z_lisp_char_pos().as_i64();
    if pos <= 0 || pos > max_char_pos {
        return Ok(Value::NIL);
    }

    let byte_pos = char_pos_to_buffer_emacs_byte_pos(buf, LispCharPos1::new(pos).to_char_pos());
    Ok(Value::fixnum(byte_pos.to_lisp_byte_pos().as_i64()))
}

#[doc(hidden)]
pub fn builtin_get_byte<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
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

        let string = host.lisp_string(args[1]).expect("string");
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
        // One character, not the whole string decoded (GNU `Fget_byte':
        // `string_char_to_byte' then `STRING_CHAR').
        let code = neovm_value::strings_core::lisp_string_char_at(string, pos)
            .expect("POSITION was range-checked against SCHARS");
        return get_byte_from_multibyte_char_code(code);
    }

    // Buffer path: POSITION is a 1-based character position. The tail makes
    // no host calls, so one phase stays open to the end of the function.
    let (edit, _vars) = host.edit_and_vars();
    let buf = edit
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let byte_pos = if args.is_empty() || args[0].is_nil() {
        buf.point_emacs_byte_pos()
    } else {
        let pos = expect_integer_or_marker_in_buffers(&edit.buffers, &args[0])?;
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
#[doc(hidden)]
pub fn builtin_next_overlay_change(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    builtin_next_overlay_change_in_buffers(edit_buffers, args)
}

#[doc(hidden)]
pub fn builtin_next_overlay_change_in_buffers(
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
#[doc(hidden)]
pub fn builtin_previous_overlay_change(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    builtin_previous_overlay_change_in_buffers(edit_buffers, args)
}

#[doc(hidden)]
pub fn builtin_previous_overlay_change_in_buffers(
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
#[doc(hidden)]
pub fn builtin_make_overlay(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    builtin_make_overlay_in_buffers(edit_buffers, args)
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
    let overlay = Value::make_overlay(neovm_value::heap_types::OverlayDataInit {
        serial: neovm_value::heap_types::OverlaySerialSource::Mint,
        plist: Value::NIL,
        buffer: Some(buf_id),
        start: byte_range.start().get(),
        end: byte_range.end().get(),
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
#[doc(hidden)]
pub fn builtin_delete_overlay<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    if !super::redisplay_policy::gnu_redisplay_hooks_enabled() {
        let (edit, _) = host.edit_and_vars();
        return builtin_delete_overlay_in_buffers(&mut edit.buffers, args);
    }
    expect_args("delete-overlay", &args, 1)?;
    let overlay = expect_overlay(&args[0])?;
    let buffer = resolve_overlay_buffer_id(overlay);
    let result = {
        let (edit, _) = host.edit_and_vars();
        builtin_delete_overlay_in_buffers(&mut edit.buffers, args)?
    };
    if let Some(buffer) = buffer {
        host.mark_buffer_redisplay(buffer);
    }
    Ok(result)
}

#[inline(always)]
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
#[doc(hidden)]
pub fn builtin_overlay_put<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    if !super::redisplay_policy::gnu_redisplay_hooks_enabled() {
        let (edit, _) = host.edit_and_vars();
        return builtin_overlay_put_in_buffers(&mut edit.buffers, args);
    }
    expect_args("overlay-put", &args, 3)?;
    let overlay = expect_overlay(&args[0])?;
    let (result, redisplay) = {
        let (edit, _) = host.edit_and_vars();
        let before = resolve_overlay_buffer_id(overlay).and_then(|buffer| {
            edit.buffers
                .get(buffer)
                .map(|value| (buffer, value.overlay_modified_tick()))
        });
        let result = builtin_overlay_put_in_buffers(&mut edit.buffers, args)?;
        let redisplay = match before {
            Some((buffer, tick))
                if edit
                    .buffers
                    .get(buffer)
                    .is_some_and(|value| value.overlay_modified_tick() != tick) =>
            {
                Some(buffer)
            }
            _ => None,
        };
        (result, redisplay)
    };
    if let Some(buffer) = redisplay {
        host.mark_buffer_redisplay(buffer);
    }
    Ok(result)
}

#[inline(always)]
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
                let (plist, changed) = neovm_value::plist::plist_put(object.plist, args[1], val)?;
                object.plist = plist;
                Ok::<bool, neovm_value::error::Flow>(changed)
            })
            .unwrap()?
    };
    if let Some(buf_id) = resolve_overlay_buffer_id(overlay)
        && changed
    {
        let _ = buffers.note_overlay_modification(buf_id);
        let evaporate = args[1].is_bare_symbol_named("evaporate") && val.is_truthy();
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
#[doc(hidden)]
pub fn builtin_overlay_get<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("overlay-get", &args, 2)?;
    let overlay = expect_overlay(&args[0])?;
    let (edit, vars) = host.edit_and_vars();
    Ok(lookup_overlay_property(
        vars,
        &edit.buffers,
        overlay,
        args[1],
    ))
}

/// (overlayp OBJ)
#[doc(hidden)]
pub fn builtin_overlayp<H: EditHost>(_host: &mut H, args: &[Value]) -> EvalResult {
    builtin_overlayp_pure(args)
}

pub(crate) fn builtin_overlayp_pure(args: &[Value]) -> EvalResult {
    expect_args("overlayp", &args, 1)?;
    if args[0].is_overlay() {
        return Ok(Value::T);
    }
    Ok(Value::NIL)
}

/// (overlays-at POS &optional SORTED)
#[doc(hidden)]
pub fn builtin_overlays_at<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_min_args("overlays-at", &args, 1)?;
    expect_max_args("overlays-at", &args, 2)?;
    let (edit, vars) = host.edit_and_vars();
    let buffers = &edit.buffers;
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
            ids.retain(|ov| {
                lookup_overlay_property(vars, buffers, *ov, window_sym)
                    .as_window_id()
                    .is_none_or(|wid| wid == target_window_id)
            });
        }
        // GNU buffer.c:3292 resolves priority with Foverlay_get, including
        // the category symbol and char-property aliases.
        let priority = Value::symbol("priority");
        buf.overlays
            .sort_overlay_ids_by_priority_desc_with(&mut ids, &|overlay| {
                Some(lookup_overlay_property(vars, buffers, overlay, priority))
            });
    }
    Ok(Value::list(ids))
}

/// (overlays-in BEG END)
#[doc(hidden)]
pub fn builtin_overlays_in(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    builtin_overlays_in_in_buffers(edit_buffers, args)
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
    // GNU `overlays_in` includes an empty overlay at END only when END is at `Z`, the end of
    // the whole buffer, so a narrowed `point-max` doesn't qualify.
    let ids = buf
        .overlays
        .overlays_in_accessible_emacs_byte_range(byte_range, buf.total_emacs_byte_end_pos());
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
#[doc(hidden)]
pub fn builtin_overlay_lists(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    builtin_overlay_lists_in_buffers(edit_buffers, args)
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
    let full = EmacsByteRange::new(EmacsBytePos::new(0), buf.total_emacs_byte_end_pos());
    let before = Value::list(buf.overlays.overlays_in_gnu_full_region(full));
    Ok(Value::cons(before, Value::NIL))
}

/// (overlay-recenter POS)
///
/// Mirrors GNU `Foverlay_recenter` (buffer.c): since Emacs 29.1 this is a
/// no-op (overlay lookup is fast at any position with the `itree` store), but
/// it still type-checks POS as a fixnum-or-marker and returns nil.
#[doc(hidden)]
pub fn builtin_overlay_recenter(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("overlay-recenter", &args, 1)?;
    let _ = expect_integer_or_marker_in_buffers(edit_buffers, &args[0])?;
    Ok(Value::NIL)
}

/// (move-overlay OVERLAY BEG END &optional BUFFER)
#[doc(hidden)]
pub fn builtin_move_overlay<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    if !super::redisplay_policy::gnu_redisplay_hooks_enabled() {
        let (edit, _) = host.edit_and_vars();
        return builtin_move_overlay_in_buffers(&mut edit.buffers, args);
    }
    expect_min_args("move-overlay", &args, 3)?;
    expect_max_args("move-overlay", &args, 4)?;
    let overlay = expect_overlay(&args[0])?;
    let (old_buffer, new_buffer, result) = {
        let (edit, _) = host.edit_and_vars();
        let old_buffer = resolve_overlay_buffer_id(overlay);
        let new_buffer = if args.get(3).is_some_and(|value| value.is_truthy()) {
            resolve_buffer_id_in_buffers(&edit.buffers, args.get(3))?
        } else {
            old_buffer
                .or_else(|| edit.buffers.current_buffer_id())
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        };
        let result = builtin_move_overlay_in_buffers(&mut edit.buffers, args)?;
        (old_buffer, new_buffer, result)
    };
    if let Some(buffer) = old_buffer.filter(|buffer| *buffer != new_buffer) {
        host.mark_buffer_redisplay(buffer);
    }
    // GNU modify_overlay runs even when endpoints are unchanged; also retain
    // the destination publication if the empty overlay evaporated there.
    host.mark_buffer_redisplay(new_buffer);
    Ok(result)
}

#[inline(always)]
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
        // Same buffer: release the owner borrow before publishing to siblings.
        {
            let buf = buffers
                .get_mut(new_buf_id)
                .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
            let byte_range = elisp_range_to_byte_clipped_full(buf, beg, end);
            buf.overlays
                .move_overlay_to_emacs_byte_range(overlay, byte_range);
        }
        let _ = buffers.note_overlay_modification(new_buf_id);
        Ok(args[0])
    } else {
        if let Some(old_buf_id) = old_buf_id {
            let detached = buffers
                .get_mut(old_buf_id)
                .is_some_and(|buf| buf.overlays.detach_overlay(overlay));
            if detached {
                let _ = buffers.note_overlay_modification(old_buf_id);
            }
        }

        let evaporated = {
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
            byte_range.is_empty()
                && new_buf
                    .overlays
                    .overlay_get_named(overlay, Value::symbol("evaporate"))
                    .is_some_and(|value| value.is_truthy())
                && new_buf.overlays.delete_overlay(overlay)
        };
        // No Lisp code runs between insertion and evaporation. Publish their
        // original separate modification events after releasing the owner.
        let _ = buffers.note_overlay_modification(new_buf_id);
        if evaporated {
            let _ = buffers.note_overlay_modification(new_buf_id);
        }
        Ok(args[0])
    }
}

/// (overlay-start OVERLAY)
#[doc(hidden)]
pub fn builtin_overlay_start(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    builtin_overlay_start_in_buffers(edit_buffers, args)
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
#[doc(hidden)]
pub fn builtin_overlay_end(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    builtin_overlay_end_in_buffers(edit_buffers, args)
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
#[doc(hidden)]
pub fn builtin_overlay_buffer(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    builtin_overlay_buffer_in_buffers(edit_buffers, args)
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
#[doc(hidden)]
pub fn builtin_overlay_properties(
    edit_buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    builtin_overlay_properties_in_buffers(edit_buffers, args)
}

pub(crate) fn builtin_overlay_properties_in_buffers(
    _buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("overlay-properties", &args, 1)?;
    let overlay = expect_overlay(&args[0])?;
    neovm_value::sequence_copy::copy_sequence_with_symbols(
        overlay.as_overlay_data().map_or(Value::NIL, |d| d.plist),
        SymbolMode::Bare,
    )
}

#[doc(hidden)]
impl ReplaceRegionChangeRun {
    #[inline]
    pub fn a_start(&self) -> usize {
        self.a_start
    }
    #[inline]
    pub fn a_end(&self) -> usize {
        self.a_end
    }
    #[inline]
    pub fn b_start(&self) -> usize {
        self.b_start
    }
    #[inline]
    pub fn b_end(&self) -> usize {
        self.b_end
    }
}

#[doc(hidden)]
impl InsertPiece {
    #[inline]
    pub fn into_text(self) -> neovm_value::heap_types::LispString {
        self.text
    }
    #[inline]
    pub fn text(&self) -> &neovm_value::heap_types::LispString {
        &self.text
    }
}

#[cfg(test)]
#[path = "tests/buffer_test.rs"]
mod tests;
