//! Reader/printer builtins: read-from-string, read, prin1-to-string (enhanced),
//! format-spec, and various interactive-input stubs.

use super::error::{signal, EvalResult, Flow};
use super::expr::Expr;
use super::value::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_string(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

/// Print a value without quoting strings or escaping special characters.
/// Used when NOESCAPE is non-nil in `prin1-to-string`.
fn print_value_no_escape(value: &Value) -> String {
    match value {
        Value::Str(s) => (**s).clone(),
        Value::Symbol(s) => s.clone(),
        Value::Nil => "nil".to_string(),
        Value::True => "t".to_string(),
        // Emacs chars are integer values, so print as codepoint.
        Value::Char(c) => (*c as u32).to_string(),
        other => super::print::print_value(other),
    }
}

// ---------------------------------------------------------------------------
// 1. read-from-string
// ---------------------------------------------------------------------------

/// `(read-from-string STRING &optional START END)`
///
/// Parse a single Lisp object from STRING starting at position START (default 0).
/// Returns `(OBJECT . END-POSITION)` where END-POSITION is the character index
/// after the parsed object.
pub(crate) fn builtin_read_from_string(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("read-from-string", &args, 1)?;

    let full_string = expect_string(&args[0])?;

    let start = if args.len() > 1 && args[1].is_truthy() {
        match &args[1] {
            Value::Int(n) => {
                if *n < 0 {
                    return Err(signal(
                        "args-out-of-range",
                        vec![args[0].clone(), args[1].clone()],
                    ));
                }
                *n as usize
            }
            _ => 0,
        }
    } else {
        0
    };

    let end = if args.len() > 2 && args[2].is_truthy() {
        match &args[2] {
            Value::Int(n) => {
                if *n < 0 || (*n as usize) > full_string.len() {
                    return Err(signal(
                        "args-out-of-range",
                        vec![args[0].clone(), args[2].clone()],
                    ));
                }
                *n as usize
            }
            _ => full_string.len(),
        }
    } else {
        full_string.len()
    };

    if start > end || start > full_string.len() {
        return Err(signal(
            "args-out-of-range",
            vec![args[0].clone(), Value::Int(start as i64)],
        ));
    }

    let substring = &full_string[start..end];

    let forms = super::parser::parse_forms(substring).map_err(|e| {
        signal(
            "invalid-read-syntax",
            vec![Value::string(e.message.clone())],
        )
    })?;

    if forms.is_empty() {
        return Err(signal(
            "end-of-file",
            vec![Value::string("End of file during parsing")],
        ));
    }

    // Compute end position: re-parse to find where the first form ends.
    // We do this by parsing form-by-form and tracking consumed bytes.
    let end_pos = compute_read_end_position(substring, &forms[0]);
    let consumed = &substring[..end_pos.min(substring.len())];
    let value = if first_form_is_reader_hash_dollar(&forms[0], consumed) {
        eval.obarray()
            .symbol_value("load-file-name")
            .cloned()
            .unwrap_or(Value::Nil)
    } else {
        super::eval::quote_to_value(&forms[0])
    };
    let absolute_end = start + end_pos;

    Ok(Value::cons(value, Value::Int(absolute_end as i64)))
}

fn first_form_is_reader_hash_dollar(expr: &Expr, consumed: &str) -> bool {
    matches!(expr, Expr::Symbol(s) if s == "load-file-name")
        && consumed_represents_hash_dollar(consumed)
}

fn consumed_represents_hash_dollar(input: &str) -> bool {
    let bytes = input.as_bytes();
    let mut pos = skip_ws_comments(input, 0);
    loop {
        if pos + 1 >= bytes.len() {
            return false;
        }
        if bytes[pos] == b'#' && bytes[pos + 1] == b'@' {
            pos += 2;
            let digits_start = pos;
            while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                pos += 1;
            }
            if pos == digits_start {
                return false;
            }
            let len = std::str::from_utf8(&bytes[digits_start..pos])
                .ok()
                .and_then(|s| s.parse::<usize>().ok());
            let Some(len) = len else {
                return false;
            };
            let Some(after_data) = pos.checked_add(len) else {
                return false;
            };
            if after_data > bytes.len() {
                return false;
            }
            pos = skip_ws_comments(input, after_data);
            continue;
        }
        return bytes[pos] == b'#' && bytes[pos + 1] == b'$';
    }
}

/// Estimate the end position of the first parsed form in the input string.
/// We re-parse character by character to find where the parser would stop
/// after reading one expression.
fn compute_read_end_position(input: &str, _first_form: &Expr) -> usize {
    // Use a simple approach: parse just one form and see how far we get.
    // We create a mini-parser that tracks position.
    let mut pos = 0;
    let bytes = input.as_bytes();

    // Skip leading whitespace and comments
    pos = skip_ws_comments(input, pos);

    if pos >= input.len() {
        return input.len();
    }

    // Now skip one sexp
    pos = skip_one_sexp(input, pos);

    // Skip any trailing whitespace up to the end of the consumed region
    // (Emacs `read-from-string` stops right after the sexp, no trailing ws skip)
    let _ = bytes;
    pos
}

