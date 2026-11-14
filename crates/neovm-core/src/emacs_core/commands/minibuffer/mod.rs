//! Minibuffer and completion system.
//!
//! Provides:
//! - `MinibufferManager` — owns all minibuffer state, history, and completion logic
//! - `CompletionTable` — what can be completed against (list, function, file names, etc.)
//! - `CompletionStyle` — matching strategy (prefix, substring, flex, basic)
//! - Builtin functions for Elisp: `read-from-minibuffer`, `completing-read`, `y-or-n-p`, etc.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args, expect_min_args};
use crate::emacs_core::eval::VariableView;
use crate::emacs_core::eval::data::minibuffer_entry::{
    CompletionStateVariable, completion_display_string_from_value, expect_lisp_string,
};
pub(crate) use crate::emacs_core::eval::data::minibuffer_entry::{
    builtin_all_completions, builtin_test_completion, builtin_try_completion,
    completion_ignore_case, completion_regexp_lisp_list, lisp_string_matches_completion_regexps,
};
use crate::emacs_core::keyboard::CommandLoopHost;

use crate::buffer::{BufferId, BufferManager, EmacsBytePos, EmacsByteRange, LispCharPos1};
use crate::heap_types::LispString;

pub(crate) mod read_entries;
mod redisplay;
mod window_state;

pub(crate) use read_entries::*;
pub(crate) use window_state::*;

use super::error::{EvalResult, Flow, signal};
use super::intern::resolve_sym;
use super::reader::KeyboardInputRuntime;
use super::textprop::StickinessProperty;
use super::value::{Value, ValueKind, VecLikeType};
use neovm_edit::emacs_core::edit_host::EditHost;

// ---------------------------------------------------------------------------
// Argument helpers (local copies, same pattern as builtins.rs / builtins_extra.rs)
// ---------------------------------------------------------------------------

fn first_default_value(default: Value) -> Value {
    match default.kind() {
        ValueKind::Cons => default.cons_car(),
        _other => default,
    }
}

fn normalize_symbol_reader_default(default: Value) -> Value {
    match first_default_value(default).kind() {
        ValueKind::Symbol(id) => Value::string(resolve_sym(id)),
        _other => first_default_value(default),
    }
}

fn normalize_buffer_reader_default(buffers: &BufferManager, default: Value) -> Value {
    let first = first_default_value(default);
    match first.kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => first
            .as_buffer_id()
            .and_then(|id| buffers.get(id))
            .map(|buffer| buffer.name_value())
            .unwrap_or(first),
        _ => first,
    }
}

fn strip_read_buffer_prompt_suffix(prompt: &[u8]) -> &[u8] {
    if let Some(stripped) = prompt.strip_suffix(b": ") {
        stripped
    } else if let Some(stripped) = prompt.strip_suffix(b":") {
        stripped
    } else if let Some(stripped) = prompt.strip_suffix(b" ") {
        stripped
    } else {
        prompt
    }
}

fn prompt_lisp_from_bytes(bytes: Vec<u8>, multibyte: bool) -> LispString {
    if multibyte {
        LispString::from_emacs_bytes(bytes)
    } else {
        LispString::from_unibyte(bytes)
    }
}

/// Substitute `%s` (with DEFAULT) and `%%` (with `%`) in the
/// `minibuffer-default-prompt-format` string, over raw Emacs bytes so eight-bit
/// content in the default survives faithfully. `%s`/`%%` are ASCII, so matching
/// on bytes is safe even when the format string is multibyte.
fn format_default_prompt(format: &LispString, default: &LispString) -> LispString {
    let bytes = format.as_bytes();
    let fmt_multibyte = format.is_multibyte();
    let mut result = prompt_lisp_from_bytes(Vec::new(), false);
    let mut literal: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b's' => {
                    if !literal.is_empty() {
                        result = result.concat(&prompt_lisp_from_bytes(
                            std::mem::take(&mut literal),
                            fmt_multibyte,
                        ));
                    }
                    result = result.concat(default);
                    i += 2;
                    continue;
                }
                b'%' => {
                    literal.push(b'%');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        literal.push(bytes[i]);
        i += 1;
    }
    if !literal.is_empty() {
        result = result.concat(&prompt_lisp_from_bytes(literal, fmt_multibyte));
    }
    result
}

fn read_buffer_prompt(vars: VariableView<'_>, raw_prompt: Value, default: Value) -> Value {
    if default.is_nil() {
        return raw_prompt;
    }
    let Some(prompt) = raw_prompt.as_lisp_string() else {
        return raw_prompt;
    };
    let prompt_base = strip_read_buffer_prompt_suffix(prompt.as_bytes());
    let mut formatted = prompt_lisp_from_bytes(prompt_base.to_vec(), prompt.is_multibyte());
    if let Some(default_text) = default.as_lisp_string()
        && !default_text.as_bytes().is_empty()
    {
        let default_prompt_format = vars
            .value_by_name("minibuffer-default-prompt-format")
            .and_then(|value| value.as_lisp_string().cloned())
            .unwrap_or_else(|| LispString::from_unibyte(b" (default %s)".to_vec()));
        formatted = formatted.concat(&format_default_prompt(&default_prompt_format, default_text));
    }
    formatted = formatted.concat(&LispString::from_unibyte(b": ".to_vec()));
    Value::heap_string(formatted)
}

