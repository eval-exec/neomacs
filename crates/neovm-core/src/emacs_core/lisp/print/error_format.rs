//! Evaluator-aware value and error-result formatting.
//!
//! This is the printing half of the error boundary. Rendering a value for an
//! error message needs the printer, the evaluator's variable view, and the
//! buffer/window/thread managers for opaque handles. It lives above `error`
//! (which keeps only the typed payloads) so the bottom `neovm-value` layer
//! never reaches up into the printer.

use std::cell::RefCell;

use strum::{EnumString, IntoStaticStr};

use super::PrintOptions;
use crate::buffer::EmacsBytePos;
use crate::emacs_core::error::{
    EvalError, EvalResult, Flow, FlowKind, FlowRef, LispCondition, SignalData, signal,
};
use crate::emacs_core::errors::signal_matches_condition_value_sym;
use crate::emacs_core::intern::format_symbol_name_for_diagnostic;
use crate::emacs_core::intern::intern;
use crate::emacs_core::string_escape::{format_lisp_string_bytes_emacs, format_lisp_string_emacs};
use crate::emacs_core::value::{Value, ValueKind, VecLikeType};
use crate::window::WindowId;
use std::fmt::{self, Display, Formatter};

thread_local! {
    static FORMAT_OBJECT_STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

/// Format an eval result for the compat test harness (TSV output).
pub fn format_eval_result(result: &Result<Value, EvalError>) -> String {
    match result {
        Ok(value) => format!("OK {}", super::print_value(value)),
        Err(EvalError::Signal {
            symbol,
            data,
            raw_data,
            ..
        }) => {
            let payload = format_signal_payload(raw_data.as_ref(), data);
            format!(
                "ERR ({} {})",
                format_symbol_name_for_diagnostic(*symbol),
                payload
            )
        }
        Err(EvalError::UncaughtThrow { tag, value, .. }) => {
            format!(
                "ERR (no-catch ({} {}))",
                super::print_value(tag),
                super::print_value(value),
            )
        }
        Err(EvalError::Shutdown(request)) => format!("ERR (kill-emacs {})", request.exit_code),
    }
}

fn format_signal_payload(raw_data: Option<&Value>, data: &[Value]) -> String {
    if let Some(raw) = raw_data {
        return super::print_value(raw);
    }
    if data.is_empty() {
        "nil".to_string()
    } else {
        super::print_value(&Value::list(data.to_vec()))
    }
}

pub(crate) fn format_opaque_handle_in_state(
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
) -> Option<String> {
    if let Some(handle) = crate::emacs_core::terminal::pure::print_terminal_handle(value) {
        return Some(handle);
    }
    if crate::emacs_core::marker::is_marker(value)
        && let Some(marker) = value.as_marker_data()
    {
        let mut out = String::from("#<marker ");
        if marker.insertion_type() {
            out.push_str("(moves after insertion) ");
        }
        // T7: read authoritative charpos (1-based Lisp shape) directly
        // from LispMarker, not the deleted stale `position` cache.
        if let Some(buffer_id) = marker.buffer_id()
            && let Some(buffer) = buffers.get(buffer_id)
        {
            out.push_str(&format!(
                "at {} in {}",
                marker.charpos.get() + 1,
                buffer.name_runtime_string_owned()
            ));
        } else {
            out.push_str("in no buffer");
        }
        out.push('>');
        return Some(out);
    }
    if let Some(overlay) = value.as_overlay_data() {
        if let Some(buffer_id) = overlay.buffer
            && let Some(buffer) = buffers.get(buffer_id)
        {
            let (start, end) = overlay.current_range();
            return Some(format!(
                "#<overlay from {} to {} in {}>",
                buffer
                    .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(start))
                    .as_i64(),
                buffer
                    .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(end))
                    .as_i64(),
                buffer.name_runtime_string_owned()
            ));
        }
        return Some("#<overlay in no buffer>".to_string());
    }
    if let Some(id) = value.as_window_id() {
        return Some(format_window_handle_in_state(buffers, frames, WindowId(id)));
    }
    if let Some(id) = threads.thread_id_from_handle(value) {
        return Some(format!("#<thread {id}>"));
    }
    if let Some(id) = threads.mutex_id_from_handle(value) {
        return Some(format!("#<mutex {id}>"));
    }
    if let Some(id) = threads.condition_variable_id_from_handle(value) {
        return Some(format!("#<condvar {id}>"));
    }
    if let Some(buf_id) = value.as_buffer_id() {
        if let Some(buf) = buffers.get(buf_id) {
            return Some(format!("#<buffer {}>", buf.name_runtime_string_owned()));
        }
        if buffers.dead_buffer_last_name_value(buf_id).is_some() {
            return Some("#<killed buffer>".to_string());
        }
    }
    None
}

pub(crate) fn format_window_handle_in_state(
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    window_id: WindowId,
) -> String {
    let sequence = frames.window_sequence_number(window_id);
    if let Some(frame_id) = frames.find_window_frame_id(window_id)
        && let Some(frame) = frames.get(frame_id)
        && let Some(window) = frame.find_window(window_id)
    {
        if let Some(buffer_id) = window.buffer_id()
            && let Some(buffer) = buffers.get(buffer_id)
        {
            return format!(
                "#<window {sequence} on {}>",
                buffer.name_runtime_string_owned()
            );
        }
        return format!(
            "#<window {sequence} on {}>",
            frame.name_runtime_string_owned()
        );
    }
    format!("#<window {sequence}>")
}

