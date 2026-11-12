//! Minibuffer read entry points (GNU `minibuf.c` `read_minibuf` and friends).
//!
//! `read-from-minibuffer`, `read-string`, `completing-read`, `read` and the
//! yes-or-no family bind minibuffer/terminal behaviour, so they are R0
//! command-layer entries (STAGE4 design §5) and live in core.  They read
//! evaluator state through `&mut Context` and call the display-side minibuffer
//! window machinery downward; the reader's pure parse machinery stays in eval.

use crate::buffer::{EmacsBytePos, EmacsByteRange};
use crate::emacs_core::error::{
    EvalResult, Flow, FlowKind, FlowResultExt, LispCondition, expect_args, expect_max_args,
    expect_min_args, signal,
};
use crate::emacs_core::eval::{Context, VariableView};
use crate::emacs_core::intern::{intern, resolve_sym};
use crate::emacs_core::keyboard::CommandLoopHost;
use crate::emacs_core::minibuffer::MinibufferManagerExt;
use std::io::Write;

use strum::{EnumString, IntoStaticStr};

use super::window_state::{
    activate_minibuffer_window, find_or_create_minibuffer_buffer_in_state,
    install_minibuffer_ambient_directory_in_state, minibuffer_ambient_directory_in_state,
    record_active_minibuffer_selection, run_minibuffer_mode_if_bound,
};
use crate::emacs_core::reader::{
    CallingFrame, KeyboardInputRuntime, MinibufferBufferUnwind, MinibufferExitDisposition,
    MinibufferInputSource, MinibufferSessionUnwind, ReadSourceObject,
    add_minibuffer_history_after_unwind, current_read_symbol_shorthands, expect_lisp_string,
    finish_minibuffer_result_after_unwind, history_add_new_input_enabled,
    initialize_unbound_minibuffer_history, minibuffer_history_entry, minibuffer_history_spec,
    read_from_active_load_cursor, read_from_callable, read_from_string_impl_inner,
    signal_reader_error_from_buffer, signal_reader_error_from_string, stdin_end_of_file_error,
};
use crate::emacs_core::value::{Value, ValueKind, VecLikeType};
use crate::emacs_core::value_reader::SymbolPositionMode;

use super::{MinibufferEntryRejection, RecursiveMinibufferPolicy};
use neovm_edit::emacs_core::edit_host::EditHost;

fn reader_initial_contents(
    value: Option<&Value>,
) -> Result<Option<super::MinibufferInitialContents>, Flow> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value.kind() {
        ValueKind::Nil => Ok(None),
        ValueKind::String => Ok(value
            .as_lisp_string()
            .cloned()
            .map(super::MinibufferInitialContents::at_end)),
        ValueKind::Cons => {
            let text_value = value.cons_car();
            let text = text_value.as_lisp_string().cloned().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), text_value],
                )
            })?;
            let position_value = value.cons_cdr();
            if position_value.is_nil() {
                return Ok(Some(super::MinibufferInitialContents::at_end(text)));
            }
            let position = position_value.as_fixnum().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("fixnump"), position_value],
                )
            })?;
            let cursor = usize::try_from(position.saturating_sub(1).max(0))
                .map(crate::buffer::CharPos0::new)
                .map_err(|_| signal(LispCondition::EndOfBuffer, vec![]))?;
            super::MinibufferInitialContents::at_character_offset(text, cursor)
                .map(Some)
                .ok_or_else(|| signal(LispCondition::EndOfBuffer, vec![]))
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum RequireMatchSymbol {
    #[strum(to_string = "t")]
    T,
    Confirm,
    ConfirmAfterCompletion,
}

impl RequireMatchSymbol {
    fn from_lisp_value(value: Value) -> Option<Self> {
        value
            .as_bare_symbol_name()
            .and_then(|name| name.parse().ok())
    }
}

fn minibuffer_text_properties_enabled_in_buffer(
    vars: VariableView<'_>,
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
) -> bool {
    let option = intern("minibuffer-allow-text-properties");
    buffer_id
        .and_then(|buffer_id| buffers.get(buffer_id))
        .and_then(|buffer| buffer.get_buffer_local_by_sym_id(option))
        // A localized symbol's ordinary value cell may be cached for a
        // different buffer.  The absence of a local binding means its
        // defcell is authoritative, just as GNU's BLV lookup specifies.
        .or_else(|| vars.default_value(option))
        .is_some_and(|value| value.is_truthy())
}

fn expect_initial_input_stringish(value: &Value) -> Result<(), Flow> {
    reader_initial_contents(Some(value)).map(drop)
}

fn expect_completing_read_initial_input(value: &Value) -> Result<(), Flow> {
    match value.kind() {
        ValueKind::Nil | ValueKind::String => Ok(()),
        ValueKind::Cons => {
            let pair_car = value.cons_car();
            let pair_cdr = value.cons_cdr();
            if !pair_car.is_string() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), pair_car],
                ));
            }
            if !(pair_cdr.is_fixnum() || pair_cdr.as_char().is_some()) {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("number-or-marker-p"), pair_cdr],
                ));
            }
            Ok(())
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

/// Semantic result of the recursive command loop.  The command loop's
/// low-level `Flow::Throw` representation must not leak into lifecycle code:
/// only `(throw 'exit nil)` is an accepted minibuffer read; every other flow
/// is an abort that must be propagated after cleanup.
enum MinibufferCommandOutcome {
    Accepted,
    Aborted(Flow),
}

impl MinibufferCommandOutcome {
    fn from_recursive_edit(result: EvalResult) -> Self {
        let result = result.kinded();
        match result {
            Ok(_) => Self::Accepted,
            Err(FlowKind::Throw(ref thrown))
                if thrown.tag.is_bare_symbol_named("exit") && !thrown.value.is_truthy() =>
            {
                Self::Accepted
            }
            Err(flow) => Self::Aborted(Flow::from_kind(flow)),
        }
    }

    fn disposition(&self) -> MinibufferExitDisposition {
        match self {
            Self::Accepted => MinibufferExitDisposition::Accepted,
            Self::Aborted(_) => MinibufferExitDisposition::Aborted,
        }
    }
}

/// `(read &optional STREAM)`
///
/// Read one Lisp expression from STREAM.
/// - If STREAM is a string, read from that string (equivalent to car of read-from-string).
/// - If STREAM is nil, read from `standard-input`.
/// - If STREAM is a buffer, read from buffer at point.
/// - If STREAM is a marker, read from its buffer and advance only the marker.
/// - Any other object is a READCHARFUN: call it with no arguments for each
///   character, and with one character to push that character back
///   (GNU `src/lread.c:323-327`, `486-496`).
pub fn builtin_read(ctx: &mut crate::emacs_core::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_read_impl(ctx, args, false)
}

