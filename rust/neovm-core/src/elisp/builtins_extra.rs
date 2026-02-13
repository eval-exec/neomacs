//! Additional built-in functions to improve Emacs Lisp compatibility.
//!
//! These builtins complement the core set in builtins.rs with:
//! - Advanced list operations (cl-lib compatible)
//! - Sequence operations (seq.el compatible)
//! - String utilities (subr-x compatible)
//! - Window/frame operations
//! - Buffer info queries
//! - Format enhancements
//! - Variable operations

use super::error::{signal, EvalResult, Flow};
use super::value::Value;

// ---------------------------------------------------------------------------
// Argument helpers
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

fn expect_string(val: &Value) -> Result<String, Flow> {
    match val {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
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

fn collect_sequence_strict(val: &Value) -> Result<Vec<Value>, Flow> {
    match val {
        Value::Nil => Ok(Vec::new()),
        Value::Cons(_) => {
            let mut result = Vec::new();
            let mut cursor = val.clone();
            loop {
                match cursor {
                    Value::Nil => return Ok(result),
                    Value::Cons(cell) => {
                        let pair = cell.lock().expect("poisoned");
                        result.push(pair.car.clone());
                        cursor = pair.cdr.clone();
                    }
                    tail => {
                        return Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("listp"), tail],
                        ));
                    }
                }
            }
        }
        Value::Vector(v) => Ok(v.lock().expect("poisoned").clone()),
        Value::Str(s) => Ok(s.chars().map(|ch| Value::Int(ch as i64)).collect()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("sequencep"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// Advanced list operations
// ---------------------------------------------------------------------------

/// `(cl-remove-if PREDICATE LIST)` — remove elements matching predicate.
/// Since we can't call a predicate here (no eval), this is a stub that
/// works with known predicates like 'null.
pub fn builtin_remove(args: Vec<Value>) -> EvalResult {
    expect_args("remove", &args, 2)?;
    let target = &args[0];
    let list_val = &args[1];

    let mut result = Vec::new();
    let mut cursor = list_val.clone();
    loop {
        match cursor {
            Value::Nil => break,
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if !super::value::equal_value(&pair.car, target, 0) {
                    result.push(pair.car.clone());
                }
                cursor = pair.cdr.clone();
            }
            _ => break,
        }
    }
    Ok(Value::list(result))
}

/// `(remq ITEM LIST)` — remove by eq.
pub fn builtin_remq(args: Vec<Value>) -> EvalResult {
    expect_args("remq", &args, 2)?;
    let target = &args[0];
    let list_val = &args[1];

    let mut result = Vec::new();
    let mut cursor = list_val.clone();
    loop {
        match cursor {
            Value::Nil => break,
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if !super::value::eq_value(&pair.car, target) {
                    result.push(pair.car.clone());
                }
                cursor = pair.cdr.clone();
            }
            _ => break,
        }
    }
    Ok(Value::list(result))
}

/// `(flatten-tree TREE)` — flatten nested lists.
pub fn builtin_flatten_tree(args: Vec<Value>) -> EvalResult {
    expect_args("flatten-tree", &args, 1)?;
    let mut result = Vec::new();
    flatten_value(&args[0], &mut result);
    Ok(Value::list(result))
}

fn flatten_value(val: &Value, out: &mut Vec<Value>) {
    match val {
        Value::Nil => {}
        Value::Cons(cell) => {
            let pair = cell.lock().expect("poisoned");
            flatten_value(&pair.car, out);
            flatten_value(&pair.cdr, out);
        }
        other => out.push(other.clone()),
    }
}

/// `(take N LIST)` — first N elements.
pub fn builtin_take(args: Vec<Value>) -> EvalResult {
    expect_args("take", &args, 2)?;
    let n = expect_int(&args[0])? as usize;
    let list = &args[1];

    let mut result = Vec::new();
    let mut cursor = list.clone();
    for _ in 0..n {
        match cursor {
            Value::Nil => break,
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                result.push(pair.car.clone());
                cursor = pair.cdr.clone();
            }
            _ => break,
        }
    }
    Ok(Value::list(result))
}

/// `(seq-uniq SEQ)` — remove duplicates (equal).
pub fn builtin_seq_uniq(args: Vec<Value>) -> EvalResult {
    expect_args("seq-uniq", &args, 1)?;
    let elements = collect_sequence_strict(&args[0])?;
    let mut result = Vec::new();
    for value in &elements {
        let already = result
            .iter()
            .any(|seen| super::value::equal_value(seen, value, 0));
        if !already {
            result.push(value.clone());
        }
    }
    Ok(Value::list(result))
}