fn skip_ws_comments(input: &str, mut pos: usize) -> usize {
    let bytes = input.as_bytes();
    loop {
        if pos >= bytes.len() {
            return pos;
        }
        let ch = bytes[pos];
        if ch.is_ascii_whitespace() {
            pos += 1;
            continue;
        }
        if ch == b';' {
            // line comment
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            if pos < bytes.len() {
                pos += 1; // skip newline
            }
            continue;
        }
        if ch == b'#' && pos + 1 < bytes.len() && bytes[pos + 1] == b'|' {
            // block comment
            pos += 2;
            let mut depth = 1;
            while depth > 0 && pos < bytes.len() {
                if bytes[pos] == b'#' && pos + 1 < bytes.len() && bytes[pos + 1] == b'|' {
                    depth += 1;
                    pos += 2;
                } else if bytes[pos] == b'|' && pos + 1 < bytes.len() && bytes[pos + 1] == b'#' {
                    depth -= 1;
                    pos += 2;
                } else {
                    pos += 1;
                }
            }
            continue;
        }
        return pos;
    }
}

fn skip_one_sexp(input: &str, mut pos: usize) -> usize {
    let bytes = input.as_bytes();
    if pos >= bytes.len() {
        return pos;
    }

    let ch = bytes[pos];

    match ch {
        b'(' => {
            pos += 1;
            let mut depth = 1;
            while depth > 0 && pos < bytes.len() {
                match bytes[pos] {
                    b'(' => {
                        depth += 1;
                        pos += 1;
                    }
                    b')' => {
                        depth -= 1;
                        pos += 1;
                    }
                    b'"' => {
                        pos = skip_string(input, pos);
                    }
                    b';' => {
                        while pos < bytes.len() && bytes[pos] != b'\n' {
                            pos += 1;
                        }
                    }
                    b'\\' => {
                        pos += 1; // skip backslash
                        if pos < bytes.len() {
                            pos += 1; // skip escaped char
                        }
                    }
                    _ => {
                        pos += 1;
                    }
                }
            }
            pos
        }
        b'[' => {
            pos += 1;
            let mut depth = 1;
            while depth > 0 && pos < bytes.len() {
                match bytes[pos] {
                    b'[' => {
                        depth += 1;
                        pos += 1;
                    }
                    b']' => {
                        depth -= 1;
                        pos += 1;
                    }
                    b'"' => {
                        pos = skip_string(input, pos);
                    }
                    b'\\' => {
                        pos += 1;
                        if pos < bytes.len() {
                            pos += 1;
                        }
                    }
                    _ => {
                        pos += 1;
                    }
                }
            }
            pos
        }
        b'"' => skip_string(input, pos),
        b'\'' | b'`' => {
            // quote / backquote — skip prefix then one sexp
            pos += 1;
            pos = skip_ws_comments(input, pos);
            skip_one_sexp(input, pos)
        }
        b',' => {
            pos += 1;
            if pos < bytes.len() && bytes[pos] == b'@' {
                pos += 1;
            }
            pos = skip_ws_comments(input, pos);
            skip_one_sexp(input, pos)
        }
        b'#' => {
            pos += 1;
            if pos >= bytes.len() {
                return pos;
            }
            match bytes[pos] {
                b'\'' => {
                    // #'symbol
                    pos += 1;
                    pos = skip_ws_comments(input, pos);
                    skip_one_sexp(input, pos)
                }
                b'(' => {
                    // #(vector)
                    skip_one_sexp(input, pos)
                }
                b'[' => {
                    // #[vector] compiled-function literal
                    skip_one_sexp(input, pos)
                }
                b'@' => {
                    // #@N<bytes> ... next-object
                    pos += 1;
                    let digits_start = pos;
                    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                        pos += 1;
                    }
                    if pos == digits_start {
                        return pos;
                    }
                    let len = std::str::from_utf8(&bytes[digits_start..pos])
                        .ok()
                        .and_then(|s| s.parse::<usize>().ok());
                    let Some(len) = len else {
                        return pos;
                    };
                    let Some(after_data) = pos.checked_add(len) else {
                        return bytes.len();
                    };
                    if after_data > bytes.len() {
                        return bytes.len();
                    }
                    pos = skip_ws_comments(input, after_data);
                    if pos >= bytes.len() {
                        pos
                    } else {
                        skip_one_sexp(input, pos)
                    }
                }
                b'$' => {
                    // #$ pseudo object
                    pos + 1
                }
                b's' => {
                    // #s(hash-table ...)
                    pos += 1;
                    if pos < bytes.len() && bytes[pos] == b'(' {
                        skip_one_sexp(input, pos)
                    } else {
                        pos
                    }
                }
                b'x' | b'X' | b'o' | b'O' | b'b' | b'B' => {
                    // radix number
                    pos += 1;
                    while pos < bytes.len()
                        && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_')
                    {
                        pos += 1;
                    }
                    pos
                }
                _ => pos,
            }
        }
        b'?' => {
            // char literal
            pos += 1;
            if pos < bytes.len() && bytes[pos] == b'\\' {
                pos += 1;
                if pos < bytes.len() {
                    let esc = bytes[pos];
                    pos += 1;
                    match esc {
                        b'x' => {
                            while pos < bytes.len() && bytes[pos].is_ascii_hexdigit() {
                                pos += 1;
                            }
                            // Optional terminating ';'
                            if pos < bytes.len() && bytes[pos] == b';' {
                                pos += 1;
                            }
                        }
                        b'u' => {
                            for _ in 0..4 {
                                if pos < bytes.len() && bytes[pos].is_ascii_hexdigit() {
                                    pos += 1;
                                }
                            }
                        }
                        b'U' => {
                            for _ in 0..8 {
                                if pos < bytes.len() && bytes[pos].is_ascii_hexdigit() {
                                    pos += 1;
                                }
                            }
                        }
                        b'0'..=b'7' => {
                            for _ in 0..2 {
                                if pos < bytes.len() && bytes[pos] >= b'0' && bytes[pos] <= b'7' {
                                    pos += 1;
                                }
                            }
                        }
                        b'C' | b'M' | b'S' => {
                            if pos < bytes.len() && bytes[pos] == b'-' {
                                pos += 1;
                                if pos < bytes.len() {
                                    pos += 1;
                                }
                            }
                        }
                        _ => {} // single escaped char already consumed
                    }
                }
            } else if pos < bytes.len() {
                // Regular character — consume one UTF-8 char
                let ch = input[pos..].chars().next();
                if let Some(c) = ch {
                    pos += c.len_utf8();
                }
            }
            pos
        }
        _ => {
            // Atom: symbol or number
            while pos < bytes.len() {
                let b = bytes[pos];
                if b.is_ascii_whitespace()
                    || b == b'('
                    || b == b')'
                    || b == b'['
                    || b == b']'
                    || b == b'\''
                    || b == b'`'
                    || b == b','
                    || b == b'"'
                    || b == b';'
                {
                    break;
                }
                pos += 1;
            }
            pos
        }
    }
}

