//! Reader-internals builtins: read, read-from-string,
//! eval-buffer, eval-region, read-char, read-event, read-char-exclusive,
//! get-load-suffixes, locate-file, locate-file-internal, read-coding-system,
//! read-non-nil-coding-system.

use super::error::{EvalResult, Flow, signal};
use super::intern::{intern, resolve_sym};
use super::value::*;
use crate::buffer::{EmacsByteRange, LispCharPos1};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_max_args, expect_min_args};
use crate::heap_types::LispString;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_integer_or_marker_in_buffers(
    buffers: &crate::buffer::BufferManager,
    value: &Value,
) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ if super::marker::is_marker(value) => {
            super::marker::marker_position_as_int_with_buffers(buffers, value)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

fn lread_string_text(value: &Value) -> Option<String> {
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

fn expect_lisp_string(value: &Value) -> Result<LispString, Flow> {
    match value.kind() {
        ValueKind::String => Ok(value.as_lisp_string().expect("checked string").clone()),
        ValueKind::Symbol(id) => Ok(LispString::from_utf8(resolve_sym(id))),
        ValueKind::Nil => Ok(LispString::from_unibyte(b"nil".to_vec())),
        ValueKind::T => Ok(LispString::from_unibyte(b"t".to_vec())),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins
// ---------------------------------------------------------------------------

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn strip_reader_prefix(source: &str) -> (&str, bool) {
    if !source.starts_with("#!") {
        return (source, false);
    }
    match source.find('\n') {
        Some(index) => (&source[index + 1..], false),
        None => ("", true),
    }
}

fn strip_reader_prefix_lisp_string(source: &crate::heap_types::LispString) -> (usize, bool) {
    let bytes = source.as_bytes();
    if !bytes.starts_with(b"#!") {
        return (0, false);
    }
    match bytes.iter().position(|&b| b == b'\n') {
        Some(index) => (index + 1, false),
        None => (0, true),
    }
}

fn signal_reader_error_for_eval_source(
    e: super::value_reader::ReadError,
    eof_source: Option<Value>,
) -> Flow {
    match e.kind {
        super::value_reader::ReadErrorKind::EndOfFile => {
            super::reader::end_of_file_error_for_source(eof_source)
        }
        super::value_reader::ReadErrorKind::Error => {
            signal("error", vec![Value::string(e.message)])
        }
        super::value_reader::ReadErrorKind::InvalidReadSyntax => signal(
            LispCondition::InvalidReadSyntax,
            vec![Value::string(format!("Read error: {}", e.message))],
        ),
        super::value_reader::ReadErrorKind::Signal => {
            signal(e.signal_symbol.as_deref().unwrap_or("error"), e.signal_data)
        }
    }
}

pub(crate) fn eval_forms_from_lisp_source(
    eval: &mut super::eval::Context,
    source: &crate::heap_types::LispString,
    eof_source: Option<Value>,
) -> EvalResult {
    let evaluation = SourceFormEvaluation::resolve(eval);
    eval_forms_from_lisp_source_streaming(eval, source, eof_source, evaluation)
}

fn eval_forms_from_lisp_source_streaming(
    eval: &mut super::eval::Context,
    source: &crate::heap_types::LispString,
    eof_source: Option<Value>,
    evaluation: SourceFormEvaluation,
) -> EvalResult {
    let (start_pos, shebang_only_line) = strip_reader_prefix_lisp_string(source);
    if shebang_only_line {
        return Err(signal(LispCondition::EndOfFile, vec![]));
    }
    if source.as_bytes().is_empty() {
        return Ok(Value::NIL);
    }

    let read_source = super::value_reader::LispReadSource::new(source);

    // Bind `standard-input` to the shared load-read cursor so `(read)` inside an
    // evaluated form reads the *next* top-level form from this same source, and
    // the loop resumes after it — GNU `readevalloop`'s `specbind
    // (Qstandard_input, readcharfun)` (lread.c), which fires for eval-buffer and
    // eval-region too.  The cursor reads a heap copy of `source`; its bytes are
    // identical, so the loop and `(read)` advance one shared byte offset.
    let setup_specpdl_base = eval.specpdl.len();
    let source_value = Value::heap_string(source.clone());
    eval.push_specpdl_root(source_value);
    eval.try_specbind_or_unwind_to(
        setup_specpdl_base,
        intern("standard-input"),
        eval.load_read_stream_token.as_lisp_value(),
    )?;
    eval.load_read_cursors.push(super::eval::LoadReadCursor {
        source: source_value,
        eof_source,
        pos: start_pos,
        shorthands: None,
    });

    let loop_result: Result<(), super::error::Flow> = (|| {
        loop {
            // Read at the shared cursor: a `(read)` in the previous form may have
            // advanced it past forms the loop must now skip.
            let pos = eval
                .load_read_cursors
                .last()
                .expect("load-read cursor present during readevalloop")
                .pos;
            let read_result = read_source
                .read_one(pos, &eval.obarray)
                .map_err(|error| signal_reader_error_for_eval_source(error, eof_source))?;
            let Some((form, next_pos)) = read_result else {
                break;
            };
            if let Some(cursor) = eval.load_read_cursors.last_mut() {
                cursor.pos = next_pos;
            }

            let eval_roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(form);
            evaluation.push_root(eval);
            let eval_result = evaluation.evaluate(eval, form);
            eval.restore_specpdl_roots(eval_roots);
            eval_result?;
        }
        Ok(())
    })();

    // Unwind the load-read cursor and the `standard-input` binding + source root
    // regardless of how the loop exited.
    eval.load_read_cursors.pop();
    let loop_result = eval
        .unbind_to_with_result(setup_specpdl_base, loop_result.map(|()| Value::NIL))
        .map(|_| ());

    loop_result?;

    Ok(Value::NIL)
}

fn map_eval_error_to_flow(err: super::error::EvalError) -> Flow {
    super::error::flow_from_eval_error(err)
}

pub(crate) fn eval_buffer_source_text_in_state(
    buffers: &crate::buffer::BufferManager,
    arg: Option<&Value>,
) -> Result<crate::heap_types::LispString, Flow> {
    let buffer_id = resolve_eval_buffer_id_in_state(buffers, arg)?;
    buffers
        .get(buffer_id)
        .map(|buffer| {
            buffer.buffer_substring_lisp_string_range(buffer.accessible_emacs_byte_range())
        })
        .ok_or_else(|| signal("error", vec![Value::string("No such buffer")]))
}

fn resolve_eval_buffer_id_in_state(
    buffers: &crate::buffer::BufferManager,
    arg: Option<&Value>,
) -> Result<crate::buffer::BufferId, Flow> {
    match arg {
        None => Ok(buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?),
        Some(v) if v.is_nil() => Ok(buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?),
        Some(v) if v.is_buffer() => Ok(v.as_buffer_id().unwrap()),
        Some(v) if v.is_string() => Ok({
            let name = lread_string_text(v).expect("checked string");
            buffers
                .find_buffer_by_name(&name)
                .ok_or_else(|| signal("error", vec![Value::string("No such buffer")]))?
        }),
        Some(other) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *other],
        )),
    }
}

fn eval_buffer_filename_in_state(
    buffers: &crate::buffer::BufferManager,
    buffer_id: crate::buffer::BufferId,
    arg: Option<&Value>,
) -> Result<Option<LispString>, Flow> {
    match arg {
        None => Ok(buffers
            .get(buffer_id)
            .and_then(|buffer| buffer.file_name_value().as_lisp_string().cloned())),
        Some(v) if v.is_nil() => Ok(buffers
            .get(buffer_id)
            .and_then(|buffer| buffer.file_name_value().as_lisp_string().cloned())),
        Some(value) => Ok(Some(expect_lisp_string(value)?)),
    }
}

fn record_eval_buffer_load_history(eval: &mut super::eval::Context, filename: &LispString) {
    let entry = Value::cons(Value::heap_string(filename.clone()), Value::NIL);
    let history = eval
        .obarray()
        .symbol_value("load-history")
        .cloned()
        .unwrap_or(Value::NIL);
    let filtered_history = Value::list(
        list_to_vec(&history)
            .unwrap_or_default()
            .into_iter()
            .filter(|existing| {
                if existing.is_cons() {
                    existing
                        .cons_car()
                        .as_lisp_string()
                        .is_none_or(|loaded| loaded != filename)
                } else {
                    true
                }
            })
            .collect(),
    );
    eval.set_variable("load-history", Value::cons(entry, filtered_history));
}

fn record_eval_buffer_save_excursion(eval: &mut super::eval::Context) {
    eval.record_save_excursion();
}

pub(crate) fn eval_region_source_text_in_state(
    buffers: &crate::buffer::BufferManager,
    args: &[Value],
) -> Result<crate::heap_types::LispString, Flow> {
    let (raw_start, raw_end) = eval_region_bounds_in_state(buffers, args)?;
    let buffer = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if raw_start >= raw_end {
        return Ok(crate::heap_types::LispString::new(
            String::new(),
            buffer.get_multibyte(),
        ));
    }

    let byte_range = EmacsByteRange::new(
        buffer.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(raw_start)),
        buffer.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(raw_end)),
    );
    Ok(buffer.buffer_substring_lisp_string_range(byte_range))
}