/// Build print options the way GNU's printer reads them.
///
/// Every `print-*` name here is a `DEFVAR_LISP` / `DEFVAR_BOOL` that GNU
/// dereferences as a bare C global -- `Vprint_level` at `src/print.c:2531`,
/// `Vprint_length` at `:2256`, and so on -- and the swap-in
/// (`src/data.c:1573-1603`) keeps every one of those globals equal to the
/// *current buffer's* binding. So the reads name the buffer (ledger 196);
/// `lisp/eshell/esh-mode.el` localises `print-level` and `print-length`.
///
/// This is narrower than it looks: when the print STREAM is a buffer, GNU's
/// `PRINTPREPARE` does `set_buffer_internal` on the stream's buffer first,
/// which swaps the binding back OUT -- which is why `prin1-to-string` and
/// `error-message-string` (`src/print.c:1058`, printing into
/// `Vprin1_to_string_buffer`) do not honour a buffer-local `print-level` in
/// either editor. Only a non-buffer stream keeps it swapped in.
///
/// With specbind, dynamic let-bindings are written directly to the obarray,
/// so this correctly handles (let ((print-escape-newlines t)) (format "%S" ...)).
pub(crate) fn print_options_from_state(
    vars: crate::emacs_core::eval::VariableView<'_>,
    buf: Option<&crate::buffer::Buffer>,
) -> PrintOptions {
    let print_gensym = vars
        .value_in_buffer_by_name(buf, "print-gensym")
        .is_some_and(|v| v.is_truthy());
    let print_circle = vars
        .value_in_buffer_by_name(buf, "print-circle")
        .is_some_and(|v| v.is_truthy());
    let print_quoted = vars
        .value_in_buffer_by_name(buf, "print-quoted")
        .is_none_or(|v| v.is_truthy());
    let print_symbols_bare = vars
        .value_in_buffer_by_name(buf, "print-symbols-bare")
        .is_some_and(|v| v.is_truthy());
    let print_escape_newlines = vars
        .value_in_buffer_by_name(buf, "print-escape-newlines")
        .is_some_and(|v| v.is_truthy());
    let print_level = vars
        .value_in_buffer_by_name(buf, "print-level")
        .and_then(|v| v.as_fixnum());
    let print_length = vars
        .value_in_buffer_by_name(buf, "print-length")
        .and_then(|v| v.as_fixnum())
        .filter(|&n| n >= 0);
    let print_escape_nonascii = vars
        .value_in_buffer_by_name(buf, "print-escape-nonascii")
        .is_some_and(|v| v.is_truthy());
    let print_escape_multibyte = vars
        .value_in_buffer_by_name(buf, "print-escape-multibyte")
        .is_some_and(|v| v.is_truthy());
    let print_escape_control_characters = vars
        .value_in_buffer_by_name(buf, "print-escape-control-characters")
        .is_some_and(|v| v.is_truthy());
    let print_integers_as_characters = vars
        .value_in_buffer_by_name(buf, "print-integers-as-characters")
        .is_some_and(|v| v.is_truthy());
    let print_continuous_numbering = vars
        .value_in_buffer_by_name(buf, "print-continuous-numbering")
        .is_some_and(|v| v.is_truthy());
    let print_number_table = if print_circle {
        vars.value_in_buffer_by_name(buf, "print-number-table")
            .filter(|v| v.is_hash_table())
    } else {
        None
    };
    let mut opts = PrintOptions::new(print_gensym, print_circle, print_level, print_length);
    opts.charset_text_property = vars
        .value_in_buffer_by_name(buf, "print-charset-text-property")
        .map_or(super::CharsetTextProperty::Guessable, Into::into);
    opts.unreadable_function = vars
        .value_in_buffer_by_name(buf, "print-unreadable-function")
        .filter(|v| !v.is_nil());
    opts.print_quoted = print_quoted;
    opts.print_symbols_bare = print_symbols_bare;
    opts.print_escape_newlines = print_escape_newlines;
    opts.print_escape_nonascii = print_escape_nonascii;
    opts.print_escape_multibyte = print_escape_multibyte;
    opts.print_escape_control_characters = print_escape_control_characters;
    opts.print_integers_as_characters = print_integers_as_characters;
    opts.print_continuous_numbering = print_continuous_numbering;
    opts.print_number_table = print_number_table;
    opts.float_output_format = vars
        .value_in_buffer_by_name(buf, "float-output-format")
        .filter(|v| v.is_string());
    opts
}

pub(crate) fn print_value_in_state(
    ctx: &crate::emacs_core::eval::Context,
    value: &Value,
) -> String {
    // GNU renders a value to a Rust-side String the way `Ferror_message_string`
    // does: through `Vprin1_to_string_buffer` (`src/print.c:1058`).  That is a
    // buffer stream, so `PRINTPREPARE` makes it current and the caller's
    // buffer-local `print-level` / `print-length` are swapped out. Ledger 196.
    print_value_in_state_with_options(ctx, value, print_options_from_state(ctx.variables(), None))
}

pub(crate) fn print_value_in_state_with_options(
    ctx: &crate::emacs_core::eval::Context,
    value: &Value,
    options: PrintOptions,
) -> String {
    format_value_in_state(
        &ctx.obarray,
        &ctx.buffers,
        &ctx.frames,
        &ctx.threads,
        value,
        options,
    )
}

fn format_cycle_stack_index(value: &Value) -> Option<usize> {
    let key = super::default_cycle_candidate_key(value)?;
    FORMAT_OBJECT_STACK.with(|stack| stack.borrow().iter().position(|entry| *entry == key))
}

fn push_format_cycle_object(value: &Value) -> bool {
    let Some(key) = super::default_cycle_candidate_key(value) else {
        return false;
    };
    FORMAT_OBJECT_STACK.with(|stack| stack.borrow_mut().push(key));
    true
}

