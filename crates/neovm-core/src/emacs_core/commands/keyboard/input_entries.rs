//! Terminal input Lisp entries (GNU keyboard.c read-char / read-event /
//! read-key-sequence, the input-mode setters and the read filters).
//!
//! These bodies bind the terminal/input layer, so they are R0 command-layer
//! entries (STAGE4 design §5) and live in core.  They read evaluator state
//! through `&mut Context` and drive it through the `KeyboardInputRuntime`
//! abstraction, which stays beside the reader in `emacs_core::reader`
//! because the minibuffer read bodies still use it.

use std::time::Duration;

use crate::emacs_core::error::{
    EvalResult, Flow, LispCondition, expect_args, expect_max_args, expect_min_args, signal,
};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::reader::KeyboardInputRuntime;
use crate::emacs_core::value::{Value, ValueKind};
use crate::emacs_core::wait::WaitContextExt;
use neovm_edit::emacs_core::edit_host::EditHost;

pub(crate) fn parse_optional_read_seconds_arg(
    value: Option<&Value>,
) -> Result<Option<Duration>, Flow> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_nil() {
        return Ok(None);
    }

    let seconds = value.as_number_f64().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("numberp"), *value],
        )
    })?;
    if seconds <= 0.0 {
        return Ok(Some(Duration::ZERO));
    }

    Ok(Some(Duration::from_secs_f64(seconds)))
}

fn event_to_int(event: &Value) -> Option<i64> {
    match event.kind() {
        ValueKind::Fixnum(n) => Some(n),
        _ => None,
    }
}

