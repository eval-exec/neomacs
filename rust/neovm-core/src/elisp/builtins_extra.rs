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
pub(crate) fn builtin_remove(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_remq(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_flatten_tree(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_take(args: Vec<Value>) -> EvalResult {
    expect_args("take", &args, 2)?;
    let n = expect_int(&args[0])?;
    if n <= 0 {
        return Ok(Value::Nil);
    }
    let list = &args[1];
    if !matches!(list, Value::Nil | Value::Cons(_)) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("listp"), list.clone()],
        ));
    }

    let mut result = Vec::new();
    let mut cursor = list.clone();
    for _ in 0..(n as usize) {
        match cursor {
            Value::Nil => break,
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
    Ok(Value::list(result))
}

/// `(seq-uniq SEQ)` — remove duplicates (equal).
pub(crate) fn builtin_seq_uniq(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_seq_contains_p(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_seq_length(args: Vec<Value>) -> EvalResult {
    expect_args("seq-length", &args, 1)?;
    let elements = collect_sequence_strict(&args[0])?;
    Ok(Value::Int(elements.len() as i64))
}

/// `(seq-into SEQ TYPE)` — convert sequence to another type.
pub(crate) fn builtin_seq_into(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_string_empty_p(args: Vec<Value>) -> EvalResult {
    expect_args("string-empty-p", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::bool(s.is_empty()))
}

/// `(string-blank-p STRING)` -> t or nil.
pub(crate) fn builtin_string_blank_p(args: Vec<Value>) -> EvalResult {
    expect_args("string-blank-p", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::bool(s.trim().is_empty()))
}

/// `(string-chop-newline STRING)` -> string without trailing newline.
pub(crate) fn builtin_string_chop_newline(args: Vec<Value>) -> EvalResult {
    expect_args("string-chop-newline", &args, 1)?;
    let s = expect_string(&args[0])?;
    let trimmed = s.strip_suffix('\n').unwrap_or(&s);
    let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);
    Ok(Value::string(trimmed))
}

/// `(string-pad STRING LENGTH &optional PADDING FROM-END)`.
pub(crate) fn builtin_string_pad(args: Vec<Value>) -> EvalResult {
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

/// `(string-replace FROM TO IN)` — replace all occurrences.
pub(crate) fn builtin_string_replace(args: Vec<Value>) -> EvalResult {
    expect_args("string-replace", &args, 3)?;
    let from = expect_string(&args[0])?;
    let to = expect_string(&args[1])?;
    let input = expect_string(&args[2])?;
    Ok(Value::string(input.replace(&from, &to)))
}

/// `(string-search NEEDLE HAYSTACK &optional START)`.
pub(crate) fn builtin_string_search(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_string_to_vector(args: Vec<Value>) -> EvalResult {
    expect_args("string-to-vector", &args, 1)?;
    let s = expect_string(&args[0])?;
    let chars: Vec<Value> = s.chars().map(Value::Char).collect();
    Ok(Value::vector(chars))
}

/// `(vconcat &rest SEQUENCES)` — concatenate into a vector.
pub(crate) fn builtin_vconcat(args: Vec<Value>) -> EvalResult {
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
// Predicate additions
// ---------------------------------------------------------------------------

/// `(proper-list-p OBJ)` -> t if OBJ is a proper list.
pub(crate) fn builtin_proper_list_p(args: Vec<Value>) -> EvalResult {
    expect_args("proper-list-p", &args, 1)?;
    Ok(Value::bool(super::value::list_to_vec(&args[0]).is_some()))
}

/// `(subrp OBJ)` -> t if OBJ is a built-in function.
pub(crate) fn builtin_subrp(args: Vec<Value>) -> EvalResult {
    expect_args("subrp", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::Subr(_))))
}

/// `(byte-code-function-p OBJ)` -> t if compiled.
pub(crate) fn builtin_byte_code_function_p(args: Vec<Value>) -> EvalResult {
    expect_args("byte-code-function-p", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::ByteCode(_))))
}

/// `(compiled-function-p OBJ)` -> t if compiled function.
pub(crate) fn builtin_compiled_function_p(args: Vec<Value>) -> EvalResult {
    expect_args("compiled-function-p", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::ByteCode(_))))
}

/// `(closurep OBJ)` -> t if closure.
pub(crate) fn builtin_closurep(args: Vec<Value>) -> EvalResult {
    expect_args("closurep", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::Lambda(_))))
}

/// `(natnump OBJ)` -> t if natural number (>= 0).
pub(crate) fn builtin_natnump(args: Vec<Value>) -> EvalResult {
    expect_args("natnump", &args, 1)?;
    let is_nat = match &args[0] {
        Value::Int(n) => *n >= 0,
        _ => false,
    };
    Ok(Value::bool(is_nat))
}

/// `(fixnump OBJ)` -> t if fixnum.
pub(crate) fn builtin_fixnump(args: Vec<Value>) -> EvalResult {
    expect_args("fixnump", &args, 1)?;
    Ok(Value::bool(matches!(&args[0], Value::Int(_))))
}

/// `(bignump OBJ)` -> nil (we don't have bignums).
pub(crate) fn builtin_bignump(args: Vec<Value>) -> EvalResult {
    expect_args("bignump", &args, 1)?;
    Ok(Value::Nil)
}

/// `(wholenump OBJ)` -> t if whole number.
pub(crate) fn builtin_wholenump(args: Vec<Value>) -> EvalResult {
    expect_args("wholenump", &args, 1)?;
    let is_whole = match &args[0] {
        Value::Int(n) => *n >= 0,
        _ => false,
    };
    Ok(Value::bool(is_whole))
}