// ---------------------------------------------------------------------------
// CompletionTable
// ---------------------------------------------------------------------------

pub use crate::emacs_core::eval::data::minibuffer::{
    CompletionDirectory, CompletionStyle, CompletionTable, MinibufferHistory, MinibufferManager,
    MinibufferState,
};

pub use crate::emacs_core::eval::data::minibuffer_entry::{
    CompletionResult, CompletionTableExt, MinibufferManagerExt, MinibufferStateExt,
};
pub(crate) use crate::emacs_core::eval::data::minibuffer_entry::{
    MinibufferEntryRejection, MinibufferInitialContents, RecursiveMinibufferPolicy,
};
pub(crate) use crate::emacs_core::eval::data::minibuffer_entry::{
    completion_char_codes, completion_fold_char, lisp_string_chars,
};

static_assertions::assert_eq_size!(CompletionDirectory, LispString);
static_assertions::assert_not_impl_any!(CompletionDirectory: Send, Sync);

// ---------------------------------------------------------------------------
// CompletionStyle
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// CompletionResult
// ---------------------------------------------------------------------------

pub(crate) fn install_minibuffer_buffer_contents(
    buffers: &mut BufferManager,
    buffer_id: BufferId,
    prompt: &LispString,
    initial: Option<&MinibufferInitialContents>,
    prompt_properties: Value,
) -> EmacsBytePos {
    // Match GNU `read_minibuf` / `erase-buffer`: clear the minibuffer through
    // the buffer edit pipeline so point, narrowing, and sibling state reset
    // together before we insert the new prompt and initial contents.
    let old_text_range = buffers
        .full_buffer_emacs_byte_range(buffer_id)
        .expect("minibuffer buffer");
    buffers
        .restore_buffer_emacs_byte_restriction(buffer_id, old_text_range)
        .expect("minibuffer buffer widen");
    if !old_text_range.is_empty() {
        buffers
            .delete_buffer_emacs_byte_range(buffer_id, old_text_range)
            .expect("minibuffer buffer delete");
    }
    buffers
        .goto_buffer_emacs_byte_pos(buffer_id, EmacsBytePos::new(0))
        .expect("minibuffer buffer goto");

    buffers
        .insert_lisp_string_into_buffer(buffer_id, prompt)
        .expect("minibuffer prompt insert");
    let prompt_end = buffers
        .get(buffer_id)
        .expect("minibuffer buffer")
        .full_emacs_byte_range()
        .end();
    if prompt_end.get() > 0 {
        let prompt_range = EmacsByteRange::new(EmacsBytePos::new(0), prompt_end);
        let buf = buffers.get_mut(buffer_id).expect("minibuffer buffer");
        buf.text_props_put_property_in_emacs_byte_range(
            prompt_range,
            Value::symbol("field"),
            Value::T,
        );
        buf.text_props_put_property_in_emacs_byte_range(
            prompt_range,
            StickinessProperty::FrontSticky.value(),
            Value::T,
        );
        buf.text_props_put_property_in_emacs_byte_range(
            prompt_range,
            StickinessProperty::RearNonsticky.value(),
            Value::T,
        );
        apply_minibuffer_prompt_properties(buf, prompt_end, prompt_properties);
    }

    if let Some(initial) = initial {
        buffers
            .insert_lisp_string_into_buffer(buffer_id, initial.text())
            .expect("minibuffer initial input insert");
    }

    let full_end = buffers
        .get(buffer_id)
        .expect("minibuffer buffer")
        .full_emacs_byte_range()
        .end();
    buffers
        .restore_buffer_emacs_byte_restriction(
            buffer_id,
            buffers
                .full_buffer_emacs_byte_range(buffer_id)
                .expect("minibuffer buffer"),
        )
        .expect("minibuffer buffer widen");
    let initial_point = initial
        .map(|contents| prompt_end.add_len(contents.cursor_byte_offset()))
        .unwrap_or(full_end);
    buffers
        .goto_buffer_emacs_byte_pos(buffer_id, initial_point)
        .expect("minibuffer buffer goto");
    prompt_end
}