fn event_to_char(event: &Value) -> Option<char> {
    match event.kind() {
        ValueKind::Fixnum(c) => char::from_u32(c as u32),
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

fn non_character_input_event_error() -> Flow {
    signal("error", vec![Value::string("Non-character input-event")])
}

/// The deadline policy for one command-event read.
///
/// GNU's `read_filtered_event` always enters `read_char`, with or without a
/// terminal.  The low-level unified wait is the sole authority on whether
/// input can still arrive because it sees every source: keyboard input,
/// timers, processes, and file notifications.  This type carries only the
/// caller's deadline policy so upper layers cannot prematurely classify that
/// changing set of sources and bypass part of the wait loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandEventReadPlan {
    Unbounded,
    Timed { timeout: Duration },
}

impl CommandEventReadPlan {
    pub(crate) fn from_seconds(seconds: Option<&Value>) -> Result<Self, Flow> {
        Ok(match parse_optional_read_seconds_arg(seconds)? {
            Some(timeout) => Self::Timed { timeout },
            None => Self::Unbounded,
        })
    }

    pub(crate) fn timeout(self) -> Option<Duration> {
        match self {
            Self::Unbounded => None,
            Self::Timed { timeout } => Some(timeout),
        }
    }
}

/// GNU passes `prev_event == t` to its low-level reader when
/// INHERIT-INPUT-METHOD is nil.  Besides suppressing input methods, that asks a
/// TTY reader to return transport bytes without applying
/// `keyboard-coding-system`.
pub(crate) fn tty_input_decoding_from_read_args(
    args: &[Value],
) -> neovm_value::system_vocab::input::TtyInputDecoding {
    if args.get(1).is_some_and(|value| value.is_truthy()) {
        neovm_value::system_vocab::input::TtyInputDecoding::KeyboardCodingSystem
    } else {
        neovm_value::system_vocab::input::TtyInputDecoding::RawBytes
    }
}

pub(crate) fn read_key_sequence_options_from_args(
    args: &[Value],
) -> neovm_value::system_vocab::input::ReadKeySequenceOptions {
    // GNU `Fread_key_sequence`/`Fread_key_sequence_vector` signature
    // (keyboard.c:11935) is
    //   (PROMPT CONTINUE-ECHO DONT-DOWNCASE-LAST CAN-RETURN-SWITCH-FRAME ...).
    // Arg 1 (CONTINUE-ECHO) governs whether the previous command's
    // `this-command-keys` is preserved (non-nil) or cleared for a fresh
    // sequence (nil); `read-key` (subr.el) passes nil here and relies on the
    // clear so its idle-timer `(this-command-keys-vector)` probe is empty.
    neovm_value::system_vocab::input::ReadKeySequenceOptions::new(
        args.first().copied().unwrap_or(Value::NIL),
        args.get(1).is_some_and(|v| v.is_truthy()),
        args.get(2).is_some_and(|v| v.is_truthy()),
        args.get(3).is_some_and(|v| v.is_truthy()),
    )
}

fn read_key_sequence_string_result(keys: &[Value]) -> Value {
    let mut chars_only = true;
    let mut s = String::new();
    for key in keys {
        if let Some(c) = event_to_char(key) {
            s.push(c);
        } else {
            chars_only = false;
            break;
        }
    }
    if chars_only {
        Value::string(s)
    } else {
        read_key_sequence_vector_result(keys)
    }
}

fn read_key_sequence_vector_result(keys: &[Value]) -> Value {
    Value::vector(
        keys.iter()
            .map(|key| event_to_int(key).map(Value::fixnum).unwrap_or(*key))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// 10. input-pending-p
// ---------------------------------------------------------------------------

/// `(input-pending-p &optional CHECK-TIMERS)`
///
/// Return non-nil when unread input, staged host input, or `quit-flag` is pending.
/// `CHECK-TIMERS` is accepted and fires due timers before checking.
fn input_pending_now(ctx: &crate::emacs_core::eval::Context) -> bool {
    ctx.has_pending_requeued_events()
        || ctx.has_pending_command_input_for_query()
        || !ctx.quit_flag_value().is_nil()
}

pub(crate) fn builtin_input_pending_p(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("input-pending-p", &args, 1)?;
    ctx.sync_keyboard_terminal_owner();
    ctx.service_input_pending_without_timers()?;

    if input_pending_now(ctx) {
        return Ok(Value::T);
    }

    if args.first().is_some_and(|v| v.is_truthy()) {
        // GNU `input-pending-p' can run due timers here, but it does not
        // force a redisplay the way `detect_input_pending_run_timers' does.
        ctx.service_input_pending_with_timers()?;
    }

    Ok(Value::bool_val(input_pending_now(ctx)))
}

// ---------------------------------------------------------------------------
// 11. discard-input
// ---------------------------------------------------------------------------

/// `(discard-input)`
///
/// Discard pending unread command events for the current scope.
pub(crate) fn builtin_discard_input(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("discard-input", &args, 0)?;
    // SAFETY: this storage helper only clears a runtime variable through the
    // value/forwarder path. It never calls watchers/Lisp or writes either
    // canonical dispatch word, and returns no manager/interior reference.
    let (buffers, owner, specpdl, obarray_view) = unsafe { ctx.runtime_binding_parts_mut() };
    unsafe {
        obarray_view.with_legacy_value_write(|obarray| {
            crate::emacs_core::eval::set_runtime_binding(
                obarray,
                buffers,
                owner,
                specpdl,
                intern("unread-command-events"),
                Value::NIL,
            )
        })
    }?;
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// 11b. insert-special-event
// ---------------------------------------------------------------------------

/// `(insert-special-event EVENT)` -> nil
///
/// Insert EVENT into the low-level special-event queue, so that the next
/// key-reading operation handles it through `special-event-map` instead of
/// returning it as ordinary user input.
///
/// Mirrors GNU `Finsert_special_event` at
/// `src/keyboard.c:12060`:
///
///   DEFUN ("insert-special-event", Finsert_special_event, ...)
///     (Lisp_Object event)
///   {
///     CHECK_CONS (event);
///     if (NILP (access_keymap (... Vspecial_event_map ..., event, ...)))
///       signal_error ("Invalid event kind", XCAR (event));
///     kbd_buffer_store_event (&ie);
///     return Qnil;
///   }
///
/// GNU pushes into the kernel kbd_buffer (which is a ring of
/// `struct input_event` records) so the event is delivered via the
/// same special-event path as hardware input. Neomacs keeps this queue in the
/// keyboard runtime (`unread_events`), not in `unread-command-events`: callers
/// like file notification rely on `read-event` consuming the event internally
/// and running the `special-event-map` handler.
///
/// Keyboard audit Finding 16 in
/// `drafts/keyboard-command-loop-audit.md`.
pub(crate) fn builtin_insert_special_event(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("insert-special-event", &args, 1)?;
    let event = args[0];
    if !event.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), event],
        ));
    }
    if ctx.special_event_binding(&event).is_none() {
        return Err(signal(
            "error",
            vec![Value::string("Invalid event kind"), event.cons_car()],
        ));
    }
    ctx.queue_special_event(event);
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// 12. current-input-mode / set-input-mode
// ---------------------------------------------------------------------------