fn skip_string(input: &str, mut pos: usize) -> usize {
    let bytes = input.as_bytes();
    if pos >= bytes.len() || bytes[pos] != b'"' {
        return pos;
    }
    pos += 1; // opening quote
    while pos < bytes.len() {
        match bytes[pos] {
            b'"' => {
                pos += 1;
                return pos;
            }
            b'\\' => {
                pos += 1;
                if pos < bytes.len() {
                    pos += 1;
                }
            }
            _ => {
                pos += 1;
            }
        }
    }
    pos
}

// ---------------------------------------------------------------------------
// 2. read
// ---------------------------------------------------------------------------

/// `(read &optional STREAM)`
///
/// Read one Lisp expression from STREAM.
/// - If STREAM is a string, read from that string (equivalent to car of read-from-string).
/// - If STREAM is nil, would read from stdin (returns nil in non-interactive mode).
/// - If STREAM is a buffer, read from buffer at point.
pub(crate) fn builtin_read(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    if args.is_empty() || args[0].is_nil() {
        // No stream / nil — non-interactive, return nil
        return Ok(Value::Nil);
    }

    match &args[0] {
        Value::Str(_) => {
            // Read from string
            let result = builtin_read_from_string(eval, args)?;
            // Return just the car (the parsed object)
            match &result {
                Value::Cons(cell) => {
                    let pair = cell.lock().expect("poisoned");
                    Ok(pair.car.clone())
                }
                _ => Ok(result),
            }
        }
        Value::Buffer(id) => {
            // Read from buffer at point
            let buf_id = *id;
            let (text, pt) = {
                let buf = eval
                    .buffers
                    .get(buf_id)
                    .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
                (buf.buffer_string(), buf.pt)
            };
            // pt is 1-based, substring from (pt-1)
            let start = if pt > 0 { pt - 1 } else { 0 };
            if start >= text.len() {
                return Err(signal(
                    "end-of-file",
                    vec![Value::string("End of file during parsing")],
                ));
            }
            let substring = &text[start..];
            let forms = super::parser::parse_forms(substring).map_err(|e| {
                signal(
                    "invalid-read-syntax",
                    vec![Value::string(e.message.clone())],
                )
            })?;
            if forms.is_empty() {
                return Err(signal(
                    "end-of-file",
                    vec![Value::string("End of file during parsing")],
                ));
            }
            let value = super::eval::quote_to_value(&forms[0]);
            // Advance point past the read form
            let end_offset = compute_read_end_position(substring, &forms[0]);
            let new_pt = pt + end_offset;
            if let Some(buf) = eval.buffers.get_mut(buf_id) {
                buf.pt = new_pt;
            }
            Ok(value)
        }
        _ => {
            // Unsupported stream type — treat as nil
            Ok(Value::Nil)
        }
    }
}

