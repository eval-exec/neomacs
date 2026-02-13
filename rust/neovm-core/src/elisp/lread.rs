//! Reader-internals builtins: intern, intern-soft, read, read-from-string,
//! eval-buffer, eval-region, read-char, read-event, read-char-exclusive, load,
//! get-load-suffixes, locate-file, locate-file-internal, read-coding-system,
//! read-non-nil-coding-system.

use super::error::{signal, EvalResult, Flow, SignalData};
use super::value::*;
use std::path::Path;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

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
        Value::Symbol(s) => Ok(s.clone()),
        Value::Nil => Ok("nil".to_string()),
        Value::True => Ok("t".to_string()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins
// ---------------------------------------------------------------------------

/// `(intern STRING &optional OBARRAY)`
///
/// Intern a symbol with the given name in the obarray.  If the symbol already
/// exists, return the existing one; otherwise create it.  The optional OBARRAY
/// argument is accepted but ignored (we use the single global obarray).
pub(crate) fn builtin_intern(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("intern", &args, 1)?;
    let name = expect_string(&args[0])?;
    eval.obarray.intern(&name);
    Ok(Value::symbol(name))
}

/// `(intern-soft STRING &optional OBARRAY)`
///
/// Look up STRING in the obarray without creating a new symbol.
/// Return the symbol if found, nil otherwise.
pub(crate) fn builtin_intern_soft(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("intern-soft", &args, 1)?;
    let name = expect_string(&args[0])?;
    if eval.obarray.intern_soft(&name).is_some() {
        Ok(Value::symbol(name))
    } else {
        Ok(Value::Nil)
    }
}

/// `(read &optional STREAM)`
///
/// Read one Lisp expression from STREAM.
/// - If STREAM is a string, parse the first form from it.
/// - If STREAM is nil or omitted, return nil (no terminal input in batch mode).
/// - If STREAM is a buffer, read from buffer at point.
pub(crate) fn builtin_read(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    if args.is_empty() || args[0].is_nil() {
        // No stream / nil -- non-interactive, return nil
        return Ok(Value::Nil);
    }

    match &args[0] {
        Value::Str(s) => {
            let input = (**s).clone();
            let forms = super::parser::parse_forms(&input).map_err(|e| {
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
            Ok(super::eval::quote_to_value(&forms[0]))
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
            let end_offset = compute_read_end_position(substring);
            let new_pt = pt + end_offset;
            if let Some(buf) = eval.buffers.get_mut(buf_id) {
                buf.pt = new_pt;
            }
            Ok(value)
        }
        _ => {
            // Unsupported stream type -- treat as nil
            Ok(Value::Nil)
        }
    }
}

/// `(read-from-string STRING &optional START END)`
///
/// Read one Lisp object from STRING starting at position START (default 0).
/// Returns `(OBJECT . END-POSITION)` where END-POSITION is the character index
/// after the parsed object.
pub(crate) fn builtin_read_from_string(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("read-from-string", &args, 1)?;

    let full_string = match &args[0] {
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ))
        }
    };

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

    let value = super::eval::quote_to_value(&forms[0]);

    // Compute end position after the first form
    let end_pos = compute_read_end_position(substring);
    let absolute_end = start + end_pos;

    Ok(Value::cons(value, Value::Int(absolute_end as i64)))
}

/// `(eval-buffer &optional BUFFER PRINTFLAG FILENAME UNIBYTE DO-ALLOW-PRINT)`
///
/// Stub: returns nil.
pub(crate) fn builtin_eval_buffer(
    _eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    Ok(Value::Nil)
}

/// `(eval-region START END &optional PRINTFLAG READ-FUNCTION)`
///
/// Stub: returns nil.
pub(crate) fn builtin_eval_region(
    _eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    Ok(Value::Nil)
}

/// `(read-char &optional PROMPT INHERIT-INPUT-METHOD SECONDS)`
///
/// Batch stub: returns nil (no terminal input available).
pub(crate) fn builtin_read_char(
    _eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    Ok(Value::Nil)
}

/// `(read-event &optional PROMPT INHERIT-INPUT-METHOD SECONDS)`
///
/// Stub: returns nil (no event input available in batch mode).
pub(crate) fn builtin_read_event(
    _eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    Ok(Value::Nil)
}

/// `(read-char-exclusive &optional PROMPT INHERIT-INPUT-METHOD SECONDS)`
///
/// Batch stub: returns nil (no terminal input available).
pub(crate) fn builtin_read_char_exclusive(
    _eval: &mut super::eval::Evaluator,
    _args: Vec<Value>,
) -> EvalResult {
    Ok(Value::Nil)
}