fn pop_format_cycle_object(pushed: bool) {
    if pushed {
        FORMAT_OBJECT_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

fn format_object_stack_len() -> usize {
    FORMAT_OBJECT_STACK.with(|stack| stack.borrow().len())
}

fn truncate_format_object_stack(len: usize) {
    FORMAT_OBJECT_STACK.with(|stack| stack.borrow_mut().truncate(len));
}

fn format_value_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> String {
    let _print_guard = super::enter_print_call(&options);
    if let Some(handle) = format_opaque_handle_in_state(buffers, frames, threads, value) {
        return handle;
    }
    // Use the stateful printer when print-circle, print-level, or print-length
    // are active. This ensures correct handling of shared structure, depth
    // limiting, and length limiting throughout the entire value tree.
    if options.print_circle || options.print_level.is_some() || options.print_length.is_some() {
        return super::print_value_stateful_with_buffers(value, Some(buffers), options);
    }
    match value.kind() {
        ValueKind::Cons
        | ValueKind::String
        | ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::Record) => {
            if let Some(index) = format_cycle_stack_index(value) {
                return format!("#{index}");
            }
            let pushed = push_format_cycle_object(value);
            let rendered =
                format_value_in_state_slow(obarray, buffers, frames, threads, value, options);
            pop_format_cycle_object(pushed);
            rendered
        }
        _ => super::print_value_with_options(value, options),
    }
}

fn format_value_in_state_slow(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> String {
    match value.kind() {
        ValueKind::String => {
            let ls = value.as_lisp_string().expect("checked string");
            if options.print_noescape {
                crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes())
            } else if let Some(runs) =
                super::printed_string_property_runs(*value, options.charset_text_property)
            {
                let mut out = String::from("#(");
                out.push_str(&format_lisp_string_emacs(ls, &options));
                for run in runs {
                    out.push(' ');
                    out.push_str(&run.start.to_string());
                    out.push(' ');
                    out.push_str(&run.end.to_string());
                    out.push(' ');
                    out.push_str(&format_value_in_state(
                        obarray, buffers, frames, threads, &run.plist, options,
                    ));
                }
                out.push(')');
                out
            } else {
                format_lisp_string_emacs(ls, &options)
            }
        }
        ValueKind::Cons => {
            if let Some(shorthand) =
                format_list_shorthand_in_state(obarray, buffers, frames, threads, value, options)
            {
                return shorthand;
            }
            let mut out = String::from("(");
            format_cons_in_state(obarray, buffers, frames, threads, value, &mut out, options);
            out.push(')');
            out
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let mut out = String::from("[");
            let items = value.as_vector_data().unwrap().clone();
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(' ');
                }
                out.push_str(&format_value_in_state(
                    obarray, buffers, frames, threads, item, options,
                ));
            }
            out.push(']');
            out
        }
        ValueKind::Veclike(VecLikeType::Record) => {
            let mut out = String::from("#s(");
            let items = value.as_record_data().unwrap().clone();
            for (idx, item) in items.iter().enumerate() {
                if let Some(length) = options.print_length
                    && idx as i64 >= length
                {
                    if idx > 0 {
                        out.push(' ');
                    }
                    out.push_str("...");
                    break;
                }
                if idx > 0 {
                    out.push(' ');
                }
                out.push_str(&format_value_in_state(
                    obarray, buffers, frames, threads, item, options,
                ));
            }
            out.push(')');
            out
        }
        _ => super::print_value_with_options(value, options),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
pub(crate) enum PrintShorthandSymbol {
    #[strum(serialize = "quote")]
    Quote,
    #[strum(serialize = "function")]
    Function,
    #[strum(serialize = "`")]
    Backquote,
    #[strum(serialize = ",")]
    Comma,
    #[strum(serialize = ",@")]
    CommaAt,
}

impl PrintShorthandSymbol {
    pub(crate) fn from_lisp_value(value: &Value) -> Option<Self> {
        value.as_bare_symbol_name()?.parse().ok()
    }

    fn string_form(
        self,
        payload: &Value,
        options: PrintOptions,
    ) -> Option<(&'static str, &Value, PrintOptions)> {
        match self {
            Self::Quote => Some(("'", payload, options)),
            Self::Function => Some(("#'", payload, options)),
            Self::Backquote => Some(("`", payload, options.enter_backquote())),
            Self::Comma => options.allow_unquote_shorthand().then_some((
                ",",
                payload,
                options.exit_backquote(),
            )),
            Self::CommaAt => options.allow_unquote_shorthand().then_some((
                ",@",
                payload,
                options.exit_backquote(),
            )),
        }
    }

    fn bytes_form(
        self,
        payload: &Value,
        options: PrintOptions,
    ) -> Option<(&'static [u8], &Value, PrintOptions)> {
        match self {
            Self::Quote => Some((b"'" as &[u8], payload, options)),
            Self::Function => Some((b"#'" as &[u8], payload, options)),
            Self::Backquote => Some((b"`" as &[u8], payload, options.enter_backquote())),
            Self::Comma => options.allow_unquote_shorthand().then_some((
                b"," as &[u8],
                payload,
                options.exit_backquote(),
            )),
            Self::CommaAt => options.allow_unquote_shorthand().then_some((
                b",@" as &[u8],
                payload,
                options.exit_backquote(),
            )),
        }
    }

    #[cfg(test)]
    pub(crate) fn name(self) -> &'static str {
        self.into()
    }
}

/// Name-based quote-shorthand payload test, as this printer has always used.
///
/// [`super::quote_payload`] compares symbol identity, which is what GNU's
/// `EQ (XCAR (obj), Qquote)` does for the ordinary printer. This stateful
/// path predates that helper and resolves the shorthand by symbol NAME; seam
/// 2 moves it without changing its output.
fn quote_payload(value: &Value) -> Option<Value> {
    let items = crate::emacs_core::value::list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }
    if PrintShorthandSymbol::from_lisp_value(&items[0]) == Some(PrintShorthandSymbol::Quote) {
        Some(items[1])
    } else {
        None
    }
}

fn format_list_shorthand_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Option<String> {
    let items = crate::emacs_core::value::list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }

    if items[0].as_bare_symbol_name() == Some("make-hash-table-from-literal") {
        let payload = quote_payload(&items[1])?;
        return Some(format!(
            "#s{}",
            format_value_in_state(obarray, buffers, frames, threads, &payload, options)
        ));
    }

    let shorthand = PrintShorthandSymbol::from_lisp_value(&items[0])?;
    if !options.print_quoted {
        return None;
    }

    let (prefix, quoted, nested_options) = shorthand.string_form(&items[1], options)?;

    Some(format!(
        "{prefix}{}",
        format_value_in_state(obarray, buffers, frames, threads, quoted, nested_options)
    ))
}