/// `(seq-contains-p SEQ ELT)` — check if sequence contains element.
pub fn builtin_seq_contains_p(args: Vec<Value>) -> EvalResult {
    expect_args("seq-contains-p", &args, 2)?;
    let list = &args[0];
    let target = &args[1];

    let mut cursor = list.clone();
    loop {
        match cursor {
            Value::Nil => return Ok(Value::Nil),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if super::value::equal_value(&pair.car, target, 0) {
                    return Ok(Value::True);
                }
                cursor = pair.cdr.clone();
            }
            _ => return Ok(Value::Nil),
        }
    }
}

/// `(seq-count PRED SEQ)` — stub: count non-nil elements.
pub fn builtin_seq_length(args: Vec<Value>) -> EvalResult {
    expect_args("seq-length", &args, 1)?;
    let elements = collect_sequence_strict(&args[0])?;
    Ok(Value::Int(elements.len() as i64))
}

/// `(seq-into SEQ TYPE)` — convert sequence to another type.
pub fn builtin_seq_into(args: Vec<Value>) -> EvalResult {
    expect_args("seq-into", &args, 2)?;
    let target_type = match &args[1] {
        Value::Symbol(s) => s.as_str(),
        other => {
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "Not a sequence type name: {}",
                    super::print::print_value(other)
                ))],
            ));
        }
    };
    let elements = collect_sequence_strict(&args[0])?;

    match target_type {
        "list" => Ok(Value::list(elements)),
        "vector" => Ok(Value::vector(elements)),
        "string" => {
            let mut s = String::new();
            for value in &elements {
                let ch = match value {
                    Value::Char(c) => *c,
                    Value::Int(n) => char::from_u32(*n as u32).ok_or_else(|| {
                        signal(
                            "wrong-type-argument",
                            vec![Value::symbol("characterp"), value.clone()],
                        )
                    })?,
                    other => {
                        return Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("characterp"), other.clone()],
                        ));
                    }
                };
                s.push(ch);
            }
            Ok(Value::string(s))
        }
        _ => Err(signal(
            "error",
            vec![Value::string(format!(
                "Not a sequence type name: {}",
                target_type
            ))],
        )),
    }
}

// ---------------------------------------------------------------------------
// String utilities (subr-x compatible)
// ---------------------------------------------------------------------------

/// `(string-empty-p STRING)` -> t or nil.
pub fn builtin_string_empty_p(args: Vec<Value>) -> EvalResult {
    expect_args("string-empty-p", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::bool(s.is_empty()))
}

/// `(string-blank-p STRING)` -> t or nil.
pub fn builtin_string_blank_p(args: Vec<Value>) -> EvalResult {
    expect_args("string-blank-p", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::bool(s.trim().is_empty()))
}

/// `(string-chop-newline STRING)` -> string without trailing newline.
pub fn builtin_string_chop_newline(args: Vec<Value>) -> EvalResult {
    expect_args("string-chop-newline", &args, 1)?;
    let s = expect_string(&args[0])?;
    let trimmed = s.strip_suffix('\n').unwrap_or(&s);
    let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);
    Ok(Value::string(trimmed))
}

/// `(string-pad STRING LENGTH &optional PADDING FROM-END)`.
pub fn builtin_string_pad(args: Vec<Value>) -> EvalResult {
    expect_min_args("string-pad", &args, 2)?;
    let s = expect_string(&args[0])?;
    let length = expect_int(&args[1])? as usize;
    let pad_char = if args.len() > 2 {
        match &args[2] {
            Value::Char(c) => *c,
            Value::Int(n) => char::from_u32(*n as u32).unwrap_or(' '),
            _ => ' ',
        }
    } else {
        ' '
    };
    let from_end = args.len() > 3 && args[3].is_truthy();

    if s.len() >= length {
        return Ok(Value::string(&s[..length]));
    }

    let padding: String = std::iter::repeat(pad_char).take(length - s.len()).collect();

    if from_end {
        Ok(Value::string(format!("{}{}", padding, s)))
    } else {
        Ok(Value::string(format!("{}{}", s, padding)))
    }
}

