//! Miscellaneous commonly-needed builtins.
//!
//! Contains:
//! - Special forms: prog2, with-temp-buffer, save-current-buffer, track-mouse, with-syntax-table
//! - Pure builtins: copy-alist, rassoc, rassq, assoc-default, make-list, safe-length,
//!   subst-char-in-string, replace-regexp-in-string, string-match-p, string/char encoding
//!   stubs, nconc (improved), locale-info
//! - Eval-dependent builtins: backtrace-frame, recursion-depth, abort-recursive-edit

use super::error::{signal, EvalResult, Flow};
use super::expr::Expr;
use super::string_escape::{bytes_to_unibyte_storage_string, encode_nonunicode_char_for_storage};
use super::value::*;

const RAW_BYTE_SENTINEL_BASE: u32 = 0xE000;
const RAW_BYTE_SENTINEL_MIN: u32 = 0xE080;
const RAW_BYTE_SENTINEL_MAX: u32 = 0xE0FF;
const UNIBYTE_BYTE_SENTINEL_BASE: u32 = 0xE300;
const UNIBYTE_BYTE_SENTINEL_MIN: u32 = 0xE300;
const UNIBYTE_BYTE_SENTINEL_MAX: u32 = 0xE3FF;
const MAX_EMACS_CHAR: i64 = 0x3FFFFF;

// ---------------------------------------------------------------------------
// Argument helpers (local to this module)
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

fn expect_int(val: &Value) -> Result<i64, Flow> {
    match val {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), other.clone()],
        )),
    }
}

fn expect_string(val: &Value) -> Result<String, Flow> {
    match val {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

fn expect_char(val: &Value) -> Result<char, Flow> {
    match val {
        Value::Char(c) => Ok(*c),
        Value::Int(n) => char::from_u32(*n as u32).ok_or_else(|| {
            signal(
                "wrong-type-argument",
                vec![Value::symbol("characterp"), val.clone()],
            )
        }),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("characterp"), other.clone()],
        )),
    }
}

fn expect_character_code(val: &Value) -> Result<i64, Flow> {
    match val {
        Value::Char(c) => Ok(*c as i64),
        Value::Int(n) if (0..=MAX_EMACS_CHAR).contains(n) => Ok(*n),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("characterp"), other.clone()],
        )),
    }
}

fn convert_unibyte_storage_to_multibyte(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let cp = ch as u32;
        if (UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&cp) {
            let byte = cp - UNIBYTE_BYTE_SENTINEL_BASE;
            if byte <= 0x7F {
                out.push(char::from_u32(byte).expect("ascii scalar"));
            } else {
                let raw_code = 0x3FFF00 + byte;
                let encoded = encode_nonunicode_char_for_storage(raw_code)
                    .expect("raw-byte code should be encodable");
                out.push_str(&encoded);
            }
            continue;
        }
        out.push(ch);
    }
    out
}

// ===========================================================================
// Special forms
// ===========================================================================

/// `(prog2 FORM1 FORM2 BODY...)` -- evaluate all forms, return result of FORM2.
pub(crate) fn sf_prog2(eval: &mut super::eval::Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.len() < 2 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }
    // Evaluate FORM1, discard
    eval.eval(&tail[0])?;
    // Evaluate FORM2, save result
    let second = eval.eval(&tail[1])?;
    // Evaluate remaining BODY forms, discard
    for form in &tail[2..] {
        eval.eval(form)?;
    }
    Ok(second)
}

/// `(with-temp-buffer BODY...)` -- create a temp buffer, make it current,
/// execute BODY, kill the buffer, restore previous buffer, return last result.
pub(crate) fn sf_with_temp_buffer(eval: &mut super::eval::Evaluator, tail: &[Expr]) -> EvalResult {
    // Save current buffer
    let saved_buf = eval.buffers.current_buffer().map(|b| b.id);

    // Create a temporary buffer
    let temp_name = eval.buffers.generate_new_buffer_name(" *temp*");
    let temp_id = eval.buffers.create_buffer(&temp_name);
    eval.buffers.set_current(temp_id);

    // Execute body
    let result = eval.sf_progn(tail);

    // Kill temp buffer and restore
    eval.buffers.kill_buffer(temp_id);
    if let Some(saved_id) = saved_buf {
        eval.buffers.set_current(saved_id);
    }
    result
}

/// `(save-current-buffer BODY...)` -- save the current buffer, execute BODY,
/// then restore the previous current buffer.
pub(crate) fn sf_save_current_buffer(
    eval: &mut super::eval::Evaluator,
    tail: &[Expr],
) -> EvalResult {
    let saved_buf = eval.buffers.current_buffer().map(|b| b.id);
    let result = eval.sf_progn(tail);
    if let Some(saved_id) = saved_buf {
        eval.buffers.set_current(saved_id);
    }
    result
}

/// `(track-mouse BODY...)` -- stub: just evaluates body forms.
/// In real Emacs this enables mouse tracking; we skip that for now.
pub(crate) fn sf_track_mouse(eval: &mut super::eval::Evaluator, tail: &[Expr]) -> EvalResult {
    eval.sf_progn(tail)
}