fn eval_region_bounds_in_state(
    buffers: &crate::buffer::BufferManager,
    args: &[Value],
) -> Result<(i64, i64), Flow> {
    expect_min_args("eval-region", args, 2)?;
    expect_max_args("eval-region", args, 4)?;

    let buffer = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let point_char_pos = buffer.point_lisp_char_pos().as_i64();
    let max_char_pos = buffer.point_max_lisp_char_pos().as_i64();
    let raw_start = if args[0].is_nil() {
        point_char_pos
    } else {
        expect_integer_or_marker_in_buffers(buffers, &args[0])?
    };
    let raw_end = if args[1].is_nil() {
        point_char_pos
    } else {
        expect_integer_or_marker_in_buffers(buffers, &args[1])?
    };

    if raw_start < 1 || raw_start > max_char_pos || raw_end < 1 || raw_end > max_char_pos {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![args[0], args[1]],
        ));
    }
    Ok((raw_start, raw_end))
}

/// `(eval-buffer &optional BUFFER PRINTFLAG FILENAME UNIBYTE DO-ALLOW-PRINT)`
///
/// Evaluate all forms from BUFFER (or current buffer) and return nil.
pub(crate) fn builtin_eval_buffer(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("eval-buffer", &args, 5)?;
    let buffer_id = resolve_eval_buffer_id_in_state(&eval.buffers, args.first())?;
    let source = eval_buffer_source_text_in_state(&eval.buffers, args.first())?;
    let filename = eval_buffer_filename_in_state(&eval.buffers, buffer_id, args.get(2))?;

    let specpdl_count = eval.specpdl.len();

    let gc_roots = eval.save_specpdl_roots();

    let result = (|| -> EvalResult {
        let buffer_value = Value::make_buffer(buffer_id);
        let prior_eval_buffer_list = eval.visible_variable_value_or_nil("eval-buffer-list");
        eval.push_specpdl_root(buffer_value);
        eval.push_specpdl_root(prior_eval_buffer_list);
        let eval_buffer_list = Value::cons(buffer_value, prior_eval_buffer_list);
        eval.push_specpdl_root(eval_buffer_list);
        eval.try_specbind_or_unwind_to(
            specpdl_count,
            intern("eval-buffer-list"),
            eval_buffer_list,
        )?;

        let do_allow_print = args.get(4).is_some_and(|v| v.is_truthy());
        let standard_output = if args.get(1).is_none_or(|v| v.is_nil()) && !do_allow_print {
            Value::symbol("symbolp")
        } else {
            args.get(1).copied().unwrap_or(Value::NIL)
        };
        eval.try_specbind_or_unwind_to(specpdl_count, intern("standard-output"), standard_output)?;

        // GNU `Feval_buffer` records an excursion before evaluating the
        // source buffer.  Source loads depend on this: `load-with-code-conversion`
        // evaluates a temporary *load* buffer while the caller's buffer remains
        // current, and any `set-buffer` during evaluation must unwind afterward.
        record_eval_buffer_save_excursion(eval);

        if let Some(filename) = filename.as_ref() {
            let filename_value = Value::heap_string(filename.clone());
            eval.push_specpdl_root(filename_value);
            let current_load_list = Value::cons(filename_value, Value::NIL);
            eval.push_specpdl_root(current_load_list);
            eval.try_specbind_or_unwind_to(
                specpdl_count,
                intern("current-load-list"),
                current_load_list,
            )?;
        }

        let buffer_has_local_lexical_binding = eval
            .buffers
            .get(buffer_id)
            .and_then(|buffer| buffer.get_buffer_local_binding("lexical-binding"))
            .and_then(|binding| binding.as_value())
            .is_some();
        if !buffer_has_local_lexical_binding {
            let lexical_binding = match super::load::source_lexical_binding_for_lisp_source(
                eval,
                &source,
                Some(buffer_value),
            ) {
                Ok(enabled) => enabled,
                Err(err) => return Err(map_eval_error_to_flow(err)),
            };
            eval.try_specbind_or_unwind_to(
                specpdl_count,
                intern("lexical-binding"),
                Value::bool_val(lexical_binding),
            )?;
        }

        // GNU `readevalloop` derives `internal-interpreter-environment` from
        // the current visible `lexical-binding` and unwinds it through the
        // specpdl.  Do not restore `lexical-binding` by direct assignment here:
        // nested source loads may have swapped the active buffer-local binding
        // cell by the time `eval-buffer` returns.
        let lexical_binding = eval.visible_variable_value_or_nil("lexical-binding");
        eval.specpdl.push(super::eval::SpecBinding::LexicalEnv {
            old_lexenv: eval.lexenv,
        });
        eval.lexenv = if lexical_binding.is_truthy() {
            Value::list(vec![Value::T])
        } else {
            Value::NIL
        };

        let loading_source_file = eval
            .visible_variable_value_or_nil("load-in-progress")
            .is_truthy()
            && filename.is_some();
        let result = match FormReader::resolve(eval, None) {
            // GNU `Feval_buffer' hands the BUFFER to `readevalloop'
            // (src/lread.c:2417) and takes no branch on `load-in-progress': it is
            // one readevalloop either way.  Reading the buffer text internally
            // would skip the hook and so skip Edebug entirely.
            FormReader::Lisp(reader) => {
                let result = readevalloop_buffer_with_lisp_reader(
                    eval,
                    buffer_id,
                    reader,
                    BufferReadRegion::WholeAccessible,
                );
                if result.is_ok()
                    && let Some(filename) = filename.as_ref()
                {
                    // GNU `readevalloop' calls `loadhist_initialize (sourcename)'
                    // for every shape of the loop (src/lread.c:2227).
                    record_eval_buffer_load_history(eval, filename);
                }
                result
            }
            FormReader::Internal if loading_source_file => {
                // GNU `Feval_buffer` hands `readevalloop' the BUFFER whether or
                // not a load is in progress (`src/lread.c:2417`), so reader
                // errors carry the buffer here too — that is why a truncated
                // `.el` reports `(end-of-file #<killed buffer>)` after
                // `load-with-code-conversion`'s temp buffer is killed, not a
                // bare `(end-of-file)`.
                super::load::eval_lisp_source_file_in_context(
                    eval,
                    filename
                        .as_ref()
                        .expect("load-in-progress eval-buffer must have filename"),
                    &source,
                    super::reader::ReadSourceObject::Buffer(buffer_value),
                )
                .map_err(map_eval_error_to_flow)
            }
            FormReader::Internal => {
                let result = eval_forms_from_lisp_source(eval, &source, Some(buffer_value));
                if result.is_ok()
                    && let Some(filename) = filename.as_ref()
                {
                    record_eval_buffer_load_history(eval, filename);
                }
                result
            }
        };

        eval.unbind_to_with_result(specpdl_count, result)
    })();

    eval.restore_specpdl_roots(gc_roots);
    result
}