/// `(load FILE &optional NOERROR NOMESSAGE NOSUFFIX MUST-SUFFIX)`
///
/// Load a file of Lisp code.  Delegates to the existing load module when
/// possible; otherwise signals an error or returns nil based on NOERROR.
pub(crate) fn builtin_load(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("load", &args, 1)?;
    let file = match &args[0] {
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ))
        }
    };
    let noerror = args.get(1).is_some_and(|v| v.is_truthy());
    let nosuffix = args.get(3).is_some_and(|v| v.is_truthy());
    let must_suffix = args.get(4).is_some_and(|v| v.is_truthy());
    let prefer_newer = eval
        .obarray
        .symbol_value("load-prefer-newer")
        .is_some_and(|v| v.is_truthy());

    let load_path = super::load::get_load_path(&eval.obarray);
    match super::load::find_file_in_load_path_with_flags(
        &file,
        &load_path,
        nosuffix,
        must_suffix,
        prefer_newer,
    ) {
        Some(path) => super::load::load_file(eval, &path).map_err(eval_error_to_flow),
        None => {
            if noerror {
                Ok(Value::Nil)
            } else {
                Err(signal(
                    "file-missing",
                    vec![Value::string(format!("Cannot open load file: {}", file))],
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// `(get-load-suffixes)`
///
/// Return a list of suffixes that `load` tries when searching for files.
pub(crate) fn builtin_get_load_suffixes(args: Vec<Value>) -> EvalResult {
    let _ = args;
    Ok(Value::list(vec![
        Value::string(".so"),
        Value::string(".so.gz"),
        Value::string(".el"),
        Value::string(".el.gz"),
    ]))
}

/// `(locate-file FILENAME PATH SUFFIXES &optional PREDICATE)`
///
/// Search PATH for FILENAME with each suffix in SUFFIXES.
pub(crate) fn builtin_locate_file(args: Vec<Value>) -> EvalResult {
    expect_min_args("locate-file", &args, 3)?;
    let filename = expect_string(&args[0])?;
    let path = parse_path_argument(&args[1])?;
    let suffixes = parse_suffixes_argument(&args[2])?;
    Ok(match locate_file_with_path_and_suffixes(&filename, &path, &suffixes, args.get(3))? {
        Some(found) => Value::string(found),
        None => Value::Nil,
    })
}

/// `(locate-file-internal FILENAME PATH SUFFIXES &optional PREDICATE)`
///
/// Internal variant of `locate-file`; currently uses the same lookup behavior.
pub(crate) fn builtin_locate_file_internal(args: Vec<Value>) -> EvalResult {
    expect_min_args("locate-file-internal", &args, 3)?;
    let filename = expect_string(&args[0])?;
    let path = parse_path_argument(&args[1])?;
    let suffixes = parse_suffixes_argument(&args[2])?;
    Ok(match locate_file_with_path_and_suffixes(&filename, &path, &suffixes, args.get(3))? {
        Some(found) => Value::string(found),
        None => Value::Nil,
    })
}

/// `(read-coding-system PROMPT &optional DEFAULT-CODING-SYSTEM)`
///
/// Stub: returns the symbol `utf-8`.
pub(crate) fn builtin_read_coding_system(args: Vec<Value>) -> EvalResult {
    expect_min_args("read-coding-system", &args, 1)?;
    Ok(Value::symbol("utf-8"))
}

/// `(read-non-nil-coding-system PROMPT)`
///
/// Stub: returns the symbol `utf-8`.
pub(crate) fn builtin_read_non_nil_coding_system(args: Vec<Value>) -> EvalResult {
    expect_min_args("read-non-nil-coding-system", &args, 1)?;
    Ok(Value::symbol("utf-8"))
}

// ---------------------------------------------------------------------------
// Error conversion helper
// ---------------------------------------------------------------------------

/// Convert an `EvalError` back to a `Flow` for builtins that call `load_file`.
fn eval_error_to_flow(e: super::error::EvalError) -> Flow {
    match e {
        super::error::EvalError::Signal { symbol, data } => {
            Flow::Signal(SignalData { symbol, data })
        }
        super::error::EvalError::UncaughtThrow { tag, value } => Flow::Throw { tag, value },
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn expect_list(value: &Value) -> Result<Vec<Value>, Flow> {
    list_to_vec(value).ok_or_else(|| {
        signal(
            "wrong-type-argument",
            vec![Value::symbol("listp"), value.clone()],
        )
    })
}

fn parse_path_argument(value: &Value) -> Result<Vec<String>, Flow> {
    let mut path = Vec::new();
    for entry in expect_list(value)? {
        match entry {
            Value::Nil => path.push(".".to_string()),
            Value::Str(s) => path.push((*s).clone()),
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), other],
                ))
            }
        }
    }
    Ok(path)
}

fn parse_suffixes_argument(value: &Value) -> Result<Vec<String>, Flow> {
    let mut suffixes = Vec::new();
    for entry in expect_list(value)? {
        match entry {
            Value::Nil => suffixes.push(String::new()),
            Value::Str(s) => suffixes.push((*s).clone()),
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), other],
                ))
            }
        }
    }
    Ok(suffixes)
}