/// Closed dispatch plan for Lisp reader streams after nil has resolved through
/// `standard-input`.  Keeping the opaque load token as its own variant makes
/// that privileged route identity-only; an ordinary symbol can never reach it
/// merely because its printed name happens to match.
#[derive(Clone, Copy, Debug)]
enum ResolvedReadStream {
    Empty,
    String(Value),
    Marker(Value),
    Buffer(crate::buffer::BufferId),
    ActiveLoadCursor,
    Minibuffer,
    /// GNU `init_source` (`src/lread.c:323-327`): anything that is not a
    /// buffer, marker, string, or the load-file sentinel is called as a
    /// READCHARFUN. A non-function signals from that call (`void-function`,
    /// `invalid-function`), not from stream classification.
    Callable(Value),
}

impl ResolvedReadStream {
    fn classify(stream: Value, load_token: crate::emacs_core::eval::LoadReadStreamToken) -> Self {
        match stream.kind() {
            ValueKind::Nil => Self::Empty,
            ValueKind::T => Self::Minibuffer,
            ValueKind::String => Self::String(stream),
            ValueKind::Symbol(symbol) if load_token.identifies(symbol) => Self::ActiveLoadCursor,
            // GNU `Fread` (`src/lread.c:2474-2481`) rewrites `t` to the
            // symbol `read-char`, then that symbol (and only that symbol)
            // selects the minibuffer. An uninterned or other symbol is a
            // normal READCHARFUN.
            ValueKind::Symbol(symbol) if symbol == intern("read-char") => Self::Minibuffer,
            ValueKind::Veclike(VecLikeType::Marker) => Self::Marker(stream),
            ValueKind::Veclike(VecLikeType::Buffer) => {
                Self::Buffer(stream.as_buffer_id().expect("buffer value kind"))
            }
            ValueKind::Symbol(_)
            | ValueKind::Fixnum(_)
            | ValueKind::Cons
            | ValueKind::Float
            | ValueKind::Subr(_)
            | ValueKind::Veclike(_)
            | ValueKind::Unbound
            | ValueKind::Unknown => Self::Callable(stream),
        }
    }
}

/// Shared implementation for `read` and `read-positioning-symbols`.
/// In positioned mode, every interned symbol (except nil) is
/// wrapped in a `symbol-with-pos` object carrying GNU's read-character offset.
pub fn builtin_read_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
    locate_syms: bool,
) -> EvalResult {
    let symbol_position_mode = SymbolPositionMode::from(locate_syms);
    expect_max_args("read", &args, 1)?;

    let stream = if args.is_empty() || args[0].is_nil() {
        ctx.builtin_var_value_by_name("standard-input")
            .unwrap_or(Value::NIL)
    } else {
        args[0]
    };

    let shorthands = current_read_symbol_shorthands(ctx);
    match ResolvedReadStream::classify(stream, ctx.load_read_stream_token) {
        ResolvedReadStream::Empty => {
            // In batch/non-interactive runs, stdin-backed read signals EOF.
            Err(signal(
                LispCondition::EndOfFile,
                vec![Value::string("End of file during parsing")],
            ))
        }
        ResolvedReadStream::String(stream) => {
            // Read from string
            let result = read_from_string_impl_inner(
                ctx.variables(),
                vec![stream],
                symbol_position_mode,
                shorthands.as_ref(),
                ctx.symbol_mode(),
            )?;
            // Return just the car (the parsed object)
            match result.kind() {
                ValueKind::Cons => {
                    let pair_car = result.cons_car();
                    ctx.obarray_mut().materialize_read_symbols(pair_car);
                    Ok(pair_car)
                }
                _ => Ok(result),
            }
        }
        ResolvedReadStream::Marker(stream) => {
            let Some((Some(buf_id), Some(position), _)) =
                crate::emacs_core::marker::marker_logical_fields(&stream)
            else {
                return Err(ReadSourceObject::Anonymous.end_of_file_error());
            };

            let (read_result, new_position) = {
                let buf = ctx
                    .edit
                    .buffers
                    .get(buf_id)
                    .ok_or_else(|| ReadSourceObject::Anonymous.end_of_file_error())?;
                let start = buf.char_pos_to_emacs_byte_pos_clamped(position.to_char_pos());
                let end = buf.accessible_emacs_byte_region().end();
                if start >= end {
                    return Err(ReadSourceObject::Anonymous.end_of_file_error());
                }

                match crate::emacs_core::value_reader::read_one_from_buffer_with_locate_syms_with_symbols(
                    buf,
                    EmacsByteRange::new(start, end),
                    crate::emacs_core::value_reader::BufferReadcharOffsetOrigin::Zero,
                    symbol_position_mode,
                    ctx.variables(),
                    shorthands.as_ref(),
                    ctx.symbol_mode(),
                ) {
                    Ok((value, new_byte_position)) => (
                        Ok(value),
                        buf.emacs_byte_pos_to_lisp_char_pos(new_byte_position),
                    ),
                    Err(error) => {
                        // GNU's source_marker_get/unget updates the marker as
                        // each character is consumed, including on a reader
                        // error.  The value reader reports that consumed byte
                        // position, so publish it before propagating the
                        // matching Lisp signal.
                        let new_byte_position =
                            crate::buffer::EmacsBytePos::new(error.position).min(end);
                        let new_position = buf.emacs_byte_pos_to_lisp_char_pos(new_byte_position);
                        // A marker identifies its cursor but GNU does not add
                        // buffer line/column context to reader signals for it.
                        // Keep marker errors source-less, like string streams.
                        (Err(signal_reader_error_from_string(error)), new_position)
                    }
                }
            };

            crate::emacs_core::marker::builtin_set_marker_in_buffers(
                &mut ctx.edit.buffers,
                &[
                    stream,
                    Value::fixnum(new_position.as_i64()),
                    Value::make_buffer(buf_id),
                ],
            )?;
            let value =
                read_result?.ok_or_else(|| ReadSourceObject::Anonymous.end_of_file_error())?;
            ctx.obarray_mut().materialize_read_symbols(value);
            Ok(value)
        }
        ResolvedReadStream::Buffer(buf_id) => {
            // GNU reads a buffer stream through `readchar`, which advances
            // point as it consumes.  A reader error therefore leaves point
            // where reading STOPPED, not where it started -- `end-of-file` on
            // an unterminated form lands at point-max, and a bad token lands on
            // the offending character.  Callers depend on that: the semantic
            // elisp fontifier runs
            //   (while (< (point) end) (ignore-errors (read ...)))
            // over a buffer that is routinely incomplete mid-edit, and makes
            // progress only because the failed read still moved point.
            // Restoring point on error spun that loop forever (issue #375).
            let outcome = {
                let buf =
                    ctx.edit.buffers.get(buf_id).ok_or_else(|| {
                        signal("error", vec![Value::string("Buffer does not exist")])
                    })?;

                let start = buf.point_emacs_byte_pos();
                let end = buf.accessible_emacs_byte_region().end();
                if start >= end {
                    return Err(
                        ReadSourceObject::Buffer(Value::make_buffer(buf_id)).end_of_file_error()
                    );
                }

                match crate::emacs_core::value_reader::read_one_from_buffer_with_locate_syms_with_symbols(
                        buf,
                        EmacsByteRange::new(start, end),
                        crate::emacs_core::value_reader::BufferReadcharOffsetOrigin::BufferPoint,
                        symbol_position_mode,
                        ctx.variables(),
                        shorthands.as_ref(),
                        ctx.symbol_mode(),
                    ) {
                        Ok(result) => Ok(result),
                        Err(e) => {
                            // Every `ReadError` records `position: self.pos`, the
                            // reader's own stop offset, in the same absolute Emacs
                            // byte space the success path hands to `goto`.  Clamped
                            // to the accessible end like the marker arm above, so a
                            // stop offset at the boundary cannot land past it.
                            let stopped_at = EmacsBytePos::new(e.position).min(end);
                            Err((e, stopped_at))
                        }
                    }
            };

            let (maybe_value, new_pt) = match outcome {
                Ok(result) => result,
                Err((error, stopped_at)) => {
                    let _ = &mut ctx
                        .edit
                        .buffers
                        .goto_buffer_emacs_byte_pos(buf_id, stopped_at);
                    return Err(signal_reader_error_from_buffer(ctx, buf_id, error));
                }
            };

            let _ = &mut ctx.edit.buffers.goto_buffer_emacs_byte_pos(buf_id, new_pt);
            let value = maybe_value.ok_or_else(|| {
                ReadSourceObject::Buffer(Value::make_buffer(buf_id)).end_of_file_error()
            })?;
            ctx.obarray_mut().materialize_read_symbols(value);
            Ok(value)
        }
        ResolvedReadStream::ActiveLoadCursor => {
            read_from_active_load_cursor(ctx, symbol_position_mode)
        }
        ResolvedReadStream::Callable(stream) => {
            read_from_callable(ctx, stream, symbol_position_mode)
        }
        ResolvedReadStream::Minibuffer => {
            // GNU `Fread` (lread.c): a `t` stream -- including the batch default
            // `standard-input` = t reached by `(read)` with no argument -- maps
            // to `(read-minibuffer "Lisp expression: ")`: read one line (from the
            // minibuffer interactively, or from stdin in `--batch`, prompt and
            // all) and parse it as a single Lisp expression. neomacs previously
            // signaled `end-of-file` outright, so a piped `echo '(+ 1 2)' |
            // neomacs --batch --eval '(print (read))'` couldn't read its input.
            let prompt = Value::string("Lisp expression: ");
            let input = builtin_read_from_minibuffer(ctx, vec![prompt])?;
            let result = read_from_string_impl_inner(
                ctx.variables(),
                vec![input],
                symbol_position_mode,
                shorthands.as_ref(),
                ctx.symbol_mode(),
            )?;
            match result.kind() {
                ValueKind::Cons => {
                    let pair_car = result.cons_car();
                    ctx.obarray_mut().materialize_read_symbols(pair_car);
                    Ok(pair_car)
                }
                _ => Ok(result),
            }
        }
    }
}