fn format_cons_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
    out: &mut String,
    options: PrintOptions,
) {
    let mut cursor = *value;
    let mut first = true;
    let mut maxlen = options.print_length.unwrap_or(i64::MAX);
    let mut tortoise = *value;
    let mut n: i64 = 2;
    let mut m: i64 = 2;
    let mut tortoise_idx: i64 = 0;
    let stack_len = format_object_stack_len();
    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                if first {
                    if maxlen == 0 {
                        out.push_str("...");
                        truncate_format_object_stack(stack_len);
                        return;
                    }
                } else {
                    out.push(' ');
                    maxlen = maxlen.saturating_sub(1);
                    if maxlen <= 0 {
                        out.push_str("...");
                        truncate_format_object_stack(stack_len);
                        return;
                    }

                    n -= 1;
                    if n == 0 {
                        tortoise_idx = tortoise_idx.saturating_add(m);
                        m = m.saturating_mul(2);
                        n = m;
                        tortoise = cursor;
                    } else if crate::emacs_core::value::eq_value(&cursor, &tortoise) {
                        out.push_str(&format!(". #{tortoise_idx}"));
                        truncate_format_object_stack(stack_len);
                        return;
                    }
                }
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                out.push_str(&format_value_in_state(
                    obarray, buffers, frames, threads, &pair_car, options,
                ));
                cursor = pair_cdr;
                first = false;
            }
            ValueKind::Nil => {
                truncate_format_object_stack(stack_len);
                return;
            }
            _ => {
                if !first {
                    out.push_str(" . ");
                }
                out.push_str(&format_value_in_state(
                    obarray, buffers, frames, threads, &cursor, options,
                ));
                truncate_format_object_stack(stack_len);
                return;
            }
        }
    }
}

pub(crate) fn print_value_bytes_in_state(
    vars: crate::emacs_core::eval::VariableView<'_>,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
) -> Vec<u8> {
    let obarray = vars.obarray();
    if let Some(handle) = format_opaque_handle_in_state(buffers, frames, threads, value) {
        return handle.into_bytes();
    }
    format_value_bytes_in_state_with_options(
        obarray,
        buffers,
        frames,
        threads,
        value,
        // Same `Vprin1_to_string_buffer` stream as `print_value_in_state`.
        print_options_from_state(vars, None),
    )
}

/// Byte-producing sibling of [`print_value_in_state_with_options`].  Renders a
/// value as canonical Emacs internal-encoding bytes (eight-bit/non-Unicode as
/// disjoint extended sequences), the form `prin1`/`print` feed straight to the
/// byte print sink (issue #131).
pub(crate) fn print_value_bytes_in_state_with_options(
    ctx: &crate::emacs_core::eval::Context,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    format_value_bytes_in_state_with_options(
        &ctx.obarray,
        &ctx.buffers,
        &ctx.frames,
        &ctx.threads,
        value,
        options,
    )
}

pub(crate) fn format_value_bytes_in_state_with_options(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    let _print_guard = super::enter_print_call(&options);
    if options.print_noescape
        && let Some(id) = value.as_buffer_id()
        && let Some(buffer) = buffers.get(id)
        && let Some(name) = buffer.name_value().as_lisp_string()
    {
        return super::princ_string_bytes(name, options.princ_string_output()).into_owned();
    }
    if let Some(handle) = format_opaque_handle_in_state(buffers, frames, threads, value) {
        return handle.into_bytes();
    }
    // Use the stateful printer when print-circle, print-level, or print-length
    // are active. Its canonical sink is byte-based so byte8/non-Unicode
    // characters survive graph preprocessing and depth/length limiting.
    if options.print_circle || options.print_level.is_some() || options.print_length.is_some() {
        return super::print_value_stateful_bytes_with_buffers(value, Some(buffers), options);
    }
    match value.kind() {
        ValueKind::Cons => {
            if let Some(index) = format_cycle_stack_index(value) {
                return format!("#{index}").into_bytes();
            }
            let pushed = push_format_cycle_object(value);
            let rendered =
                format_cons_bytes_in_state(obarray, buffers, frames, threads, value, options);
            pop_format_cycle_object(pushed);
            rendered
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            if let Some(index) = format_cycle_stack_index(value) {
                return format!("#{index}").into_bytes();
            }
            let pushed = push_format_cycle_object(value);
            let rendered =
                format_vector_bytes_in_state(obarray, buffers, frames, threads, value, options);
            pop_format_cycle_object(pushed);
            rendered
        }
        ValueKind::String => {
            if let Some(index) = format_cycle_stack_index(value) {
                return format!("#{index}").into_bytes();
            }
            let pushed = push_format_cycle_object(value);
            let rendered =
                format_string_bytes_in_state(obarray, buffers, frames, threads, value, options);
            pop_format_cycle_object(pushed);
            rendered
        }
        ValueKind::Veclike(VecLikeType::Record) => {
            if let Some(index) = format_cycle_stack_index(value) {
                return format!("#{index}").into_bytes();
            }
            let pushed = push_format_cycle_object(value);
            let rendered =
                format_record_bytes_in_state(obarray, buffers, frames, threads, value, options);
            pop_format_cycle_object(pushed);
            rendered
        }
        _ => super::print_value_bytes_with_options(value, options),
    }
}

