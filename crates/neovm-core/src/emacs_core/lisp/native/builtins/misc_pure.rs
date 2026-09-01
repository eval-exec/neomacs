use super::*;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};

// ===========================================================================
// Misc
// ===========================================================================

pub(crate) fn builtin_identity(args: Vec<Value>) -> EvalResult {
    expect_args("identity", &args, 1)?;
    Ok(args[0])
}

pub(crate) fn builtin_prefix_numeric_value(args: Vec<Value>) -> EvalResult {
    expect_args("prefix-numeric-value", &args, 1)?;
    let numeric = crate::emacs_core::prefix::prefix_numeric_value(&args[0]);
    Ok(Value::fixnum(numeric))
}

/// Parse a logged message line into its base text and repeat count: a bare
/// `"MSG"` -> (MSG, 1); a coalesced `"MSG [K times]"` -> (MSG, K). Mirrors GNU
/// `message_log_check_duplicate` (xdisp.c).
fn parse_logged_message_line(line: &[u8]) -> (&[u8], i64) {
    const SUFFIX: &[u8] = b" times]";
    if let Some(rest) = line.strip_suffix(SUFFIX)
        && let Some(open) = rest.iter().rposition(|&b| b == b'[')
    {
        let digits = &rest[open + 1..];
        if open >= 2
            && rest[open - 1] == b' '
            && !digits.is_empty()
            && digits.iter().all(u8::is_ascii_digit)
            && let Ok(k) = std::str::from_utf8(digits).unwrap_or("").parse::<i64>()
        {
            return (&rest[..open - 1], k);
        }
    }
    (line, 1)
}

/// GNU `message_log_check_duplicate` treats a later message as a progress
/// update when its first mismatch with the previous line occurs after `...`
/// in their shared prefix.  The rule is intentionally asymmetric: a longer
/// completion such as `"Indexing...done"` replaces `"Indexing..."`, while a
/// shorter or unrelated message is appended.
fn message_log_is_progress_update(previous: &[u8], next: &[u8]) -> bool {
    let mut seen_dots = false;
    for (index, &next_byte) in next.iter().enumerate() {
        if index >= 3 && previous.get(index - 3..index) == Some(b"...") {
            seen_dots = true;
        }
        if previous.get(index).copied() != Some(next_byte) {
            return seen_dots;
        }
    }
    false
}

/// GNU `message_dolog` coalescing: when MSG duplicates the last logged line of
/// the messages buffer (a bare `MSG` or `MSG [K times]`), return the coalesced
/// `MSG [N times]`.  When MSG completes a shared `...` progress prefix, return
/// MSG itself.  Either replacement includes the byte position of the previous
/// line's start so the caller can delete it before re-logging.  Otherwise
/// return `(MSG, old_full_end)` for a plain append.
fn message_log_coalesce(
    ctx: &super::eval::Context,
    buf_id: crate::buffer::BufferId,
    msg: &crate::heap_types::LispString,
    old_full_end: crate::buffer::EmacsBytePos,
) -> (crate::heap_types::LispString, crate::buffer::EmacsBytePos) {
    use crate::buffer::{EmacsByteLen, EmacsByteRange};
    let no_coalesce = (msg.clone(), old_full_end);
    let Some(buf) = ctx.buffers.get(buf_id) else {
        return no_coalesce;
    };
    let bob = buf.full_emacs_byte_range().start();
    if old_full_end <= bob {
        return no_coalesce;
    }
    let msg_bytes = msg.as_bytes();
    let final_newline = old_full_end.saturating_sub_len(EmacsByteLen::new(1));
    if buf.buffer_substring_bytes_range(EmacsByteRange::new(final_newline, old_full_end)) != b"\n" {
        return no_coalesce;
    };
    let line_start = buf
        .prev_newline_emacs_byte(final_newline, bob)
        .map(|newline| newline.add_len(EmacsByteLen::new(1)))
        .unwrap_or(bob);
    let line = buf.buffer_substring_bytes_range(EmacsByteRange::new(line_start, final_newline));
    let (base, count) = parse_logged_message_line(&line);
    if base == msg_bytes {
        let mut new_bytes = msg_bytes.to_vec();
        new_bytes.extend_from_slice(format!(" [{} times]", count + 1).as_bytes());
        let new_text = if msg.is_multibyte() {
            crate::heap_types::LispString::from_emacs_bytes(new_bytes)
        } else {
            crate::heap_types::LispString::from_unibyte(new_bytes)
        };
        return (new_text, line_start);
    }
    if message_log_is_progress_update(&line, msg_bytes) {
        return (msg.clone(), line_start);
    }
    no_coalesce
}