/// `(eval-region START END &optional PRINTFLAG READ-FUNCTION)`
///
/// Evaluate forms in the [START, END) region of the current buffer.
pub(crate) fn builtin_eval_region(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if let FormReader::Lisp(read_function) = FormReader::resolve(eval, args.get(3).copied()) {
        let (start, end) = eval_region_bounds_in_state(&eval.buffers, &args)?;
        if start >= end {
            return Ok(Value::NIL);
        }
        let buffer_id = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
            .id;
        return readevalloop_buffer_with_lisp_reader(
            eval,
            buffer_id,
            read_function,
            BufferReadRegion::Bounded { start, end },
        );
    }

    let source = eval_region_source_text_in_state(&eval.buffers, &args)?;
    if source.as_bytes().is_empty() {
        return Ok(Value::NIL);
    }
    let buffer = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    eval_forms_from_lisp_source(eval, &source, Some(Value::make_buffer(buffer.id)))
}

/// Which function reads each top-level form of a `readevalloop`.
///
/// GNU picks it once per form, in this order (src/lread.c:2302-2320):
///   1. an explicit READ-FUNCTION argument (`eval-region' only),
///   2. `load-read-function',
///   3. the internal reader.
///
/// `load-read-function' defaults to the bare symbol `read' (src/lread.c:5741),
/// and calling that builtin on this stream is observably identical to reading
/// internally, so the default keeps the fast internal path.  That is the same
/// discrimination `load' already makes in
/// `streaming_readevalloop_lisp_source' (load.rs); making it a type keeps the
/// two entry points from drifting apart, and keeps the bootstrap off the Lisp
/// call route.
#[derive(Clone, Copy, Debug)]
enum FormReader {
    Internal,
    Lisp(Value),
}

impl FormReader {
    fn resolve(eval: &super::eval::Context, explicit: Option<Value>) -> Self {
        if let Some(read_function) = explicit.filter(|value| !value.is_nil()) {
            return Self::Lisp(read_function);
        }
        match eval
            .obarray
            .symbol_value("load-read-function")
            .copied()
            .filter(|hook| !hook.is_nil() && !hook.is_symbol_named("read"))
        {
            Some(hook) => Self::Lisp(hook),
            None => Self::Internal,
        }
    }
}