fn format_string_bytes_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    let ls = value.as_lisp_string().expect("checked string");
    if options.print_noescape {
        return super::princ_string_bytes(ls, options.princ_string_output()).into_owned();
    }
    let str_bytes = format_lisp_string_bytes_emacs(ls, &options);
    let Some(runs) = super::printed_string_property_runs(*value, options.charset_text_property)
    else {
        return str_bytes;
    };
    let mut out = Vec::new();
    out.extend_from_slice(b"#(");
    out.extend_from_slice(&str_bytes);
    for run in runs {
        out.push(b' ');
        out.extend_from_slice(run.start.to_string().as_bytes());
        out.push(b' ');
        out.extend_from_slice(run.end.to_string().as_bytes());
        out.push(b' ');
        out.extend(format_value_bytes_in_state_with_options(
            obarray, buffers, frames, threads, &run.plist, options,
        ));
    }
    out.push(b')');
    out
}

fn format_cons_bytes_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    if let Some(shorthand) =
        format_list_shorthand_bytes_in_state(obarray, buffers, frames, threads, value, options)
    {
        return shorthand;
    }
    let mut out = Vec::new();
    out.push(b'(');
    append_cons_bytes_in_state(obarray, buffers, frames, threads, value, &mut out, options);
    out.push(b')');
    out
}

fn format_vector_bytes_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(b'[');
    let Some(values) = value.as_vector_data() else {
        out.push(b']');
        return out;
    };
    for (idx, item) in values.iter().enumerate() {
        if idx > 0 {
            out.push(b' ');
        }
        out.extend(format_value_bytes_in_state_with_options(
            obarray, buffers, frames, threads, item, options,
        ));
    }
    out.push(b']');
    out
}

fn format_record_bytes_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"#s(");
    let Some(values) = value.as_record_data() else {
        out.push(b')');
        return out;
    };
    for (idx, item) in values.iter().enumerate() {
        if let Some(length) = options.print_length
            && idx as i64 >= length
        {
            if idx > 0 {
                out.push(b' ');
            }
            out.extend_from_slice(b"...");
            break;
        }
        if idx > 0 {
            out.push(b' ');
        }
        out.extend(format_value_bytes_in_state_with_options(
            obarray, buffers, frames, threads, item, options,
        ));
    }
    out.push(b')');
    out
}

fn format_list_shorthand_bytes_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
    options: PrintOptions,
) -> Option<Vec<u8>> {
    let items = crate::emacs_core::value::list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }

    if items[0].as_bare_symbol_name() == Some("make-hash-table-from-literal") {
        let payload = quote_payload(&items[1])?;
        let mut out = Vec::new();
        out.extend_from_slice(b"#s");
        out.extend(format_value_bytes_in_state_with_options(
            obarray, buffers, frames, threads, &payload, options,
        ));
        return Some(out);
    }

    let shorthand = PrintShorthandSymbol::from_lisp_value(&items[0])?;
    if !options.print_quoted {
        return None;
    }

    let (prefix, quoted, nested_options) = shorthand.bytes_form(&items[1], options)?;

    let mut out = Vec::new();
    out.extend_from_slice(prefix);
    out.extend(format_value_bytes_in_state_with_options(
        obarray,
        buffers,
        frames,
        threads,
        quoted,
        nested_options,
    ));
    Some(out)
}

fn append_cons_bytes_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    frames: &crate::window::FrameManager,
    threads: &crate::emacs_core::threads::ThreadManager,
    value: &Value,
    out: &mut Vec<u8>,
    options: PrintOptions,
) {
    let mut cursor = *value;
    let mut first = true;
    let mut maxlen = options.print_length.unwrap_or(i64::MAX);
    let mut tortoise = *value;
    let mut n: i64 = 2;
    let mut m: i64 = 2;
    let mut tortoise_idx: i64 = 0;
    let stack_len = format_object_stack_len();
    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                if first {
                    if maxlen == 0 {
                        out.extend_from_slice(b"...");
                        truncate_format_object_stack(stack_len);
                        return;
                    }
                } else {
                    out.push(b' ');
                    maxlen = maxlen.saturating_sub(1);
                    if maxlen <= 0 {
                        out.extend_from_slice(b"...");
                        truncate_format_object_stack(stack_len);
                        return;
                    }

                    n -= 1;
                    if n == 0 {
                        tortoise_idx = tortoise_idx.saturating_add(m);
                        m = m.saturating_mul(2);
                        n = m;
                        tortoise = cursor;
                    } else if crate::emacs_core::value::eq_value(&cursor, &tortoise) {
                        out.extend_from_slice(format!(". #{tortoise_idx}").as_bytes());
                        truncate_format_object_stack(stack_len);
                        return;
                    }
                }
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                out.extend(format_value_bytes_in_state_with_options(
                    obarray, buffers, frames, threads, &pair_car, options,
                ));
                cursor = pair_cdr;
                first = false;
            }
            ValueKind::Nil => {
                truncate_format_object_stack(stack_len);
                return;
            }
            _ => {
                if !first {
                    out.extend_from_slice(b" . ");
                }
                out.extend(format_value_bytes_in_state_with_options(
                    obarray, buffers, frames, threads, &cursor, options,
                ));
                truncate_format_object_stack(stack_len);
                return;
            }
        }
    }
}

/// Render a value with evaluator-context-aware opaque handle formatting.
pub fn print_value_with_eval(eval: &crate::emacs_core::eval::Context, value: &Value) -> String {
    print_value_in_state(eval, value)
}

/// Render a value as bytes with evaluator-context-aware opaque handle formatting.
pub fn print_value_bytes_with_eval(
    eval: &crate::emacs_core::eval::Context,
    value: &Value,
) -> Vec<u8> {
    print_value_bytes_in_state(
        eval.variables(),
        &eval.buffers,
        &eval.frames,
        &eval.threads,
        value,
    )
}