/// `(with-syntax-table TABLE BODY...)` -- stub: evaluates TABLE (discards),
/// then evaluates BODY. Syntax table switching not yet implemented.
pub(crate) fn sf_with_syntax_table(eval: &mut super::eval::Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.is_empty() {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }
    // Evaluate TABLE expression (discard result)
    eval.eval(&tail[0])?;
    // Evaluate body
    eval.sf_progn(&tail[1..])
}

// ===========================================================================
// Pure builtins (no eval needed)
// ===========================================================================

/// `(copy-alist ALIST)` -- shallow copy an association list.
/// Each top-level cons is copied; the car/cdr of each entry are shared.
pub(crate) fn builtin_copy_alist(args: Vec<Value>) -> EvalResult {
    expect_args("copy-alist", &args, 1)?;
    let alist = &args[0];
    let mut result = Vec::new();
    let mut cursor = alist.clone();
    loop {
        match cursor {
            Value::Nil => break,
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                // If the element is a cons, copy it; otherwise keep as-is
                let entry = match &pair.car {
                    Value::Cons(inner) => {
                        let inner_pair = inner.lock().expect("poisoned");
                        Value::cons(inner_pair.car.clone(), inner_pair.cdr.clone())
                    }
                    other => other.clone(),
                };
                result.push(entry);
                cursor = pair.cdr.clone();
            }
            _ => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), alist.clone()],
                ));
            }
        }
    }
    Ok(Value::list(result))
}

/// `(rassoc KEY ALIST)` -- find the first entry in ALIST whose cdr equals KEY
/// (using `equal`).
pub(crate) fn builtin_rassoc(args: Vec<Value>) -> EvalResult {
    expect_args("rassoc", &args, 2)?;
    let key = &args[0];
    let alist = &args[1];
    let mut cursor = alist.clone();
    loop {
        match cursor {
            Value::Nil => return Ok(Value::Nil),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if let Value::Cons(inner) = &pair.car {
                    let inner_pair = inner.lock().expect("poisoned");
                    if equal_value(&inner_pair.cdr, key, 0) {
                        return Ok(pair.car.clone());
                    }
                }
                cursor = pair.cdr.clone();
            }
            _ => return Ok(Value::Nil),
        }
    }
}

/// `(rassq KEY ALIST)` -- like rassoc but uses `eq` for comparison.
pub(crate) fn builtin_rassq(args: Vec<Value>) -> EvalResult {
    expect_args("rassq", &args, 2)?;
    let key = &args[0];
    let alist = &args[1];
    let mut cursor = alist.clone();
    loop {
        match cursor {
            Value::Nil => return Ok(Value::Nil),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if let Value::Cons(inner) = &pair.car {
                    let inner_pair = inner.lock().expect("poisoned");
                    if eq_value(&inner_pair.cdr, key) {
                        return Ok(pair.car.clone());
                    }
                }
                cursor = pair.cdr.clone();
            }
            _ => return Ok(Value::Nil),
        }
    }
}

/// `(assoc-default KEY ALIST &optional TEST DEFAULT)` -- find KEY in ALIST,
/// return the cdr of the matching entry (or DEFAULT if not found).
/// TEST defaults to `equal`.
pub(crate) fn builtin_assoc_default(args: Vec<Value>) -> EvalResult {
    expect_min_args("assoc-default", &args, 2)?;
    let key = &args[0];
    let alist = &args[1];
    let use_eq = if args.len() > 2 {
        // If test is 'eq, use eq; otherwise use equal
        match &args[2] {
            Value::Symbol(s) if s == "eq" => true,
            _ => false,
        }
    } else {
        false
    };
    let default = args.get(3).cloned().unwrap_or(Value::Nil);

    let mut cursor = alist.clone();
    loop {
        match cursor {
            Value::Nil => return Ok(default),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if let Value::Cons(inner) = &pair.car {
                    let inner_pair = inner.lock().expect("poisoned");
                    let matches = if use_eq {
                        eq_value(&inner_pair.car, key)
                    } else {
                        equal_value(&inner_pair.car, key, 0)
                    };
                    if matches {
                        return Ok(inner_pair.cdr.clone());
                    }
                }
                cursor = pair.cdr.clone();
            }
            _ => return Ok(default),
        }
    }
}

/// `(make-list LENGTH INIT)` -- create a list of LENGTH elements, each INIT.
pub(crate) fn builtin_make_list(args: Vec<Value>) -> EvalResult {
    expect_args("make-list", &args, 2)?;
    let length = expect_int(&args[0])?;
    if length < 0 {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("natnump"), args[0].clone()],
        ));
    }
    let init = &args[1];
    let items: Vec<Value> = (0..length as usize).map(|_| init.clone()).collect();
    Ok(Value::list(items))
}