/// How a source form is evaluated after its reader has produced it.
///
/// GNU `readevalloop' chooses this independently from the form reader: a
/// custom `load-read-function' (notably Edebug's advice around `read') still
/// flows through `internal-macroexpand-for-load'.  Keeping the alternatives
/// exhaustive prevents a new reader route from silently becoming a plain-eval
/// route and skipping compiler macros.
#[derive(Clone, Copy, Debug)]
enum SourceFormEvaluation {
    Direct,
    EagerMacroexpand { function: Value },
}

impl SourceFormEvaluation {
    fn resolve(eval: &super::eval::Context) -> Self {
        match super::load::get_eager_macroexpand_fn(eval) {
            Some(function) => Self::EagerMacroexpand { function },
            None => Self::Direct,
        }
    }

    fn push_root(self, eval: &mut super::eval::Context) {
        if let Self::EagerMacroexpand { function } = self {
            eval.push_specpdl_root(function);
        }
    }

    fn evaluate(self, eval: &mut super::eval::Context, form: Value) -> EvalResult {
        match self {
            Self::Direct => eval.eval_sub(form),
            Self::EagerMacroexpand { function } => {
                super::load::eager_expand_eval(eval, form, function)
                    .map_err(super::error::flow_from_eval_error)
            }
        }
    }
}

/// The two shapes GNU `readevalloop' takes over a buffer, selected by whether
/// its START argument is nil (src/lread.c:2237-2264).
#[derive(Clone, Copy, Debug)]
enum BufferReadRegion {
    /// `eval-buffer': START is nil, so GNU neither switches buffers nor
    /// narrows -- it reads straight from the buffer's own point
    /// (`source_buffer_get', src/lread.c:332), which `Feval_buffer' has already
    /// set to BEGV (src/lread.c:2411-2416).
    WholeAccessible,
    /// `eval-region': START is non-nil, so GNU records an excursion, makes the
    /// buffer current and narrows to [BEGV, END) around every single read.
    Bounded { start: i64, end: i64 },
}

/// GNU `readevalloop' skips whitespace and comments and breaks on EOF *before*
/// invoking the reader (src/lread.c:2272-2288), so the reader is called exactly
/// once per form and a trailing newline never provokes an extra end-of-file
/// read.  Returns false when only whitespace and comments remain.
fn skip_to_next_form_in_buffer(
    buffer: &mut crate::buffer::Buffer,
    end_byte: crate::buffer::EmacsBytePos,
) -> bool {
    loop {
        let pos = buffer.point_emacs_byte_pos();
        if pos >= end_byte {
            return false;
        }
        let Some(ch) = buffer.char_at_emacs_byte_pos(pos) else {
            return false;
        };
        let Some(len) = buffer.char_after_emacs_byte_len(pos) else {
            return false;
        };
        match ch {
            ';' => {
                buffer.goto_emacs_byte_pos(pos.add_len(len));
                // `while ((c = readchar (&source)) != '\n' && c != -1);`
                loop {
                    let pos = buffer.point_emacs_byte_pos();
                    if pos >= end_byte {
                        return false;
                    }
                    let (Some(ch), Some(len)) = (
                        buffer.char_at_emacs_byte_pos(pos),
                        buffer.char_after_emacs_byte_len(pos),
                    ) else {
                        return false;
                    };
                    buffer.goto_emacs_byte_pos(pos.add_len(len));
                    if ch == '\n' {
                        break;
                    }
                }
            }
            // The exact set GNU treats as inter-form whitespace, NO-BREAK SPACE
            // included (src/lread.c:2285-2286).
            ' ' | '\t' | '\n' | '\u{c}' | '\r' | '\u{a0}' => {
                buffer.goto_emacs_byte_pos(pos.add_len(len));
            }
            _ => return true,
        }
    }
}