/// `(current-input-mode)` -> `(INTERRUPT FLOW META QUIT)`
pub(crate) fn builtin_current_input_mode(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-input-mode", &args, 0)?;
    let (interrupt, flow, meta, quit) = ctx.current_input_mode_tuple();
    Ok(Value::list(vec![
        Value::bool_val(interrupt),
        Value::bool_val(flow),
        Value::bool_val(meta),
        Value::fixnum(quit),
    ]))
}

/// `(set-input-mode INTERRUPT FLOW META QUIT)`
///
/// Batch-compatible behavior currently tracks INTERRUPT plus Lisp-visible
/// QUIT while leaving FLOW/META fixed.
pub(crate) fn builtin_set_input_mode(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("set-input-mode", &args, 3)?;
    expect_max_args("set-input-mode", &args, 4)?;
    eval.set_input_mode_interrupt(args[0].is_truthy());
    if let Some(quit) = args.get(3).copied()
        && !quit.is_nil()
    {
        set_quit_char_in_context(eval, quit)?;
    }
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// 13. input mode helper setters
// ---------------------------------------------------------------------------

/// `(set-input-interrupt-mode INTERRUPT)`
pub(crate) fn builtin_set_input_interrupt_mode(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("set-input-interrupt-mode", &args, 1)?;
    eval.set_input_mode_interrupt(args[0].is_truthy());
    Ok(Value::NIL)
}

pub(crate) fn builtin_read_char_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("read-char"), Value::fixnum(args.len() as i64)],
        ));
    }
    expect_optional_prompt_string(args)?;
    let seconds_is_nil_or_omitted = args.get(2).is_none_or(|v| v.is_nil());

    if let Some(event) = runtime.peek_unread_command_event() {
        if let Some(n) = event_to_int(&event) {
            let event = runtime
                .pop_unread_command_event()
                .expect("peeked unread event should still be present");
            if runtime.read_command_keys().is_empty() && seconds_is_nil_or_omitted {
                runtime.set_read_command_keys(vec![event]);
            }
            return Ok(Some(Value::fixnum(n)));
        }
        runtime.replace_unread_command_event_with_singleton(event);
        runtime.record_input_event(event);
        return Err(non_character_input_event_error());
    }

    Ok(None)
}

pub(crate) fn builtin_read_key_sequence_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    expect_min_args("read-key-sequence", args, 1)?;
    expect_max_args("read-key-sequence", args, 6)?;
    expect_optional_prompt_string(args)?;

    if runtime.peek_unread_command_event().is_some() {
        let (keys, _binding) =
            runtime.read_key_sequence_blocking(read_key_sequence_options_from_args(args))?;
        return Ok(Some(read_key_sequence_string_result(&keys)));
    }

    Ok(None)
}

pub(crate) fn builtin_read_key_sequence_vector_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<Option<Value>, Flow> {
    expect_min_args("read-key-sequence-vector", args, 1)?;
    expect_max_args("read-key-sequence-vector", args, 6)?;
    expect_optional_prompt_string(args)?;

    if runtime.peek_unread_command_event().is_some() {
        let (keys, _binding) =
            runtime.read_key_sequence_blocking(read_key_sequence_options_from_args(args))?;
        return Ok(Some(read_key_sequence_vector_result(&keys)));
    }

    Ok(None)
}

/// `(set-input-meta-mode META)`
///
/// Batch-compatible behavior: accepts GNU-compatible optional TERMINAL and returns nil.
pub(crate) fn builtin_set_input_meta_mode(args: Vec<Value>) -> EvalResult {
    expect_min_args("set-input-meta-mode", &args, 1)?;
    expect_max_args("set-input-meta-mode", &args, 2)?;
    Ok(Value::NIL)
}

/// `(set-output-flow-control FLOW)`
///
/// Batch-compatible behavior: accepts one argument and returns nil.
pub(crate) fn builtin_set_output_flow_control(args: Vec<Value>) -> EvalResult {
    expect_min_args("set-output-flow-control", &args, 1)?;
    expect_max_args("set-output-flow-control", &args, 2)?;
    Ok(Value::NIL)
}

/// `(set-quit-char CHAR)`
///
fn set_quit_char_in_context(eval: &mut Context, quit: Value) -> EvalResult {
    let Some(quit) = quit.as_fixnum() else {
        return Err(signal(
            "error",
            vec![Value::string("QUIT must be an ASCII character")],
        ));
    };
    if !(0..=0o400).contains(&quit) {
        return Err(signal(
            "error",
            vec![Value::string("QUIT must be an ASCII character")],
        ));
    }

    eval.set_quit_char(quit);
    Ok(Value::NIL)
}