/// Like [`print_value_bytes_with_eval`], but for a printer whose result becomes a
/// multibyte string (e.g. `format`/`message`'s `%S`, which GNU implements via
/// `Fprin1_to_string` printing into the multibyte `Vprin1_to_string_buffer`).
/// `print_prepare` binds `print-escape-nonascii' for a multibyte target, so a
/// unibyte string's raw high bytes are octal-escaped rather than emitted raw.
pub fn print_value_bytes_escaped_with_eval(
    eval: &crate::emacs_core::eval::Context,
    value: &Value,
) -> Vec<u8> {
    if let Some(handle) =
        format_opaque_handle_in_state(&eval.buffers, &eval.frames, &eval.threads, value)
    {
        return handle.into_bytes();
    }
    let mut options = print_options_from_state(eval.variables(), None);
    options.print_escape_nonascii = true;
    format_value_bytes_in_state_with_options(
        &eval.obarray,
        &eval.buffers,
        &eval.frames,
        &eval.threads,
        value,
        options,
    )
}

fn print_data_payload_with_eval(eval: &crate::emacs_core::eval::Context, data: &[Value]) -> String {
    if data.is_empty() {
        "nil".to_string()
    } else {
        let parts = data
            .iter()
            .map(|v| print_value_with_eval(eval, v))
            .collect::<Vec<_>>();
        format!("({})", parts.join(" "))
    }
}

fn print_signal_payload_with_eval(
    eval: &crate::emacs_core::eval::Context,
    raw_data: Option<&Value>,
    data: &[Value],
) -> String {
    if let Some(raw) = raw_data {
        return print_value_with_eval(eval, raw);
    }
    print_data_payload_with_eval(eval, data)
}

/// Render one signal in Lisp-readable form for diagnostics.
///
/// This intentionally avoids `Debug` on `Value`, which prints heap object
/// identities such as `String@0x...`.  Runtime diagnostics should show the
/// same string payloads a Lisp user would see.
pub(crate) fn format_signal_data_with_eval(
    eval: &crate::emacs_core::eval::Context,
    sig: &SignalData,
) -> String {
    let payload = print_signal_payload_with_eval(eval, sig.raw_data.as_ref(), &sig.data);
    format!(
        "({} {})",
        format_symbol_name_for_diagnostic(sig.symbol),
        payload
    )
}

/// Render non-local control flow in Lisp-readable form for diagnostics.
pub(crate) fn format_flow_with_eval(
    eval: &crate::emacs_core::eval::Context,
    flow: &Flow,
) -> String {
    match flow.kind() {
        FlowRef::Signal(sig) => format_signal_data_with_eval(eval, sig),
        FlowRef::Throw(thrown) => format!(
            "(no-catch ({} {}))",
            print_value_with_eval(eval, &thrown.tag),
            print_value_with_eval(eval, &thrown.value)
        ),
        FlowRef::ThreadBlocked(blocked) => {
            format!(
                "(thread-blocked {})",
                print_value_with_eval(eval, &blocked.blocker)
            )
        }
        FlowRef::Shutdown(request) => format!("(kill-emacs {})", request.exit_code),
    }
}

fn append_print_value_bytes_with_eval(
    eval: &crate::emacs_core::eval::Context,
    value: &Value,
    out: &mut Vec<u8>,
) {
    out.extend_from_slice(&print_value_bytes_with_eval(eval, value));
}

/// Format an eval result for harnesses that have evaluator context and need
/// opaque handle rendering for thread/mutex/condvar/terminal values.
pub fn format_eval_result_with_eval(
    eval: &crate::emacs_core::eval::Context,
    result: &Result<Value, EvalError>,
) -> String {
    match result {
        Ok(value) => format!("OK {}", print_value_with_eval(eval, value)),
        Err(EvalError::Signal {
            symbol,
            data,
            raw_data,
            ..
        }) => {
            let payload = print_signal_payload_with_eval(eval, raw_data.as_ref(), data);
            format!(
                "ERR ({} {})",
                format_symbol_name_for_diagnostic(*symbol),
                payload
            )
        }
        Err(EvalError::UncaughtThrow { tag, value, .. }) => {
            format!(
                "ERR (no-catch ({} {}))",
                print_value_with_eval(eval, tag),
                print_value_with_eval(eval, value),
            )
        }
        Err(EvalError::Shutdown(request)) => format!("ERR (kill-emacs {})", request.exit_code),
    }
}

fn append_signal_payload_bytes_with_eval(
    eval: &crate::emacs_core::eval::Context,
    raw_data: Option<&Value>,
    data: &[Value],
    out: &mut Vec<u8>,
) {
    if let Some(raw) = raw_data {
        append_print_value_bytes_with_eval(eval, raw, out);
    } else if data.is_empty() {
        out.extend_from_slice(b"nil");
    } else {
        out.push(b'(');
        for (idx, item) in data.iter().enumerate() {
            if idx > 0 {
                out.push(b' ');
            }
            append_print_value_bytes_with_eval(eval, item, out);
        }
        out.push(b')');
    }
}

/// Byte-preserving variant of `format_eval_result_with_eval`.
///
/// This preserves non-UTF-8 byte payloads in printed string literals used by
/// vm-compat corpus checks while still applying evaluator-aware opaque-handle
/// rendering for thread/mutex/condvar/terminal values.
pub fn format_eval_result_bytes_with_eval(
    eval: &crate::emacs_core::eval::Context,
    result: &Result<Value, EvalError>,
) -> Vec<u8> {
    let mut out = Vec::new();
    match result {
        Ok(value) => {
            out.extend_from_slice(b"OK ");
            append_print_value_bytes_with_eval(eval, value, &mut out);
        }
        Err(EvalError::Signal {
            symbol,
            data,
            raw_data,
            ..
        }) => {
            out.extend_from_slice(b"ERR (");
            append_print_value_bytes_with_eval(eval, &Value::from_sym_id(*symbol), &mut out);
            out.push(b' ');
            append_signal_payload_bytes_with_eval(eval, raw_data.as_ref(), data, &mut out);
            out.push(b')');
        }
        Err(EvalError::Shutdown(request)) => {
            out.extend_from_slice(format!("ERR (kill-emacs {})", request.exit_code).as_bytes());
        }
        Err(EvalError::UncaughtThrow { tag, value, .. }) => {
            out.extend_from_slice(b"ERR (no-catch (");
            append_print_value_bytes_with_eval(eval, tag, &mut out);
            out.push(b' ');
            append_print_value_bytes_with_eval(eval, value, &mut out);
            out.extend_from_slice(b"))");
        }
    }
    out
}