/// GNU `readevalloop' over a buffer, with a Lisp function reading each form.
///
/// The reader is the SOLE reader of each form: it reads from the buffer at
/// point and leaves point after the form it consumed.  Edebug relies on exactly
/// that (`edebug-read-and-maybe-wrap-form' reads the current buffer at point),
/// and on being handed the buffer itself as the stream -- `edebug--read'
/// instruments only when `(eq stream (current-buffer))'
/// (lisp/emacs-lisp/edebug.el:441-458).
fn readevalloop_buffer_with_lisp_reader(
    eval: &mut super::eval::Context,
    buffer_id: crate::buffer::BufferId,
    read_function: Value,
    region: BufferReadRegion,
) -> EvalResult {
    let buffer_value = Value::make_buffer(buffer_id);
    let evaluation = SourceFormEvaluation::resolve(eval);

    let gc_roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(read_function);
    eval.push_specpdl_root(buffer_value);
    evaluation.push_root(eval);
    let outer_specpdl = eval.specpdl.len();
    // GNU: `specbind (Qstandard_input, readcharfun);' (src/lread.c:2207), so a
    // bare `(read)' inside an evaluated form reads on from the same buffer.
    if let Err(flow) =
        eval.try_specbind_or_unwind_to(outer_specpdl, intern("standard-input"), buffer_value)
    {
        eval.restore_specpdl_roots(gc_roots);
        return Err(flow);
    }

    if let BufferReadRegion::WholeAccessible = region {
        // `BUF_TEMP_SET_PT (XBUFFER (buf), BUF_BEGV (XBUFFER (buf)));'
        // (src/lread.c:2411) -- note GNU sets the point of BUF, which need not
        // be the current buffer, and does not switch to it.
        if let Some(buffer) = eval.buffers.get_mut(buffer_id) {
            let begv = buffer.accessible_emacs_byte_region().start();
            buffer.goto_emacs_byte_pos(begv);
        }
    }

    let result = (|| {
        // `Bounded' unwinds an excursion after every read, so it cannot leave the
        // cursor in the buffer's point; GNU carries it in the loop's START
        // (`start = Fpoint_marker ()', src/lread.c:2330).  `WholeAccessible'
        // reads straight from the buffer's own point, which nothing unwinds.
        let mut next_start = match region {
            BufferReadRegion::Bounded { start, .. } => start,
            BufferReadRegion::WholeAccessible => 0,
        };
        loop {
            // GNU `readevalloop` captures the reader's advanced point *before*
            // evaluating the returned form: Edebug's transformed form is allowed
            // to move point without changing where the next read begins.
            let iteration_specpdl = eval.specpdl.len();
            let mut read_result = (|| -> Result<Option<(Value, i64)>, Flow> {
                let end = match region {
                    BufferReadRegion::Bounded { end, .. } => {
                        // GNU saves the caller's excursion, switches to the source
                        // buffer, saves that buffer's point/restriction, and only
                        // then invokes READ-FUNCTION (src/lread.c:2237-2258).
                        eval.record_save_excursion();
                        eval.set_current_buffer_unrecorded(buffer_id)?;
                        eval.record_save_excursion();
                        if let Some(state) = eval.buffers.save_current_restriction_state() {
                            eval.specpdl
                                .push(super::eval::SpecBinding::save_restriction(state));
                        }
                        Some(end)
                    }
                    BufferReadRegion::WholeAccessible => None,
                };

                let end_byte = {
                    let buffer = eval.buffers.get(buffer_id).ok_or_else(|| {
                        signal("error", vec![Value::string("Reading from killed buffer")])
                    })?;
                    match end {
                        Some(end) => {
                            buffer.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(end))
                        }
                        None => buffer.accessible_emacs_byte_region().end(),
                    }
                };
                if let BufferReadRegion::Bounded { .. } = region {
                    let (accessible_start, start_byte) = {
                        let buffer = eval
                            .buffers
                            .get(buffer_id)
                            .expect("buffer checked live above");
                        (
                            buffer.accessible_emacs_byte_region().start(),
                            buffer.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(
                                next_start,
                            )),
                        )
                    };
                    let buffer = eval
                        .buffers
                        .get_mut(buffer_id)
                        .expect("buffer checked live");
                    buffer.narrow_to_emacs_byte_range(EmacsByteRange::new(
                        accessible_start,
                        end_byte,
                    ));
                    buffer.goto_emacs_byte_pos(start_byte);
                }

                let buffer = eval.buffers.get_mut(buffer_id).ok_or_else(|| {
                    signal("error", vec![Value::string("Reading from killed buffer")])
                })?;
                if !skip_to_next_form_in_buffer(buffer, end_byte) {
                    return Ok(None);
                }
                let form_start = buffer.point_lisp_char_pos().as_i64();

                let form = eval.funcall_general(read_function, vec![buffer_value])?;
                let after_read = eval
                    .buffers
                    .get(buffer_id)
                    .ok_or_else(|| {
                        signal("error", vec![Value::string("Reading from killed buffer")])
                    })?
                    .point_lisp_char_pos()
                    .as_i64();
                if after_read <= form_start {
                    return Err(signal(
                        "error",
                        vec![Value::string(
                            "eval-region read function did not advance the input stream",
                        )],
                    ));
                }
                Ok(Some((form, after_read)))
            })();

            // `Bounded' unwinds the excursion/restriction it recorded above; the
            // reader's advanced point survives in `after_read'.
            if let BufferReadRegion::Bounded { .. } = region {
                let root_scope = eval.save_vm_roots();
                if let Ok(Some((form, _))) = &read_result {
                    eval.push_vm_frame_root(*form);
                }
                read_result = match read_result {
                    Ok(value) => {
                        match eval.unbind_to_with_result(iteration_specpdl, Ok(Value::NIL)) {
                            Ok(_) => Ok(value),
                            Err(flow) => Err(flow),
                        }
                    }
                    Err(flow) => match eval.unbind_to_with_result(iteration_specpdl, Err(flow)) {
                        Err(flow) => Err(flow),
                        Ok(_) => unreachable!("unwinding an error cannot produce a value"),
                    },
                };
                eval.restore_vm_roots(root_scope);
            }

            let Some((form, after_read)) = read_result? else {
                return Ok(Value::NIL);
            };
            next_start = after_read;

            let form_roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(form);
            let eval_result = evaluation.evaluate(eval, form);
            eval.restore_specpdl_roots(form_roots);
            eval_result?;
        }
    })();

    let result = eval.unbind_to_with_result(outer_specpdl, result);
    eval.restore_specpdl_roots(gc_roots);
    result
}

fn event_to_int(event: &Value) -> Option<i64> {
    match event.kind() {
        ValueKind::Fixnum(n) => Some(n),
        _ => None,
    }
}

fn expect_optional_prompt_string(args: &[Value]) -> Result<(), Flow> {
    if args.is_empty() || args[0].is_nil() || args[0].is_string() {
        return Ok(());
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("stringp"), args[0]],
    ))
}

/// `(read-event &optional PROMPT INHERIT-INPUT-METHOD SECONDS)`
///
/// Read an event from the command input.
/// In batch mode, reads from `unread-command-events`, returns nil if empty.
/// In interactive mode, blocks on the input channel via `read_char()`.
pub(crate) fn builtin_read_event(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::reader::display_read_prompt(eval, &args);
    if let Some(value) = builtin_read_event_in_runtime(eval, &args)? {
        return Ok(value);
    }

    finish_read_event_in_eval(eval, &args)
}

pub(crate) fn finish_read_event_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_event_interactive_in_runtime(eval, args)
}

pub(crate) fn finish_read_event_interactive_in_runtime(
    runtime: &mut impl super::reader::KeyboardInputRuntime,
    args: &[Value],
) -> EvalResult {
    let read_plan = super::reader::CommandEventReadPlan::from_seconds(args.get(2))?;
    let tty_input_decoding = super::reader::tty_input_decoding_from_read_args(args);
    let Some(event) = runtime.read_char_with_timeout(read_plan.timeout(), tty_input_decoding)?
    else {
        return Ok(Value::NIL);
    };
    let seconds_is_nil_or_omitted = args.get(2).is_none_or(|v| v.is_nil());
    if runtime.read_command_keys().is_empty() && seconds_is_nil_or_omitted {
        runtime.set_read_command_keys(vec![event]);
    }
    if let Some(n) = event_to_int(&event) {
        return Ok(Value::fixnum(n));
    }
    Ok(event)
}

/// `(read-char-exclusive &optional PROMPT INHERIT-INPUT-METHOD SECONDS)`
///
/// Read a character from the command input, discarding non-character events.
/// In batch mode, consumes `unread-command-events` until a character is found.
/// In interactive mode, blocks on the input channel, skipping non-character events.
pub(crate) fn builtin_read_char_exclusive(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::reader::display_read_prompt(eval, &args);
    if let Some(value) = builtin_read_char_exclusive_in_runtime(eval, &args)? {
        return Ok(value);
    }

    finish_read_char_exclusive_in_eval(eval, &args)
}