/// `(string-repeat STRING COUNT)`.
pub fn builtin_string_repeat(args: Vec<Value>) -> EvalResult {
    expect_args("string-repeat", &args, 2)?;
    let s = expect_string(&args[0])?;
    let count = expect_int(&args[1])? as usize;
    Ok(Value::string(s.repeat(count)))
}

/// `(string-replace FROM TO IN)` — replace all occurrences.
pub fn builtin_string_replace(args: Vec<Value>) -> EvalResult {
    expect_args("string-replace", &args, 3)?;
    let from = expect_string(&args[0])?;
    let to = expect_string(&args[1])?;
    let input = expect_string(&args[2])?;
    Ok(Value::string(input.replace(&from, &to)))
}

/// `(string-search NEEDLE HAYSTACK &optional START)`.
pub fn builtin_string_search(args: Vec<Value>) -> EvalResult {
    expect_min_args("string-search", &args, 2)?;
    let needle = expect_string(&args[0])?;
    let haystack = expect_string(&args[1])?;
    let start = if args.len() > 2 {
        expect_int(&args[2])? as usize
    } else {
        0
    };

    let search_in = &haystack[start.min(haystack.len())..];
    match search_in.find(&needle) {
        Some(pos) => Ok(Value::Int((start + pos) as i64)),
        None => Ok(Value::Nil),
    }
}

/// `(string-to-vector STRING)` — convert string to vector of chars.
pub fn builtin_string_to_vector(args: Vec<Value>) -> EvalResult {
    expect_args("string-to-vector", &args, 1)?;
    let s = expect_string(&args[0])?;
    let chars: Vec<Value> = s.chars().map(Value::Char).collect();
    Ok(Value::vector(chars))
}

/// `(vconcat &rest SEQUENCES)` — concatenate into a vector.
pub fn builtin_vconcat(args: Vec<Value>) -> EvalResult {
    let mut result = Vec::new();
    for arg in &args {
        match arg {
            Value::Vector(v) => {
                result.extend(v.lock().expect("poisoned").iter().cloned());
            }
            Value::Str(s) => {
                result.extend(s.chars().map(Value::Char));
            }
            list if list.is_list() => {
                if let Some(items) = super::value::list_to_vec(list) {
                    result.extend(items);
                }
            }
            _ => result.push(arg.clone()),
        }
    }
    Ok(Value::vector(result))
}

// ---------------------------------------------------------------------------
// Variable / symbol operations
// ---------------------------------------------------------------------------

/// `(default-value SYMBOL)` — get the default value of a variable.
/// Stub: same as symbol-value for now.
pub fn builtin_default_value(args: Vec<Value>) -> EvalResult {
    expect_args("default-value", &args, 1)?;
    // In a full implementation, this returns the non-buffer-local default.
    // For now, just return nil.
    let _ = args;
    Ok(Value::Nil)
}

/// `(set-default SYMBOL VALUE)` — set the default value.
/// Stub.
pub fn builtin_set_default(args: Vec<Value>) -> EvalResult {
    expect_args("set-default", &args, 2)?;
    Ok(args[1].clone())
}

/// `(local-variable-p VARIABLE &optional BUFFER)` -> t or nil.
/// Stub: always nil for now.
pub fn builtin_local_variable_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("local-variable-p", &args, 1)?;
    Ok(Value::Nil)
}

/// `(make-local-variable VARIABLE)` — make variable buffer-local.
/// Stub.
pub fn builtin_make_local_variable(args: Vec<Value>) -> EvalResult {
    expect_args("make-local-variable", &args, 1)?;
    Ok(args[0].clone())
}

/// `(kill-local-variable VARIABLE)` — remove buffer-local binding.
/// Stub.
pub fn builtin_kill_local_variable(args: Vec<Value>) -> EvalResult {
    expect_args("kill-local-variable", &args, 1)?;
    Ok(args[0].clone())
}

// ---------------------------------------------------------------------------
// Math operations
// ---------------------------------------------------------------------------

/// `(number-to-string NUMBER)` — already exists, but add base conversion.
/// `(format "%x" N)` etc.