// ---------------------------------------------------------------------------
// 3. prin1-to-string (enhanced)
// ---------------------------------------------------------------------------

/// `(prin1-to-string OBJECT &optional NOESCAPE)`
///
/// Return the printed representation of OBJECT as a string.
/// If NOESCAPE is non-nil, don't escape special characters (like `princ`).
pub(crate) fn builtin_prin1_to_string_full(args: Vec<Value>) -> EvalResult {
    expect_min_args("prin1-to-string", &args, 1)?;

    let noescape = args.get(1).is_some_and(|v| v.is_truthy());

    if noescape {
        Ok(Value::string(print_value_no_escape(&args[0])))
    } else {
        Ok(Value::string(super::print::print_value(&args[0])))
    }
}

// ---------------------------------------------------------------------------
// 4. format-spec
// ---------------------------------------------------------------------------

/// `(format-spec FORMAT SPECIFICATION)`
///
/// Format a string using an alist of (CHAR . REPLACEMENT) pairs.
/// Each `%X` in FORMAT is replaced by the value associated with character X
/// in SPECIFICATION. `%%` produces a literal `%`.
pub(crate) fn builtin_format_spec(args: Vec<Value>) -> EvalResult {
    expect_args("format-spec", &args, 2)?;

    let fmt_str = expect_string(&args[0])?;

    // Build lookup from the alist
    let spec_list = list_to_vec(&args[1]).ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("listp"), args[1].clone()],
        )
    })?;

    let mut lookup = std::collections::HashMap::new();
    for entry in &spec_list {
        match entry {
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                let ch = match &pair.car {
                    Value::Char(c) => *c,
                    Value::Int(n) => char::from_u32(*n as u32).unwrap_or('?'),
                    _ => continue,
                };
                let replacement = match &pair.cdr {
                    Value::Str(s) => (**s).clone(),
                    Value::Int(n) => n.to_string(),
                    Value::Float(f) => format!("{}", f),
                    Value::Nil => "nil".to_string(),
                    Value::True => "t".to_string(),
                    Value::Symbol(s) => s.clone(),
                    other => super::print::print_value(other),
                };
                lookup.insert(ch, replacement);
            }
            _ => continue,
        }
    }

    let mut result = String::new();
    let mut chars = fmt_str.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '%' {
            match chars.peek() {
                Some(&'%') => {
                    chars.next();
                    result.push('%');
                }
                Some(&spec_ch) => {
                    chars.next();
                    if let Some(replacement) = lookup.get(&spec_ch) {
                        result.push_str(replacement);
                    } else {
                        // Unknown spec — keep as-is (Emacs signals error, but we are lenient)
                        result.push('%');
                        result.push(spec_ch);
                    }
                }
                None => {
                    // Trailing % at end of string
                    result.push('%');
                }
            }
        } else {
            result.push(ch);
        }
    }

    Ok(Value::string(result))
}

// ---------------------------------------------------------------------------
// 5. read-from-minibuffer (stub)
// ---------------------------------------------------------------------------

/// `(read-from-minibuffer PROMPT ...)`
///
/// Batch-mode behavior: signal `end-of-file` (no interactive input available).
pub(crate) fn builtin_read_from_minibuffer(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("read-from-minibuffer", &args, 1)?;
    let _prompt = expect_string(&args[0])?;
    Err(signal("end-of-file", vec![]))
}

// ---------------------------------------------------------------------------
// 6. read-string (stub)
// ---------------------------------------------------------------------------

/// `(read-string PROMPT ...)`
///
/// Batch-mode behavior: signal `end-of-file`.
pub(crate) fn builtin_read_string(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("read-string", &args, 1)?;
    let _prompt = expect_string(&args[0])?;
    Err(signal("end-of-file", vec![]))
}

// ---------------------------------------------------------------------------
// 7. read-number (stub)
// ---------------------------------------------------------------------------

/// `(read-number PROMPT &optional DEFAULT)`
///
/// Stub: returns DEFAULT if provided, otherwise 0.
pub(crate) fn builtin_read_number(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("read-number", &args, 1)?;
    Err(signal("end-of-file", vec![]))
}

// ---------------------------------------------------------------------------
// 8. completing-read (stub)
// ---------------------------------------------------------------------------

/// `(completing-read PROMPT COLLECTION ...)`
///
/// Batch-mode behavior: signal `end-of-file`.
pub(crate) fn builtin_completing_read(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("completing-read", &args, 2)?;
    let _prompt = expect_string(&args[0])?;
    Err(signal("end-of-file", vec![]))
}