pub(crate) fn finish_read_char_exclusive_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_char_exclusive_interactive_in_runtime(eval, args)
}

pub(crate) fn finish_read_char_exclusive_interactive_in_runtime(
    runtime: &mut impl super::reader::KeyboardInputRuntime,
    args: &[Value],
) -> EvalResult {
    let read_plan = super::reader::CommandEventReadPlan::from_seconds(args.get(2))?;
    let tty_input_decoding = super::reader::tty_input_decoding_from_read_args(args);
    let deadline = read_plan
        .timeout()
        .map(|timeout| crate::host_time::Instant::now() + timeout);
    loop {
        let remaining = deadline.map(|deadline| {
            deadline.saturating_duration_since(crate::host_time::Instant::now())
        });
        let Some(event) = runtime.read_char_with_timeout(remaining, tty_input_decoding)? else {
            return Ok(Value::NIL);
        };
        let seconds_is_nil_or_omitted = args.get(2).is_none_or(|v| v.is_nil());
        if let Some(n) = event_to_int(&event) {
            if runtime.read_command_keys().is_empty() && seconds_is_nil_or_omitted {
                runtime.set_read_command_keys(vec![event]);
            }
            return Ok(Value::fixnum(n));
        }
    }
}

pub(crate) fn builtin_read_event_in_runtime(
    runtime: &mut impl super::reader::KeyboardInputRuntime,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("read-event"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    expect_optional_prompt_string(args)?;
    let seconds_is_nil_or_omitted = args.get(2).is_none_or(|v| v.is_nil());

    if let Some(event) = runtime.pop_unread_command_event() {
        if runtime.read_command_keys().is_empty() && seconds_is_nil_or_omitted {
            runtime.set_read_command_keys(vec![event]);
        }
        if let Some(n) = event_to_int(&event) {
            return Ok(Some(Value::fixnum(n)));
        }
        return Ok(Some(event));
    }

    Ok(None)
}

pub(crate) fn builtin_read_char_exclusive_in_runtime(
    runtime: &mut impl super::reader::KeyboardInputRuntime,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("read-char-exclusive"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    expect_optional_prompt_string(args)?;
    let seconds_is_nil_or_omitted = args.get(2).is_none_or(|v| v.is_nil());

    while let Some(event) = runtime.pop_unread_command_event() {
        if let Some(n) = event_to_int(&event) {
            if runtime.read_command_keys().is_empty() && seconds_is_nil_or_omitted {
                runtime.set_read_command_keys(vec![event]);
            }
            return Ok(Some(Value::fixnum(n)));
        }
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// `(get-load-suffixes)`
///
/// Return a list of suffixes that `load` tries when searching for files.
/// GNU lread.c: combines `load-suffixes` with `load-file-rep-suffixes`.
pub(crate) fn builtin_get_load_suffixes(
    obarray: &super::symbol::Obarray,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("get-load-suffixes", &args, 0)?;
    Ok(Value::list(
        super::load::LoadSuffixPlan::from_obarray(obarray)?.required_values(),
    ))
}

/// The dynamic-module file suffixes of one platform.
///
/// GNU decides these at configure time (configure.ac) and every user reads the
/// same two macros:
///
/// ```text
/// case $opsys in
///   cygwin|mingw32) DYNAMIC_LIB_SUFFIX=".dll" ;;
///   darwin)         DYNAMIC_LIB_SUFFIX=".dylib" ;;
///   *)              DYNAMIC_LIB_SUFFIX=".so" ;;
/// esac
/// case "${opsys}" in
///   darwin) DYNAMIC_LIB_SECONDARY_SUFFIX='.so' ;;
///   *)      DYNAMIC_LIB_SECONDARY_SUFFIX='' ;;
/// esac
/// ```
///
/// macOS is the only platform with a SECONDARY suffix, and dropping it is not
/// cosmetic: module builds there routinely emit `.so` (emacs-libvterm writes
/// `vterm-module.so`), so without it `load` neither finds the file nor
/// recognises it as a module (neomacs#193).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ModuleSuffixes {
    /// GNU `MODULES_SUFFIX`, also the value of `module-file-suffix`.
    pub(crate) primary: &'static str,
    /// GNU `MODULES_SECONDARY_SUFFIX`, absent on every platform but darwin.
    pub(crate) secondary: Option<&'static str>,
}

/// GNU's configure-time `case $opsys`, as a function of the OS so every platform
/// can be tested from any host (`std::env::consts::OS` naming).
pub(crate) fn module_suffixes_for_os(os: &str) -> ModuleSuffixes {
    match os {
        "macos" | "ios" => ModuleSuffixes {
            primary: ".dylib",
            secondary: Some(".so"),
        },
        "windows" => ModuleSuffixes {
            primary: ".dll",
            secondary: None,
        },
        _ => ModuleSuffixes {
            primary: ".so",
            secondary: None,
        },
    }
}

/// The suffixes of the platform this build runs on.
pub(crate) fn module_suffixes() -> ModuleSuffixes {
    module_suffixes_for_os(std::env::consts::OS)
}

pub(crate) fn module_file_suffix() -> &'static str {
    module_suffixes().primary
}

/// The startup value of `load-suffixes` for an OS.
///
/// GNU (syms_of_lread) starts from `(".elc" ".el")`, conses `MODULES_SUFFIX` and
/// then `MODULES_SECONDARY_SUFFIX`, so on darwin the secondary suffix is tried
/// FIRST: `(".so" ".dylib" ".elc" ".el")`.
pub(crate) fn load_suffixes_startup_values_for_os(os: &str) -> Vec<&'static str> {
    let suffixes = module_suffixes_for_os(os);
    let mut values = Vec::with_capacity(4);
    values.extend(suffixes.secondary);
    values.push(suffixes.primary);
    values.push(".elc");
    values.push(".el");
    values
}

/// The startup value of `dynamic-library-suffixes` for an OS.
///
/// GNU conses the secondary suffix unconditionally, so a platform without one
/// still reports an empty string in the list (GNU 31 on GNU/Linux: `(".so" "")`).
pub(crate) fn dynamic_library_suffixes_for_os(os: &str) -> Vec<&'static str> {
    let suffixes = module_suffixes_for_os(os);
    vec![suffixes.primary, suffixes.secondary.unwrap_or("")]
}