/// `(safe-length LIST)` -- return the length of LIST, returning 0 for
/// non-lists and stopping at circular references (up to a limit).
pub(crate) fn builtin_safe_length(args: Vec<Value>) -> EvalResult {
    expect_args("safe-length", &args, 1)?;
    let list = &args[0];
    if list.is_nil() {
        return Ok(Value::Int(0));
    }
    if !list.is_cons() {
        return Ok(Value::Int(0));
    }

    // Traverse once while running tortoise-and-hare cycle detection.
    // `length` tracks visited cons cells via `slow`.
    let mut slow = list.clone();
    let mut fast = list.clone();
    let mut length: i64 = 0;

    loop {
        // Advance slow by 1
        match slow {
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                slow = pair.cdr.clone();
                length += 1;
            }
            _ => return Ok(Value::Int(length)),
        }

        // Advance fast by 2 when possible. If it reaches a non-cons, we still
        // continue counting via `slow` so proper odd-length lists are exact.
        for _ in 0..2 {
            match fast {
                Value::Cons(cell) => {
                    let pair = cell.lock().expect("poisoned");
                    fast = pair.cdr.clone();
                }
                _ => {
                    fast = Value::Nil;
                    break;
                }
            }
        }

        // Check for cycle (pointer equality)
        if let (Value::Cons(a), Value::Cons(b)) = (&slow, &fast) {
            if std::sync::Arc::ptr_eq(a, b) {
                // Circular list detected; return count so far
                return Ok(Value::Int(length));
            }
        }

        // Safety limit to avoid infinite loops
        if length > 10_000_000 {
            return Ok(Value::Int(length));
        }
    }
}

/// `(subst-char-in-string FROMCHAR TOCHAR STRING &optional INPLACE)` --
/// replace all occurrences of FROMCHAR with TOCHAR in STRING.
/// INPLACE is ignored (we always return a new string).
pub(crate) fn builtin_subst_char_in_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("subst-char-in-string", &args, 3)?;
    let from_char = expect_char(&args[0])?;
    let to_char = expect_char(&args[1])?;
    let s = expect_string(&args[2])?;
    let result: String = s
        .chars()
        .map(|c| if c == from_char { to_char } else { c })
        .collect();
    Ok(Value::string(result))
}

/// `(replace-regexp-in-string REGEXP REP STRING &optional FIXEDCASE LITERAL SUBEXP START)` --
/// replace all matches of REGEXP in STRING with REP.
pub(crate) fn builtin_replace_regexp_in_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("replace-regexp-in-string", &args, 3)?;
    let pattern = expect_string(&args[0])?;
    let rep = expect_string(&args[1])?;
    let s = expect_string(&args[2])?;
    let _fixedcase = args.get(3).is_some_and(|v| v.is_truthy());
    let literal = args.get(4).is_some_and(|v| v.is_truthy());
    let _subexp = args.get(5);
    let start = if args.len() > 6 {
        expect_int(&args[6])? as usize
    } else {
        0
    };

    let rust_pattern = super::regex::translate_emacs_regex(&pattern);
    let re = regex::Regex::new(&rust_pattern)
        .map_err(|e| signal("invalid-regexp", vec![Value::string(format!("{e}"))]))?;

    let search_region = if start > 0 && start < s.len() {
        &s[start..]
    } else {
        &s
    };

    let result = if literal {
        re.replace_all(search_region, regex::NoExpand(&rep))
            .into_owned()
    } else {
        // Translate Emacs-style back-references (\1, \2, etc.) to regex crate style ($1, $2)
        let rust_rep = rep
            .replace("\\&", "${0}")
            .replace("\\1", "${1}")
            .replace("\\2", "${2}")
            .replace("\\3", "${3}")
            .replace("\\4", "${4}")
            .replace("\\5", "${5}")
            .replace("\\6", "${6}")
            .replace("\\7", "${7}")
            .replace("\\8", "${8}")
            .replace("\\9", "${9}");
        re.replace_all(search_region, rust_rep.as_str())
            .into_owned()
    };

    if start > 0 && start < s.len() {
        Ok(Value::string(format!("{}{}", &s[..start], result)))
    } else {
        Ok(Value::string(result))
    }
}

/// `(string-match-p REGEXP STRING &optional START)` -- like `string-match`
/// but does not change match data.
pub(crate) fn builtin_string_match_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("string-match-p", &args, 2)?;
    let pattern = expect_string(&args[0])?;
    let s = expect_string(&args[1])?;
    let start = if args.len() > 2 {
        expect_int(&args[2])? as usize
    } else {
        0
    };

    // Use string_match_full with a throwaway match data so we don't
    // modify the evaluator's match data.
    let mut throwaway: Option<super::regex::MatchData> = None;
    match super::regex::string_match_full(&pattern, &s, start, &mut throwaway) {
        Ok(Some(pos)) => Ok(Value::Int(pos as i64)),
        Ok(None) => Ok(Value::Nil),
        Err(msg) => Err(signal("invalid-regexp", vec![Value::string(msg)])),
    }
}

/// `(string-to-multibyte STRING)` -- convert unibyte storage bytes to multibyte chars.
pub(crate) fn builtin_string_to_multibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-to-multibyte", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::string(convert_unibyte_storage_to_multibyte(&s)))
}