/// Log a message to the *Messages* buffer, matching GNU Emacs message_dolog
/// in xdisp.c.  Creates the buffer if it doesn't exist.
fn message_dolog(ctx: &mut super::eval::Context, msg: &crate::heap_types::LispString) {
    // GNU: check message-log-max; if nil, don't log
    let log_max = ctx.visible_variable_value_or_nil("message-log-max");
    if log_max.is_nil() {
        return;
    }

    // GNU's `message3` passes the message to `message_dolog` as raw bytes
    // (`SSDATA`), so the *Messages* log never carries the echo-area text
    // properties (e.g. the `help-key-binding` face substitute-command-keys
    // adds). Strip them by rebuilding the string from its bytes.
    let plain = if msg.is_multibyte() {
        crate::heap_types::LispString::from_emacs_bytes(msg.as_bytes().to_vec())
    } else {
        crate::heap_types::LispString::from_unibyte(msg.as_bytes().to_vec())
    };
    let msg = &plain;

    // GNU xdisp.c defaults `messages-buffer-name` to "*Messages*" and lets
    // Lisp rebind it to redirect message logging.
    let messages_name = ctx
        .visible_variable_value_or_nil("messages-buffer-name")
        .as_str_owned()
        .unwrap_or_else(|| "*Messages*".to_string());
    let buf_id = if let Some(id) = ctx.buffers.find_buffer_by_name(&messages_name) {
        id
    } else {
        ctx.buffers.create_buffer(&messages_name)
    };

    // Insert the message text at the end, followed by newline.
    // Save and restore current buffer like GNU does.
    let old_buf = ctx.buffers.current_buffer().map(|b| b.id);
    let _ = ctx.set_current_buffer_unrecorded(buf_id);
    let Some((old_pt_byte, old_accessible, old_full_end, point_at_end, zv_at_end)) =
        ctx.buffers.get(buf_id).map(|buf| {
            let old_pt_byte = buf.point_emacs_byte_pos();
            let old_accessible = buf.accessible_region_snapshot();
            let old_full_end = buf.full_emacs_byte_range().end();
            let point_at_end = old_pt_byte == old_full_end;
            let zv_at_end = old_accessible.end_emacs_byte() == old_full_end;
            (
                old_pt_byte,
                old_accessible,
                old_full_end,
                point_at_end,
                zv_at_end,
            )
        })
    else {
        if let Some(old) = old_buf {
            ctx.restore_current_buffer_if_live(old);
        }
        return;
    };
    if let Some(full_range) = ctx.buffers.full_buffer_emacs_byte_range(buf_id) {
        let _ = ctx
            .buffers
            .restore_buffer_emacs_byte_restriction(buf_id, full_range);
    }
    // GNU `message_dolog` collapses consecutive identical messages into
    // "MSG [N times]" instead of logging a new line each time.
    let (log_text, delete_from) = message_log_coalesce(ctx, buf_id, msg, old_full_end);
    if ctx
        .buffers
        .goto_buffer_emacs_byte_pos(buf_id, delete_from)
        .is_some()
    {
        if delete_from.get() < old_full_end.get() {
            let del_range = crate::buffer::EmacsByteRange::new(delete_from, old_full_end);
            if let Ok(edit) =
                crate::emacs_core::editfns::buffer_edit_range_for_byte_range_in_manager(
                    &ctx.buffers,
                    buf_id,
                    del_range,
                )
            {
                let _ = ctx.buffers.delete_buffer_measured_region(buf_id, edit);
            }
        }
        // Issue #131: `*Messages*` content must stay byte-faithful. Always go
        // through the LispString insert path, which performs GNU's
        // `insert_from_string` byte-level multibyte conversion (raw eight-bit
        // bytes preserved as raw-byte chars) instead of a lossy Rust-String
        // round-trip that would corrupt them.
        let _ = ctx
            .buffers
            .insert_lisp_string_into_buffer(buf_id, &log_text);
        let _ = ctx.buffers.insert_into_buffer(buf_id, "\n");
    }

    if let Some(new_full_end) = ctx
        .buffers
        .get(buf_id)
        .map(|buf| buf.full_emacs_byte_range().end())
    {
        if zv_at_end {
            let _ = ctx
                .buffers
                .restore_buffer_accessible_region_with_current_full_end(buf_id, old_accessible);
        } else {
            let _ = ctx
                .buffers
                .restore_buffer_accessible_region(buf_id, old_accessible);
        }
        let restored_point = if point_at_end {
            new_full_end
        } else {
            old_pt_byte
        };
        let _ = ctx
            .buffers
            .goto_buffer_emacs_byte_pos(buf_id, restored_point);
    }
    if let Some(old) = old_buf {
        ctx.restore_current_buffer_if_live(old);
    }
}