/// Whether `path` names a dynamic module on `os` -- GNU's
/// `suffix_p (found, MODULES_SUFFIX) || suffix_p (found, MODULES_SECONDARY_SUFFIX)`
/// (src/lread.c), which decides whether `load` dlopens the file or reads it as
/// Lisp.
pub(crate) fn path_has_module_suffix_for_os(path: &str, os: &str) -> bool {
    let suffixes = module_suffixes_for_os(os);
    path.ends_with(suffixes.primary)
        || suffixes
            .secondary
            .is_some_and(|secondary| path.ends_with(secondary))
}

/// `(locate-file FILENAME PATH &optional SUFFIXES PREDICATE)`
///
/// Search PATH for FILENAME with each suffix in SUFFIXES.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_locate_file(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("locate-file", &args, 2)?;
    expect_max_args("locate-file", &args, 4)?;
    let filename = expect_lisp_string(&args[0])?;
    let path = parse_path_argument(&args[1])?;
    let suffixes = if args.len() > 2 {
        parse_suffixes_argument(&args[2])?
    } else {
        Vec::new()
    };
    let predicate = match args.get(3).copied() {
        Some(predicate) => Some(normalize_locate_file_public_predicate(eval, predicate)?),
        None => None,
    };
    Ok(
        match locate_file_with_path_and_suffixes(
            eval,
            &filename,
            &path,
            &suffixes,
            predicate.as_ref(),
        )? {
            Some(found) => Value::heap_string(found),
            None => Value::NIL,
        },
    )
}

/// `(locate-file-internal FILENAME PATH SUFFIXES &optional PREDICATE)`
///
/// Internal variant of `locate-file`; currently uses the same lookup behavior.
pub(crate) fn builtin_locate_file_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("locate-file-internal", &args, 2)?;
    expect_max_args("locate-file-internal", &args, 4)?;
    let filename = expect_lisp_string(&args[0])?;
    let path = parse_path_argument(&args[1])?;
    // GNU Emacs: SUFFIXES is optional (nil when omitted)
    let suffixes = if args.len() > 2 {
        parse_suffixes_argument(&args[2])?
    } else {
        Vec::new()
    };
    Ok(
        match locate_file_with_path_and_suffixes(eval, &filename, &path, &suffixes, args.get(3))? {
            Some(found) => Value::heap_string(found),
            None => Value::NIL,
        },
    )
}

/// `(read-coding-system PROMPT &optional DEFAULT-CODING-SYSTEM)`
///
/// Faithful port of GNU's `Fread_coding_system` (src/coding.c): binds
/// `completion-ignore-case' to t and delegates to `completing-read', then
/// interns the (non-empty) result.  In batch mode `completing-read' writes the
/// prompt to stdout and signals `end-of-file' on empty stdin, matching GNU.
pub(crate) fn builtin_read_coding_system(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("read-coding-system", &args, 1)?;
    expect_max_args("read-coding-system", &args, 2)?;
    if !args[0].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        ));
    }
    read_coding_system_via_completing_read(eval, &args, false)
}

/// `(read-non-nil-coding-system PROMPT)`
///
/// Like `read-coding-system' but rejects null input.  GNU loops in Lisp until a
/// non-empty coding system is read; in batch mode the underlying
/// `completing-read' signals `end-of-file' before any such check.
pub(crate) fn builtin_read_non_nil_coding_system(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("read-non-nil-coding-system"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    if !args[0].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        ));
    }
    read_coding_system_via_completing_read(eval, &args, true)
}

/// Shared body for `read-coding-system` / `read-non-nil-coding-system`.
fn read_coding_system_via_completing_read(
    eval: &mut super::eval::Context,
    args: &[Value],
    require_match: bool,
) -> EvalResult {
    // GNU passes the symbol-name string of a symbolic DEFAULT-CODING-SYSTEM.
    let default = match args.get(1).copied() {
        Some(value) if value.is_symbol() && !value.is_nil() => Value::string(
            super::intern::resolve_sym(value.as_symbol_id().expect("checked symbol")),
        ),
        Some(value) => value,
        None => Value::NIL,
    };
    let collection = eval
        .eval_symbol("coding-system-alist")
        .unwrap_or(Value::NIL);
    // GNU's `Fread_coding_system` always passes REQUIRE-MATCH = t.
    let _ = require_match;
    let completing_args = vec![
        args[0],
        collection,
        Value::NIL,
        Value::T,
        Value::NIL,
        Value::symbol("coding-system-history"),
        default,
    ];
    // GNU `specbind`s `completion-ignore-case` to t around the call; use the
    // specpdl so the binding is unwound even when `completing-read' signals
    // (e.g. end-of-file on empty stdin in batch mode).
    let count = eval.specpdl.len();
    eval.try_specbind_or_unwind_to(
        count,
        super::intern::intern("completion-ignore-case"),
        Value::T,
    )?;
    let val = super::reader::builtin_completing_read(eval, completing_args);
    let val = eval.unbind_to_with_result(count, val)?;
    let Some(name) = eval.lisp_string(val) else {
        return Ok(Value::NIL);
    };
    if name.schars() == 0 {
        return Ok(Value::NIL);
    }
    Ok(Value::symbol(crate::emacs_core::emacs_char::to_utf8_lossy(
        name.as_bytes(),
    )))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn expect_list(value: &Value) -> Result<Vec<Value>, Flow> {
    list_to_vec(value).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *value],
        )
    })
}

fn parse_path_argument(value: &Value) -> Result<Vec<LispString>, Flow> {
    let mut path = Vec::new();
    let Some(entries) = list_to_vec(value) else {
        return Ok(path);
    };
    for entry in entries {
        match entry.kind() {
            ValueKind::Nil => path.push(LispString::from_unibyte(b".".to_vec())),
            ValueKind::String => path.push(entry.as_lisp_string().expect("checked string").clone()),
            _ => {}
        }
    }
    Ok(path)
}