/// `(read-from-minibuffer PROMPT &optional INITIAL KEYMAP READ HIST DEFAULT INHERIT-INPUT-METHOD)`
///
/// Read a string from the minibuffer.
/// In interactive mode, sets up the minibuffer buffer, enters recursive-edit,
/// and returns the user's input when they press RET (exit-minibuffer).
/// In batch mode, reads one line from standard input and applies the same
/// READ/DEFAULT result processing as an interactive minibuffer read.
pub(crate) fn builtin_read_from_minibuffer(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = builtin_read_from_minibuffer_in_runtime(eval, &args)? {
        return Ok(result);
    }
    finish_read_from_minibuffer_in_eval(eval, &args)
}

/// GNU `barf_if_interaction_inhibited` (src/minibuf.c): signal `inhibited-interaction`, without
/// data, when `inhibit-interaction` is non-nil.  GNU calls it before anything else in
/// `read-from-minibuffer` and the `read-char` family, so it also stops batch reads from stdin.
pub(crate) fn barf_if_interaction_inhibited(eval: &Context) -> Result<(), Flow> {
    if eval
        .builtin_var_value_by_name("inhibit-interaction")
        .is_some_and(|v| v.is_truthy())
    {
        return Err(signal("inhibited-interaction", vec![]));
    }
    Ok(())
}

fn read_from_stdin_noninteractive(prompt: &str) -> EvalResult {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => Err(stdin_end_of_file_error()),
        Ok(_) => {
            let input = line.trim_end_matches(['\n', '\r']);
            Ok(Value::string(input))
        }
        Err(_) => Err(stdin_end_of_file_error()),
    }
}

/// GNU's `read-minibuffer-restore-windows` policy, made exhaustive at the
/// runtime boundary shared by interpreted and byte-compiled minibuffer reads.
///
/// GNU `read_minibuf` installs the saved configuration as an unwind action, so
/// it runs after `minibuffer-exit-hook` and minibuffer teardown for normal
/// return, `C-g`, and errors alike.  Keeping this as an enum prevents a caller
/// from representing “restoration requested but no configuration captured”.
#[derive(Clone, Copy, Debug)]
enum MinibufferWindowRestoration {
    KeepChanges,
    Restore(MinibufferWindowRestorationPlan),
}

/// The one- or two-frame restore stack installed by GNU `read_minibuf`.
/// Caller is recorded first and the separate minibuffer-owner frame second,
/// so specpdl LIFO restores the owner frame before the caller configuration.
#[derive(Clone, Copy, Debug)]
struct MinibufferWindowRestorationPlan {
    caller: crate::emacs_core::builtins::SavedWindowConfiguration,
    minibuffer_owner: Option<crate::emacs_core::builtins::SavedWindowConfiguration>,
}

/// GNU's `read_minibuffer_restore_windows` -- the `DEFVAR_BOOL` cell
/// (`src/minibuf.c:2706`) that `read_minibuf` dereferences at
/// `src/minibuf.c:695` and `:702`.
///
/// A named predicate rather than an inline read because GNU's C reads one
/// `bool` global in two places, and because the swap-in
/// (`src/data.c:1573-1603`) means that global is the *current buffer's*
/// binding whenever a buffer has localised it (ledger 196).
pub(crate) fn minibuffer_restore_windows_requested(eval: &Context) -> bool {
    eval.variables()
        .value_in_buffer_by_name(
            eval.edit.buffers.current_buffer(),
            "read-minibuffer-restore-windows",
        )
        .is_some_and(|value| value.is_truthy())
}

impl MinibufferWindowRestoration {
    fn capture(eval: &mut Context) -> Result<Self, Flow> {
        if !minibuffer_restore_windows_requested(eval) {
            return Ok(Self::KeepChanges);
        }

        let caller =
            crate::emacs_core::builtins::SavedWindowConfiguration::capture(eval, Value::NIL)?;
        let selected_frame = eval.frame_manager().selected_frame().map(|frame| frame.id);
        let minibuffer_owner = eval
            .frame_manager()
            .selected_frame()
            .and_then(|frame| frame.minibuffer_window)
            .and_then(|window| eval.frame_manager().find_window_frame_id(window))
            .filter(|owner| Some(*owner) != selected_frame)
            .map(|owner| {
                crate::emacs_core::builtins::SavedWindowConfiguration::capture(
                    eval,
                    Value::make_frame(owner.0),
                )
            })
            .transpose()?;
        Ok(Self::Restore(MinibufferWindowRestorationPlan {
            caller,
            minibuffer_owner,
        }))
    }