/// GNU-compatible quit-char setter for the current evaluator.
pub(crate) fn builtin_set_quit_char(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("set-quit-char", &args, 1)?;
    set_quit_char_in_context(eval, args[0])
}

// ---------------------------------------------------------------------------
// 14. waiting-for-user-input-p
// ---------------------------------------------------------------------------

/// `(waiting-for-user-input-p)`
///
/// Batch-mode compatibility: always returns nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_waiting_for_user_input_p(args: Vec<Value>) -> EvalResult {
    expect_args("waiting-for-user-input-p", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_waiting_for_user_input_p_ctx(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("waiting-for-user-input-p", &args, 0)?;
    Ok(Value::bool_val(eval.waiting_for_user_input()))
}

// ---------------------------------------------------------------------------
// 17. read-char
// ---------------------------------------------------------------------------

/// `(read-char &optional PROMPT INHERIT-INPUT-METHOD SECONDS)`
///
/// Read a character from the command input (keyboard or macro).
/// In batch mode, checks `unread-command-events` and returns nil if empty.
/// In interactive mode, blocks on the input channel via `read_char()`.
/// GNU `read_char` (keyboard.c) displays a non-nil string PROMPT in the echo
/// area for the duration of the read. Mirror that so prompts such as
/// `perform-replace`'s `(read-key "Query replacing ...: (? for help) ")` are
/// visible while waiting for the key. A nil/omitted or empty prompt shows
/// nothing (GNU only echoes a non-empty string prompt).
pub(crate) fn display_read_prompt(eval: &mut Context, args: &[Value]) {
    if let Some(prompt) = args.first().and_then(|v| eval.lisp_string(*v))
        && !prompt.is_empty()
    {
        eval.set_current_message(Some(prompt.clone()));
    }
}

pub(crate) fn builtin_read_char(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    // GNU checks `inhibit-interaction` before showing the prompt (src/keyboard.c).
    crate::emacs_core::minibuffer::barf_if_interaction_inhibited(eval)?;
    display_read_prompt(eval, &args);
    if let Some(value) = builtin_read_char_in_runtime(eval, &args)? {
        return Ok(value);
    }

    finish_read_char_in_eval(eval, &args)
}

pub(crate) fn finish_read_char_in_eval(eval: &mut Context, args: &[Value]) -> EvalResult {
    finish_read_char_interactive_in_runtime(eval, args)
}

pub(crate) fn finish_read_char_interactive_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    args: &[Value],
) -> EvalResult {
    let read_plan = CommandEventReadPlan::from_seconds(args.get(2))?;
    let tty_input_decoding = tty_input_decoding_from_read_args(args);
    let Some(event) = runtime.read_char_with_timeout(read_plan.timeout(), tty_input_decoding)?
    else {
        return Ok(Value::NIL);
    };
    let seconds_is_nil_or_omitted = args.get(2).is_none_or(|v| v.is_nil());
    if let Some(n) = event_to_int(&event) {
        if runtime.read_command_keys().is_empty() && seconds_is_nil_or_omitted {
            runtime.set_read_command_keys(vec![event]);
        }
        return Ok(Value::fixnum(n));
    }
    runtime.replace_unread_command_event_with_singleton(event);
    runtime.record_input_event(event);
    Err(non_character_input_event_error())
}

/// `(read-key &optional PROMPT)`
///
/// Read a key from the command input.
/// In batch mode, returns next `unread-command-events` event, else nil.
/// In interactive mode, blocks on the input channel via `read_char()`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_read_key(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("read-key"), Value::fixnum(args.len() as i64)],
        ));
    }
    expect_optional_prompt_string(&args)?;

    // 1. Check unread-command-events first
    if let Some(event) = eval.pop_unread_command_event() {
        eval.record_nonmenu_input_event(event);
        eval.set_read_command_keys(vec![event]);
        if let Some(n) = event_to_int(&event) {
            return Ok(Value::fixnum(n));
        }
        return Ok(event);
    }

    // 2. Interactive mode: block on input channel
    if eval.input_rx().as_ref().is_some() {
        let event = eval.read_char()?;
        eval.record_nonmenu_input_event(event);
        eval.set_read_command_keys(vec![event]);
        if let Some(n) = event_to_int(&event) {
            return Ok(Value::fixnum(n));
        }
        return Ok(event);
    }

    // 3. Batch mode: no input
    eval.clear_read_command_keys();
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// 18. read-key-sequence
// ---------------------------------------------------------------------------