fn parse_suffixes_argument(value: &Value) -> Result<Vec<LispString>, Flow> {
    let mut suffixes = Vec::new();
    for entry in expect_list(value)? {
        match entry.kind() {
            ValueKind::Nil => suffixes.push(LispString::from_unibyte(Vec::new())),
            ValueKind::String => {
                suffixes.push(entry.as_lisp_string().expect("checked string").clone())
            }
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), entry],
                ));
            }
        }
    }
    Ok(suffixes)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn normalize_locate_file_public_predicate(
    eval: &mut super::eval::Context,
    predicate: Value,
) -> Result<Value, Flow> {
    if predicate.is_nil() {
        return Ok(predicate);
    }

    let functionp = crate::emacs_core::builtins::builtin_functionp_1(eval, predicate)?.is_truthy();
    if matches!(predicate.kind(), ValueKind::Symbol(_)) && !functionp {
        return Ok(access_mask_from_predicate_symbols(&[predicate]));
    }
    if predicate.is_cons()
        && !functionp
        && let Some(items) = list_to_vec(&predicate)
    {
        return Ok(access_mask_from_predicate_symbols(&items));
    }
    Ok(predicate)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn access_mask_from_predicate_symbols(items: &[Value]) -> Value {
    let mut mask = 0;
    for item in items {
        if eq_value(item, &Value::symbol("executable")) {
            mask |= 1;
        }
        if eq_value(item, &Value::symbol("writable")) {
            mask |= 2;
        }
        if eq_value(item, &Value::symbol("readable")) {
            mask |= 4;
        }
    }
    Value::fixnum(mask)
}

fn locate_file_with_path_and_suffixes(
    eval: &mut super::eval::Context,
    filename: &LispString,
    path: &[LispString],
    suffixes: &[LispString],
    predicate: Option<&Value>,
) -> Result<Option<LispString>, Flow> {
    let effective_suffixes: Vec<LispString> = if suffixes.is_empty() {
        vec![LispString::from_unibyte(Vec::new())]
    } else {
        suffixes.to_vec()
    };

    let absolute = matches!(filename.as_bytes().first(), Some(b'/') | Some(b'~'));
    if absolute || path.is_empty() {
        let expanded = locate_file_expand_name(eval, filename, None)?;
        for suffix in &effective_suffixes {
            let candidate_lisp = append_lisp_file_name_suffix(&expanded, suffix);
            if candidate_matches_openp(eval, predicate, &candidate_lisp)? {
                return Ok(Some(candidate_lisp));
            }
        }
        return Ok(None);
    }

    for dir in path {
        let base = locate_file_expand_name(eval, filename, Some(dir))?;
        for suffix in &effective_suffixes {
            let candidate_lisp = append_lisp_file_name_suffix(&base, suffix);
            if candidate_matches_openp(eval, predicate, &candidate_lisp)? {
                return Ok(Some(candidate_lisp));
            }
        }
    }

    Ok(None)
}

fn locate_file_expand_name(
    eval: &mut super::eval::Context,
    name: &LispString,
    default_dir: Option<&LispString>,
) -> Result<LispString, Flow> {
    let mut args = vec![Value::heap_string(name.clone())];
    if let Some(dir) = default_dir {
        args.push(Value::heap_string(dir.clone()));
    }
    let expanded = crate::emacs_core::fileio::builtin_expand_file_name(eval, args)?;
    Ok(expanded
        .as_lisp_string()
        .expect("expand-file-name should return a string")
        .clone())
}

fn append_lisp_file_name_suffix(base: &LispString, suffix: &LispString) -> LispString {
    let mut bytes = base.as_bytes().to_vec();
    bytes.extend_from_slice(suffix.as_bytes());
    if base.is_multibyte() || suffix.is_multibyte() {
        LispString::from_emacs_bytes(bytes)
    } else {
        LispString::from_unibyte(bytes)
    }
}

fn candidate_matches_openp(
    eval: &mut super::eval::Context,
    predicate: Option<&Value>,
    candidate: &LispString,
) -> Result<bool, Flow> {
    let Some(predicate) = predicate else {
        return Ok(readable_non_directory_candidate(candidate));
    };
    if predicate.is_nil() {
        return Ok(readable_non_directory_candidate(candidate));
    }
    if predicate.is_t() {
        return Ok(readable_non_directory_candidate(candidate));
    }

    if let Some(mask) = predicate.as_fixnum() {
        return Ok(integer_access_predicate_matches(candidate, mask));
    }

    let result = eval.funcall_general(*predicate, vec![Value::heap_string(candidate.clone())])?;
    if result.is_nil() {
        return Ok(false);
    }
    if eq_value(&result, &Value::symbol("dir-ok")) {
        return Ok(true);
    }
    Ok(!candidate_is_directory(candidate))
}

fn readable_non_directory_candidate(candidate: &LispString) -> bool {
    let path = crate::emacs_core::fileio::lisp_file_name_to_path_buf(candidate);
    match std::fs::File::open(&path).and_then(|file| file.metadata()) {
        Ok(meta) => !meta.is_dir(),
        Err(_) => false,
    }
}

fn candidate_is_directory(candidate: &LispString) -> bool {
    let path = crate::emacs_core::fileio::lisp_file_name_to_path_buf(candidate);
    std::fs::metadata(path).is_ok_and(|meta| meta.is_dir())
}

fn integer_access_predicate_matches(candidate: &LispString, mask: i64) -> bool {
    let path = crate::emacs_core::fileio::lisp_file_name_to_path_buf(candidate);
    if std::fs::metadata(&path).is_ok_and(|meta| meta.is_dir()) {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;

        let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            return false;
        };
        let mut mode = 0;
        if (mask & 1) != 0 {
            mode |= libc::X_OK;
        }
        if (mask & 2) != 0 {
            mode |= libc::W_OK;
        }
        if (mask & 4) != 0 {
            mode |= libc::R_OK;
        }
        unsafe { libc::access(c_path.as_ptr(), mode) == 0 }
    }
    #[cfg(not(unix))]
    {
        let meta = match std::fs::metadata(path) {
            Ok(meta) => meta,
            Err(_) => return false,
        };
        if (mask & 2) != 0 && meta.permissions().readonly() {
            return false;
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