    fn record(&self, eval: &mut Context) {
        if let Self::Restore(plan) = *self {
            eval.record_native_unwind(
                crate::emacs_core::eval::NativeUnwindAction::RestoreWindowConfiguration {
                    configuration: plan.caller,
                    options: crate::emacs_core::builtins::WindowConfigurationRestoreOptions {
                        selected_frame:
                            crate::emacs_core::builtins::SelectedFrameRestoration::KeepSelected,
                        minibuffer_window:
                            crate::emacs_core::builtins::MinibufferWindowRestoration::KeepCurrent,
                    },
                },
            );
            if let Some(configuration) = plan.minibuffer_owner {
                eval.record_native_unwind(
                    crate::emacs_core::eval::NativeUnwindAction::RestoreWindowConfiguration {
                        configuration,
                        options: crate::emacs_core::builtins::WindowConfigurationRestoreOptions {
                            selected_frame: crate::emacs_core::builtins::SelectedFrameRestoration::RestoreSaved,
                            minibuffer_window:
                                crate::emacs_core::builtins::MinibufferWindowRestoration::KeepCurrent,
                        },
                    },
                );
            }
        }
    }
}

/// Everything GNU captures at the boundary of one interactive read.
///
/// Keeping the caller identity beside the configuration stack makes the
/// required post-unwind frame selection impossible to forget or accidentally
/// apply before the configurations have been restored.
#[derive(Clone, Copy, Debug)]
struct MinibufferInvocationRestoration {
    calling_frame: Option<CallingFrame>,
    windows: MinibufferWindowRestoration,
}

impl MinibufferInvocationRestoration {
    fn capture(eval: &mut Context) -> Result<Self, Flow> {
        Ok(Self {
            calling_frame: eval
                .frame_manager()
                .selected_frame()
                .map(|frame| CallingFrame(frame.id)),
            windows: MinibufferWindowRestoration::capture(eval)?,
        })
    }

    fn record(&self, eval: &mut Context) {
        self.windows.record(eval);
    }

    fn select_calling_frame(&self, eval: &mut Context) -> EvalResult {
        // GNU `read_minibuf` explicitly reselects the invoking frame after
        // `unbind_to` has restored the owner/caller configuration stack.  The
        // restore options intentionally keep the then-current selected frame,
        // so this final step is distinct from configuration restoration.
        if let Some(calling_frame) = self.calling_frame
            && eval.frame_manager().get(calling_frame.0).is_some()
        {
            super::redisplay::publish_frame_switch(eval, calling_frame.0);
            let _ = eval.frame_manager_mut().select_frame(calling_frame.0);
        }
        crate::emacs_core::frame::sync_gui_frame_focus_redirects(eval)
    }
}

pub(crate) fn finish_read_from_minibuffer_in_eval(
    eval: &mut Context,
    args: &[Value],
) -> EvalResult {
    finish_read_from_minibuffer_in_eval_with_setup(eval, args, |_| Ok(Value::NIL))
}

pub(crate) fn finish_read_from_minibuffer_in_eval_with_setup(
    eval: &mut Context,
    args: &[Value],
    run_before_setup_hook: impl FnMut(&mut Context) -> EvalResult,
) -> EvalResult {
    // There is one production lifecycle implementation for interpreted and
    // byte-compiled callers.  Keeping the evaluator path as a thin adapter
    // prevents the two paths from acquiring different early-return holes.
    finish_read_from_minibuffer_in_vm_runtime_interactive(eval, args, run_before_setup_hook)
}

pub(crate) fn builtin_read_from_minibuffer_in_runtime(
    runtime: &mut Context,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    barf_if_interaction_inhibited(runtime)?;
    expect_min_args("read-from-minibuffer", args, 1)?;
    expect_max_args("read-from-minibuffer", args, 7)?;
    let prompt = expect_lisp_string(&args[0])?;
    if let Some(initial) = args.get(1) {
        expect_initial_input_stringish(initial)?;
    }

    match runtime.minibuffer_input_source() {
        MinibufferInputSource::CommandLoop => Ok(None),
        MinibufferInputSource::StandardInput => {
            let result = read_from_stdin_noninteractive(
                &crate::emacs_core::emacs_char::to_utf8_lossy(prompt.as_bytes()),
            )?;
            let result = expect_lisp_string(&result)?;
            let read = args.get(3).copied().unwrap_or(Value::NIL);
            let default = args.get(5).copied().unwrap_or(Value::NIL);
            finish_minibuffer_result_after_unwind(runtime, result, read, default).map(Some)
        }
    }
}

// ---------------------------------------------------------------------------
// 6. read-string
// ---------------------------------------------------------------------------

/// `(read-string PROMPT &optional INITIAL HISTORY DEFAULT INHERIT-INPUT-METHOD)`
///
/// Read a string from the minibuffer.  Delegates to `read-from-minibuffer`.
pub(crate) fn builtin_read_string(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = builtin_read_string_in_runtime(eval, &args)? {
        return Ok(result);
    }
    finish_read_string_in_eval(eval, &args)
}

pub(crate) fn finish_read_string_in_eval(eval: &mut Context, args: &[Value]) -> EvalResult {
    finish_read_string_with_minibuffer(args, |minibuffer_args| {
        finish_read_from_minibuffer_in_eval(eval, minibuffer_args)
    })
}

pub(crate) fn builtin_read_string_in_runtime(
    runtime: &Context,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    expect_min_args("read-string", args, 1)?;
    expect_max_args("read-string", args, 5)?;
    let prompt = args[0];
    if let Some(initial) = args.get(1) {
        expect_initial_input_stringish(initial)?;
    }

    match runtime.minibuffer_input_source() {
        MinibufferInputSource::CommandLoop => Ok(None),
        MinibufferInputSource::StandardInput => {
            barf_if_interaction_inhibited(runtime)?;
            let prompt_str = expect_lisp_string(&prompt)?;
            read_from_stdin_noninteractive(&crate::emacs_core::emacs_char::to_utf8_lossy(
                prompt_str.as_bytes(),
            ))
            .map(Some)
        }
    }
}

pub(crate) fn finish_read_string_with_minibuffer(
    args: &[Value],
    mut read_from_minibuffer: impl FnMut(&[Value]) -> EvalResult,
) -> EvalResult {
    let prompt = args[0];

    // (read-from-minibuffer PROMPT INITIAL nil nil HIST DEFAULT INHERIT-INPUT-METHOD)
    let initial = args.get(1).copied().unwrap_or(Value::NIL);
    let history = args.get(2).copied().unwrap_or(Value::NIL);
    let default = args.get(3).copied().unwrap_or(Value::NIL);
    let inherit = args.get(4).copied().unwrap_or(Value::NIL);

    let minibuffer_args = [
        prompt,
        initial,
        Value::NIL,
        Value::NIL,
        history,
        default,
        inherit,
    ];
    read_from_minibuffer(&minibuffer_args)
}