#[cfg(test)]
pub(crate) fn install_minibuffer_buffer_text(
    buffers: &mut BufferManager,
    buffer_id: BufferId,
    prompt: &LispString,
    initial: Option<&LispString>,
    prompt_properties: Value,
) -> EmacsBytePos {
    let initial = initial.cloned().map(MinibufferInitialContents::at_end);
    install_minibuffer_buffer_contents(
        buffers,
        buffer_id,
        prompt,
        initial.as_ref(),
        prompt_properties,
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn default_minibuffer_prompt_properties() -> Value {
    Value::list(vec![Value::symbol("read-only"), Value::T])
}

fn apply_minibuffer_prompt_properties(
    buf: &mut crate::buffer::Buffer,
    prompt_end: EmacsBytePos,
    prompt_properties: Value,
) {
    let prompt_range = EmacsByteRange::new(EmacsBytePos::new(0), prompt_end);
    let mut cursor = prompt_properties;
    while cursor.is_cons() {
        let key = cursor.cons_car();
        cursor = cursor.cons_cdr();
        if !cursor.is_cons() {
            break;
        }
        let value = cursor.cons_car();
        cursor = cursor.cons_cdr();
        buf.text_props_put_property_in_emacs_byte_range(prompt_range, key, value);
    }
}

// ---------------------------------------------------------------------------
// MinibufferHistory
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// MinibufferManager
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Builtin functions for Elisp
// ---------------------------------------------------------------------------

/// `(read-buffer PROMPT &optional DEFAULT REQUIRE-MATCH PREDICATE)`
///
/// Read a buffer name from the minibuffer with completion.
/// In interactive mode, delegates to completing-read with buffer name candidates.
pub(crate) fn builtin_read_buffer(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    execute_read_buffer_plan(eval, &args, |eval, completing_args| {
        builtin_completing_read(eval, completing_args.to_vec())
    })
}

/// The two GNU `Fread_buffer` dispatch paths.
///
/// Keeping this choice explicit prevents callers such as interactive control
/// letters from bypassing `read-buffer-function` by constructing completion
/// arguments directly.
#[derive(Clone, Debug)]
pub(crate) enum ReadBufferPlan {
    Override {
        function: Value,
        arguments: Vec<Value>,
    },
    Complete {
        arguments: [Value; 7],
    },
}

fn plan_read_buffer(eval: &mut super::eval::Context, args: &[Value]) -> ReadBufferPlan {
    let default = normalize_buffer_reader_default(
        eval.buffer_manager(),
        args.get(1).copied().unwrap_or(Value::NIL),
    );
    let function = eval.visible_variable_value_or_nil("read-buffer-function");
    if !function.is_nil() {
        let require_match = args.get(2).copied().unwrap_or(Value::NIL);
        let predicate = args.get(3).copied().unwrap_or(Value::NIL);
        let mut arguments = vec![args[0], default, require_match];
        // GNU keeps backward compatibility with older three-argument reader
        // functions when no predicate was supplied.
        if !predicate.is_nil() {
            arguments.push(predicate);
        }
        ReadBufferPlan::Override {
            function,
            arguments,
        }
    } else {
        ReadBufferPlan::Complete {
            arguments: read_buffer_completing_args(eval.variables(), eval.buffer_manager(), args),
        }
    }
}

fn execute_read_buffer_plan(
    eval: &mut super::eval::Context,
    args: &[Value],
    complete: impl FnOnce(&mut super::eval::Context, &[Value]) -> EvalResult,
) -> EvalResult {
    builtin_read_buffer_in_runtime(eval, args)?;

    // GNU binds this around both the override and completion paths.
    let specpdl_count = eval.specpdl().len();
    let ignore_case = eval.visible_variable_value_or_nil("read-buffer-completion-ignore-case");
    eval.try_specbind_or_unwind_to(
        specpdl_count,
        CompletionStateVariable::CompletionIgnoreCase.symbol_id(),
        ignore_case,
    )?;

    let result = match plan_read_buffer(eval, args) {
        ReadBufferPlan::Override {
            function,
            arguments,
        } => eval.funcall_general(function, arguments),
        ReadBufferPlan::Complete { arguments } => complete(eval, &arguments),
    };
    eval.unbind_to_with_result(specpdl_count, result)
}

pub(crate) fn finish_read_buffer_in_vm_runtime(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    execute_read_buffer_plan(eval, args, |eval, completing_args| {
        finish_completing_read_in_vm_runtime(eval, completing_args)
    })
}

pub(crate) fn builtin_read_buffer_in_runtime(
    runtime: &impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<(), Flow> {
    // Validation only; batch prompt/stdin handling lives in the shared
    // `completing-read` path reached from `finish_read_buffer_*`.
    let _ = runtime;
    expect_min_args("read-buffer", args, 1)?;
    expect_max_args("read-buffer", args, 4)?;
    let _prompt = expect_lisp_string(&args[0])?;
    Ok(())
}

pub(crate) fn read_buffer_completing_args(
    vars: VariableView<'_>,
    buffers: &BufferManager,
    args: &[Value],
) -> [Value; 7] {
    let default =
        normalize_buffer_reader_default(buffers, args.get(1).copied().unwrap_or(Value::NIL));
    let prompt = read_buffer_prompt(vars, args[0], default);
    let require_match = args.get(2).copied().unwrap_or(Value::NIL);
    let predicate = args.get(3).copied().unwrap_or(Value::NIL);

    let buf_ids = buffers.buffer_list();
    let buffer_names: Vec<Value> = buf_ids
        .iter()
        .filter_map(|id| buffers.get(*id))
        .map(|b| b.name_value())
        .collect();
    let collection = Value::list(buffer_names);

    [
        prompt,
        collection,
        predicate,
        require_match,
        Value::NIL,
        Value::NIL,
        default,
    ]
}

/// `(read-command PROMPT &optional DEFAULT)`
///
/// Read a command name from the minibuffer.
/// In interactive mode, uses read-from-minibuffer and interns the result.
pub(crate) fn builtin_read_command(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_read_command_in_runtime(eval, &args)?;
    finish_read_command_in_eval(eval, &args)
}

pub(crate) fn finish_read_command_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_command_with_minibuffer(args, |minibuffer_args| {
        builtin_read_from_minibuffer(eval, minibuffer_args.to_vec())
    })
}

pub(crate) fn builtin_read_command_in_runtime(
    runtime: &impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<(), Flow> {
    // Validation only.  GNU's `Fread_command` routes through `Fcompleting_read`
    // -> `read_minibuf` -> `read_minibuf_noninteractive` in batch mode, which
    // writes the prompt to stdout before reading stdin; the actual batch
    // prompt/stdin handling happens in the shared `read-from-minibuffer` path
    // reached from `finish_read_command_*`.
    let _ = runtime;
    expect_min_args("read-command", args, 1)?;
    expect_max_args("read-command", args, 2)?;
    let _prompt = expect_lisp_string(&args[0])?;
    Ok(())
}

fn symbol_reader_minibuffer_args(args: &[Value]) -> [Value; 6] {
    let prompt = args[0];
    let default = normalize_symbol_reader_default(args.get(1).copied().unwrap_or(Value::NIL));
    [
        prompt,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        default,
    ]
}

fn intern_symbol_reader_result(result: Value) -> Value {
    if let Some(name) = result.as_lisp_string() {
        // read-symbol results are interned symbol names (ASCII / decoded text);
        // decode lossily and reuse Value::symbol's nil/t/keyword canonicalization.
        return Value::symbol(crate::emacs_core::emacs_char::to_utf8_lossy(
            name.as_bytes(),
        ));
    }
    result
}

fn finish_symbol_reader_with_minibuffer(
    args: &[Value],
    mut read_from_minibuffer: impl FnMut(&[Value]) -> EvalResult,
) -> EvalResult {
    let minibuffer_args = symbol_reader_minibuffer_args(args);
    let result = read_from_minibuffer(&minibuffer_args)?;
    Ok(intern_symbol_reader_result(result))
}

pub(crate) fn finish_read_command_with_minibuffer(
    args: &[Value],
    read_from_minibuffer: impl FnMut(&[Value]) -> EvalResult,
) -> EvalResult {
    finish_symbol_reader_with_minibuffer(args, read_from_minibuffer)
}

/// `(read-variable PROMPT &optional DEFAULT)`
///
/// Read a variable name from the minibuffer.
/// In interactive mode, uses read-from-minibuffer and interns the result.
pub(crate) fn builtin_read_variable(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_read_variable_in_runtime(eval, &args)?;
    finish_read_variable_in_eval(eval, &args)
}

pub(crate) fn finish_read_variable_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_variable_with_minibuffer(args, |minibuffer_args| {
        builtin_read_from_minibuffer(eval, minibuffer_args.to_vec())
    })
}

pub(crate) fn builtin_read_variable_in_runtime(
    runtime: &impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<(), Flow> {
    // Validation only; batch prompt/stdin handling lives in the shared
    // `read-from-minibuffer` path reached from `finish_read_variable_*`.
    let _ = runtime;
    expect_min_args("read-variable", args, 1)?;
    expect_max_args("read-variable", args, 2)?;
    let _prompt = expect_lisp_string(&args[0])?;
    Ok(())
}

pub(crate) fn finish_read_variable_with_minibuffer(
    args: &[Value],
    read_from_minibuffer: impl FnMut(&[Value]) -> EvalResult,
) -> EvalResult {
    finish_symbol_reader_with_minibuffer(args, read_from_minibuffer)
}

/// `(minibuffer-prompt)` — returns the current minibuffer prompt or nil.
///
/// Stub: returns nil (no active minibuffer in non-interactive mode).
pub(crate) fn builtin_minibuffer_prompt_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("minibuffer-prompt", &args, 0)?;
    Ok(eval
        .minibuffers()
        .current()
        .map(|state| Value::heap_string(state.prompt.clone()))
        .unwrap_or(Value::NIL))
}

pub(crate) fn builtin_minibuffer_prompt_end_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("minibuffer-prompt-end", &args, 0)?;

    Ok(Value::fixnum(
        minibuffer_prompt_end_in_state(eval.variables(), &eval.edit.buffers, &eval.minibuffers())?
            .as_i64(),
    ))
}

fn minibuffer_prompt_end_in_state(
    vars: VariableView<'_>,
    buffers: &BufferManager,
    minibuffers: &MinibufferManager,
) -> Result<LispCharPos1, Flow> {
    let Some((current_id, point_min, point_max)) = buffers.current_buffer().map(|buffer| {
        (
            buffer.id(),
            buffer.point_min_lisp_char_pos().as_i64(),
            buffer.point_max_lisp_char_pos().as_i64(),
        )
    }) else {
        return Ok(LispCharPos1::new(1));
    };

    let Some(state) = minibuffers.current() else {
        return Ok(LispCharPos1::new(point_min));
    };
    if state.buffer_id != current_id {
        return Ok(LispCharPos1::new(point_min));
    }

    let (_, prompt_end_pos) = super::buffer::find_field_bounds_in_state(
        vars,
        &[],
        buffers,
        Some(&Value::fixnum(point_min)),
        false,
        None,
        None,
    )?;

    // GNU `Fminibuffer_prompt_end` falls back to point-min when the active
    // minibuffer has no prompt field at BEGV, even if `field-end` reaches ZV.
    if prompt_end_pos == point_max {
        let buffer = buffers
            .get(current_id)
            .expect("current minibuffer must remain available");
        let point_min_byte =
            buffer.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(point_min));
        if buffer
            .text_props_get_property_at_emacs_byte_pos(point_min_byte, Value::symbol("field"))
            .is_none()
        {
            return Ok(LispCharPos1::new(point_min));
        }
    }

    Ok(LispCharPos1::new(prompt_end_pos))
}