/// `(string-to-unibyte STRING)` -- convert to unibyte storage.
pub(crate) fn builtin_string_to_unibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-to-unibyte", &args, 1)?;
    let s = expect_string(&args[0])?;

    let mut bytes = Vec::with_capacity(s.chars().count());
    for (idx, ch) in s.chars().enumerate() {
        let cp = ch as u32;
        if cp <= 0x7F {
            bytes.push(cp as u8);
            continue;
        }
        if (RAW_BYTE_SENTINEL_MIN..=RAW_BYTE_SENTINEL_MAX).contains(&cp) {
            bytes.push((cp - RAW_BYTE_SENTINEL_BASE) as u8);
            continue;
        }
        if (UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&cp) {
            bytes.push((cp - UNIBYTE_BYTE_SENTINEL_BASE) as u8);
            continue;
        }

        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Cannot convert character at index {idx} to unibyte"
            ))],
        ));
    }

    Ok(Value::string(bytes_to_unibyte_storage_string(&bytes)))
}

/// `(string-as-unibyte STRING)` -- reinterpret as unibyte byte sequence.
pub(crate) fn builtin_string_as_unibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-as-unibyte", &args, 1)?;
    let s = expect_string(&args[0])?;

    let mut bytes = Vec::with_capacity(s.len());
    for ch in s.chars() {
        let cp = ch as u32;
        if cp <= 0x7F {
            bytes.push(cp as u8);
            continue;
        }
        if (RAW_BYTE_SENTINEL_MIN..=RAW_BYTE_SENTINEL_MAX).contains(&cp) {
            bytes.push((cp - RAW_BYTE_SENTINEL_BASE) as u8);
            continue;
        }
        if (UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&cp) {
            bytes.push((cp - UNIBYTE_BYTE_SENTINEL_BASE) as u8);
            continue;
        }

        let mut utf8 = [0u8; 4];
        let encoded = ch.encode_utf8(&mut utf8);
        bytes.extend_from_slice(encoded.as_bytes());
    }

    Ok(Value::string(bytes_to_unibyte_storage_string(&bytes)))
}

/// `(string-as-multibyte STRING)` -- reinterpret unibyte storage as multibyte.
pub(crate) fn builtin_string_as_multibyte(args: Vec<Value>) -> EvalResult {
    expect_args("string-as-multibyte", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::string(convert_unibyte_storage_to_multibyte(&s)))
}

/// `(unibyte-char-to-multibyte CHAR)` -- map 0..255 to multibyte/raw-byte char code.
pub(crate) fn builtin_unibyte_char_to_multibyte(args: Vec<Value>) -> EvalResult {
    expect_args("unibyte-char-to-multibyte", &args, 1)?;
    let code = expect_character_code(&args[0])?;
    if code > 0xFF {
        return Err(signal(
            "error",
            vec![Value::string(format!("Not a unibyte character: {code}"))],
        ));
    }
    if code < 0x80 {
        Ok(Value::Int(code))
    } else {
        Ok(Value::Int(code + 0x3FFF00))
    }
}

/// `(multibyte-char-to-unibyte CHAR)` -- map multibyte/raw-byte char code to byte.
pub(crate) fn builtin_multibyte_char_to_unibyte(args: Vec<Value>) -> EvalResult {
    expect_args("multibyte-char-to-unibyte", &args, 1)?;
    let code = expect_character_code(&args[0])?;
    if code <= 0xFF {
        return Ok(Value::Int(code));
    }
    if (0x3FFF80..=0x3FFFFF).contains(&code) {
        return Ok(Value::Int(code - 0x3FFF00));
    }
    Ok(Value::Int(-1))
}

/// `(decode-char CHARSET CODE)` -- delegate to charset semantics.
pub(crate) fn builtin_decode_char(args: Vec<Value>) -> EvalResult {
    super::charset::builtin_decode_char(args)
}

/// `(encode-char CHAR CHARSET)` -- delegate to charset semantics.
pub(crate) fn builtin_encode_char(args: Vec<Value>) -> EvalResult {
    super::charset::builtin_encode_char(args)
}

/// Improved `nconc` that properly handles nil arguments and non-list tails.
/// `(nconc &rest LISTS)` -- destructively concatenate lists.
pub(crate) fn builtin_nconc_improved(args: Vec<Value>) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::Nil);
    }
    let mut all_items: Vec<Value> = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        match arg {
            Value::Nil => {}
            Value::Cons(_) => {
                let items = list_to_vec(arg).ok_or_else(|| {
                    signal(
                        "wrong-type-argument",
                        vec![Value::symbol("listp"), arg.clone()],
                    )
                })?;
                all_items.extend(items);
            }
            _ => {
                // Last argument can be a non-list (dotted tail)
                if i == args.len() - 1 {
                    if all_items.is_empty() {
                        return Ok(arg.clone());
                    }
                    // Build a dotted list: (a b c . last)
                    let tail = arg.clone();
                    return Ok(all_items
                        .into_iter()
                        .rev()
                        .fold(tail, |acc, item| Value::cons(item, acc)));
                } else {
                    return Err(signal(
                        "wrong-type-argument",
                        vec![Value::symbol("listp"), arg.clone()],
                    ));
                }
            }
        }
    }
    Ok(Value::list(all_items))
}