// ---------------------------------------------------------------------------
// 7. read-number -- NOT here
// ---------------------------------------------------------------------------
//
// `read-number' is `(defun read-number (prompt &optional default hist) ...)'
// at lisp/subr.el:3725, over `read-from-minibuffer'.  GNU has no C version,
// and the one place C reaches it -- the `n' and `N' interactive code letters
// -- goes through the FUNCTION CELL: `calln (Qread_number, callint_message)',
// src/callint.c:645.  `interactive.rs' does the same
// (`read_number_through_the_function_cell'), so nothing in Rust needs a
// `read-number' of its own (DIVERGENCES.md 152).

// ---------------------------------------------------------------------------
// 8. completing-read
// ---------------------------------------------------------------------------

const COMPLETING_READ_MAX_ARGS: usize = 8;

/// `(completing-read PROMPT COLLECTION &optional PREDICATE REQUIRE-MATCH
///                    INITIAL-INPUT HIST DEF INHERIT-INPUT-METHOD)`
///
/// Read a string from the minibuffer with completion.
/// In interactive mode, delegates to read-from-minibuffer with
/// minibuffer-local-completion-map (or minibuffer-local-must-match-map
/// if REQUIRE-MATCH is non-nil).
pub(crate) fn builtin_completing_read(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    validate_completing_read_arity(&args)?;
    if let Some(function) = completing_read_function_value(eval) {
        return eval.apply(function, completing_read_function_args(args));
    }

    if let Some(result) = builtin_completing_read_in_runtime(eval, &args)? {
        return Ok(result);
    }
    finish_completing_read_in_eval(eval, &args)
}

pub(crate) fn finish_completing_read_in_eval(eval: &mut Context, args: &[Value]) -> EvalResult {
    let minibuffer_args = completing_read_minibuffer_args(eval.variables(), args);
    let collection = args[1];
    let predicate = args.get(2).copied().unwrap_or(Value::NIL);
    let require_match = args.get(3).copied().unwrap_or(Value::NIL);
    let original_buffer = eval
        .edit
        .buffers
        .current_buffer_id()
        .map(Value::make_buffer)
        .unwrap_or(Value::NIL);
    let completion_ignore_case = eval
        .eval_symbol("completion-ignore-case")
        .unwrap_or(Value::NIL);

    finish_read_from_minibuffer_in_eval_with_setup(eval, &minibuffer_args, move |eval| {
        install_completing_read_minibuffer_locals(
            eval,
            collection,
            predicate,
            require_match,
            original_buffer,
            completion_ignore_case,
        );
        Ok(Value::NIL)
    })
}

pub(crate) fn builtin_completing_read_in_runtime(
    runtime: &Context,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    validate_completing_read_arity(args)?;
    let prompt = expect_lisp_string(&args[0])?;
    if let Some(initial) = args.get(4) {
        expect_completing_read_initial_input(initial)?;
    }

    match runtime.minibuffer_input_source() {
        MinibufferInputSource::CommandLoop => Ok(None),
        MinibufferInputSource::StandardInput => {
            barf_if_interaction_inhibited(runtime)?;
            // Batch/noninteractive: GNU's `Fcompleting_read` routes through
            // `read_minibuf` -> `read_minibuf_noninteractive` (minibuf.c), which
            // writes the prompt to stdout and reads the answer from stdin, exactly
            // like `read-from-minibuffer`.  Mirror that so the prompt is emitted
            // before the (likely) end-of-file signal on empty stdin.
            read_from_stdin_noninteractive(&crate::emacs_core::emacs_char::to_utf8_lossy(
                prompt.as_bytes(),
            ))
            .map(Some)
        }
    }
}

pub(crate) fn validate_completing_read_arity(args: &[Value]) -> Result<(), Flow> {
    expect_min_args("completing-read", args, 2)?;
    expect_max_args("completing-read", args, COMPLETING_READ_MAX_ARGS)?;
    Ok(())
}

fn completing_read_function_args(mut args: Vec<Value>) -> Vec<Value> {
    // GNU's fixed-arity Fcompleting_read receives nil for omitted optional
    // arguments, then calls completing-read-function with all eight values.
    // Keep that adapter interface stable even though Neomacs builtins receive
    // only the arguments supplied by the Lisp caller.
    args.resize(COMPLETING_READ_MAX_ARGS, Value::NIL);
    args
}

pub(crate) fn completing_read_function_value(eval: &Context) -> Option<Value> {
    eval.eval_symbol("completing-read-function")
        .ok()
        .filter(|function| !function.is_nil())
}

pub(crate) fn finish_read_from_minibuffer_in_vm_runtime(
    shared: &mut Context,
    args: &[Value],
) -> EvalResult {
    finish_read_from_minibuffer_in_vm_runtime_with_setup(shared, args, |_| Ok(Value::NIL))
}

fn finish_read_from_minibuffer_in_vm_runtime_with_setup(
    shared: &mut Context,
    args: &[Value],
    run_before_setup_hook: impl FnMut(&mut Context) -> EvalResult,
) -> EvalResult {
    if let Some(result) = builtin_read_from_minibuffer_in_runtime(shared, args)? {
        return Ok(result);
    }

    finish_read_from_minibuffer_in_vm_runtime_interactive(shared, args, run_before_setup_hook)
}