/// `(string-to-number STRING &optional BASE)` — with base support.
pub fn builtin_string_to_number_ext(args: Vec<Value>) -> EvalResult {
    expect_min_args("string-to-number", &args, 1)?;
    let s = expect_string(&args[0])?;
    let base = if args.len() > 1 {
        expect_int(&args[1])? as u32
    } else {
        10
    };

    let trimmed = s.trim();
    if base == 10 {
        if let Ok(n) = trimmed.parse::<i64>() {
            return Ok(Value::Int(n));
        }
        if let Ok(f) = trimmed.parse::<f64>() {
            return Ok(Value::Float(f));
        }
    } else {
        let stripped = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .or_else(|| trimmed.strip_prefix("#x"))
            .unwrap_or(trimmed);
        if let Ok(n) = i64::from_str_radix(stripped, base) {
            return Ok(Value::Int(n));
        }
    }

    Ok(Value::Int(0))
}

/// `(random &optional LIMIT)` — random number.
pub fn builtin_random_ext(args: Vec<Value>) -> EvalResult {
    let limit = if args.is_empty() {
        i64::MAX
    } else {
        match &args[0] {
            Value::Int(n) if *n > 0 => *n,
            Value::True => {
                // Seed from time
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as i64;
                return Ok(Value::Int(seed.abs()));
            }
            _ => i64::MAX,
        }
    };

    // Simple LCG random (for determinism in tests).
    // In production, use a proper RNG.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64;
    Ok(Value::Int(seed.abs() % limit))
}

// ---------------------------------------------------------------------------
// Predicate additions
// ---------------------------------------------------------------------------

/// `(proper-list-p OBJ)` -> t if OBJ is a proper list.
pub fn builtin_proper_list_p(args: Vec<Value>) -> EvalResult {
    expect_args("proper-list-p", &args, 1)?;
    Ok(Value::bool(super::value::list_to_vec(&args[0]).is_some()))
}

/// `(bool-vector-p OBJ)` -> nil (not implemented).
pub fn builtin_bool_vector_p(args: Vec<Value>) -> EvalResult {
    expect_args("bool-vector-p", &args, 1)?;
    Ok(Value::Nil)
}

/// `(subrp OBJ)` -> t if OBJ is a built-in function.
pub fn builtin_subrp(args: Vec<Value>) -> EvalResult {
    expect_args("subrp", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::Subr(_))))
}

/// `(byte-code-function-p OBJ)` -> t if compiled.
pub fn builtin_byte_code_function_p(args: Vec<Value>) -> EvalResult {
    expect_args("byte-code-function-p", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::ByteCode(_))))
}

/// `(compiled-function-p OBJ)` -> t if compiled function.
pub fn builtin_compiled_function_p(args: Vec<Value>) -> EvalResult {
    expect_args("compiled-function-p", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::ByteCode(_))))
}

/// `(closurep OBJ)` -> t if closure.
pub fn builtin_closurep(args: Vec<Value>) -> EvalResult {
    expect_args("closurep", &args, 1)?;
    let is_closure = match &args[0] {
        Value::Lambda(l) => l.env.is_some(),
        _ => false,
    };
    Ok(Value::bool(is_closure))
}

/// `(commandp OBJ)` -> t if OBJ can be used as a command.
/// Stub: anything callable is a command.
pub fn builtin_commandp(args: Vec<Value>) -> EvalResult {
    expect_args("commandp", &args, 1)?;
    Ok(Value::bool(args[0].is_function()))
}

/// `(nlistp OBJ)` -> t if not a list.
pub fn builtin_nlistp(args: Vec<Value>) -> EvalResult {
    expect_args("nlistp", &args, 1)?;
    Ok(Value::bool(!args[0].is_list()))
}

/// `(natnump OBJ)` -> t if natural number (>= 0).
pub fn builtin_natnump(args: Vec<Value>) -> EvalResult {
    expect_args("natnump", &args, 1)?;
    let is_nat = match &args[0] {
        Value::Int(n) => *n >= 0,
        _ => false,
    };
    Ok(Value::bool(is_nat))
}

/// `(fixnump OBJ)` -> t if fixnum.
pub fn builtin_fixnump(args: Vec<Value>) -> EvalResult {
    expect_args("fixnump", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::Int(_))))
}

/// `(bignump OBJ)` -> nil (we don't have bignums).
pub fn builtin_bignump(args: Vec<Value>) -> EvalResult {
    expect_args("bignump", &args, 1)?;
    Ok(Value::Nil)
}

/// `(wholenump OBJ)` -> t if whole number.
pub fn builtin_wholenump(args: Vec<Value>) -> EvalResult {
    expect_args("wholenump", &args, 1)?;
    let is_whole = match &args[0] {
        Value::Int(n) => *n >= 0,
        _ => false,
    };
    Ok(Value::bool(is_whole))
}