impl super::eval::Context {
    /// Append a plain diagnostic to the configured messages buffer without
    /// changing the echo area.
    ///
    /// This is the Rust-side equivalent of GNU Emacs `add_to_log`: redisplay
    /// and other native subsystems use it for diagnostics that belong in
    /// `*Messages*`, but must not become the current echo-area message.
    pub fn add_to_log(&mut self, message: &str) {
        message_dolog(self, &crate::heap_types::LispString::from_utf8(message));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EchoMessageSetResult {
    EchoArea(crate::heap_types::LispString),
    LispHandled,
}

fn message_echo_result(
    ctx: &mut super::eval::Context,
    msg: &crate::heap_types::LispString,
) -> Result<EchoMessageSetResult, crate::emacs_core::error::Flow> {
    if ctx
        .visible_variable_value_or_nil("inhibit-message")
        .is_truthy()
    {
        return Ok(EchoMessageSetResult::LispHandled);
    }

    let set_message_function = ctx.visible_variable_value_or_nil("set-message-function");
    if set_message_function.is_nil()
        || ctx.gc_inhibit_depth > 0
        || builtin_functionp_1(ctx, set_message_function)?.is_nil()
    {
        return Ok(EchoMessageSetResult::EchoArea(msg.clone()));
    }

    // GNU xdisp.c `set_message` calls `set-message-function` through
    // `dsafe_call1`: redisplay is inhibited, quit is inhibited, and hook
    // errors are demoted instead of escaping from `message`.
    let specpdl_count = ctx.specpdl.len();
    ctx.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-redisplay"), Value::T)?;
    ctx.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-quit"), Value::T)?;
    let result = ctx.funcall_general(set_message_function, vec![Value::heap_string(msg.clone())]);
    let result = ctx.unbind_to_with_result(specpdl_count, result);

    let result = match result {
        Ok(result) => result,
        Err(err) => {
            tracing::warn!(
                "set-message-function signaled while setting echo message: {:?}",
                err
            );
            return Ok(EchoMessageSetResult::EchoArea(msg.clone()));
        }
    };
    if result.is_nil() {
        return Ok(EchoMessageSetResult::EchoArea(msg.clone()));
    }
    if let Some(string) = ctx.lisp_string(result) {
        return Ok(EchoMessageSetResult::EchoArea(string.clone()));
    }
    Ok(EchoMessageSetResult::LispHandled)
}

/// Whether `message_to_stderr` ends with a newline (GNU src/xdisp.c:12579-12602).
///
/// GNU writes the message text only when it is a string, and then emits the
/// trailing newline `if (STRINGP (m) || !cursor_in_echo_area)`.  The comment
/// above the function states the consequence plainly: "Log the message M to
/// stderr.  Log an empty line if M is not a string."  So `(message nil)` in
/// batch is not silent -- it prints a bare newline unless the cursor is in the
/// echo area, and that is what makes each keystroke of a keyboard macro visible
/// on stderr, since the command loop clears the echo area per iteration.
pub(crate) fn stderr_message_ends_with_newline(
    message_is_string: bool,
    cursor_in_echo_area: bool,
) -> bool {
    message_is_string || !cursor_in_echo_area
}

pub(crate) fn builtin_message(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("message", &args, 1)?;
    // GNU Emacs: nil or empty string clears the echo area and returns as-is.
    // GNU routes both through `message1 (0)' -> `message3 (Qnil)', which logs
    // and then, unless `inhibit-message', reaches `message_to_stderr (Qnil)' in
    // batch -- so the clear is not silent there.
    if args[0].is_nil() {
        clear_echo_area_and_report_to_stderr(ctx);
        return Ok(Value::NIL);
    }
    if args[0].is_string()
        && args[0]
            .as_lisp_string()
            .expect("string")
            .as_bytes()
            .is_empty()
    {
        clear_echo_area_and_report_to_stderr(ctx);
        return Ok(args[0]);
    }
    // GNU Emacs's `message` ALWAYS calls `format-message` on the args,
    // even for a single string argument.  This converts %% -> % and
    // applies text-quoting (curly quotes).
    let formatted = super::strings::builtin_format_message(ctx, args.clone())?;
    // GNU Fmessage returns the formatted Lisp object after display/logging
    // side effects. Keep that object rooted while those side effects allocate.
    let root_scope = ctx.save_vm_roots();
    ctx.push_vm_frame_root(formatted);
    let msg = match ctx.lisp_string(formatted) {
        Some(string) => string.clone(),
        None => crate::heap_types::LispString::from_emacs_bytes(Vec::new()),
    };
    let side_effects = (|| {
        // GNU xdisp.c `message3` logs to *Messages* (`log_message`) FIRST and
        // unconditionally (independent of `inhibit-message`).
        message_dolog(ctx, &msg);
        tracing::info!(msg = %crate::emacs_core::emacs_char::to_utf8_lossy(msg.as_bytes()));

        // GNU xdisp.c `message3_frame_nolog`: when the selected frame is the
        // initial frame (FRAME_INITIAL_P, i.e. batch / noninteractive), the
        // message is *only* sent to stderr via `message_to_stderr`.
        // `set_message` is NOT called (so `set-message-function` never runs), the
        // echo-area buffer stays empty, and `current-message` (which reads that
        // buffer) returns nil.  Only the interactive branch
        // (INTERACTIVE && glyphs_initialized_p) populates the echo area.
        if ctx.noninteractive() {
            // GNU `message3`: `if (! inhibit_message) message3_nolog (m);` — an
            // inhibited message is logged to *Messages* only, never printed.
            let inhibit_message = ctx
                .visible_variable_value_or_nil("inhibit-message")
                .is_truthy();
            if !inhibit_message {
                use std::io::Write;
                let text = crate::emacs_core::emacs_char::to_utf8_lossy(msg.as_bytes());
                let _ = std::io::stderr().write_all(text.as_bytes());
                let _ = std::io::stderr().write_all(b"\n");
                let _ = std::io::stderr().flush();
            }
            return Ok(());
        }

        // Interactive display path (GNU `set_message`): consult
        // `set-message-function`, then materialize the echo-area buffers and
        // store the message so `current-message` can read it back.
        let displayed_message = message_echo_result(ctx, &msg)?;
        if matches!(displayed_message, EchoMessageSetResult::EchoArea(_)) {
            ctx.ensure_echo_area_buffers();
        }
        match &displayed_message {
            EchoMessageSetResult::EchoArea(displayed) => {
                ctx.set_current_message(Some(displayed.clone()))
            }
            EchoMessageSetResult::LispHandled => ctx.discard_current_message_without_clear_hook(),
        }
        Ok(())
    })();
    ctx.restore_vm_roots(root_scope);
    side_effects?;
    Ok(formatted)
}

/// GNU `message3 (Qnil)` for the echo-area-clearing case: log, then unless
/// `inhibit-message' reach `message3_nolog' -- which in batch is
/// `message_to_stderr', printing the empty line described above.
fn clear_echo_area_and_report_to_stderr(ctx: &mut super::eval::Context) {
    ctx.clear_echo_area_message();
    if !ctx.noninteractive() {
        return;
    }
    if ctx
        .visible_variable_value_or_nil("inhibit-message")
        .is_truthy()
    {
        return;
    }
    let cursor_in_echo_area = ctx
        .visible_variable_value_or_nil("cursor-in-echo-area")
        .is_truthy();
    if stderr_message_ends_with_newline(false, cursor_in_echo_area) {
        use std::io::Write;
        let mut err = std::io::stderr();
        let _ = err.write_all(b"\n");
        let _ = err.flush();
    }
}

pub(crate) fn builtin_message_box(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("message-box", &args, 1)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    // GNU Emacs: always calls format-message, even for single-arg.
    let formatted = super::strings::builtin_format_message(ctx, args.clone())?;
    if let Some(ls) = ctx.lisp_string(formatted) {
        tracing::info!(msg = %crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()));
    }
    Ok(formatted)
}

pub(crate) fn builtin_message_or_box(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("message-or-box", &args, 1)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }
    // GNU Emacs: always calls format-message, even for single-arg.
    let formatted = super::strings::builtin_format_message(ctx, args.clone())?;
    if let Some(ls) = ctx.lisp_string(formatted) {
        tracing::info!(msg = %crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()));
    }
    Ok(formatted)
}