fn locate_file_with_path_and_suffixes(
    filename: &str,
    path: &[String],
    suffixes: &[String],
    predicate: Option<&Value>,
) -> Result<Option<String>, Flow> {
    let effective_suffixes: Vec<&str> = if suffixes.is_empty() {
        vec![""]
    } else {
        suffixes.iter().map(|s| s.as_str()).collect()
    };

    let absolute = Path::new(filename).is_absolute();
    if absolute || path.is_empty() {
        for suffix in &effective_suffixes {
            let candidate = format!("{filename}{suffix}");
            if Path::new(&candidate).exists()
                && predicate_matches_candidate(predicate, &candidate)?
            {
                return Ok(Some(candidate));
            }
        }
        return Ok(None);
    }

    for dir in path {
        let base = Path::new(dir).join(filename);
        let base = base.to_string_lossy();
        for suffix in &effective_suffixes {
            let candidate = format!("{base}{suffix}");
            if Path::new(&candidate).exists()
                && predicate_matches_candidate(predicate, &candidate)?
            {
                return Ok(Some(candidate));
            }
        }
    }

    Ok(None)
}

fn predicate_matches_candidate(predicate: Option<&Value>, candidate: &str) -> Result<bool, Flow> {
    let Some(predicate) = predicate else {
        return Ok(true);
    };
    if predicate.is_nil() {
        return Ok(true);
    }

    let Some(symbol) = predicate.as_symbol_name() else {
        // We currently only support symbol predicates in pure dispatch;
        // unknown predicate object shapes default to accepting candidate.
        return Ok(true);
    };
    let Some(result) = super::builtins::dispatch_builtin_pure(symbol, vec![Value::string(candidate)])
    else {
        // Emacs locate-file tolerates non-callable predicate values in practice.
        // Keep search behavior instead of surfacing an execution error here.
        return Ok(true);
    };
    Ok(result?.is_truthy())
}