fn finish_read_from_minibuffer_in_vm_runtime_interactive(
    shared: &mut Context,
    args: &[Value],
    mut run_before_setup_hook: impl FnMut(&mut Context) -> EvalResult,
) -> EvalResult {
    // `read-string`, `completing-read` and `yes-or-no-p` reach the command-loop
    // minibuffer here without `builtin_read_from_minibuffer_in_runtime`'s check.
    barf_if_interaction_inhibited(shared)?;
    let prompt = expect_lisp_string(&args[0])?;
    let prompt_display = crate::emacs_core::emacs_char::to_utf8_lossy(prompt.as_bytes());
    let initial_input = reader_initial_contents(args.get(1))?;
    let keymap_arg = args.get(2).copied().unwrap_or(Value::NIL);
    let read_arg = args.get(3).copied().unwrap_or(Value::NIL);
    let history_spec = minibuffer_history_spec(args.get(4));
    let default_val = args.get(5).copied().unwrap_or(Value::NIL);

    let result = shared.with_unwind_scope(|shared| {
        // Root every Lisp argument through both lifecycle unwind and the
        // post-unwind history/parser phase.  Native argument slices are not GC
        // roots by themselves.
        for root in args.iter().copied() {
            shared.push_specpdl_root(root);
        }

        // GNU `read_minibuf` binds `minibuffer-default` to DEFAULT for the whole
        // read (src/minibuf.c:591) and unwinds it afterwards.  Everything that
        // offers the default to the user reads the variable, not the argument:
        // `next-history-element`/`M-n`, `minibuffer-default-add-function`, and
        // packages that observe the live minibuffer from
        // `minibuffer-setup-hook`.  It is bound here, above the recursion and
        // entry checks, so no path through this read can skip it.
        shared.try_specbind(intern("minibuffer-default"), default_val)?;

        let restoration = MinibufferInvocationRestoration::capture(shared)?;
        let lifecycle_result = shared.with_unwind_scope(|shared| {
            // GNU registers minibuffer_unwind before configuration actions:
            // session teardown runs first, configurations next, and only then
            // the mini-window returns to its previous source buffer.
            let buffer_unwind = crate::emacs_core::eval::gnu_redisplay_hooks_enabled().then(|| {
                shared.record_native_unwind(
                    crate::emacs_core::eval::NativeUnwindAction::MinibufferBuffer {
                        state: Box::new(MinibufferBufferUnwind::default()),
                    },
                )
            });
            restoration.record(shared);

            let recursive_policy = if shared.builtin_var_value_by_name("enable-recursive-minibuffers")
                .is_some_and(|value| value.is_truthy())
            {
                RecursiveMinibufferPolicy::Allow
            } else {
                RecursiveMinibufferPolicy::Reject
            };
        let depth_before_entry = shared.minibuffers().depth();
        let entry_permit = shared
            .minibuffers()
            .prepare_entry(recursive_policy)
            .map_err(MinibufferEntryRejection::into_flow)?;

    // Save state.  GNU read_minibuf saves Vcurrent_prefix_arg in
    // minibuf_save_list and restores it during read_minibuf_unwind;
    // minibuffer commands may clobber it while reading input.
    let saved_buffer_id = shared.edit.buffers.current_buffer().map(|b| b.id());
    let caller_allows_text_properties = minibuffer_text_properties_enabled_in_buffer(
        shared.variables(),
        &shared.edit.buffers,
        saved_buffer_id,
    );
    let saved_current_prefix_arg = shared.builtin_var_value_by_name("current-prefix-arg")
        .unwrap_or(Value::NIL);
    // GNU `read_minibuf` also saves `(this-command-keys-vector)` (minibuf.c:
    // 738-739) and `read_minibuf_unwind` restores it (minibuf.c:1144-1146) so
    // the invoking command's `this-command-keys` survives the minibuffer's own
    // command-loop reads. Byte-compiled callers (`query-replace-read-to`,
    // `register-read-with-preview`, …) reach the minibuffer through THIS VM
    // runtime path, so the save/restore must live here too — otherwise their
    // following `read-key` sees the minibuffer's terminating RET and fires its
    // idle-timer probe early.
    let saved_command_keys = shared.read_command_keys().to_vec();
    let saved_raw_command_keys = shared.read_raw_command_keys().to_vec();
    let saved_minibuffer_history_variable = shared.builtin_var_value_by_name("minibuffer-history-variable")
        .unwrap_or(Value::from_sym_id(intern("minibuffer-history")));
    let saved_minibuffer_history_position = shared.builtin_var_value_by_name("minibuffer-history-position")
        .unwrap_or(Value::NIL);
    let recursive_depth = shared.recursive_command_loop_depth();

    // GNU read_minibuf initializes an unbound requested history in the
    // caller's runtime environment before switching to the minibuffer and
    // before any mode/setup hook can inspect it (minibuf.c:765-772).
    initialize_unbound_minibuffer_history(shared, history_spec)?;

    // GNU `read_minibuf` captures the caller's directory before switching to
    // *Minibuf-N*, then installs it after minibuffer-mode has reset locals.
    let ambient_directory = minibuffer_ambient_directory_in_state(&shared.edit.buffers);

    let minibuf_depth = entry_permit.depth();
    if crate::emacs_core::eval::gnu_redisplay_hooks_enabled()
        && let Some(existing) = shared.edit.buffers.find_buffer_by_name(&format!(" *Minibuf-{minibuf_depth}*")) {
        super::redisplay::publish_expired_buffer(shared, existing);
    }
    let minibuf_id = find_or_create_minibuffer_buffer_in_state(&mut shared.edit.buffers, minibuf_depth);

    let active_window_state = activate_minibuffer_window(shared, minibuf_id, entry_permit.level());
    if let Some(token) = buffer_unwind {
        match shared.native_unwind_action_mut(token) {
            Some(crate::emacs_core::eval::NativeUnwindAction::MinibufferBuffer { state }) => {
                state.set_saved(active_window_state);
            }
            other => debug_assert!(false, "minibuffer buffer unwind disappeared: {other:?}"),
        }
    }
    if active_window_state.is_none() {
        shared.edit.buffers.switch_current(minibuf_id);
    }
    let session_unwind = MinibufferSessionUnwind::new(
        minibuf_id,
        depth_before_entry,
        active_window_state,
        saved_buffer_id,
        saved_current_prefix_arg,
        saved_minibuffer_history_variable,
        saved_minibuffer_history_position,
        saved_command_keys,
        saved_raw_command_keys,
    );
    let session_token = shared.record_native_unwind(
        crate::emacs_core::eval::NativeUnwindAction::MinibufferSession {
            state: Box::new(session_unwind),
        },
    );
    crate::emacs_core::frame::sync_gui_frame_focus_redirects(shared)?;
    if let Some(active_window_state) = active_window_state {
        record_active_minibuffer_selection(shared, active_window_state, minibuf_id)?;
    }
    shared.store_variable_by_name("minibuffer-history-variable", history_spec.variable_value);
    shared.store_variable_by_name("minibuffer-history-position", history_spec.position);
    run_minibuffer_mode_if_bound(shared, "minibuffer-mode")?;
    install_minibuffer_ambient_directory_in_state(
        &mut shared.edit.buffers,
        minibuf_id,
        ambient_directory,
    );

    let prompt_properties = shared.builtin_var_value_by_name("minibuffer-prompt-properties")
        .unwrap_or(Value::NIL);
    let mini_source_before = crate::emacs_core::eval::gnu_redisplay_hooks_enabled().then(||
        shared.edit.buffers.get(minibuf_id).map(|buffer| (buffer.chars_modified_tick(), buffer.props_modified_tick()))
    ).flatten();
    super::install_minibuffer_buffer_contents(
        &mut shared.edit.buffers,
        minibuf_id,
        &prompt,
        initial_input.as_ref(),
        prompt_properties,
    );
    if crate::emacs_core::eval::gnu_redisplay_hooks_enabled()
        && shared.edit.buffers.get(minibuf_id)
        .map(|buffer| (buffer.chars_modified_tick(), buffer.props_modified_tick())) != mini_source_before
    {
        shared.gnu_mark_buffer_redisplay(minibuf_id);
    }
    tracing::debug!(
        "read-from-minibuffer: prompt={:?} minibuf_id={:?} current_buffer={:?} active_window={:?} selected_window={:?}",
        prompt_display,
        minibuf_id,
        shared.edit.buffers.current_buffer_id(),
        shared.active_minibuffer_window(),
        shared
            .frame_manager()
            .selected_frame()
            .map(|frame| frame.selected_window)
    );

    {
        let state = shared.minibuffers_mut().enter_with_permit(
            entry_permit,
            minibuf_id,
            &prompt,
            initial_input.as_ref(),
            history_spec.history_name,
        );
        state.command_loop_depth = recursive_depth;
    }

    // GNU `read_minibuf' clears the echo area HERE -- `clear_message (1, 1)'
    // at src/minibuf.c:894, after the prompt and any initial input are in the
    // buffer and immediately before `bset_keymap (current_buffer, map)'
    // (src/minibuf.c:895) and `run_hook (Qminibuffer_setup_hook)'
    // (src/minibuf.c:900). Entry, not exit: by the time any Lisp in the session
    // runs, `current-message' is already nil, which is what a
    // `minibuffer-setup-hook' function reading it observes.
    //
    // Only on this interactive path. GNU's batch arm returns from
    // `read_minibuf_noninteractive' at src/minibuf.c:649-655, long before the
    // clear.
    let _ = shared.clear_echo_area_message_without_clear_hook();

    let minibuf_keymap = if !keymap_arg.is_nil() {
        keymap_arg
    } else {
        shared.builtin_var_value_by_name("minibuffer-local-map")
            .unwrap_or(Value::NIL)
    };
    let _ = shared.edit.buffers.set_current_local_map(minibuf_keymap);
    shared.store_variable_by_name("minibuffer-prompt", Value::heap_string(prompt.clone()));
    shared.store_variable_by_name("minibuffer-depth", Value::fixnum(minibuf_depth as i64));
    run_before_setup_hook(shared)?;
    shared.run_hook_if_bound("minibuffer-setup-hook")?;

    let command_outcome =
        MinibufferCommandOutcome::from_recursive_edit(shared.minibuffer_command_loop_inner());
    match shared.native_unwind_action_mut(session_token) {
        Some(crate::emacs_core::eval::NativeUnwindAction::MinibufferSession { state }) => {
            state.set_disposition(command_outcome.disposition());
        }
        other => {
            debug_assert!(false, "minibuffer unwind action disappeared: {other:?}");
        }
    }

    match command_outcome {
        MinibufferCommandOutcome::Accepted => {
            let _ = shared.edit.buffers.switch_current_unrecorded(minibuf_id);
            let preserve_text_properties = caller_allows_text_properties
                || minibuffer_text_properties_enabled_in_buffer(
                    shared.variables(),
                    &shared.edit.buffers,
                    Some(minibuf_id),
                );
            Ok(Value::heap_string(
                super::minibuffer_contents_lisp_string_in_state(
                    shared.variables(),
                    &shared.edit.buffers,
                    &shared.minibuffers(),
                    preserve_text_properties,
                )?,
            ))
        }
        MinibufferCommandOutcome::Aborted(flow) => Err(flow),
    }
        });

        // This is deliberately between the inner lifecycle scope and history:
        // GNU restores both configurations, then reselects the caller, then
        // calls `add-to-history` in the restored buffer-local environment.
        let redirect_result = restoration.select_calling_frame(shared);
        // `with_unwind_scope` roots the tagged result while exit hooks and
        // window restoration allocate, so string properties cannot retain
        // otherwise-unreachable Lisp objects through an untraced Rust value.
        let result_value = lifecycle_result?;
        redirect_result?;
        let result_text = result_value
            .as_lisp_string()
            .expect("an accepted minibuffer command must return its contents")
            .clone();
        if history_add_new_input_enabled(shared.variables())
            && let Some(history_name) = history_spec.history_name
            && let Some(history_entry) = minibuffer_history_entry(&result_text, default_val)
        {
            add_minibuffer_history_after_unwind(shared, history_name, &history_entry)?;
        }

        finish_minibuffer_result_after_unwind(shared, result_text, read_arg, default_val)
    });
    if result.is_ok() {
        shared.note_interactive_minibuffer_read();
    }
    result
}