// ---------------------------------------------------------------------------
// 9. y-or-n-p (stub)
// ---------------------------------------------------------------------------

/// `(y-or-n-p PROMPT)`
///
/// Batch-mode behavior: signal `end-of-file`.
pub(crate) fn builtin_y_or_n_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("y-or-n-p", &args, 1)?;
    Err(signal("end-of-file", vec![]))
}

// ---------------------------------------------------------------------------
// 10. yes-or-no-p (stub)
// ---------------------------------------------------------------------------

/// `(yes-or-no-p PROMPT)`
///
/// Batch-mode behavior: signal `end-of-file`.
pub(crate) fn builtin_yes_or_no_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("yes-or-no-p", &args, 1)?;
    Err(signal("end-of-file", vec![]))
}

// ---------------------------------------------------------------------------
// 11. read-char (stub)
// ---------------------------------------------------------------------------

/// `(read-char &optional PROMPT ...)`
///
/// Batch stub: returns nil (no input available).
pub(crate) fn builtin_read_char(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// 12. read-key-sequence (stub)
// ---------------------------------------------------------------------------

/// `(read-key-sequence PROMPT)`
///
/// Stub: returns empty vector.
pub(crate) fn builtin_read_key_sequence(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("read-key-sequence", &args, 1)?;
    Ok(Value::vector(vec![]))
}

// ---------------------------------------------------------------------------
// 13. with-temp-buffer (special form)
// ---------------------------------------------------------------------------

/// Special form: `(with-temp-buffer BODY...)`
///
/// Create a temporary buffer, make it current, evaluate BODY forms,
/// kill the buffer, and return the last value from BODY.
pub(crate) fn builtin_with_temp_buffer(
    eval: &mut super::eval::Evaluator,
    tail: &[Expr],
) -> EvalResult {
    // Generate a unique temp buffer name
    let name = eval.buffers.generate_new_buffer_name(" *temp*");
    let buf_id = eval.buffers.create_buffer(&name);

    // Save current buffer, switch to temp
    let saved = eval.buffers.current_buffer().map(|b| b.id);
    eval.buffers.set_current(buf_id);

    // Evaluate body
    let result = eval.sf_progn(tail);

    // Restore and kill temp buffer
    if let Some(saved_id) = saved {
        eval.buffers.set_current(saved_id);
    }
    eval.buffers.kill_buffer(buf_id);

    result
}

// ---------------------------------------------------------------------------
// 14. with-output-to-string (special form)
// ---------------------------------------------------------------------------

/// Special form: `(with-output-to-string BODY...)`
///
/// Evaluate BODY, capturing output from print functions into a temporary
/// buffer bound through `standard-output`.
pub(crate) fn sf_with_output_to_string(
    eval: &mut super::eval::Evaluator,
    tail: &[Expr],
) -> EvalResult {
    let temp_name = eval
        .buffers
        .generate_new_buffer_name(" *with-output-to-string*");
    let temp_id = eval.buffers.create_buffer(&temp_name);

    let mut frame = std::collections::HashMap::new();
    frame.insert("standard-output".to_string(), Value::Buffer(temp_id));
    eval.dynamic.push(frame);

    let body_result = eval.sf_progn(tail);
    let captured = eval
        .buffers
        .get(temp_id)
        .map(|buf| buf.buffer_string())
        .unwrap_or_default();

    let _ = eval.dynamic.pop();
    eval.buffers.kill_buffer(temp_id);

    body_result.map(|_| Value::string(captured))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::eval::Evaluator;
    use crate::elisp::parse_forms;

    // ===================================================================
    // read-from-string tests
    // ===================================================================

    #[test]
    fn read_from_string_integer() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("42")]).unwrap();
        // Should be (42 . 2)
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Int(42)));
                assert!(matches!(&pair.cdr, Value::Int(2)));
            }
            _ => panic!("Expected cons, got {:?}", result),
        }
    }

    #[test]
    fn read_from_string_symbol() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("hello")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Symbol(s) if s == "hello"));
                assert!(matches!(&pair.cdr, Value::Int(5)));
            }
            _ => panic!("Expected cons, got {:?}", result),
        }
    }

    #[test]
    fn read_from_string_string_value() {
        let mut ev = Evaluator::new();
        let result =
            builtin_read_from_string(&mut ev, vec![Value::string(r#""hello world""#)]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert_eq!(pair.car.as_str(), Some("hello world"));
                assert!(matches!(&pair.cdr, Value::Int(13)));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_list() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("(+ 1 2)")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                // car should be a list (+ 1 2)
                assert!(pair.car.is_cons());
                assert!(matches!(&pair.cdr, Value::Int(7)));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_with_start() {
        let mut ev = Evaluator::new();
        // "  42 rest" — start at 2
        let result =
            builtin_read_from_string(&mut ev, vec![Value::string("  42 rest"), Value::Int(2)])
                .unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Int(42)));
                assert!(matches!(&pair.cdr, Value::Int(4)));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_float() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("3.14")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Float(f) if (*f - 3.14).abs() < 1e-10));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_char() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("?a")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Char('a')));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_nil() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("nil")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(pair.car.is_nil());
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_t() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("t")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::True));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_vector() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("[1 2 3]")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(pair.car.is_vector());
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_quoted() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("'foo")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                // Should be (quote foo) as a list
                assert!(pair.car.is_cons());
                assert!(matches!(&pair.cdr, Value::Int(4)));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_dotted_pair() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("(a . b)")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                // car should be a dotted pair (a . b)
                assert!(pair.car.is_cons());
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_keyword() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string(":test")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Keyword(s) if s == ":test"));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_empty_error() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("")]);
        assert!(result.is_err());
    }

    #[test]
    fn read_from_string_whitespace_only_error() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("   ")]);
        assert!(result.is_err());
    }

    #[test]
    fn read_from_string_multiple_forms_reads_first() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("42 99")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Int(42)));
                // End position should be after "42" (position 2), not after "99"
                match &pair.cdr {
                    Value::Int(n) => assert!(*n <= 3, "end pos {} should be <= 3", n),
                    _ => panic!("Expected int end position"),
                }
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_with_start_and_end() {
        let mut ev = Evaluator::new();
        // "xxx42yyy" with start=3, end=5 -> substring "42"
        let result = builtin_read_from_string(
            &mut ev,
            vec![Value::string("xxx42yyy"), Value::Int(3), Value::Int(5)],
        )
        .unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Int(42)));
                assert!(matches!(&pair.cdr, Value::Int(5)));
            }
            _ => panic!("Expected cons"),
        }
    }

    // ===================================================================
    // read tests
    // ===================================================================

    #[test]
    fn read_from_string_stream() {
        let mut ev = Evaluator::new();
        let result = builtin_read(&mut ev, vec![Value::string("42")]).unwrap();
        assert!(matches!(result, Value::Int(42)));
    }

    #[test]
    fn read_nil_stream() {
        let mut ev = Evaluator::new();
        let result = builtin_read(&mut ev, vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn read_no_args() {
        let mut ev = Evaluator::new();
        let result = builtin_read(&mut ev, vec![]).unwrap();
        assert!(result.is_nil());
    }

    // ===================================================================
    // prin1-to-string (enhanced) tests
    // ===================================================================

    #[test]
    fn prin1_to_string_normal() {
        let result = builtin_prin1_to_string_full(vec![Value::string("hello")]).unwrap();
        // With escaping: should have quotes
        assert_eq!(result.as_str(), Some("\"hello\""));
    }

    #[test]
    fn prin1_to_string_noescape() {
        let result =
            builtin_prin1_to_string_full(vec![Value::string("hello"), Value::True]).unwrap();
        // Without escaping: no quotes
        assert_eq!(result.as_str(), Some("hello"));
    }

    #[test]
    fn prin1_to_string_int() {
        let result = builtin_prin1_to_string_full(vec![Value::Int(42)]).unwrap();
        assert_eq!(result.as_str(), Some("42"));
    }

    #[test]
    fn prin1_to_string_symbol() {
        let result = builtin_prin1_to_string_full(vec![Value::symbol("foo")]).unwrap();
        assert_eq!(result.as_str(), Some("foo"));
    }

    #[test]
    fn prin1_to_string_nil() {
        let result = builtin_prin1_to_string_full(vec![Value::Nil]).unwrap();
        assert_eq!(result.as_str(), Some("nil"));
    }

    #[test]
    fn prin1_to_string_list() {
        let lst = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let result = builtin_prin1_to_string_full(vec![lst]).unwrap();
        assert_eq!(result.as_str(), Some("(1 2)"));
    }

    // ===================================================================
    // format-spec tests
    // ===================================================================

    #[test]
    fn format_spec_basic() {
        let spec = Value::list(vec![Value::cons(Value::Char('a'), Value::string("world"))]);
        let result = builtin_format_spec(vec![Value::string("hello %a"), spec]).unwrap();
        assert_eq!(result.as_str(), Some("hello world"));
    }

    #[test]
    fn format_spec_multiple() {
        let spec = Value::list(vec![
            Value::cons(Value::Char('n'), Value::string("Alice")),
            Value::cons(Value::Char('a'), Value::Int(30)),
        ]);
        let result = builtin_format_spec(vec![Value::string("%n is %a years old"), spec]).unwrap();
        assert_eq!(result.as_str(), Some("Alice is 30 years old"));
    }

    #[test]
    fn format_spec_percent_escape() {
        let spec = Value::list(vec![Value::cons(Value::Char('x'), Value::string("100"))]);
        let result = builtin_format_spec(vec![Value::string("%x%%"), spec]).unwrap();
        assert_eq!(result.as_str(), Some("100%"));
    }

    #[test]
    fn format_spec_empty_spec() {
        let spec = Value::Nil;
        let result = builtin_format_spec(vec![Value::string("no specs here"), spec]).unwrap();
        assert_eq!(result.as_str(), Some("no specs here"));
    }

    #[test]
    fn format_spec_unknown_spec_kept() {
        let spec = Value::Nil;
        let result = builtin_format_spec(vec![Value::string("keep %z"), spec]).unwrap();
        assert_eq!(result.as_str(), Some("keep %z"));
    }

    #[test]
    fn format_spec_integer_char_key() {
        // Use integer ?a = 97 as key instead of Char
        let spec = Value::list(vec![Value::cons(Value::Int(97), Value::string("hello"))]);
        let result = builtin_format_spec(vec![Value::string("%a"), spec]).unwrap();
        assert_eq!(result.as_str(), Some("hello"));
    }

    #[test]
    fn format_spec_trailing_percent() {
        let spec = Value::Nil;
        let result = builtin_format_spec(vec![Value::string("end%"), spec]).unwrap();
        assert_eq!(result.as_str(), Some("end%"));
    }

    // ===================================================================
    // Stub function tests
    // ===================================================================

    #[test]
    fn read_from_minibuffer_signals_end_of_file() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_minibuffer(&mut ev, vec![Value::string("Prompt: ")]);
        assert!(result.is_err());
    }

    #[test]
    fn read_from_minibuffer_ignores_initial_and_signals_end_of_file() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_minibuffer(
            &mut ev,
            vec![Value::string("Prompt: "), Value::string("initial")],
        );
        assert!(result.is_err());
    }

    #[test]
    fn read_string_signals_end_of_file() {
        let mut ev = Evaluator::new();
        let result = builtin_read_string(&mut ev, vec![Value::string("Prompt: ")]);
        assert!(result.is_err());
    }

    #[test]
    fn read_string_ignores_initial_and_signals_end_of_file() {
        let mut ev = Evaluator::new();
        let result = builtin_read_string(
            &mut ev,
            vec![Value::string("Prompt: "), Value::string("initial")],
        );
        assert!(result.is_err());
    }

    #[test]
    fn read_number_signals_end_of_file_even_with_default() {
        let mut ev = Evaluator::new();
        let result = builtin_read_number(&mut ev, vec![Value::string("Number: "), Value::Int(42)]);
        assert!(result.is_err());
    }

    #[test]
    fn read_number_signals_end_of_file_without_default() {
        let mut ev = Evaluator::new();
        let result = builtin_read_number(&mut ev, vec![Value::string("Number: ")]);
        assert!(result.is_err());
    }

    #[test]
    fn completing_read_signals_end_of_file() {
        let mut ev = Evaluator::new();
        let result = builtin_completing_read(&mut ev, vec![Value::string("Choose: "), Value::Nil]);
        assert!(result.is_err());
    }

    #[test]
    fn completing_read_ignores_default_and_signals_end_of_file() {
        let mut ev = Evaluator::new();
        let result = builtin_completing_read(
            &mut ev,
            vec![
                Value::string("Choose: "),
                Value::Nil,
                Value::Nil,
                Value::Nil,
                Value::string("fallback"),
            ],
        );
        assert!(result.is_err());
    }

    #[test]
    fn y_or_n_p_signals_end_of_file() {
        let result = builtin_y_or_n_p(vec![Value::string("Continue? ")]);
        assert!(result.is_err());
    }

    #[test]
    fn yes_or_no_p_signals_end_of_file() {
        let result = builtin_yes_or_no_p(vec![Value::string("Confirm? ")]);
        assert!(result.is_err());
    }

    #[test]
    fn read_char_returns_nil() {
        let mut ev = Evaluator::new();
        let result = builtin_read_char(&mut ev, vec![]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn read_key_sequence_returns_empty_vector() {
        let mut ev = Evaluator::new();
        let result = builtin_read_key_sequence(&mut ev, vec![Value::string("key: ")]).unwrap();
        assert!(result.is_vector());
    }

    // ===================================================================
    // with-temp-buffer tests
    // ===================================================================

    #[test]
    fn with_temp_buffer_returns_last_value() {
        let mut ev = Evaluator::new();
        let body = parse_forms("1 2 3").unwrap();
        let result = builtin_with_temp_buffer(&mut ev, &body).unwrap();
        assert!(matches!(result, Value::Int(3)));
    }

    #[test]
    fn with_temp_buffer_cleans_up() {
        let mut ev = Evaluator::new();
        let initial_count = ev.buffers.buffer_list().len();
        let body = parse_forms("(insert \"test\") (buffer-string)").unwrap();
        let _result = builtin_with_temp_buffer(&mut ev, &body);
        // Temp buffer should be killed
        assert_eq!(ev.buffers.buffer_list().len(), initial_count);
    }

    // ===================================================================
    // with-output-to-string tests
    // ===================================================================

    #[test]
    fn with_output_to_string_captures_print_output() {
        let mut ev = Evaluator::new();
        let forms = parse_forms(r#"(with-output-to-string (princ "a") (prin1 '(1 2)) (print "x"))"#)
            .unwrap();
        let result = ev.eval_expr(&forms[0]).unwrap();
        assert_eq!(result.as_str(), Some("a(1 2)\n\"x\"\n"));
    }

    #[test]
    fn with_output_to_string_keeps_explicit_destination_working() {
        let mut ev = Evaluator::new();
        let forms = parse_forms(
            r#"(with-temp-buffer
                 (let ((buf (current-buffer)))
                   (with-output-to-string
                     (princ "captured")
                     (princ " to-buf" buf))
                   (buffer-string)))"#,
        )
        .unwrap();
        let result = ev.eval_expr(&forms[0]).unwrap();
        assert_eq!(result.as_str(), Some(" to-buf"));
    }

    // ===================================================================
    // Edge case / integration tests
    // ===================================================================

    #[test]
    fn read_from_string_nested_list() {
        let mut ev = Evaluator::new();
        let result =
            builtin_read_from_string(&mut ev, vec![Value::string("((a b) (c d))")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(pair.car.is_cons());
                assert!(matches!(&pair.cdr, Value::Int(13)));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_with_leading_whitespace() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("   42")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Int(42)));
                // End position should be 5 (after "   42")
                assert!(matches!(&pair.cdr, Value::Int(5)));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_negative_number() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("-7")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Int(-7)));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn format_spec_symbol_replacement() {
        let spec = Value::list(vec![Value::cons(Value::Char('s'), Value::symbol("foo"))]);
        let result = builtin_format_spec(vec![Value::string("sym: %s"), spec]).unwrap();
        assert_eq!(result.as_str(), Some("sym: foo"));
    }

    #[test]
    fn format_spec_nil_replacement() {
        let spec = Value::list(vec![Value::cons(Value::Char('v'), Value::Nil)]);
        let result = builtin_format_spec(vec![Value::string("val: %v"), spec]).unwrap();
        assert_eq!(result.as_str(), Some("val: nil"));
    }

    #[test]
    fn read_from_string_wrong_type() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::Int(42)]);
        assert!(result.is_err());
    }

    #[test]
    fn read_from_string_no_args() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn format_spec_wrong_args() {
        let result = builtin_format_spec(vec![Value::string("hello")]);
        assert!(result.is_err()); // needs 2 args
    }

    #[test]
    fn prin1_to_string_no_args() {
        let result = builtin_prin1_to_string_full(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn read_from_string_hash_syntax() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("#xff")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Int(255)));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_hash_dollar_uses_load_file_name() {
        let mut ev = Evaluator::new();
        ev.set_variable("load-file-name", Value::string("/tmp/reader-probe.elc"));
        let result = builtin_read_from_string(&mut ev, vec![Value::string("#$")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert_eq!(pair.car.as_str(), Some("/tmp/reader-probe.elc"));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_hash_dollar_defaults_to_nil() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("#$")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(pair.car.is_nil());
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_hash_skip_then_hash_dollar() {
        let mut ev = Evaluator::new();
        ev.set_variable("load-file-name", Value::string("/tmp/reader-skip.elc"));
        let result = builtin_read_from_string(&mut ev, vec![Value::string("#@4data#$")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert_eq!(pair.car.as_str(), Some("/tmp/reader-skip.elc"));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_hash_skip_bytes() {
        let mut ev = Evaluator::new();
        let input = "#@4data42 rest";
        let expected_end = input.find(" rest").unwrap() as i64;
        let result = builtin_read_from_string(&mut ev, vec![Value::string(input)]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Int(42)));
                assert_eq!(pair.cdr, Value::Int(expected_end));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_string_hash_bracket_end_position() {
        let mut ev = Evaluator::new();
        let input = "#[(x) \"\\bT\\207\" [x] 1 (#$ . 83)] tail";
        let expected_end = input.find(" tail").unwrap() as i64;
        let result = builtin_read_from_string(&mut ev, vec![Value::string(input)]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert_eq!(pair.cdr, Value::Int(expected_end));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn prin1_to_string_noescape_char() {
        // With NOESCAPE, a char should print as the character itself
        let result = builtin_prin1_to_string_full(vec![Value::Char('x'), Value::True]).unwrap();
        assert_eq!(result.as_str(), Some("x"));
    }

    #[test]
    fn format_spec_float_replacement() {
        let spec = Value::list(vec![Value::cons(Value::Char('f'), Value::Float(3.14))]);
        let result = builtin_format_spec(vec![Value::string("pi=%f"), spec]).unwrap();
        assert_eq!(result.as_str(), Some("pi=3.14"));
    }
}