pub(crate) fn builtin_current_message(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-message", &args, 0)?;
    Ok(ctx.current_message_value().unwrap_or(Value::NIL))
}

pub(crate) fn builtin_daemonp(args: Vec<Value>) -> EvalResult {
    expect_args("daemonp", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_daemon_initialized(args: Vec<Value>) -> EvalResult {
    expect_args("daemon-initialized", &args, 0)?;
    Err(signal(
        "error",
        vec![Value::string(
            "This function can only be called if emacs is run as a daemon",
        )],
    ))
}

pub(crate) fn builtin_documentation_stringp(args: Vec<Value>) -> EvalResult {
    expect_args("documentation-stringp", &args, 1)?;
    let is_compiled_ref = match args[0].kind() {
        ValueKind::Cons => {
            let pair_car = args[0].cons_car();
            let pair_cdr = args[0].cons_cdr();
            pair_car.is_string() && pair_cdr.as_int().is_some()
        }
        _ => false,
    };
    Ok(Value::bool_val(
        (args[0].is_string() || args[0].is_fixnum()) || is_compiled_ref,
    ))
}

pub(crate) fn builtin_flush_standard_output(args: Vec<Value>) -> EvalResult {
    expect_args("flush-standard-output", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_force_mode_line_update(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("force-mode-line-update", &args, 1)?;
    ctx.invalidate_redisplay();
    // GNU `Fforce_mode_line_update` (buffer.c) raises the mode-line dirty
    // flag as well as forcing a redisplay: without ALL it is
    // `bset_update_mode_line` on the current buffer, with ALL it is the
    // global `update_mode_lines = 10`. Both reach every window showing the
    // buffer, which is what `mark_chrome_dirty_all` models.
    ctx.mark_chrome_dirty_all();
    Ok(args.first().cloned().unwrap_or(Value::NIL))
}

pub(crate) fn builtin_get_internal_run_time(args: Vec<Value>) -> EvalResult {
    expect_args("get-internal-run-time", &args, 0)?;

    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let usecs = dur.subsec_micros() as i64;
    Ok(Value::list(vec![
        Value::fixnum(secs >> 16),
        Value::fixnum(secs & 0xFFFF),
        Value::fixnum(usecs),
        Value::fixnum(0),
    ]))
}

pub(crate) fn builtin_invocation_directory(
    ctx: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("invocation-directory", &args, 0)?;
    let value = ctx.eval_symbol_by_id(super::super::intern::intern("invocation-directory"))?;
    builtin_copy_sequence(vec![value])
}

pub(crate) fn builtin_invocation_name(
    ctx: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("invocation-name", &args, 0)?;
    let value = ctx.eval_symbol_by_id(super::super::intern::intern("invocation-name"))?;
    builtin_copy_sequence(vec![value])
}

pub(crate) fn builtin_secure_hash_algorithms(args: Vec<Value>) -> EvalResult {
    expect_args("secure-hash-algorithms", &args, 0)?;
    Ok(Value::list(vec![
        Value::symbol("md5"),
        Value::symbol("sha1"),
        Value::symbol("sha224"),
        Value::symbol("sha256"),
        Value::symbol("sha384"),
        Value::symbol("sha512"),
    ]))
}

pub(crate) fn builtin_symbol_name_1(eval: &mut super::eval::Context, symbol: Value) -> EvalResult {
    builtin_symbol_name_value(symbol, eval.symbols_with_pos_enabled)
}

/// GNU `SYMBOL_NAME (arg)`: a symbol's name as the STRING OBJECT it was interned
/// from, carrying that string's text properties. `None` for a non-symbol.
///
/// `format`'s `%s` needs exactly this, and needs it to be the same object
/// `symbol-name` returns -- printing the name afresh would drop the properties a
/// symbol interned from buffer text carries.
pub(crate) fn symbol_name_string_for_format(value: Value) -> Option<Value> {
    // Plain symbols only: a symbol-with-position keeps `format`'s existing
    // printed representation, as it does in GNU when symbols-with-pos are not
    // enabled. NOT via builtin_symbol_name_value: do_format probes EVERY
    // argument through here, and constructing (then discarding) that
    // function's wrong-type-argument signal — which interns `symbolp` — for
    // every fixnum argument was a measured per-format cost.
    let id = super::symbols::symbol_id_checked(&value, false)?;
    Some(crate::emacs_core::intern::materialize_symbol_name_value(id))
}

fn builtin_symbol_name_value(symbol: Value, symbols_with_pos_enabled: bool) -> EvalResult {
    match super::symbols::symbol_id_checked(&symbol, symbols_with_pos_enabled) {
        Some(id) => Ok(crate::emacs_core::intern::materialize_symbol_name_value(id)),
        None => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), symbol],
        )),
    }
}

pub(crate) fn builtin_make_symbol_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    make_symbol_value(arg)
}

fn make_symbol_value(arg: Value) -> EvalResult {
    expect_lisp_string(&arg)?;
    Ok(Value::from_sym_id(
        crate::emacs_core::intern::make_uninterned_symbol_with_name_value(arg),
    ))
}