impl Display for EvalError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Signal {
                symbol,
                data,
                raw_data,
                ..
            } => write!(
                f,
                "signal {} {}",
                format_symbol_name_for_diagnostic(*symbol),
                format_signal_payload(raw_data.as_ref(), data),
            ),
            Self::UncaughtThrow { tag, value, .. } => write!(
                f,
                "uncaught throw tag={} value={}",
                super::print_value(tag),
                super::print_value(value),
            ),
            Self::Shutdown(request) => write!(f, "kill-emacs {}", request.exit_code),
        }
    }
}

/// Convert internal flow to public EvalError.
pub fn map_flow(flow: Flow) -> EvalError {
    match flow.into_kind() {
        FlowKind::Signal(sig) => EvalError::from_signal(*sig),
        FlowKind::Throw(thrown) => EvalError::from_throw(*thrown),
        FlowKind::Shutdown(request) => EvalError::Shutdown(request),
        FlowKind::ThreadBlocked(blocked) => EvalError::signal(
            intern("error"),
            vec![Value::string(format!(
                "Thread blocked on {}",
                super::print_value(&blocked.blocker)
            ))],
            None,
        ),
    }
}

/// `(error-message-string ERROR-DATA)` — format an error for display.
///
/// ERROR-DATA is `(ERROR-SYMBOL . DATA)` as bound by `condition-case`.
/// Looks up `error-message` on the symbol's plist and appends the data.
pub(crate) fn builtin_error_message_string(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("error-message-string"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    // GNU's `Ferror_message_string' (print.c:1046-1051) short-circuits an
    // error of exactly the shape (error STRING) and hands back that very
    // string object, text properties and all.  Every other shape is rendered
    // by printing into `prin1-to-string-buffer' one character at a time, which
    // carries no properties, and the result is read back out with
    // `buffer-string'.  So the fast path is the *only* way a property reaches
    // the caller: a `user-error' message, a file-error, or an (error STRING
    // EXTRA) all come back plain even when their payload was propertized.
    //
    // Routing every other answer through one strip keeps that guarantee in a
    // single place rather than leaving it to each of the branches below.
    if let Some(message) = single_error_string_payload(&args[0]) {
        return Ok(message);
    }
    let rendered = error_message_string_rendered(eval, &args[0])?;
    Ok(match eval.lisp_string(rendered) {
        Some(string) => Value::heap_string(string.without_properties()),
        None => rendered,
    })
}

/// The STRING of an error datum of exactly the shape `(error STRING)`, which
/// GNU returns unchanged.
fn single_error_string_payload(error_data: &Value) -> Option<Value> {
    if !error_data.is_cons() || error_data.cons_car() != Value::symbol("error") {
        return None;
    }
    let tail = error_data.cons_cdr();
    if !tail.is_cons() || !tail.cons_cdr().is_nil() {
        return None;
    }
    let message = tail.cons_car();
    message.as_lisp_string().is_some().then_some(message)
}

fn error_message_string_rendered(
    eval: &mut crate::emacs_core::eval::Context,
    error_data: &Value,
) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    // Emacs expects ERROR-DATA to be a list (or nil).
    let (symbol, data) = match error_data.kind() {
        ValueKind::Cons => {
            let pair_car = error_data.cons_car();
            let pair_cdr = error_data.cons_cdr();
            let symbol = match pair_car.as_symbol_id(symbol_mode) {
                Some(symbol) => symbol,
                None => return Ok(Value::string("peculiar error")),
            };
            let rest = error_data_tail_to_vec(pair_cdr);
            (symbol, rest)
        }
        ValueKind::Nil => return Ok(Value::heap_string(lisp_lit("peculiar error"))),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), *error_data],
            ));
        }
    };

    // GNU `print_error_message` treats a signal as printable when its
    // `error-message` property is a string.  This deliberately includes
    // non-error conditions such as `quit` and `minibuffer-quit`, whose
    // `error-conditions` do not include `error`.
    let error_symbol = intern("error");
    let user_error_symbol = intern("user-error");
    let Some(base_message_value) = eval
        .obarray
        .get_property_id(symbol, intern("error-message"))
    else {
        if data.is_empty() {
            return Ok(Value::heap_string(lisp_lit("peculiar error")));
        }
        let detail = join_lisp(
            data.iter()
                .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Prin1))
                .collect(),
            ", ",
        );
        return Ok(Value::heap_string(
            lisp_lit("peculiar error: ").concat(&detail),
        ));
    };

    // GNU `print_error_message` passes the `error-message` property of every
    // condition other than plain `error` through `substitute-command-keys`.
    // Besides command substitutions, this applies the user's
    // `text-quoting-style`.  The callback is intentionally a safe funcall:
    // error rendering must keep the original message if the Lisp helper
    // itself signals.  A non-nil, non-string callback result is retained and
    // consequently renders as a peculiar error, matching GNU's downstream
    // printer behavior.
    let base_message_value = if symbol != error_symbol
        && eval
            .obarray
            .symbol_function_id(intern("substitute-command-keys"))
            .is_some()
    {
        let substituted = eval.safe_funcall(
            Value::symbol("substitute-command-keys"),
            vec![base_message_value],
        )?;
        if substituted.is_nil() {
            base_message_value
        } else {
            substituted
        }
    } else {
        base_message_value
    };

    let Some(_) = base_message_value.as_lisp_string() else {
        if data.is_empty() {
            return Ok(Value::heap_string(lisp_lit("peculiar error")));
        }
        let detail = join_lisp(
            data.iter()
                .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Prin1))
                .collect(),
            ", ",
        );
        return Ok(Value::heap_string(
            lisp_lit("peculiar error: ").concat(&detail),
        ));
    };
    let base_message = error_arg_lisp(eval, &base_message_value, ErrorDatumPrintMode::Princ);

    if data.is_empty() {
        if symbol == error_symbol {
            return Ok(Value::heap_string(lisp_lit("peculiar error")));
        }
        if symbol == user_error_symbol {
            return Ok(Value::heap_string(lisp_lit("")));
        }
        return Ok(Value::heap_string(base_message));
    }

    // `user-error` always renders payload data directly.
    if symbol == user_error_symbol {
        if let Some(first) = data.first().filter(|v| v.as_lisp_string().is_some()) {
            let first_str = error_arg_lisp(eval, first, ErrorDatumPrintMode::Princ);
            let rest = &data[1..];
            if rest.is_empty() {
                return Ok(Value::heap_string(first_str));
            }
            let rest_j = join_lisp(
                rest.iter()
                    .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Princ))
                    .collect(),
                ", ",
            );
            return Ok(Value::heap_string(
                first_str.concat(&lisp_lit(", ")).concat(&rest_j),
            ));
        }
        let detail = join_lisp(
            data.iter()
                .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Princ))
                .collect(),
            ", ",
        );
        return Ok(Value::heap_string(detail));
    }

    let is_file_error_family = signal_matches_condition_value_sym(
        &eval.obarray,
        symbol,
        &Value::from_sym_id(intern("file-error")),
        symbol_mode,
    );
    let is_file_locked = symbol == intern("file-locked");

    // `file-locked` is an oddball in Emacs: it always reports "peculiar error"
    // with all payload elements, even if the first datum is a string.
    if is_file_locked {
        let detail = join_lisp(
            data.iter()
                .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Prin1))
                .collect(),
            ", ",
        );
        return Ok(Value::heap_string(
            lisp_lit("peculiar error: ").concat(&detail),
        ));
    }

    // `error` and file-error-family conditions use a leading string for
    // user-facing detail.
    if symbol == error_symbol || is_file_error_family {
        if let Some(first) = data.first().filter(|v| v.as_lisp_string().is_some()) {
            let first_str = error_arg_lisp(eval, first, ErrorDatumPrintMode::Princ);
            let rest = &data[1..];
            if rest.is_empty() {
                return Ok(Value::heap_string(first_str));
            }
            let print_mode = if symbol == error_symbol {
                ErrorDatumPrintMode::Prin1
            } else {
                ErrorDatumPrintMode::Princ
            };
            let rest_j = join_lisp(
                rest.iter()
                    .map(|v| error_arg_lisp(eval, v, print_mode))
                    .collect(),
                ", ",
            );
            return Ok(Value::heap_string(
                first_str.concat(&lisp_lit(": ")).concat(&rest_j),
            ));
        }

        // `error` and most file-error-family members render peculiar payload
        // data from the second element onward when no leading message string
        // is present.
        if data.len() > 1 {
            let detail = join_lisp(
                data[1..]
                    .iter()
                    .map(|v| error_arg_lisp(eval, v, ErrorDatumPrintMode::Prin1))
                    .collect(),
                ", ",
            );
            return Ok(Value::heap_string(
                lisp_lit("peculiar error: ").concat(&detail),
            ));
        }
        return Ok(Value::heap_string(lisp_lit("peculiar error")));
    }

    let print_mode = if symbol == intern("end-of-file") {
        ErrorDatumPrintMode::Princ
    } else {
        ErrorDatumPrintMode::Prin1
    };
    let detail = join_lisp(
        data.iter()
            .map(|v| error_arg_lisp(eval, v, print_mode))
            .collect(),
        ", ",
    );
    Ok(Value::heap_string(
        base_message.concat(&lisp_lit(": ")).concat(&detail),
    ))
}