/// Compute the byte offset after the first s-expression in `input`.
/// Skips leading whitespace/comments, then skips one complete sexp.
fn compute_read_end_position(input: &str) -> usize {
    let pos = skip_ws_comments(input, 0);
    if pos >= input.len() {
        return input.len();
    }
    skip_one_sexp(input, pos)
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
            // block comment #| ... |#
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
            // quote / backquote -- skip prefix then one sexp
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
                // Regular character -- consume one UTF-8 char
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::eval::Evaluator;

    #[test]
    fn intern_creates_symbol() {
        let mut ev = Evaluator::new();
        let result = builtin_intern(&mut ev, vec![Value::string("my-sym")]).unwrap();
        assert!(matches!(result, Value::Symbol(ref s) if s == "my-sym"));
    }

    #[test]
    fn intern_soft_found() {
        let mut ev = Evaluator::new();
        // First intern it
        builtin_intern(&mut ev, vec![Value::string("existing")]).unwrap();
        // Then intern-soft should find it
        let result = builtin_intern_soft(&mut ev, vec![Value::string("existing")]).unwrap();
        assert!(matches!(result, Value::Symbol(ref s) if s == "existing"));
    }

    #[test]
    fn intern_soft_not_found() {
        let mut ev = Evaluator::new();
        let result = builtin_intern_soft(&mut ev, vec![Value::string("nonexistent")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn read_from_string_integer() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("42")]).unwrap();
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
                assert!(matches!(&pair.car, Value::Symbol(ref s) if s == "hello"));
                assert!(matches!(&pair.cdr, Value::Int(5)));
            }
            _ => panic!("Expected cons, got {:?}", result),
        }
    }

    #[test]
    fn read_from_string_with_start() {
        let mut ev = Evaluator::new();
        let result =
            builtin_read_from_string(&mut ev, vec![Value::string("  42 rest"), Value::Int(2)])
                .unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(matches!(&pair.car, Value::Int(42)));
                assert!(matches!(&pair.cdr, Value::Int(4)));
            }
            _ => panic!("Expected cons, got {:?}", result),
        }
    }

    #[test]
    fn read_from_string_empty_error() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("")]);
        assert!(result.is_err());
    }

    #[test]
    fn read_from_string_list() {
        let mut ev = Evaluator::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string("(+ 1 2)")]).unwrap();
        match &result {
            Value::Cons(cell) => {
                let pair = cell.lock().unwrap();
                assert!(pair.car.is_cons());
                assert!(matches!(&pair.cdr, Value::Int(7)));
            }
            _ => panic!("Expected cons"),
        }
    }

    #[test]
    fn read_from_nil_stream() {
        let mut ev = Evaluator::new();
        let result = builtin_read(&mut ev, vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn read_from_string_stream() {
        let mut ev = Evaluator::new();
        let result = builtin_read(&mut ev, vec![Value::string("42")]).unwrap();
        assert!(matches!(result, Value::Int(42)));
    }

    #[test]
    fn read_no_args() {
        let mut ev = Evaluator::new();
        let result = builtin_read(&mut ev, vec![]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn eval_buffer_returns_nil() {
        let mut ev = Evaluator::new();
        let result = builtin_eval_buffer(&mut ev, vec![]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn eval_region_returns_nil() {
        let mut ev = Evaluator::new();
        let result = builtin_eval_region(&mut ev, vec![Value::Int(1), Value::Int(10)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn read_char_returns_nil() {
        let mut ev = Evaluator::new();
        let result = builtin_read_char(&mut ev, vec![]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn read_event_returns_nil() {
        let mut ev = Evaluator::new();
        let result = builtin_read_event(&mut ev, vec![]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn read_char_exclusive_returns_nil() {
        let mut ev = Evaluator::new();
        let result = builtin_read_char_exclusive(&mut ev, vec![]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn get_load_suffixes_returns_list() {
        let result = builtin_get_load_suffixes(vec![]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].as_str(), Some(".so"));
        assert_eq!(items[1].as_str(), Some(".so.gz"));
        assert_eq!(items[2].as_str(), Some(".el"));
        assert_eq!(items[3].as_str(), Some(".el.gz"));
    }

    #[test]
    fn locate_file_finds_first_matching_suffix() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-locate-file-{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("probe.el"), "(setq vm-locate 1)\n").expect("write .el");
        fs::write(dir.join("probe.elc"), "compiled").expect("write .elc");

        let result = builtin_locate_file(vec![
            Value::string("probe"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::list(vec![Value::string(".el"), Value::string(".elc")]),
        ])
        .expect("locate-file should succeed");
        let found = result.as_str().expect("locate-file should return path");
        assert!(
            found.ends_with("probe.el"),
            "expected first matching suffix (.el), got {found}",
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn locate_file_respects_symbol_predicates() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-locate-file-predicate-{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("probe.el"), "(setq vm-locate 1)\n").expect("write .el");

        let regular = builtin_locate_file(vec![
            Value::string("probe"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::list(vec![Value::string(".el")]),
            Value::symbol("file-regular-p"),
        ])
        .expect("locate-file with file-regular-p should evaluate");
        assert!(
            regular.as_str().is_some(),
            "regular-file predicate should accept candidate",
        );

        let directory = builtin_locate_file(vec![
            Value::string("probe"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::list(vec![Value::string(".el")]),
            Value::symbol("file-directory-p"),
        ])
        .expect("locate-file with file-directory-p should evaluate");
        assert!(directory.is_nil(), "directory predicate should reject file");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn locate_file_unknown_predicate_defaults_to_truthy_match() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-locate-file-bad-predicate-{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("probe.el"), "(setq vm-locate 1)\n").expect("write .el");

        let result = builtin_locate_file(vec![
            Value::string("probe"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::list(vec![Value::string(".el")]),
            Value::symbol("definitely-not-a-real-predicate"),
        ])
        .expect("locate-file should evaluate");
        let found = result
            .as_str()
            .expect("unknown predicate should not prevent match");
        assert!(found.ends_with("probe.el"), "unexpected result: {found}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn locate_file_internal_returns_nil_when_missing() {
        let result = builtin_locate_file_internal(vec![
            Value::string("definitely-missing-neovm-file"),
            Value::list(vec![Value::string(".")]),
            Value::list(vec![Value::string(".el")]),
        ])
        .expect("locate-file-internal should evaluate");
        assert!(result.is_nil());
    }

    #[test]
    fn locate_file_internal_finds_requested_suffix() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-locate-file-internal-{unique}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("probe.elc"), "compiled").expect("write .elc");

        let result = builtin_locate_file_internal(vec![
            Value::string("probe"),
            Value::list(vec![Value::string(dir.to_string_lossy())]),
            Value::list(vec![Value::string(".elc")]),
        ])
        .expect("locate-file-internal should succeed");
        let found = result.as_str().expect("locate-file-internal should return path");
        assert!(
            found.ends_with("probe.elc"),
            "expected .elc resolution, got {found}",
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_coding_system_returns_utf8() {
        let result = builtin_read_coding_system(vec![Value::string("Coding system: ")]).unwrap();
        assert!(matches!(result, Value::Symbol(ref s) if s == "utf-8"));
    }

    #[test]
    fn read_non_nil_coding_system_returns_utf8() {
        let result =
            builtin_read_non_nil_coding_system(vec![Value::string("Coding system: ")]).unwrap();
        assert!(matches!(result, Value::Symbol(ref s) if s == "utf-8"));
    }
}