/// `(minibuffer-contents)` — returns the current minibuffer contents.
///
/// In non-interactive batch mode, Emacs exposes current buffer contents.
pub(crate) fn builtin_minibuffer_contents_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("minibuffer-contents", &args, 0)?;
    Ok(Value::heap_string(minibuffer_contents_lisp_string(
        eval, true,
    )?))
}

/// `(minibuffer-contents-no-properties)` — returns minibuffer contents
/// without text properties.
pub(crate) fn builtin_minibuffer_contents_no_properties_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("minibuffer-contents-no-properties", &args, 0)?;
    Ok(Value::heap_string(minibuffer_contents_lisp_string(
        eval, false,
    )?))
}

/// `(minibuffer-depth)` — returns the current recursive minibuffer depth.
///
/// Stub: returns 0.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_minibuffer_depth(args: Vec<Value>) -> EvalResult {
    expect_args("minibuffer-depth", &args, 0)?;
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_minibuffer_depth_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("minibuffer-depth", &args, 0)?;
    Ok(Value::fixnum(eval.minibuffers().depth() as i64))
}

/// `(minibufferp &optional BUFFER)` — returns t if BUFFER is a minibuffer.
///
/// Batch-compatible behavior: accepts 0..=2 args, validates BUFFER-like first
/// arg shape, and returns nil (no active minibuffer).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_minibufferp(args: Vec<Value>) -> EvalResult {
    validate_minibufferp_args(&args)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_minibufferp_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    validate_minibufferp_args(&args)?;
    let live_only = args.get(1).is_some_and(|v| v.is_truthy());
    let Some(buffer_id) = resolve_minibuffer_buffer_arg(&eval.edit.buffers, args.first())? else {
        return Ok(Value::NIL);
    };
    let is_live = eval.minibuffers().has_buffer(buffer_id);
    let is_minibuffer =
        is_live
            || eval.edit.buffers.get(buffer_id).is_some_and(|buffer| {
                is_minibuffer_buffer_name(&buffer.name_runtime_string_owned())
            });
    Ok(Value::bool_val(if live_only {
        is_live
    } else {
        is_minibuffer
    }))
}