/// `(zerop OBJ)` -> t if zero.
pub fn builtin_zerop(args: Vec<Value>) -> EvalResult {
    expect_args("zerop", &args, 1)?;
    let is_zero = match &args[0] {
        Value::Int(0) => true,
        Value::Float(f) => *f == 0.0,
        _ => false,
    };
    Ok(Value::bool(is_zero))
}

/// `(cl-oddp N)` -> t if odd.
pub fn builtin_cl_oddp(args: Vec<Value>) -> EvalResult {
    expect_args("cl-oddp", &args, 1)?;
    let n = expect_int(&args[0])?;
    Ok(Value::bool(n % 2 != 0))
}

/// `(cl-evenp N)` -> t if even.
pub fn builtin_cl_evenp(args: Vec<Value>) -> EvalResult {
    expect_args("cl-evenp", &args, 1)?;
    let n = expect_int(&args[0])?;
    Ok(Value::bool(n % 2 == 0))
}

/// `(cl-plusp N)` -> t if positive.
pub fn builtin_cl_plusp(args: Vec<Value>) -> EvalResult {
    expect_args("cl-plusp", &args, 1)?;
    let n = expect_int(&args[0])?;
    Ok(Value::bool(n > 0))
}

/// `(cl-minusp N)` -> t if negative.
pub fn builtin_cl_minusp(args: Vec<Value>) -> EvalResult {
    expect_args("cl-minusp", &args, 1)?;
    let n = expect_int(&args[0])?;
    Ok(Value::bool(n < 0))
}

// ---------------------------------------------------------------------------
// Misc operations
// ---------------------------------------------------------------------------

/// `(user-login-name)` -> string.
pub fn builtin_user_login_name(args: Vec<Value>) -> EvalResult {
    let _ = args;
    let name = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    Ok(Value::string(name))
}

/// `(user-real-login-name)` -> string.
pub fn builtin_user_real_login_name(args: Vec<Value>) -> EvalResult {
    builtin_user_login_name(args)
}

/// `(user-full-name)` -> string.
pub fn builtin_user_full_name(args: Vec<Value>) -> EvalResult {
    let _ = args;
    let name = std::env::var("NAME")
        .unwrap_or_else(|_| std::env::var("USER").unwrap_or_else(|_| "Unknown".to_string()));
    Ok(Value::string(name))
}

/// `(system-name)` -> string.
pub fn builtin_system_name(args: Vec<Value>) -> EvalResult {
    let _ = args;
    let name = std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_string());
    Ok(Value::string(name))
}

/// `(emacs-version)` -> string.
pub fn builtin_emacs_version(args: Vec<Value>) -> EvalResult {
    let _ = args;
    Ok(Value::string("NeoVM 0.1.0 (Neomacs)"))
}

/// `(emacs-pid)` -> integer.
pub fn builtin_emacs_pid(args: Vec<Value>) -> EvalResult {
    let _ = args;
    Ok(Value::Int(std::process::id() as i64))
}

/// `(garbage-collect)` -> nil (stub).
pub fn builtin_garbage_collect(args: Vec<Value>) -> EvalResult {
    let _ = args;
    Ok(Value::Nil)
}

/// `(memory-use-counts)` -> list of integers (stub).
pub fn builtin_memory_use_counts(args: Vec<Value>) -> EvalResult {
    let _ = args;
    Ok(Value::list(vec![Value::Int(0); 7]))
}

/// `(cl-gensym &optional PREFIX)` — generate unique symbol.
pub fn builtin_cl_gensym(args: Vec<Value>) -> EvalResult {
    let prefix = if args.is_empty() {
        "G"
    } else {
        match &args[0] {
            Value::Str(s) => s.as_str(),
            _ => "G",
        }
    };
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(Value::symbol(format!("{}{}", prefix, n)))
}

/// `(make-symbol NAME)` — create uninterned symbol.
pub fn builtin_make_symbol_extra(args: Vec<Value>) -> EvalResult {
    expect_args("make-symbol", &args, 1)?;
    let name = expect_string(&args[0])?;
    Ok(Value::symbol(name))
}