/// `(locale-info ITEM)` -- minimal locale info.
/// Returns "UTF-8" for symbol ITEM `codeset`; nil otherwise.
pub(crate) fn builtin_locale_info(args: Vec<Value>) -> EvalResult {
    expect_args("locale-info", &args, 1)?;
    match &args[0] {
        Value::Symbol(item) if item == "codeset" => Ok(Value::string("UTF-8")),
        _ => Ok(Value::Nil),
    }
}

// ===========================================================================
// Eval-dependent builtins
// ===========================================================================

/// `(backtrace-frame NFRAMES &optional BASE)` -- stub returning nil.
/// A real implementation would inspect the call stack.
pub(crate) fn builtin_backtrace_frame(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("backtrace-frame", &args, 1)?;
    Ok(Value::Nil)
}

/// `(recursion-depth)` -- return the current Lisp recursion depth.
/// Uses the dynamic binding stack depth as a proxy (the true depth counter
/// is private to the Evaluator).
pub(crate) fn builtin_recursion_depth(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("recursion-depth", &args, 0)?;
    Ok(Value::Int(eval.dynamic.len() as i64))
}

/// `(abort-recursive-edit)` -- stub: signals a quit.
pub(crate) fn builtin_abort_recursive_edit(
    _eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    Err(signal("quit", vec![]))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::string_escape;

    // ----- copy-alist -----

    #[test]
    fn copy_alist_basic() {
        let alist = Value::list(vec![
            Value::cons(Value::symbol("a"), Value::Int(1)),
            Value::cons(Value::symbol("b"), Value::Int(2)),
        ]);
        let result = builtin_copy_alist(vec![alist.clone()]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
        // Original and copy should have equal structure
        assert!(equal_value(&alist, &result, 0));
        // But the cons cells should not be eq (different Arc pointers)
        if let (Value::Cons(a), Value::Cons(b)) = (&items[0], &list_to_vec(&alist).unwrap()[0]) {
            assert!(!std::sync::Arc::ptr_eq(a, b));
        }
    }

    #[test]
    fn copy_alist_empty() {
        let result = builtin_copy_alist(vec![Value::Nil]).unwrap();
        assert!(result.is_nil());
    }

    // ----- rassoc / rassq -----

    #[test]
    fn rassoc_found() {
        let alist = Value::list(vec![
            Value::cons(Value::symbol("a"), Value::Int(1)),
            Value::cons(Value::symbol("b"), Value::Int(2)),
            Value::cons(Value::symbol("c"), Value::Int(3)),
        ]);
        let result = builtin_rassoc(vec![Value::Int(2), alist]).unwrap();
        // Should return (b . 2)
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert!(eq_value(&pair.car, &Value::symbol("b")));
        } else {
            panic!("expected cons");
        }
    }

    #[test]
    fn rassoc_not_found() {
        let alist = Value::list(vec![Value::cons(Value::symbol("a"), Value::Int(1))]);
        let result = builtin_rassoc(vec![Value::Int(99), alist]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn rassq_found() {
        let alist = Value::list(vec![
            Value::cons(Value::symbol("x"), Value::symbol("yes")),
            Value::cons(Value::symbol("y"), Value::symbol("no")),
        ]);
        let result = builtin_rassq(vec![Value::symbol("yes"), alist]).unwrap();
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert!(eq_value(&pair.car, &Value::symbol("x")));
        } else {
            panic!("expected cons");
        }
    }

    #[test]
    fn rassq_not_found() {
        let alist = Value::list(vec![Value::cons(Value::symbol("a"), Value::Int(1))]);
        let result = builtin_rassq(vec![Value::Int(99), alist]).unwrap();
        assert!(result.is_nil());
    }

    // ----- assoc-default -----

    #[test]
    fn assoc_default_found() {
        let alist = Value::list(vec![Value::cons(Value::string("key"), Value::Int(42))]);
        let result = builtin_assoc_default(vec![Value::string("key"), alist]).unwrap();
        assert!(eq_value(&result, &Value::Int(42)));
    }

    #[test]
    fn assoc_default_not_found_uses_default() {
        let alist = Value::list(vec![Value::cons(Value::string("key"), Value::Int(42))]);
        let result = builtin_assoc_default(vec![
            Value::string("missing"),
            alist,
            Value::Nil,
            Value::Int(-1),
        ])
        .unwrap();
        assert!(eq_value(&result, &Value::Int(-1)));
    }

    #[test]
    fn assoc_default_eq_test() {
        let alist = Value::list(vec![Value::cons(Value::symbol("foo"), Value::Int(10))]);
        let result =
            builtin_assoc_default(vec![Value::symbol("foo"), alist, Value::symbol("eq")]).unwrap();
        assert!(eq_value(&result, &Value::Int(10)));
    }

    // ----- make-list -----

    #[test]
    fn make_list_basic() {
        let result = builtin_make_list(vec![Value::Int(3), Value::symbol("x")]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 3);
        for item in &items {
            assert!(eq_value(item, &Value::symbol("x")));
        }
    }

    #[test]
    fn make_list_zero() {
        let result = builtin_make_list(vec![Value::Int(0), Value::Int(1)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn make_list_negative_errors() {
        let result = builtin_make_list(vec![Value::Int(-1), Value::Int(1)]);
        assert!(result.is_err());
    }

    // ----- safe-length -----

    #[test]
    fn safe_length_proper_list() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_safe_length(vec![list]).unwrap();
        assert!(eq_value(&result, &Value::Int(3)));
    }

    #[test]
    fn safe_length_nil() {
        let result = builtin_safe_length(vec![Value::Nil]).unwrap();
        assert!(eq_value(&result, &Value::Int(0)));
    }

    #[test]
    fn safe_length_non_list() {
        let result = builtin_safe_length(vec![Value::Int(42)]).unwrap();
        assert!(eq_value(&result, &Value::Int(0)));
    }

    // ----- subst-char-in-string -----

    #[test]
    fn subst_char_basic() {
        let result = builtin_subst_char_in_string(vec![
            Value::Char('.'),
            Value::Char('/'),
            Value::string("a.b.c"),
        ])
        .unwrap();
        assert_eq!(result.as_str().unwrap(), "a/b/c");
    }

    #[test]
    fn subst_char_no_match() {
        let result = builtin_subst_char_in_string(vec![
            Value::Char('z'),
            Value::Char('!'),
            Value::string("hello"),
        ])
        .unwrap();
        assert_eq!(result.as_str().unwrap(), "hello");
    }

    // ----- replace-regexp-in-string -----

    #[test]
    fn replace_regexp_basic() {
        let result = builtin_replace_regexp_in_string(vec![
            Value::string("o"),
            Value::string("0"),
            Value::string("foo bar boo"),
        ])
        .unwrap();
        assert_eq!(result.as_str().unwrap(), "f00 bar b00");
    }

    #[test]
    fn replace_regexp_literal() {
        // With literal flag set, REP is not treated as a replacement pattern
        let result = builtin_replace_regexp_in_string(vec![
            Value::string("o"),
            Value::string("$0"),
            Value::string("foo"),
            Value::Nil,  // fixedcase
            Value::True, // literal
        ])
        .unwrap();
        assert_eq!(result.as_str().unwrap(), "f$0$0");
    }

    // ----- string-match-p -----

    #[test]
    fn string_match_p_found() {
        let result =
            builtin_string_match_p(vec![Value::string("ba."), Value::string("foobar")]).unwrap();
        // Should find "bar" starting at position 3
        assert!(eq_value(&result, &Value::Int(3)));
    }

    #[test]
    fn string_match_p_not_found() {
        let result =
            builtin_string_match_p(vec![Value::string("xyz"), Value::string("foobar")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn string_match_p_with_start() {
        let result = builtin_string_match_p(vec![
            Value::string("o"),
            Value::string("foobar"),
            Value::Int(3),
        ])
        .unwrap();
        // No 'o' at or after position 3
        assert!(result.is_nil());
    }

    // ----- string encoding identity stubs -----

    #[test]
    fn string_to_multibyte_identity() {
        let s = Value::string("hello");
        let result = builtin_string_to_multibyte(vec![s.clone()]).unwrap();
        assert!(equal_value(&s, &result, 0));
    }

    #[test]
    fn string_to_multibyte_converts_unibyte_high_bytes_to_raw_byte_chars() {
        let mut s = String::new();
        s.push(char::from_u32(0xE3FF).expect("valid unibyte sentinel"));
        let result = builtin_string_to_multibyte(vec![Value::string(s)]).unwrap();
        let out = result.as_str().unwrap();
        assert_eq!(string_escape::storage_byte_len(out), 2);
        assert_eq!(string_escape::decode_storage_char_codes(out), vec![0x3FFFFF]);
    }

    #[test]
    fn string_to_unibyte_ascii_storage() {
        let result = builtin_string_to_unibyte(vec![Value::string("world")]).unwrap();
        let s = result.as_str().unwrap();
        assert_eq!(string_escape::storage_byte_len(s), 5);
        assert_eq!(string_escape::decode_storage_char_codes(s), vec![119, 111, 114, 108, 100]);
    }

    #[test]
    fn string_to_unibyte_rejects_unicode_scalar() {
        let result = builtin_string_to_unibyte(vec![Value::string("é")]);
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Cannot convert character at index 0 to unibyte")]
                );
            }
            other => panic!("expected conversion error, got {other:?}"),
        }
    }

    #[test]
    fn string_to_unibyte_preserves_existing_unibyte_storage() {
        let mut s = String::new();
        s.push(char::from_u32(0xE3FF).expect("valid unibyte sentinel"));
        let result = builtin_string_to_unibyte(vec![Value::string(s)]).unwrap();
        let out = result.as_str().unwrap();
        assert_eq!(string_escape::storage_byte_len(out), 1);
        assert_eq!(string_escape::decode_storage_char_codes(out), vec![255]);
    }

    #[test]
    fn string_as_unibyte_utf8_bytes_for_unicode() {
        let result = builtin_string_as_unibyte(vec![Value::string("é")]).unwrap();
        let s = result.as_str().unwrap();
        assert_eq!(string_escape::storage_byte_len(s), 2);
        assert_eq!(string_escape::decode_storage_char_codes(s), vec![195, 169]);
    }

    #[test]
    fn string_as_unibyte_ascii_passthrough_bytes() {
        let result = builtin_string_as_unibyte(vec![Value::string("test")]).unwrap();
        let s = result.as_str().unwrap();
        assert_eq!(string_escape::storage_byte_len(s), 4);
        assert_eq!(string_escape::decode_storage_char_codes(s), vec![116, 101, 115, 116]);
    }

    #[test]
    fn string_as_unibyte_preserves_unibyte_storage_bytes() {
        let mut s = String::new();
        s.push(char::from_u32(0xE3FF).expect("valid unibyte sentinel"));
        let result = builtin_string_as_unibyte(vec![Value::string(s)]).unwrap();
        let out = result.as_str().unwrap();
        assert_eq!(string_escape::storage_byte_len(out), 1);
        assert_eq!(string_escape::decode_storage_char_codes(out), vec![255]);
    }

    #[test]
    fn string_as_multibyte_identity_for_multibyte_input() {
        let s = Value::string("test");
        let result = builtin_string_as_multibyte(vec![s.clone()]).unwrap();
        assert!(equal_value(&s, &result, 0));
    }

    #[test]
    fn string_as_multibyte_converts_unibyte_high_bytes_to_raw_byte_chars() {
        let mut s = String::new();
        s.push(char::from_u32(0xE3FF).expect("valid unibyte sentinel"));
        let result = builtin_string_as_multibyte(vec![Value::string(s)]).unwrap();
        let out = result.as_str().unwrap();
        assert_eq!(string_escape::storage_byte_len(out), 2);
        assert_eq!(string_escape::decode_storage_char_codes(out), vec![0x3FFFFF]);
    }

    // ----- char encoding conversions -----

    #[test]
    fn unibyte_char_to_multibyte_ascii_identity() {
        let result = builtin_unibyte_char_to_multibyte(vec![Value::Int(65)]).unwrap();
        assert!(eq_value(&result, &Value::Int(65)));
    }

    #[test]
    fn unibyte_char_to_multibyte_high_byte_maps_to_raw_range() {
        let result = builtin_unibyte_char_to_multibyte(vec![Value::Int(255)]).unwrap();
        assert!(eq_value(&result, &Value::Int(0x3FFFFF)));
    }

    #[test]
    fn unibyte_char_to_multibyte_rejects_non_unibyte_code() {
        let result = builtin_unibyte_char_to_multibyte(vec![Value::Int(256)]);
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Not a unibyte character: 256")]
                );
            }
            other => panic!("expected conversion error, got {other:?}"),
        }
    }

    #[test]
    fn multibyte_char_to_unibyte_ascii_passthrough() {
        let result = builtin_multibyte_char_to_unibyte(vec![Value::Int(65)]).unwrap();
        assert!(eq_value(&result, &Value::Int(65)));
    }

    #[test]
    fn multibyte_char_to_unibyte_raw_range_maps_to_byte() {
        let result = builtin_multibyte_char_to_unibyte(vec![Value::Int(0x3FFFFF)]).unwrap();
        assert!(eq_value(&result, &Value::Int(255)));
    }

    #[test]
    fn multibyte_char_to_unibyte_returns_minus_one_for_non_unibyte_unicode() {
        let result = builtin_multibyte_char_to_unibyte(vec![Value::Int(256)]).unwrap();
        assert!(eq_value(&result, &Value::Int(-1)));
    }

    // ----- decode-char / encode-char -----

    #[test]
    fn decode_char_basic() {
        let result = builtin_decode_char(vec![Value::symbol("unicode"), Value::Int(65)]).unwrap();
        assert!(eq_value(&result, &Value::Int(65)));
    }

    #[test]
    fn encode_char_basic() {
        let result = builtin_encode_char(vec![Value::Char('A'), Value::symbol("unicode")]).unwrap();
        assert!(eq_value(&result, &Value::Int(65)));
    }

    // ----- nconc_improved -----

    #[test]
    fn nconc_improved_basic() {
        let a = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let b = Value::list(vec![Value::Int(3), Value::Int(4)]);
        let result = builtin_nconc_improved(vec![a, b]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn nconc_improved_with_nils() {
        let a = Value::Nil;
        let b = Value::list(vec![Value::Int(1)]);
        let c = Value::Nil;
        let result = builtin_nconc_improved(vec![a, b, c]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn nconc_improved_empty() {
        let result = builtin_nconc_improved(vec![]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn nconc_improved_dotted_tail() {
        let a = Value::list(vec![Value::Int(1)]);
        let b = Value::Int(99);
        let result = builtin_nconc_improved(vec![a, b]).unwrap();
        // Should be (1 . 99) - a dotted pair
        if let Value::Cons(cell) = &result {
            let pair = cell.lock().unwrap();
            assert!(eq_value(&pair.car, &Value::Int(1)));
            assert!(eq_value(&pair.cdr, &Value::Int(99)));
        } else {
            panic!("expected cons");
        }
    }

    // ----- locale-info -----

    #[test]
    fn locale_info_codeset_returns_utf8() {
        let result = builtin_locale_info(vec![Value::symbol("codeset")]).unwrap();
        assert_eq!(result.as_str(), Some("UTF-8"));
    }

    #[test]
    fn locale_info_other_items_return_nil() {
        let result = builtin_locale_info(vec![Value::symbol("codeset")]).unwrap();
        assert!(result.is_truthy());
        let result = builtin_locale_info(vec![Value::symbol("time")]).unwrap();
        assert!(result.is_nil());
        let result = builtin_locale_info(vec![Value::string("codeset")]).unwrap();
        assert!(result.is_nil());
        let result = builtin_locale_info(vec![Value::Int(1)]).unwrap();
        assert!(result.is_nil());
    }

    // ----- eval-dependent builtins (need Evaluator) -----

    #[test]
    fn recursion_depth_zero() {
        let mut eval = super::super::eval::Evaluator::new();
        let result = builtin_recursion_depth(&mut eval, vec![]).unwrap();
        // At top level, depth is 0
        assert!(eq_value(&result, &Value::Int(0)));
    }

    #[test]
    fn backtrace_frame_stub() {
        let mut eval = super::super::eval::Evaluator::new();
        let result = builtin_backtrace_frame(&mut eval, vec![Value::Int(0)]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn abort_recursive_edit_signals_quit() {
        let mut eval = super::super::eval::Evaluator::new();
        let result = builtin_abort_recursive_edit(&mut eval, vec![]);
        assert!(result.is_err());
    }

    // ----- special form: prog2 -----

    #[test]
    fn sf_prog2_returns_second() {
        use super::super::expr::Expr;
        let mut ev = super::super::eval::Evaluator::new();
        let tail = [Expr::Int(1), Expr::Int(2), Expr::Int(3)];
        let result = sf_prog2(&mut ev, &tail).unwrap();
        assert!(eq_value(&result, &Value::Int(2)));
    }

    #[test]
    fn sf_prog2_minimum_args() {
        use super::super::expr::Expr;
        let mut ev = super::super::eval::Evaluator::new();
        let tail = [Expr::Int(10), Expr::Int(20)];
        let result = sf_prog2(&mut ev, &tail).unwrap();
        assert!(eq_value(&result, &Value::Int(20)));
    }

    #[test]
    fn sf_prog2_too_few_args() {
        use super::super::expr::Expr;
        let mut ev = super::super::eval::Evaluator::new();
        let tail = [Expr::Int(1)];
        let result = sf_prog2(&mut ev, &tail);
        assert!(result.is_err());
    }

    // ----- special form: save-current-buffer -----

    #[test]
    fn sf_save_current_buffer_restores() {
        use super::super::expr::Expr;
        let mut ev = super::super::eval::Evaluator::new();
        // Create a buffer and make it current
        let buf_id = ev.buffers.create_buffer("*test*");
        ev.buffers.set_current(buf_id);

        // save-current-buffer with body that just returns 42
        let tail = [Expr::Int(42)];
        let result = sf_save_current_buffer(&mut ev, &tail).unwrap();
        assert!(eq_value(&result, &Value::Int(42)));
        // Current buffer should still be *test*
        assert_eq!(ev.buffers.current_buffer().unwrap().id, buf_id);
    }

    // ----- special form: track-mouse (stub) -----

    #[test]
    fn sf_track_mouse_evaluates_body() {
        use super::super::expr::Expr;
        let mut ev = super::super::eval::Evaluator::new();
        let tail = [Expr::Int(99)];
        let result = sf_track_mouse(&mut ev, &tail).unwrap();
        assert!(eq_value(&result, &Value::Int(99)));
    }

    // ----- special form: with-syntax-table (stub) -----

    #[test]
    fn sf_with_syntax_table_evaluates_body() {
        use super::super::expr::Expr;
        let mut ev = super::super::eval::Evaluator::new();
        // TABLE=nil, BODY=(+ 10 20)  -- but since we're calling sf directly,
        // BODY must be Expr nodes. We use a literal for simplicity.
        let tail = [Expr::Symbol("nil".to_string()), Expr::Int(30)];
        let result = sf_with_syntax_table(&mut ev, &tail).unwrap();
        assert!(eq_value(&result, &Value::Int(30)));
    }

    #[test]
    fn sf_with_syntax_table_needs_args() {
        use super::super::expr::Expr;
        let mut ev = super::super::eval::Evaluator::new();
        let tail: [Expr; 0] = [];
        let result = sf_with_syntax_table(&mut ev, &tail);
        assert!(result.is_err());
    }

    // ----- special form: with-temp-buffer -----

    #[test]
    fn sf_with_temp_buffer_returns_body_result() {
        use super::super::expr::Expr;
        let mut ev = super::super::eval::Evaluator::new();
        let tail = [Expr::Int(77)];
        let result = sf_with_temp_buffer(&mut ev, &tail).unwrap();
        assert!(eq_value(&result, &Value::Int(77)));
    }
}