pub(crate) fn builtin_minibuffer_innermost_command_loop_p_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("minibuffer-innermost-command-loop-p", &args, 0, 1)?;
    let Some(buffer_id) = resolve_minibuffer_buffer_arg(&eval.edit.buffers, args.first())? else {
        return Ok(Value::NIL);
    };
    let recursive_depth = eval.recursive_command_loop_depth();
    let command_loop_depth = eval
        .minibuffers()
        .state_stack
        .iter()
        .find(|state| state.buffer_id == buffer_id)
        .map(|state| state.command_loop_depth);
    Ok(Value::bool_val(
        command_loop_depth.is_some_and(|depth| depth == recursive_depth),
    ))
}

pub(crate) fn builtin_innermost_minibuffer_p_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("innermost-minibuffer-p", &args, 0, 1)?;
    let Some(buffer_id) = resolve_minibuffer_buffer_arg(&eval.edit.buffers, args.first())? else {
        return Ok(Value::NIL);
    };
    Ok(Value::bool_val(
        eval.minibuffers()
            .current()
            .is_some_and(|state| state.buffer_id == buffer_id),
    ))
}

fn validate_minibufferp_args(args: &[Value]) -> Result<(), Flow> {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("minibufferp"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    if let Some(bufferish) = args.first() {
        match bufferish.kind() {
            ValueKind::Nil | ValueKind::String | ValueKind::Veclike(VecLikeType::Buffer) => {}
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("bufferp"), *bufferish],
                ));
            }
        }
    }
    Ok(())
}

/// Eval-aware `(recursive-edit)` — enters the command loop.
///
/// Mirrors GNU Emacs keyboard.c:772 `Frecursive_edit`.
pub(crate) fn builtin_recursive_edit(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("recursive-edit", &args, 0)?;
    let restore = RecursiveEditBuffer::record(eval);
    let result = eval.recursive_edit_inner();
    restore.unwind(eval);
    result
}

/// The buffer `recursive-edit` has to put back when its command loop unwinds.
///
/// GNU `Frecursive_edit` (src/keyboard.c:811-816) records the current buffer
/// only when it is NOT the selected window's buffer, and
/// `recursive_edit_unwind` (src/keyboard.c:837-844) makes it current again --
/// on every exit, including the `(throw 'exit ...)` that
/// `exit-recursive-edit' uses.  Without it a `recursive-edit' entered inside
/// `with-temp-buffer' returns with the window's buffer current, so everything
/// the caller then does to "its" buffer silently lands elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecursiveEditBuffer {
    /// The current buffer is already the selected window's, so the command
    /// loop leaves it current by itself and GNU records nothing.
    SelectedWindows,
    /// A buffer only this Lisp frame knows about; make it current on unwind.
    Restore(crate::buffer::BufferId),
}