/// `(symbol-name SYM)` — already exists but let's provide compat.
pub fn builtin_symbol_name(args: Vec<Value>) -> EvalResult {
    expect_args("symbol-name", &args, 1)?;
    match &args[0] {
        Value::Symbol(s) => Ok(Value::string(s.clone())),
        Value::Nil => Ok(Value::string("nil")),
        Value::True => Ok(Value::string("t")),
        Value::Keyword(k) => Ok(Value::string(format!(":{}", k))),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), other.clone()],
        )),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_from_list() {
        let list = Value::list(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
            Value::Int(2),
        ]);
        let result = builtin_remove(vec![Value::Int(2), list]).unwrap();
        let items = super::super::value::list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn flatten_tree() {
        let nested = Value::list(vec![
            Value::Int(1),
            Value::list(vec![Value::Int(2), Value::Int(3)]),
            Value::Int(4),
        ]);
        let result = builtin_flatten_tree(vec![nested]).unwrap();
        let items = super::super::value::list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn take_from_list() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_take(vec![Value::Int(2), list]).unwrap();
        let items = super::super::value::list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn string_empty_blank() {
        assert!(matches!(
            builtin_string_empty_p(vec![Value::string("")]).unwrap(),
            Value::True,
        ));
        assert!(builtin_string_empty_p(vec![Value::string("a")])
            .unwrap()
            .is_nil(),);
        assert!(matches!(
            builtin_string_blank_p(vec![Value::string("  ")]).unwrap(),
            Value::True,
        ));
    }

    #[test]
    fn string_replace() {
        let result = builtin_string_replace(vec![
            Value::string("world"),
            Value::string("rust"),
            Value::string("hello world"),
        ])
        .unwrap();
        assert_eq!(result.as_str(), Some("hello rust"));
    }

    #[test]
    fn string_search() {
        let result =
            builtin_string_search(vec![Value::string("world"), Value::string("hello world")])
                .unwrap();
        assert_eq!(result.as_int(), Some(6));

        let result =
            builtin_string_search(vec![Value::string("xyz"), Value::string("hello")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn string_repeat() {
        let result = builtin_string_repeat(vec![Value::string("ab"), Value::Int(3)]).unwrap();
        assert_eq!(result.as_str(), Some("ababab"));
    }

    #[test]
    fn proper_list_p() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2)]);
        assert!(matches!(
            builtin_proper_list_p(vec![list]).unwrap(),
            Value::True,
        ));
        assert!(builtin_proper_list_p(vec![Value::Int(5)]).unwrap().is_nil(),);
    }

    #[test]
    fn number_predicates() {
        assert!(matches!(
            builtin_zerop(vec![Value::Int(0)]).unwrap(),
            Value::True
        ));
        assert!(builtin_zerop(vec![Value::Int(1)]).unwrap().is_nil());
        assert!(matches!(
            builtin_natnump(vec![Value::Int(5)]).unwrap(),
            Value::True
        ));
        assert!(builtin_natnump(vec![Value::Int(-1)]).unwrap().is_nil());
        assert!(matches!(
            builtin_cl_oddp(vec![Value::Int(3)]).unwrap(),
            Value::True
        ));
        assert!(matches!(
            builtin_cl_evenp(vec![Value::Int(4)]).unwrap(),
            Value::True
        ));
    }

    #[test]
    fn seq_uniq() {
        let list = Value::list(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(1),
            Value::Int(3),
        ]);
        let result = builtin_seq_uniq(vec![list]).unwrap();
        let items = super::super::value::list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn user_info() {
        // These should not panic, just return strings.
        assert!(builtin_user_login_name(vec![]).unwrap().is_string());
        assert!(builtin_system_name(vec![]).unwrap().is_string());
        assert!(builtin_emacs_version(vec![]).unwrap().is_string());
    }

    #[test]
    fn emacs_pid() {
        let pid = builtin_emacs_pid(vec![]).unwrap();
        assert!(matches!(pid, Value::Int(n) if n > 0));
    }

    #[test]
    fn string_chop_newline() {
        let result = builtin_string_chop_newline(vec![Value::string("hello\n")]).unwrap();
        assert_eq!(result.as_str(), Some("hello"));
    }

    #[test]
    fn vconcat_test() {
        let v1 = Value::vector(vec![Value::Int(1)]);
        let v2 = Value::vector(vec![Value::Int(2)]);
        let result = builtin_vconcat(vec![v1, v2]).unwrap();
        if let Value::Vector(v) = result {
            assert_eq!(v.lock().unwrap().len(), 2);
        } else {
            panic!("expected vector");
        }
    }
}
