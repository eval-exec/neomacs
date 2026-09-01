//! Rectangle operation builtins for the Elisp interpreter.
//!
//! Implements rectangle manipulation commands:
//! - `extract-rectangle-line`
//! - `extract-rectangle`, `delete-rectangle`, `kill-rectangle`
//! - `yank-rectangle`, `insert-rectangle`, `open-rectangle`
//! - `clear-rectangle`, `string-rectangle`, `replace-rectangle`
//! - `delete-extract-rectangle`
//!
//! These implement compatibility-focused rectangle behavior used by
//! vm-compat batches. Remaining edge drift is tracked and locked by
//! oracle corpora.

#[cfg(test)]
use super::error::EvalResult;
use super::error::{Flow, signal};
use super::value::*;
use crate::emacs_core::error::LispCondition;
#[cfg(test)]
#[cfg(test)]
use crate::emacs_core::error::{expect_max_args, expect_min_args};
use crate::emacs_core::value::ValueKind;
use crate::heap_types::LispString;

// ---------------------------------------------------------------------------
// Argument helpers (local copies — same pattern as other modules)
// ---------------------------------------------------------------------------

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}
#[cfg(test)]
fn expect_string(value: &Value) -> Result<LispString, Flow> {
    match value.kind() {
        ValueKind::String => Ok(value
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload")
            .clone()),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

/// Route symbol-value reads through the full GNU lookup path so
/// LOCALIZED BLV / FORWARDED slot / specpdl let-binding state is
/// observed. See the extended comment on the identical helper in
/// `builtins/misc_eval.rs` (audit finding #3 in
/// `drafts/regex-search-audit.md`).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn dynamic_or_global_symbol_value(eval: &super::eval::Context, name: &str) -> Option<Value> {
    let id = crate::emacs_core::intern::intern(name);
    eval.eval_symbol_by_id(id).ok()
}

// ---------------------------------------------------------------------------
// RectangleState — stores the last killed rectangle
// ---------------------------------------------------------------------------

/// Persistent state for rectangle operations across the session.
#[derive(Clone, Debug)]
pub(crate) struct RectangleState {
    /// The last killed rectangle: one string per line.
    pub killed: Vec<LispString>,
}

impl RectangleState {
    pub fn new() -> Self {
        Self { killed: Vec::new() }
    }
}

impl Default for RectangleState {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn empty_lisp_string(multibyte: bool) -> LispString {
    super::builtins::lisp_string_from_buffer_bytes(Vec::new(), multibyte)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn space_lisp_string(width: usize, multibyte: bool) -> LispString {
    super::builtins::lisp_string_from_buffer_bytes(vec![b' '; width], multibyte)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn slice_lisp_string_chars(string: &LispString, start: usize, end: usize) -> LispString {
    let start = start.min(string.schars());
    let end = end.min(string.schars());
    if start >= end {
        return empty_lisp_string(string.is_multibyte());
    }

    if !string.is_multibyte() {
        return LispString::from_unibyte(string.as_bytes()[start..end].to_vec());
    }

    let start_byte = crate::emacs_core::emacs_char::char_to_byte_pos(string.as_bytes(), start);
    let end_byte = crate::emacs_core::emacs_char::char_to_byte_pos(string.as_bytes(), end);
    LispString::from_emacs_bytes(string.as_bytes()[start_byte..end_byte].to_vec())
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn split_lisp_string_lines(text: &LispString) -> Vec<LispString> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (idx, &byte) in text.as_bytes().iter().enumerate() {
        if byte == b'\n' {
            lines.push(
                text.slice(start, idx)
                    .expect("newline split must stay within string bounds"),
            );
            start = idx + 1;
        }
    }
    lines.push(
        text.slice(start, text.sbytes())
            .expect("line tail split must stay within string bounds"),
    );
    lines
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn join_lisp_string_lines(lines: &[LispString], multibyte: bool) -> LispString {
    let mut out = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if idx > 0 {
            out.push(b'\n');
        }
        out.extend_from_slice(line.as_bytes());
    }
    super::builtins::lisp_string_from_buffer_bytes(out, multibyte)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
struct LineColumn {
    line: usize,
    column: usize,
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn extract_line_columns(line: &LispString, start_col: usize, end_col: usize) -> LispString {
    if start_col >= end_col {
        return empty_lisp_string(line.is_multibyte());
    }
    let len = line.schars();

    if start_col >= len {
        return space_lisp_string(end_col - start_col, line.is_multibyte());
    }

    let mut out = slice_lisp_string_chars(line, start_col, len.min(end_col));
    if end_col > len {
        out.mutate_bytes(|bytes| bytes.extend(std::iter::repeat_n(b' ', end_col - len)));
    }
    out
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn rectangle_lines_for_extract(start_line: usize, end_line: usize) -> Vec<usize> {
    if start_line <= end_line {
        (start_line..=end_line).collect()
    } else {
        vec![start_line]
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn delete_extract_rectangle_from_text(
    text: &LispString,
    start_line: usize,
    end_line: usize,
    left_col: usize,
    right_col: usize,
) -> (Vec<LispString>, LispString) {
    let mut lines = split_lisp_string_lines(text);
    let mut extracted = Vec::new();
    let width = right_col.saturating_sub(left_col);

    for line_index in rectangle_lines_for_extract(start_line, end_line) {
        let Some(line) = lines.get_mut(line_index) else {
            extracted.push(space_lisp_string(width, text.is_multibyte()));
            continue;
        };

        let line_len = line.schars();
        if line_len < left_col {
            extracted.push(space_lisp_string(width, line.is_multibyte()));
            continue;
        }

        extracted.push(extract_line_columns(line, left_col, right_col));
        let del_end_char = line_len.min(right_col);
        let del_start_byte = if line.is_multibyte() {
            crate::emacs_core::emacs_char::char_to_byte_pos(line.as_bytes(), left_col)
        } else {
            left_col
        };
        let del_end_byte = if line.is_multibyte() {
            crate::emacs_core::emacs_char::char_to_byte_pos(line.as_bytes(), del_end_char)
        } else {
            del_end_char
        };
        if del_start_byte < del_end_byte {
            line.mutate_bytes(|bytes| {
                bytes.drain(del_start_byte..del_end_byte);
            });
        }
    }

    (
        extracted,
        join_lisp_string_lines(&lines, text.is_multibyte()),
    )
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins
// ---------------------------------------------------------------------------

/// `(extract-rectangle-line STARTCOL ENDCOL &optional LINE)` -- extract one
/// line of a rectangle as a string.
///
/// Compatibility behavior:
/// - with optional LINE, returns substring between STARTCOL and ENDCOL
/// - without LINE, returns an empty string (legacy stub path)
#[cfg(test)]
pub(crate) fn builtin_extract_rectangle_line(args: Vec<Value>) -> EvalResult {
    expect_min_args("extract-rectangle-line", &args, 2)?;
    expect_max_args("extract-rectangle-line", &args, 3)?;
    let start_col = expect_int(&args[0])?;
    let end_col = expect_int(&args[1])?;
    if start_col < 0 || end_col < 0 {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::fixnum(start_col), Value::fixnum(end_col)],
        ));
    }
    if args.len() == 3 {
        let line = expect_string(&args[2])?;
        let mut lo = start_col as usize;
        let mut hi = end_col as usize;
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        lo = lo.min(line.schars());
        hi = hi.min(line.schars());
        if lo >= hi {
            return Ok(Value::heap_string(empty_lisp_string(line.is_multibyte())));
        }
        return Ok(Value::heap_string(slice_lisp_string_chars(&line, lo, hi)));
    }
    Ok(Value::string(""))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