impl RecursiveEditBuffer {
    fn record(eval: &mut super::eval::Context) -> Self {
        let Some(current) = eval.edit.buffers.current_buffer_id() else {
            return Self::SelectedWindows;
        };
        let window_buffer = super::window_cmds::builtin_window_buffer(eval, &[])
            .ok()
            .and_then(|value| value.as_buffer_id());
        if window_buffer == Some(current) {
            Self::SelectedWindows
        } else {
            Self::Restore(current)
        }
    }

    fn unwind(self, eval: &mut super::eval::Context) {
        if let Self::Restore(buffer_id) = self
            && eval.edit.buffers.get(buffer_id).is_some()
        {
            let _ = eval.edit.buffers.switch_current(buffer_id);
        }
    }
}

/// `(top-level)` — exit all recursive edits.
///
/// Mirrors GNU Emacs keyboard.c:1187 `Ftop_level`.
/// Throws to the `top-level` tag to unwind all recursive edits.
pub(crate) fn builtin_top_level(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("top-level", &args, 0)?;
    Err(Flow::throw_with_symbols(
        Value::symbol("top-level"),
        Value::NIL,
        eval.symbol_mode(),
    ))
}

/// `(exit-recursive-edit)` — exit innermost recursive edit.
///
/// Mirrors GNU Emacs keyboard.c:1211 `Fexit_recursive_edit`.
/// Throws to the `exit` tag with nil value.
pub(crate) fn builtin_exit_recursive_edit(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("exit-recursive-edit", &args, 0)?;
    // GNU Emacs checks: command_loop_level > 0 || minibuf_level > 0
    if eval.recursive_command_loop_depth() == 0 && eval.minibuffers().depth() == 0 {
        return Err(signal(
            LispCondition::UserError,
            vec![Value::string("No recursive edit is in progress")],
        ));
    }
    Err(Flow::throw_with_symbols(
        Value::symbol("exit"),
        Value::NIL,
        eval.symbol_mode(),
    ))
}

/// `(exit-minibuffer)` — exit the active minibuffer.
///
/// Emacs exits by throwing to the `exit` tag; without a catch this
/// surfaces as `no-catch`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_exit_minibuffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("exit-minibuffer", &args, 0)?;
    Err(Flow::throw_with_symbols(
        Value::symbol("exit"),
        Value::NIL,
        eval.symbol_mode(),
    ))
}

/// `(abort-minibuffers)` — abort active minibuffer sessions.
///
/// Batch/non-interactive mode has no active minibuffer, so this matches GNU
/// Emacs by signaling a plain `error`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_abort_minibuffers(args: Vec<Value>) -> EvalResult {
    expect_args("abort-minibuffers", &args, 0)?;
    Err(signal("error", vec![Value::string("Not in a minibuffer")]))
}

/// GNU minibuf.c `this_minibuffer_depth`: the 1-based minibuffer level of the
/// current buffer, or 0 when it is not one of the active minibuffers.
fn this_minibuffer_depth(eval: &super::eval::Context) -> usize {
    let Some(buffer_id) = eval.edit.buffers.current_buffer_id() else {
        return 0;
    };
    eval.minibuffers()
        .state_stack
        .iter()
        .position(|state| state.buffer_id == buffer_id)
        .map_or(0, |index| index + 1)
}

/// Mirrors GNU minibuf.c `Fabort_minibuffers`.
///
/// It deliberately does not throw to `exit` itself: that is
/// `abort-recursive-edit`, whose `t` means a plain `quit`.  Aborting a
/// minibuffer instead delegates to `minibuffer-quit-recursive-edit`, which
/// throws a thunk signaling the distinguishable `minibuffer-quit`.
pub(crate) fn builtin_abort_minibuffers_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("abort-minibuffers", &args, 0)?;
    let minibuf_depth = this_minibuffer_depth(eval);
    if minibuf_depth == 0 {
        return Err(signal("error", vec![Value::string("Not in a minibuffer")]));
    }
    if builtin_minibuffer_innermost_command_loop_p_ctx(eval, vec![])?.is_nil() {
        return Err(signal(
            "error",
            vec![Value::string("Not in most nested command loop")],
        ));
    }

    let minibuf_level = eval.minibuffers().depth();
    let quit_recursive_edit = Value::symbol("minibuffer-quit-recursive-edit");
    if minibuf_depth < minibuf_level {
        // Aborting this minibuffer also aborts every one nested inside it, so
        // GNU confirms first and then quits that many recursive edits at once.
        let levels = minibuf_level - minibuf_depth + 1;
        let prompt = Value::string(format!("Abort {levels} minibuffer levels? "));
        if eval
            .apply(Value::symbol("yes-or-no-p"), vec![prompt])?
            .is_truthy()
        {
            eval.apply(quit_recursive_edit, vec![Value::fixnum(levels as i64)])?;
        }
    } else {
        eval.apply(quit_recursive_edit, vec![])?;
    }
    Ok(Value::NIL)
}