/// `(zerop OBJ)` -> t if zero.
pub(crate) fn builtin_zerop(args: Vec<Value>) -> EvalResult {
    expect_args("zerop", &args, 1)?;
    let is_zero = match &args[0] {
        Value::Int(0) => true,
        Value::Float(f) => *f == 0.0,
        _ => false,
    };
    Ok(Value::bool(is_zero))
}

// ---------------------------------------------------------------------------
// Misc operations
// ---------------------------------------------------------------------------

/// `(user-login-name)` -> string.
pub(crate) fn builtin_user_login_name(args: Vec<Value>) -> EvalResult {
    let _ = args;
    let name = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    Ok(Value::string(name))
}

/// `(user-real-login-name)` -> string.
pub(crate) fn builtin_user_real_login_name(args: Vec<Value>) -> EvalResult {
    builtin_user_login_name(args)
}

/// `(user-full-name)` -> string.
pub(crate) fn builtin_user_full_name(args: Vec<Value>) -> EvalResult {
    let _ = args;
    let name = std::env::var("NAME")
        .unwrap_or_else(|_| std::env::var("USER").unwrap_or_else(|_| "Unknown".to_string()));
    Ok(Value::string(name))
}

/// `(system-name)` -> string.
pub(crate) fn builtin_system_name(args: Vec<Value>) -> EvalResult {
    let _ = args;
    let name = std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_string());
    Ok(Value::string(name))
}

/// `(emacs-version)` -> string.
pub(crate) fn builtin_emacs_version(args: Vec<Value>) -> EvalResult {
    let _ = args;
    Ok(Value::string("NeoVM 0.1.0 (Neomacs)"))
}

/// `(emacs-pid)` -> integer.
pub(crate) fn builtin_emacs_pid(args: Vec<Value>) -> EvalResult {
    let _ = args;
    Ok(Value::Int(std::process::id() as i64))
}

fn gc_bucket(name: &str, counts: &[i64]) -> Value {
    let mut items = Vec::with_capacity(counts.len() + 1);
    items.push(Value::symbol(name));
    items.extend(counts.iter().copied().map(Value::Int));
    Value::list(items)
}

/// `(garbage-collect)` -> GC stats list.
pub(crate) fn builtin_garbage_collect(args: Vec<Value>) -> EvalResult {
    expect_args("garbage-collect", &args, 0)?;
    Ok(Value::list(vec![
        gc_bucket("conses", &[0, 0, 0]),
        gc_bucket("symbols", &[0, 0, 0]),
        gc_bucket("strings", &[0, 0, 0]),
        gc_bucket("string-bytes", &[0, 0]),
        gc_bucket("vectors", &[0, 0]),
        gc_bucket("vector-slots", &[0, 0, 0]),
        gc_bucket("floats", &[0, 0, 0]),
        gc_bucket("intervals", &[0, 0, 0]),
        gc_bucket("buffers", &[0, 0]),
    ]))
}

/// `(memory-use-counts)` -> list of integers (stub).
pub(crate) fn builtin_memory_use_counts(args: Vec<Value>) -> EvalResult {
    expect_args("memory-use-counts", &args, 0)?;
    Ok(Value::list(vec![Value::Int(0); 7]))
}

/// `(symbol-name SYM)` — already exists but let's provide compat.
pub(crate) fn builtin_symbol_name(args: Vec<Value>) -> EvalResult {
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
    use crate::elisp::value::{LambdaData, LambdaParams};
    use std::sync::Arc;

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
    fn proper_list_p() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2)]);
        assert!(matches!(
            builtin_proper_list_p(vec![list]).unwrap(),
            Value::True,
        ));
        assert!(builtin_proper_list_p(vec![Value::Int(5)]).unwrap().is_nil(),);
    }

    #[test]
    fn closurep_true_for_lambda_values() {
        let lambda = Value::Lambda(Arc::new(LambdaData {
            params: LambdaParams::simple(vec!["x".to_string()]),
            body: vec![],
            env: None,
            docstring: None,
        }));
        assert!(builtin_closurep(vec![lambda]).unwrap().is_truthy());
        assert!(builtin_closurep(vec![Value::Int(1)]).unwrap().is_nil());
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
    fn garbage_collect_shape_and_arity() {
        let gc = builtin_garbage_collect(vec![]).unwrap();
        let buckets = super::super::value::list_to_vec(&gc).expect("gc list");
        assert_eq!(buckets.len(), 9);
        let names = buckets
            .iter()
            .map(|bucket| {
                let bucket_items = super::super::value::list_to_vec(bucket).expect("bucket list");
                match bucket_items.first() {
                    Some(Value::Symbol(name)) => name.clone(),
                    other => panic!("expected bucket symbol, got {other:?}"),
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "conses".to_string(),
                "symbols".to_string(),
                "strings".to_string(),
                "string-bytes".to_string(),
                "vectors".to_string(),
                "vector-slots".to_string(),
                "floats".to_string(),
                "intervals".to_string(),
                "buffers".to_string(),
            ]
        );

        let err = builtin_garbage_collect(vec![Value::Int(1)]).unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "wrong-number-of-arguments"),
            other => panic!("expected signal, got {other:?}"),
        }
    }

    #[test]
    fn memory_use_counts_shape_and_arity() {
        let counts = builtin_memory_use_counts(vec![]).unwrap();
        let items = super::super::value::list_to_vec(&counts).expect("counts list");
        assert_eq!(items.len(), 7);
        assert!(items.iter().all(|item| matches!(item, Value::Int(_))));

        let err = builtin_memory_use_counts(vec![Value::Int(1)]).unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "wrong-number-of-arguments"),
            other => panic!("expected signal, got {other:?}"),
        }
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