pub(crate) fn finish_completing_read_in_vm_runtime(
    shared: &mut Context,
    args: &[Value],
) -> EvalResult {
    if let Some(result) = builtin_completing_read_in_runtime(shared, args)? {
        return Ok(result);
    }
    let minibuffer_args = completing_read_minibuffer_args(shared.variables(), args);
    let collection = args[1];
    let predicate = args.get(2).copied().unwrap_or(Value::NIL);
    let require_match = args.get(3).copied().unwrap_or(Value::NIL);
    let original_buffer = shared
        .edit
        .buffers
        .current_buffer_id()
        .map(Value::make_buffer)
        .unwrap_or(Value::NIL);
    let completion_ignore_case = shared
        .eval_symbol("completion-ignore-case")
        .unwrap_or(Value::NIL);

    finish_read_from_minibuffer_in_vm_runtime_with_setup(shared, &minibuffer_args, move |shared| {
        install_completing_read_minibuffer_locals(
            shared,
            collection,
            predicate,
            require_match,
            original_buffer,
            completion_ignore_case,
        );
        Ok(Value::NIL)
    })
}

/// Map the `REQUIRE-MATCH` argument of `completing-read` to the value
/// stored in `minibuffer-completion-confirm`.
///
/// GNU semantics:
///   nil        → nil
///   t          → nil
///   confirm    → confirm
///   confirm-after-completion → confirm-after-completion
///   function / other non-t, non-nil → unchanged
pub(crate) fn completion_confirm_from_require_match(require_match: Value) -> Value {
    match RequireMatchSymbol::from_lisp_value(require_match) {
        Some(RequireMatchSymbol::T) => Value::NIL,
        Some(RequireMatchSymbol::Confirm | RequireMatchSymbol::ConfirmAfterCompletion) => {
            require_match
        }
        None if require_match.is_nil() => Value::NIL,
        None => require_match,
    }
}

fn install_completing_read_minibuffer_locals(
    eval: &mut Context,
    collection: Value,
    predicate: Value,
    require_match: Value,
    original_buffer: Value,
    completion_ignore_case: Value,
) {
    let Some(current_id) = eval.edit.buffers.current_buffer_id() else {
        return;
    };
    for (name, value) in [
        ("minibuffer-completion-table", collection),
        ("minibuffer-completion-predicate", predicate),
        (
            "minibuffer-completion-confirm",
            completion_confirm_from_require_match(require_match),
        ),
        ("minibuffer--require-match", require_match),
        ("minibuffer--original-buffer", original_buffer),
        ("completion-ignore-case", completion_ignore_case),
    ] {
        let _ = eval.set_buffer_local_binding_by_id(current_id, intern(name), value);
    }
}

pub(crate) fn completing_read_minibuffer_args(
    vars: VariableView<'_>,
    args: &[Value],
) -> [Value; 7] {
    let prompt = args[0];
    let require_match = args.get(3).copied().unwrap_or(Value::NIL);
    let initial_input = args.get(4).copied().unwrap_or(Value::NIL);
    let hist = args.get(5).copied().unwrap_or(Value::NIL);
    let default_val = args.get(6).copied().unwrap_or(Value::NIL);
    let inherit = args.get(7).copied().unwrap_or(Value::NIL);

    let keymap = if !require_match.is_nil() {
        vars.value_by_name("minibuffer-local-must-match-map")
            .unwrap_or(Value::NIL)
    } else {
        vars.value_by_name("minibuffer-local-completion-map")
            .unwrap_or(Value::NIL)
    };

    [
        prompt,
        initial_input,
        keymap,
        Value::NIL,
        hist,
        default_val,
        inherit,
    ]
}