pub(crate) fn minibuffer_contents_lisp_string_in_state(
    vars: VariableView<'_>,
    buffers: &BufferManager,
    minibuffers: &MinibufferManager,
    preserve_properties: bool,
) -> Result<LispString, Flow> {
    let Some((point_max, current_id)) = buffers
        .current_buffer()
        .map(|buffer| (buffer.accessible_emacs_byte_region().end(), buffer.id()))
    else {
        return Ok(LispString::from_utf8(""));
    };
    let prompt_end = minibuffer_prompt_end_in_state(vars, buffers, minibuffers)?;
    let buffer = buffers
        .get(current_id)
        .expect("current buffer must remain available");
    let start = buffer.lisp_pos_to_accessible_emacs_byte_pos(prompt_end);
    let range = EmacsByteRange::new(start, point_max);
    Ok(if preserve_properties {
        buffer.buffer_substring_lisp_string_range(range)
    } else {
        buffer.buffer_substring_lisp_string_no_properties_range(range)
    })
}

fn minibuffer_contents_lisp_string(
    eval: &mut super::eval::Context,
    preserve_properties: bool,
) -> Result<LispString, Flow> {
    minibuffer_contents_lisp_string_in_state(
        eval.variables(),
        &eval.edit.buffers,
        &eval.minibuffers(),
        preserve_properties,
    )
}

fn resolve_minibuffer_buffer_arg(
    buffers: &BufferManager,
    bufferish: Option<&Value>,
) -> Result<Option<BufferId>, Flow> {
    let Some(val) = bufferish else {
        return Ok(buffers.current_buffer_id());
    };
    match val.kind() {
        ValueKind::Nil => Ok(buffers.current_buffer_id()),
        ValueKind::Veclike(VecLikeType::Buffer) => Ok(val.as_buffer_id()),
        ValueKind::String => Ok(val
            .as_utf8_str()
            .and_then(|name| buffers.find_buffer_by_name(name))),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), *val],
        )),
    }
}

fn is_minibuffer_buffer_name(name: &str) -> bool {
    name.starts_with(" *Minibuf-") && name.ends_with('*')
}

/// `(abort-recursive-edit)` — abort the innermost recursive edit.
///
/// Mirrors GNU Emacs keyboard.c:1222 `Fabort_recursive_edit`.
/// Throws to the `exit` tag with `t` value (signals quit on catch).
pub(crate) fn builtin_abort_recursive_edit(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("abort-recursive-edit", &args, 0)?;
    // GNU Emacs checks: command_loop_level > 0 || minibuf_level > 0
    if eval.recursive_command_loop_depth() == 0 && eval.minibuffers().depth() == 0 {
        return Err(signal(
            LispCondition::UserError,
            vec![Value::string("No recursive edit is in progress")],
        ));
    }
    Err(Flow::throw_with_symbols(
        Value::symbol("exit"),
        Value::T,
        eval.symbol_mode(),
    ))
}

// ---------------------------------------------------------------------------
// Value-to-string-list conversion helper
// ---------------------------------------------------------------------------