/// `(read-key-sequence PROMPT &optional ...)`
///
/// Read a sequence of keystrokes that forms a complete key binding.
/// In batch mode, consumes one queued event. In interactive mode, uses the
/// evaluator's `read_key_sequence()` to accumulate keys through prefix keymaps.
pub(crate) fn builtin_read_key_sequence(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(value) = builtin_read_key_sequence_in_runtime(eval, &args)? {
        return Ok(value);
    }

    finish_read_key_sequence_in_eval(eval, &args)
}

pub(crate) fn finish_read_key_sequence_in_eval(eval: &mut Context, args: &[Value]) -> EvalResult {
    finish_read_key_sequence_interactive_in_runtime(eval, read_key_sequence_options_from_args(args))
}

pub(crate) fn finish_read_key_sequence_interactive_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    options: neovm_value::system_vocab::input::ReadKeySequenceOptions,
) -> EvalResult {
    let (keys, _binding) = runtime.read_key_sequence_blocking(options)?;
    let mut chars_only = true;
    let mut s = String::new();
    for k in &keys {
        if let Some(c) = event_to_char(k) {
            s.push(c);
        } else {
            chars_only = false;
            break;
        }
    }
    if chars_only {
        return Ok(Value::string(s));
    }
    Ok(Value::vector(keys))
}

/// `(read-key-sequence-vector PROMPT)`
///
/// Batch mode: returns next `unread-command-events` event as a single-element
/// vector when present, otherwise an empty vector.
pub(crate) fn builtin_read_key_sequence_vector(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(value) = builtin_read_key_sequence_vector_in_runtime(eval, &args)? {
        return Ok(value);
    }
    finish_read_key_sequence_vector_interactive_in_runtime(
        eval,
        read_key_sequence_options_from_args(&args),
    )
}

pub(crate) fn finish_read_key_sequence_vector_interactive_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    options: neovm_value::system_vocab::input::ReadKeySequenceOptions,
) -> EvalResult {
    let (keys, _binding) = runtime.read_key_sequence_blocking(options)?;
    Ok(Value::vector(keys))
}

/// `(read-event &optional PROMPT INHERIT-INPUT-METHOD SECONDS)`
///
/// Read an event from the command input.
/// In batch mode, reads from `unread-command-events`, returns nil if empty.
/// In interactive mode, blocks on the input channel via `read_char()`.
pub(crate) fn builtin_read_event(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    // GNU checks `inhibit-interaction` before showing the prompt (src/keyboard.c).
    crate::emacs_core::minibuffer::barf_if_interaction_inhibited(eval)?;
    display_read_prompt(eval, &args);
    if let Some(value) = builtin_read_event_in_runtime(eval, &args)? {
        return Ok(value);
    }

    finish_read_event_in_eval(eval, &args)
}

pub(crate) fn finish_read_event_in_eval(eval: &mut Context, args: &[Value]) -> EvalResult {
    finish_read_event_interactive_in_runtime(eval, args)
}

pub(crate) fn finish_read_event_interactive_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    args: &[Value],
) -> EvalResult {
    let read_plan = CommandEventReadPlan::from_seconds(args.get(2))?;
    let tty_input_decoding = tty_input_decoding_from_read_args(args);
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
pub(crate) fn builtin_read_char_exclusive(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    // GNU checks `inhibit-interaction` before showing the prompt (src/keyboard.c).
    crate::emacs_core::minibuffer::barf_if_interaction_inhibited(eval)?;
    display_read_prompt(eval, &args);
    if let Some(value) = builtin_read_char_exclusive_in_runtime(eval, &args)? {
        return Ok(value);
    }

    finish_read_char_exclusive_in_eval(eval, &args)
}

pub(crate) fn finish_read_char_exclusive_in_eval(eval: &mut Context, args: &[Value]) -> EvalResult {
    finish_read_char_exclusive_interactive_in_runtime(eval, args)
}

pub(crate) fn finish_read_char_exclusive_interactive_in_runtime(
    runtime: &mut impl KeyboardInputRuntime,
    args: &[Value],
) -> EvalResult {
    let read_plan = CommandEventReadPlan::from_seconds(args.get(2))?;
    let tty_input_decoding = tty_input_decoding_from_read_args(args);
    let deadline = read_plan
        .timeout()
        .map(|timeout| std::time::Instant::now() + timeout);
    loop {
        let remaining =
            deadline.map(|deadline| deadline.saturating_duration_since(std::time::Instant::now()));
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
    runtime: &mut impl KeyboardInputRuntime,
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
    runtime: &mut impl KeyboardInputRuntime,
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