/// `(yes-or-no-p PROMPT)`
///
/// Ask user a yes-or-no question requiring "yes" or "no" typed in full.
/// In interactive mode, uses read-from-minibuffer.
/// In batch mode, signals end-of-file.
pub(crate) fn builtin_yes_or_no_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    validate_yes_or_no_p_args(&args)?;
    if let Some(result) = yes_or_no_p_dialog_result(eval, &args)? {
        return Ok(result);
    }
    if yes_or_no_p_use_short_answers(eval) {
        return eval.apply(Value::symbol("y-or-n-p"), args);
    }
    if let Some(result) = builtin_yes_or_no_p_in_runtime(eval, &args)? {
        return Ok(result);
    }
    finish_yes_or_no_p_in_eval(eval, &args)
}

pub(crate) fn finish_yes_or_no_p_in_eval(eval: &mut Context, args: &[Value]) -> EvalResult {
    // GNU `Fyes_or_no_p` dynamically binds `real-this-command` around the
    // recursive minibuffer (src/fns.c).  The minibuffer command loop is
    // allowed to assign this variable while it dispatches self-insert and
    // `exit-minibuffer`, but those assignments belong to the nested command
    // context.  Keeping the boundary in the specpdl means every return path --
    // success, quit, signal, or future retry error -- restores the caller.
    let specpdl_count = eval.specpdl().len();
    let caller = eval.eval_symbol("real-this-command").unwrap_or(Value::NIL);
    eval.try_specbind_or_unwind_to(specpdl_count, intern("real-this-command"), caller)?;
    let result = finish_yes_or_no_p_with_minibuffer(args, |minibuffer_args| {
        finish_read_from_minibuffer_in_eval(eval, minibuffer_args)
    });
    eval.unbind_to_with_result(specpdl_count, result)
}

pub(crate) fn finish_yes_or_no_p_with_minibuffer(
    args: &[Value],
    mut read_from_minibuffer: impl FnMut(&[Value]) -> EvalResult,
) -> EvalResult {
    let prompt_ls = if args[0].is_string() {
        args[0].as_lisp_string().expect("checked string").clone()
    } else {
        crate::heap_types::LispString::from_unibyte(Vec::new())
    };
    // Build the prompt exactly like GNU `Fyes_or_no_p` (fns.c): append
    // `yes-or-no-prompt` ("(yes or no) "), preceded by a single space only when
    // the prompt does not already end in whitespace.
    let ends_in_blank = prompt_ls
        .as_bytes()
        .last()
        .is_some_and(|&b| b == b' ' || b == b'\t');
    let suffix: &[u8] = if ends_in_blank {
        b"(yes or no) "
    } else {
        b" (yes or no) "
    };
    let full_prompt = prompt_ls.concat(&crate::heap_types::LispString::from_unibyte(
        suffix.to_vec(),
    ));
    loop {
        let result = read_from_minibuffer(&[Value::heap_string(full_prompt.clone())])?;
        if result.is_string() {
            let answer = result.as_lisp_string().expect("checked string");
            // The valid answers are ASCII ("yes"/"no"); decode lossily to compare.
            match crate::emacs_core::emacs_char::to_utf8_lossy(answer.as_bytes()).trim() {
                "yes" => return Ok(Value::T),
                "no" => return Ok(Value::NIL),
                _ => continue,
            }
        }
    }
}

pub(crate) fn builtin_yes_or_no_p_in_runtime(
    runtime: &Context,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    validate_yes_or_no_p_args(args)?;

    match runtime.minibuffer_input_source() {
        MinibufferInputSource::CommandLoop => Ok(None),
        MinibufferInputSource::StandardInput => {
            barf_if_interaction_inhibited(runtime)?;
            // Batch/noninteractive: GNU's `read_minibuf_noninteractive` (minibuf.c)
            // writes the prompt to stdout and reads the answer from stdin. Mirror
            // that — including the yes/no re-prompt loop — instead of failing before
            // the prompt is ever shown, so batch `yes-or-no-p` emits the prompt
            // exactly like GNU (and still signals end-of-file on empty stdin).
            finish_yes_or_no_p_with_minibuffer(args, |minibuffer_args| {
                let prompt = minibuffer_args[0]
                    .as_lisp_string()
                    .expect("yes-or-no-p minibuffer prompt is a string");
                read_from_stdin_noninteractive(&crate::emacs_core::emacs_char::to_utf8_lossy(
                    prompt.as_bytes(),
                ))
            })
            .map(Some)
        }
    }
}

fn validate_yes_or_no_p_args(args: &[Value]) -> Result<(), Flow> {
    expect_args("yes-or-no-p", args, 1)?;
    if !args[0].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        ));
    }
    Ok(())
}

fn yes_or_no_p_dialog_result(eval: &mut Context, args: &[Value]) -> Result<Option<Value>, Flow> {
    if !yes_or_no_p_should_use_dialog(eval) {
        return Ok(None);
    }
    let menu = Value::cons(
        args[0],
        Value::list(vec![
            Value::cons(Value::string("Yes"), Value::T),
            Value::cons(Value::string("No"), Value::NIL),
        ]),
    );
    crate::emacs_core::display::builtin_x_popup_dialog(eval, vec![Value::T, menu, Value::NIL])
        .map(Some)
}

fn yes_or_no_p_use_short_answers(eval: &Context) -> bool {
    eval.builtin_var_value_by_name("use-short-answers")
        .is_some_and(|v| v.is_truthy())
}

fn yes_or_no_p_should_use_dialog(runtime: &impl KeyboardInputRuntime) -> bool {
    let last_input_event = runtime.symbol_value_or_nil("last-input-event");
    if last_input_event.is_nil() || !runtime.symbol_value_or_nil("use-dialog-box").is_truthy() {
        return false;
    }

    let last_nonmenu_event = runtime.symbol_value_or_nil("last-nonmenu-event");
    let from_tty_menu = runtime.symbol_value_or_nil("from--tty-menu-p");
    last_nonmenu_event.is_cons()
        || (last_nonmenu_event.is_nil() && last_input_event.is_cons())
        || (from_tty_menu.is_truthy() && from_tty_menu.as_bare_symbol_name() != Some("unbound"))
}

/// `(read-coding-system PROMPT &optional DEFAULT-CODING-SYSTEM)`
///
/// Faithful port of GNU's `Fread_coding_system` (src/coding.c): binds
/// `completion-ignore-case' to t and delegates to `completing-read', then
/// interns the (non-empty) result.  In batch mode `completing-read' writes the
/// prompt to stdout and signals `end-of-file' on empty stdin, matching GNU.
pub(crate) fn builtin_read_coding_system(eval: &mut Context, args: Vec<Value>) -> EvalResult {
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
    eval: &mut Context,
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
    eval: &mut Context,
    args: &[Value],
    require_match: bool,
) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    // GNU passes the symbol-name string of a symbolic DEFAULT-CODING-SYSTEM.
    let default = match args.get(1).copied() {
        Some(value) if value.is_symbol(symbol_mode) && !value.is_nil() => Value::string(
            resolve_sym(value.as_symbol_id(symbol_mode).expect("checked symbol")),
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
    let count = eval.specpdl().len();
    eval.try_specbind_or_unwind_to(count, intern("completion-ignore-case"), Value::T)?;
    let val = builtin_completing_read(eval, completing_args);
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