fn error_data_tail_to_vec(mut tail: Value) -> Vec<Value> {
    let mut data = Vec::new();
    while tail.is_cons() {
        data.push(tail.cons_car());
        tail = tail.cons_cdr();
    }
    data
}

/// GNU `print_error_message` chooses between `Fprinc` and `Fprin1` for an
/// entire error-data object, not only for string quoting.  Keep that protocol
/// typed so a new condition branch must select the full object printer.
#[derive(Clone, Copy)]
enum ErrorDatumPrintMode {
    Princ,
    Prin1,
}

/// Render an error-data argument as faithful Emacs internal-encoding bytes.
/// Both printer paths are shared with the public Lisp primitives, so buffers,
/// nested strings, opaque handles, and raw unibyte data keep their GNU-visible
/// `princ`/`prin1` distinctions.
fn error_arg_lisp(
    eval: &crate::emacs_core::eval::Context,
    value: &Value,
    mode: ErrorDatumPrintMode,
) -> crate::heap_types::LispString {
    let bytes = match mode {
        ErrorDatumPrintMode::Princ => {
            crate::emacs_core::builtins::misc_eval::print_value_princ_bytes_to_multibyte_buffer(
                eval, value,
            )
        }
        ErrorDatumPrintMode::Prin1 => print_value_bytes_escaped_with_eval(eval, value),
    };
    crate::heap_types::LispString::from_emacs_bytes(bytes)
}

/// A static ASCII message literal as a LispString piece. Built unibyte so it
/// does not force the concatenated result multibyte; `concat` promotes it when a
/// multibyte piece is present.
fn lisp_lit(s: &str) -> crate::heap_types::LispString {
    crate::heap_types::LispString::from_unibyte(s.as_bytes().to_vec())
}

/// Concatenate LispString `items` with `sep` between them, using Emacs string
/// concatenation (which unifies unibyte/multibyte pieces correctly).
fn join_lisp(
    items: Vec<crate::heap_types::LispString>,
    sep: &str,
) -> crate::heap_types::LispString {
    let sep_ls = lisp_lit(sep);
    let mut iter = items.into_iter();
    let Some(mut acc) = iter.next() else {
        return crate::heap_types::LispString::from_unibyte(Vec::new());
    };
    for item in iter {
        acc = acc.concat(&sep_ls).concat(&item);
    }
    acc
}