/// Extract a list of strings from a Value.
///
/// Handles:
/// - Proper list of strings
/// - Alist of (string . _) pairs
/// - Vector of strings
/// - nil → empty
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn value_to_string_list(val: &Value) -> Vec<String> {
    match val.kind() {
        ValueKind::Nil => Vec::new(),
        ValueKind::Cons => {
            let items = match super::value::list_to_vec(val) {
                Some(v) => v,
                None => return Vec::new(),
            };
            items
                .iter()
                .filter_map(|item| match item.kind() {
                    ValueKind::String => completion_display_string_from_value(item),
                    ValueKind::Symbol(id) => Some(resolve_sym(id).to_owned()),
                    // Alist entry: (STRING . _)
                    ValueKind::Cons => {
                        let pair_car = item.cons_car();
                        completion_display_string_from_value(&pair_car)
                    }
                    _ => None,
                })
                .collect()
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let vec = val.as_vector_data().unwrap().clone();
            vec.iter()
                .filter_map(|item| match item.kind() {
                    ValueKind::String => completion_display_string_from_value(item),
                    ValueKind::Symbol(id) => Some(resolve_sym(id).to_owned()),
                    _ => None,
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// `(completion--flex-cost-gotoh PAT STR)`
///
/// Compute the cost of PAT matching STR using a modified Gotoh affine-gap
/// algorithm.  Returns nil when there is no match, else `(COST . MATCHES)`
/// where COST is a fixnum (lower is better) and MATCHES is a list, the same
/// length as PAT, whose i-th element is the position in STR where PAT's i-th
/// character matched.
///
/// Faithful port of GNU Emacs 31.0.90 `Fcompletion__flex_cost_gotoh`
/// (src/minibuf.c:2334).
pub(crate) fn builtin_flex_cost_gotoh(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    // Pre-allocated matrix size limits, mirroring the C macros.
    const FLEX_MAX_STR_SIZE: usize = 512;
    const FLEX_MAX_PAT_SIZE: usize = 128;
    const FLEX_MAX_MATRIX_SIZE: usize = FLEX_MAX_PAT_SIZE * FLEX_MAX_STR_SIZE;

    let pat = expect_lisp_string(&args[0])?;
    let str = expect_lisp_string(&args[1])?;

    // Operate on character vectors (GNU uses SCHARS / fetch_string_char).
    // Completion text is UTF-8; fall back to a lossy decode for raw bytes so
    // we never panic, matching GNU's "process anyway" intent.
    let pat_chars: Vec<char> = match pat.as_utf8_str() {
        Some(s) => s.chars().collect(),
        None => crate::emacs_core::emacs_char::to_utf8_lossy(pat.as_bytes())
            .chars()
            .collect(),
    };
    let str_chars: Vec<char> = match str.as_utf8_str() {
        Some(s) => s.chars().collect(),
        None => crate::emacs_core::emacs_char::to_utf8_lossy(str.as_bytes())
            .chars()
            .collect(),
    };

    let patlen = pat_chars.len();
    let strlen = str_chars.len();
    let width = strlen + 1;
    let size = (patlen + 1) * width;

    const GAP_OPEN_COST: i32 = 10;
    const GAP_EXTEND_COST: i32 = 1;
    const POS_INF: i32 = i32::MAX / 2;

    // Bail if strings are empty or matrix too large.
    if patlen == 0 || strlen == 0 || size > FLEX_MAX_MATRIX_SIZE {
        return Ok(Value::NIL);
    }

    let ignore_case = completion_ignore_case(eval.variables(), &eval.edit.buffers);

    // Cheap subsequence pre-filter for the common case-sensitive case: if PAT
    // is not a subsequence of STR there can be no match, so bail before the
    // O(N*M) DP below.
    if !ignore_case {
        let mut pi = 0;
        for &sc in &str_chars {
            if pi >= patlen {
                break;
            }
            if sc == pat_chars[pi] {
                pi += 1;
            }
        }
        if pi < patlen {
            return Ok(Value::NIL);
        }
    }

    // Flat (patlen+1) x width matrices, indexed via MAT(i, j) for i in
    // -1..patlen-1 and j in -1..strlen-1.  Initialize to +inf...
    let mut m = vec![POS_INF; size];
    let mut d = vec![POS_INF; size];
    // ...except the first row of D, which gets gap_open_cost/2 for cheaper
    // leading gaps, and D[-1,-1] = 0 to promote matches at the beginning.
    d[..width].fill(GAP_OPEN_COST / 2);
    d[0] = 0;

    let idx = |i: isize, j: isize| -> usize { ((i + 1) as usize) * width + (j + 1) as usize };

    let downcased =
        |c: char| -> i64 { crate::emacs_core::builtins::downcase_char_code_emacs_compat(c as i64) };

    // Position (column index) of the first match found in the previous row, to
    // save iterations.
    let mut prev_match: usize = 0;

    // Forward pass.
    for (i, &pat_char) in pat_chars.iter().enumerate() {
        let mut match_seen = false;
        let start = prev_match;
        for (j, &str_char) in str_chars.iter().enumerate().skip(start) {
            let cmatch = if ignore_case {
                downcased(pat_char) == downcased(str_char)
            } else {
                pat_char == str_char
            };

            let ii = i as isize;
            let jj = j as isize;

            if cmatch {
                if !match_seen {
                    match_seen = true;
                    prev_match = j;
                }
                // Best of "previous char also matched" (M[i-1,j-1]) and
                // "arrive at this match from a gap" (D[i-1,j-1]).
                m[idx(ii, jj)] = m[idx(ii - 1, jj - 1)].min(d[idx(ii - 1, jj - 1)]);
            }
            // Best accumulated gapping cost: open a gap from a match on this
            // row, or extend a gap started earlier.
            d[idx(ii, jj)] =
                (m[idx(ii, jj - 1)] + GAP_OPEN_COST).min(d[idx(ii, jj - 1)] + GAP_EXTEND_COST);
        }
    }

    // Find lowest cost in last row.
    let mut best_cost = POS_INF;
    let mut lastcol: isize = -1;
    for j in 0..strlen {
        let cost = m[idx(patlen as isize - 1, j as isize)];
        if cost < best_cost {
            best_cost = cost;
            lastcol = j as isize;
        }
    }

    // Return early if no match.
    if lastcol < 0 || best_cost >= POS_INF {
        return Ok(Value::NIL);
    }

    // Go backwards to build the match-positions list.
    let mut positions: Vec<i64> = Vec::with_capacity(patlen);
    positions.push(lastcol as i64);
    let mut l: isize = lastcol;
    let mut i: isize = patlen as isize - 2;
    while i >= 0 {
        // do { --l } while (l >= 0 && M[i,l] >= D[i,l]);
        loop {
            l -= 1;
            if !(l >= 0 && m[idx(i, l)] >= d[idx(i, l)]) {
                break;
            }
        }
        positions.push(l as i64);
        i -= 1;
    }
    // positions was built last-to-first; reverse to PAT order.
    positions.reverse();

    let mut matches = Value::NIL;
    for &pos in positions.iter().rev() {
        matches = Value::cons(Value::fixnum(pos), matches);
    }
    Ok(Value::cons(Value::fixnum(best_cost as i64), matches))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/minibuffer_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/declared_string_completion.rs"]
mod declared_string_completion_tests;
