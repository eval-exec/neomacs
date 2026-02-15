//! CL-lib, seq.el, and JSON built-in functions.
//!
//! Provides Common Lisp compatibility functions, sequence operations,
//! and JSON parsing/serialization for the Elisp interpreter.

use super::error::{signal, EvalResult, Flow};
use super::value::*;

// ---------------------------------------------------------------------------
// Argument helpers (local copies for this module)
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

fn expect_number_or_marker(val: &Value) -> Result<f64, Flow> {
    match val {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        Value::Char(c) => Ok(*c as i64 as f64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("number-or-marker-p"), other.clone()],
        )),
    }
}

/// Collect elements from any sequence type into a Vec.
fn collect_sequence(val: &Value) -> Vec<Value> {
    match val {
        Value::Nil => Vec::new(),
        Value::Cons(_) => list_to_vec(val).unwrap_or_default(),
        Value::Vector(v) => v.lock().expect("poisoned").clone(),
        Value::Str(s) => s.chars().map(Value::Char).collect(),
        _ => vec![val.clone()],
    }
}

/// Convert a type-name symbol to a string for type dispatch.
fn type_name_str(val: &Value) -> &str {
    match val {
        Value::Symbol(s) => s.as_str(),
        Value::Keyword(s) => s.as_str(),
        _ => "",
    }
}

fn seq_position_list_elements(seq: &Value) -> Result<Vec<Value>, Flow> {
    let mut elements = Vec::new();
    let mut cursor = seq.clone();
    loop {
        match cursor {
            Value::Nil => return Ok(elements),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                elements.push(pair.car.clone());
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

fn seq_position_elements(seq: &Value) -> Result<Vec<Value>, Flow> {
    match seq {
        Value::Nil => Ok(Vec::new()),
        Value::Cons(_) => seq_position_list_elements(seq),
        Value::Vector(v) => Ok(v.lock().expect("poisoned").clone()),
        Value::Str(s) => Ok(s.chars().map(|ch| Value::Int(ch as i64)).collect()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("sequencep"), other.clone()],
        )),
    }
}

fn seq_default_match(left: &Value, right: &Value) -> bool {
    if equal_value(left, right, 0) {
        return true;
    }
    match (left, right) {
        (Value::Char(a), Value::Int(b)) => (*a as i64) == *b,
        (Value::Int(a), Value::Char(b)) => *a == (*b as i64),
        _ => false,
    }
}

fn seq_collect_concat_arg(arg: &Value) -> Result<Vec<Value>, Flow> {
    match arg {
        Value::Nil => Ok(Vec::new()),
        Value::Cons(_) => {
            let mut out = Vec::new();
            let mut cursor = arg.clone();
            loop {
                match cursor {
                    Value::Nil => return Ok(out),
                    Value::Cons(cell) => {
                        let pair = cell.lock().expect("poisoned");
                        out.push(pair.car.clone());
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
            "error",
            vec![Value::string(format!(
                "Cannot convert {} into a sequence",
                super::print::print_value(other)
            ))],
        )),
    }
}

// ===========================================================================
// CL-lib pure list operations
// ===========================================================================

/// `(cl-find ITEM SEQ)` — find item in sequence using `equal`.
pub(crate) fn builtin_cl_find(args: Vec<Value>) -> EvalResult {
    expect_args("cl-find", &args, 2)?;
    let item = &args[0];
    let elems = collect_sequence(&args[1]);
    for e in &elems {
        if equal_value(item, e, 0) {
            return Ok(e.clone());
        }
    }
    Ok(Value::Nil)
}

/// `(cl-position ITEM SEQ)` — position of item in sequence using `equal`.
pub(crate) fn builtin_cl_position(args: Vec<Value>) -> EvalResult {
    expect_args("cl-position", &args, 2)?;
    let item = &args[0];
    let elems = collect_sequence(&args[1]);
    for (i, e) in elems.iter().enumerate() {
        if equal_value(item, e, 0) {
            return Ok(Value::Int(i as i64));
        }
    }
    Ok(Value::Nil)
}

/// `(cl-count ITEM SEQ)` — count occurrences of item using `equal`.
pub(crate) fn builtin_cl_count(args: Vec<Value>) -> EvalResult {
    expect_args("cl-count", &args, 2)?;
    let item = &args[0];
    let elems = collect_sequence(&args[1]);
    let count = elems.iter().filter(|e| equal_value(item, e, 0)).count();
    Ok(Value::Int(count as i64))
}

/// `(cl-remove ITEM SEQ)` — remove all occurrences of item using `equal`.
pub(crate) fn builtin_cl_remove(args: Vec<Value>) -> EvalResult {
    expect_args("cl-remove", &args, 2)?;
    let item = &args[0];
    let elems = collect_sequence(&args[1]);
    let result: Vec<Value> = elems
        .into_iter()
        .filter(|e| !equal_value(item, e, 0))
        .collect();
    Ok(Value::list(result))
}

/// `(cl-substitute NEW OLD SEQ)` — replace old with new in sequence using `equal`.
pub(crate) fn builtin_cl_substitute(args: Vec<Value>) -> EvalResult {
    expect_args("cl-substitute", &args, 3)?;
    let new = &args[0];
    let old = &args[1];
    let elems = collect_sequence(&args[2]);
    let result: Vec<Value> = elems
        .into_iter()
        .map(|e| {
            if equal_value(old, &e, 0) {
                new.clone()
            } else {
                e
            }
        })
        .collect();
    Ok(Value::list(result))
}

/// `(cl-intersection LIST1 LIST2)` — set intersection using `equal`.
pub(crate) fn builtin_cl_intersection(args: Vec<Value>) -> EvalResult {
    expect_args("cl-intersection", &args, 2)?;
    let elems1 = collect_sequence(&args[0]);
    let elems2 = collect_sequence(&args[1]);
    let result: Vec<Value> = elems1
        .into_iter()
        .filter(|e| elems2.iter().any(|e2| equal_value(e, e2, 0)))
        .collect();
    Ok(Value::list(result))
}

/// `(cl-union LIST1 LIST2)` — set union using `equal`.
pub(crate) fn builtin_cl_union(args: Vec<Value>) -> EvalResult {
    expect_args("cl-union", &args, 2)?;
    let elems1 = collect_sequence(&args[0]);
    let elems2 = collect_sequence(&args[1]);
    let mut result = elems1;
    for e in elems2 {
        if !result.iter().any(|r| equal_value(r, &e, 0)) {
            result.push(e);
        }
    }
    Ok(Value::list(result))
}

/// `(cl-set-difference LIST1 LIST2)` — set difference using `equal`.
pub(crate) fn builtin_cl_set_difference(args: Vec<Value>) -> EvalResult {
    expect_args("cl-set-difference", &args, 2)?;
    let elems1 = collect_sequence(&args[0]);
    let elems2 = collect_sequence(&args[1]);
    let result: Vec<Value> = elems1
        .into_iter()
        .filter(|e| !elems2.iter().any(|e2| equal_value(e, e2, 0)))
        .collect();
    Ok(Value::list(result))
}

/// `(cl-subsetp LIST1 LIST2)` — is list1 a subset of list2?
pub(crate) fn builtin_cl_subsetp(args: Vec<Value>) -> EvalResult {
    expect_args("cl-subsetp", &args, 2)?;
    let elems1 = collect_sequence(&args[0]);
    let elems2 = collect_sequence(&args[1]);
    let all = elems1
        .iter()
        .all(|e| elems2.iter().any(|e2| equal_value(e, e2, 0)));
    Ok(Value::bool(all))
}

/// `(cl-adjoin ITEM LIST)` — add item if not present using `equal`.
pub(crate) fn builtin_cl_adjoin(args: Vec<Value>) -> EvalResult {
    expect_args("cl-adjoin", &args, 2)?;
    let item = &args[0];
    let elems = collect_sequence(&args[1]);
    if elems.iter().any(|e| equal_value(item, e, 0)) {
        Ok(args[1].clone())
    } else {
        Ok(Value::cons(item.clone(), args[1].clone()))
    }
}

/// `(cl-remove-duplicates SEQ)` — remove duplicates using `equal`.
pub(crate) fn builtin_cl_remove_duplicates(args: Vec<Value>) -> EvalResult {
    expect_args("cl-remove-duplicates", &args, 1)?;
    let elems = collect_sequence(&args[0]);
    let mut result: Vec<Value> = Vec::new();
    for e in elems {
        if !result.iter().any(|r| equal_value(r, &e, 0)) {
            result.push(e);
        }
    }
    Ok(Value::list(result))
}

/// `(cl-member ITEM LIST)` — like member but using `equal` (same as member).
pub(crate) fn builtin_cl_member(args: Vec<Value>) -> EvalResult {
    expect_args("cl-member", &args, 2)?;
    let item = &args[0];
    let mut cursor = args[1].clone();
    loop {
        match cursor {
            Value::Nil => return Ok(Value::Nil),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if equal_value(item, &pair.car, 0) {
                    drop(pair);
                    return Ok(Value::Cons(cell));
                }
                cursor = pair.cdr.clone();
            }
            _ => return Ok(Value::Nil),
        }
    }
}

// ---------------------------------------------------------------------------
// cl-first through cl-tenth, cl-rest
// ---------------------------------------------------------------------------

fn nth_helper(name: &str, args: &[Value], n: usize) -> EvalResult {
    expect_args(name, args, 1)?;
    let mut cursor = args[0].clone();
    for _ in 0..n {
        match cursor {
            Value::Nil => return Ok(Value::Nil),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                cursor = pair.cdr.clone();
            }
            _ => return Ok(Value::Nil),
        }
    }
    match cursor {
        Value::Nil => Ok(Value::Nil),
        Value::Cons(cell) => {
            let pair = cell.lock().expect("poisoned");
            Ok(pair.car.clone())
        }
        _ => Ok(Value::Nil),
    }
}

pub(crate) fn builtin_cl_first(args: Vec<Value>) -> EvalResult {
    nth_helper("cl-first", &args, 0)
}
pub(crate) fn builtin_cl_second(args: Vec<Value>) -> EvalResult {
    nth_helper("cl-second", &args, 1)
}
pub(crate) fn builtin_cl_third(args: Vec<Value>) -> EvalResult {
    nth_helper("cl-third", &args, 2)
}
pub(crate) fn builtin_cl_fourth(args: Vec<Value>) -> EvalResult {
    nth_helper("cl-fourth", &args, 3)
}

/// `(cl-rest LIST)` — alias for cdr.
pub(crate) fn builtin_cl_rest(args: Vec<Value>) -> EvalResult {
    expect_args("cl-rest", &args, 1)?;
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Cons(cell) => {
            let pair = cell.lock().expect("poisoned");
            Ok(pair.cdr.clone())
        }
        _ => Ok(Value::Nil),
    }
}

/// `(cl-subseq SEQ START &optional END)` — subsequence.
pub(crate) fn builtin_cl_subseq(args: Vec<Value>) -> EvalResult {
    expect_min_args("cl-subseq", &args, 2)?;
    let elems = collect_sequence(&args[0]);
    let start = expect_int(&args[1])? as usize;
    let end = if args.len() > 2 && !args[2].is_nil() {
        expect_int(&args[2])? as usize
    } else {
        elems.len()
    };
    if start > elems.len() || end > elems.len() || start > end {
        return Err(signal(
            "args-out-of-range",
            vec![
                args[0].clone(),
                Value::Int(start as i64),
                Value::Int(end as i64),
            ],
        ));
    }
    let result: Vec<Value> = elems[start..end].to_vec();
    // Return same type as input
    match &args[0] {
        Value::Vector(_) => Ok(Value::vector(result)),
        Value::Str(_) => {
            let s: String = result
                .iter()
                .filter_map(|v| match v {
                    Value::Char(c) => Some(*c),
                    _ => None,
                })
                .collect();
            Ok(Value::string(s))
        }
        _ => Ok(Value::list(result)),
    }
}

/// `(cl-concatenate TYPE &rest SEQUENCES)` — concatenate sequences into target type.
pub(crate) fn builtin_cl_concatenate(args: Vec<Value>) -> EvalResult {
    expect_min_args("cl-concatenate", &args, 1)?;
    let target = type_name_str(&args[0]);
    let mut combined = Vec::new();
    for arg in &args[1..] {
        combined.extend(collect_sequence(arg));
    }
    match target {
        "vector" => Ok(Value::vector(combined)),
        "string" => {
            let s: String = combined
                .iter()
                .filter_map(|v| match v {
                    Value::Char(c) => Some(*c),
                    Value::Int(n) => char::from_u32(*n as u32),
                    _ => None,
                })
                .collect();
            Ok(Value::string(s))
        }
        _ => Ok(Value::list(combined)),
    }
}

/// `(cl-coerce OBJ TYPE)` — type coercion.
pub(crate) fn builtin_cl_coerce(args: Vec<Value>) -> EvalResult {
    expect_args("cl-coerce", &args, 2)?;
    let obj = &args[0];
    let target = type_name_str(&args[1]);
    match target {
        "list" => {
            let elems = collect_sequence(obj);
            Ok(Value::list(elems))
        }
        "vector" => {
            let elems = collect_sequence(obj);
            Ok(Value::vector(elems))
        }
        "string" => {
            let elems = collect_sequence(obj);
            let s: String = elems
                .iter()
                .filter_map(|v| match v {
                    Value::Char(c) => Some(*c),
                    Value::Int(n) => char::from_u32(*n as u32),
                    _ => None,
                })
                .collect();
            Ok(Value::string(s))
        }
        "float" => match obj {
            Value::Int(n) => Ok(Value::Float(*n as f64)),
            Value::Float(_) => Ok(obj.clone()),
            _ => Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("numberp"), obj.clone()],
            )),
        },
        "integer" => match obj {
            Value::Float(f) => Ok(Value::Int(*f as i64)),
            Value::Int(_) => Ok(obj.clone()),
            Value::Char(c) => Ok(Value::Int(*c as i64)),
            _ => Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("numberp"), obj.clone()],
            )),
        },
        _ => Ok(obj.clone()),
    }
}

// ===========================================================================
// CL-lib eval-dependent operations
// ===========================================================================

/// `(cl-every PRED &rest SEQS)` — all elements satisfy predicate.
pub(crate) fn builtin_cl_every(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("cl-every", &args, 2)?;
    let pred = args[0].clone();
    let seqs: Vec<Vec<Value>> = args[1..].iter().map(|s| collect_sequence(s)).collect();
    if seqs.is_empty() {
        return Ok(Value::True);
    }
    let min_len = seqs.iter().map(|s| s.len()).min().unwrap_or(0);
    for i in 0..min_len {
        let call_args: Vec<Value> = seqs.iter().map(|s| s[i].clone()).collect();
        let result = eval.apply(pred.clone(), call_args)?;
        if result.is_nil() {
            return Ok(Value::Nil);
        }
    }
    Ok(Value::True)
}

/// `(cl-some PRED &rest SEQS)` — some element satisfies predicate.
pub(crate) fn builtin_cl_some(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("cl-some", &args, 2)?;
    let pred = args[0].clone();
    let seqs: Vec<Vec<Value>> = args[1..].iter().map(|s| collect_sequence(s)).collect();
    if seqs.is_empty() {
        return Ok(Value::Nil);
    }
    let min_len = seqs.iter().map(|s| s.len()).min().unwrap_or(0);
    for i in 0..min_len {
        let call_args: Vec<Value> = seqs.iter().map(|s| s[i].clone()).collect();
        let result = eval.apply(pred.clone(), call_args)?;
        if result.is_truthy() {
            return Ok(result);
        }
    }
    Ok(Value::Nil)
}

/// `(cl-reduce FN SEQ &optional INITIAL-VALUE)` — reduce/fold.
pub(crate) fn builtin_cl_reduce(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("cl-reduce", &args, 2)?;
    let func = args[0].clone();
    let elems = collect_sequence(&args[1]);
    let initial = if args.len() > 2 {
        Some(args[2].clone())
    } else {
        None
    };

    let mut iter = elems.into_iter();
    let mut acc = match initial {
        Some(v) => v,
        None => match iter.next() {
            Some(v) => v,
            None => return Ok(Value::Nil),
        },
    };
    for elem in iter {
        acc = eval.apply(func.clone(), vec![acc, elem])?;
    }
    Ok(acc)
}

// ===========================================================================
// Seq.el pure operations
// ===========================================================================

/// `(seq-reverse SEQ)` — reverse a sequence.
pub(crate) fn builtin_seq_reverse(args: Vec<Value>) -> EvalResult {
    expect_args("seq-reverse", &args, 1)?;
    let mut elems = seq_position_elements(&args[0])?;
    elems.reverse();
    match &args[0] {
        Value::Vector(_) => Ok(Value::vector(elems)),
        Value::Str(_) => {
            let mut s = String::new();
            for value in &elems {
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
        _ => Ok(Value::list(elems)),
    }
}

/// `(seq-drop SEQ N)` — drop first n elements.
pub(crate) fn builtin_seq_drop(args: Vec<Value>) -> EvalResult {
    expect_args("seq-drop", &args, 2)?;
    let n = expect_int(&args[1])?;

    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Vector(v) => {
            let elems = v.lock().expect("poisoned");
            if n <= 0 {
                return Ok(Value::vector(elems.clone()));
            }
            let n = (n as usize).min(elems.len());
            Ok(Value::vector(elems[n..].to_vec()))
        }
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            if n <= 0 {
                return Ok(Value::string((**s).clone()));
            }
            let n = (n as usize).min(chars.len());
            let out: String = chars[n..].iter().collect();
            Ok(Value::string(out))
        }
        Value::Cons(_) => {
            if n <= 0 {
                return Ok(args[0].clone());
            }
            let mut cursor = args[0].clone();
            let mut remaining = n as usize;
            while remaining > 0 {
                match cursor {
                    Value::Nil => return Ok(Value::Nil),
                    Value::Cons(cell) => {
                        let pair = cell.lock().expect("poisoned");
                        cursor = pair.cdr.clone();
                        remaining -= 1;
                    }
                    _ => {
                        return Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("listp"), args[0].clone()],
                        ));
                    }
                }
            }
            Ok(cursor)
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("sequencep"), other.clone()],
        )),
    }
}

/// `(seq-take SEQ N)` — take first n elements.
pub(crate) fn builtin_seq_take(args: Vec<Value>) -> EvalResult {
    expect_args("seq-take", &args, 2)?;
    let n = expect_int(&args[1])?;

    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Vector(v) => {
            let elems = v.lock().expect("poisoned");
            if n <= 0 {
                return Ok(Value::vector(Vec::new()));
            }
            let n = (n as usize).min(elems.len());
            Ok(Value::vector(elems[..n].to_vec()))
        }
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            if n <= 0 {
                return Ok(Value::string(""));
            }
            let n = (n as usize).min(chars.len());
            let out: String = chars[..n].iter().collect();
            Ok(Value::string(out))
        }
        Value::Cons(_) => {
            if n <= 0 {
                return Ok(Value::Nil);
            }
            let mut out = Vec::new();
            let mut cursor = args[0].clone();
            let mut remaining = n as usize;
            while remaining > 0 {
                match cursor {
                    Value::Nil => break,
                    Value::Cons(cell) => {
                        let pair = cell.lock().expect("poisoned");
                        out.push(pair.car.clone());
                        cursor = pair.cdr.clone();
                        remaining -= 1;
                    }
                    tail => {
                        return Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("listp"), tail],
                        ));
                    }
                }
            }
            Ok(Value::list(out))
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("sequencep"), other.clone()],
        )),
    }
}

fn builtin_seq_subseq_legacy(args: &[Value]) -> EvalResult {
    let elems = collect_sequence(&args[0]);
    let start = expect_int(&args[1])? as usize;
    let end = if args.len() > 2 && !args[2].is_nil() {
        expect_int(&args[2])? as usize
    } else {
        elems.len()
    };
    let start = start.min(elems.len());
    let end = end.min(elems.len());
    if start > end {
        return Ok(Value::Nil);
    }
    let result: Vec<Value> = elems[start..end].to_vec();
    match &args[0] {
        Value::Vector(_) => Ok(Value::vector(result)),
        _ => Ok(Value::list(result)),
    }
}

/// `(seq-subseq SEQ START &optional END)` — subsequence.
pub(crate) fn builtin_seq_subseq(args: Vec<Value>) -> EvalResult {
    expect_min_args("seq-subseq", &args, 2)?;
    let start = expect_int(&args[1])?;
    let end = if args.len() > 2 && !args[2].is_nil() {
        Some(expect_int(&args[2])?)
    } else {
        None
    };

    // Preserve existing behavior for negative indices until full seq.el
    // index normalization support lands.
    if start < 0 || end.is_some_and(|v| v < 0) {
        return builtin_seq_subseq_legacy(&args);
    }

    match &args[0] {
        Value::Nil | Value::Cons(_) | Value::Vector(_) | Value::Str(_) => {
            let dropped = builtin_seq_drop(vec![args[0].clone(), Value::Int(start)])?;
            if let Some(end_idx) = end {
                let span = end_idx - start;
                builtin_seq_take(vec![dropped, Value::Int(span)])
            } else {
                Ok(dropped)
            }
        }
        other => Err(signal(
            "error",
            vec![Value::string(format!(
                "Unsupported sequence: {}",
                super::print::print_value(other)
            ))],
        )),
    }
}

/// `(seq-concatenate TYPE &rest SEQS)` — concatenate sequences into target type.
pub(crate) fn builtin_seq_concatenate(args: Vec<Value>) -> EvalResult {
    expect_min_args("seq-concatenate", &args, 1)?;
    let target = match &args[0] {
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
    if target != "list" && target != "vector" && target != "string" {
        return Err(signal(
            "error",
            vec![Value::string(format!("Not a sequence type name: {}", target))],
        ));
    }

    let mut combined = Vec::new();
    for arg in &args[1..] {
        combined.extend(seq_collect_concat_arg(arg)?);
    }
    match target {
        "list" => Ok(Value::list(combined)),
        "vector" => Ok(Value::vector(combined)),
        "string" => {
            let mut s = String::new();
            for value in &combined {
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
        _ => unreachable!(),
    }
}

/// `(seq-empty-p SEQ)` — is sequence empty?
pub(crate) fn builtin_seq_empty_p(args: Vec<Value>) -> EvalResult {
    expect_args("seq-empty-p", &args, 1)?;
    match &args[0] {
        Value::Nil => Ok(Value::True),
        Value::Cons(_) => Ok(Value::Nil),
        Value::Str(s) => Ok(Value::bool(s.is_empty())),
        Value::Vector(v) => Ok(Value::bool(v.lock().expect("poisoned").is_empty())),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("sequencep"), other.clone()],
        )),
    }
}

/// `(seq-min SEQ)` — minimum element (numeric).
pub(crate) fn builtin_seq_min(args: Vec<Value>) -> EvalResult {
    expect_args("seq-min", &args, 1)?;
    let elems = seq_position_elements(&args[0])?;
    if elems.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::Subr("min".to_string()), Value::Int(0)],
        ));
    }
    let mut min_val = &elems[0];
    let mut min_num = expect_number_or_marker(min_val)?;
    for e in &elems[1..] {
        let b = expect_number_or_marker(e)?;
        if b < min_num {
            min_num = b;
            min_val = e;
        }
    }
    Ok(min_val.clone())
}

/// `(seq-max SEQ)` — maximum element (numeric).
pub(crate) fn builtin_seq_max(args: Vec<Value>) -> EvalResult {
    expect_args("seq-max", &args, 1)?;
    let elems = seq_position_elements(&args[0])?;
    if elems.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::Subr("max".to_string()), Value::Int(0)],
        ));
    }
    let mut max_val = &elems[0];
    let mut max_num = expect_number_or_marker(max_val)?;
    for e in &elems[1..] {
        let b = expect_number_or_marker(e)?;
        if b > max_num {
            max_num = b;
            max_val = e;
        }
    }
    Ok(max_val.clone())
}

/// `(seq-into SEQ TYPE)` — convert sequence type.
pub(crate) fn builtin_seq_into(args: Vec<Value>) -> EvalResult {
    expect_args("seq-into", &args, 2)?;
    let elems = collect_sequence(&args[0]);
    let target = type_name_str(&args[1]);
    match target {
        "vector" => Ok(Value::vector(elems)),
        "string" => {
            let s: String = elems
                .iter()
                .filter_map(|v| match v {
                    Value::Char(c) => Some(*c),
                    Value::Int(n) => char::from_u32(*n as u32),
                    _ => None,
                })
                .collect();
            Ok(Value::string(s))
        }
        _ => Ok(Value::list(elems)),
    }
}

// ===========================================================================
// Seq.el eval-dependent operations
// ===========================================================================

/// `(seq-position SEQ ELT &optional TESTFN)` — return first matching index.
pub(crate) fn builtin_seq_position(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("seq-position", &args, 2)?;
    let seq = &args[0];
    let target = args[1].clone();
    let test_fn = if args.len() > 2 && !args[2].is_nil() {
        Some(args[2].clone())
    } else {
        None
    };
    let elements = seq_position_elements(seq)?;

    for (idx, element) in elements.into_iter().enumerate() {
        let matches = if let Some(test) = &test_fn {
            eval.apply(test.clone(), vec![element.clone(), target.clone()])?
                .is_truthy()
        } else {
            seq_default_match(&element, &target)
        };
        if matches {
            return Ok(Value::Int(idx as i64));
        }
    }
    Ok(Value::Nil)
}

/// `(seq-contains-p SEQ ELT &optional TESTFN)` — membership test for sequence.
pub(crate) fn builtin_seq_contains_p(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    if !(2..=3).contains(&args.len()) {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("seq-contains-p"), Value::Int(args.len() as i64)],
        ));
    }
    let seq = &args[0];
    let target = args[1].clone();
    let test_fn = if args.len() == 3 && !args[2].is_nil() {
        Some(args[2].clone())
    } else {
        None
    };
    let elements = seq_position_elements(seq)?;

    for element in elements {
        let matches = if let Some(test) = &test_fn {
            eval.apply(test.clone(), vec![element.clone(), target.clone()])?
                .is_truthy()
        } else {
            seq_default_match(&element, &target)
        };
        if matches {
            return Ok(Value::True);
        }
    }
    Ok(Value::Nil)
}

/// `(seq-mapn FN &rest SEQS)` — map over multiple sequences.
pub(crate) fn builtin_seq_mapn(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("seq-mapn", &args, 2)?;
    let func = args[0].clone();
    let seqs: Vec<Vec<Value>> = args[1..].iter().map(|s| collect_sequence(s)).collect();
    if seqs.is_empty() {
        return Ok(Value::Nil);
    }
    let min_len = seqs.iter().map(|s| s.len()).min().unwrap_or(0);
    let mut results = Vec::new();
    for i in 0..min_len {
        let call_args: Vec<Value> = seqs.iter().map(|s| s[i].clone()).collect();
        results.push(eval.apply(func.clone(), call_args)?);
    }
    Ok(Value::list(results))
}

/// `(seq-do FN SEQ)` — apply fn for side effects, return nil.
pub(crate) fn builtin_seq_do(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("seq-do", &args, 2)?;
    let func = args[0].clone();
    let elems = collect_sequence(&args[1]);
    for e in elems {
        eval.apply(func.clone(), vec![e])?;
    }
    Ok(Value::Nil)
}

/// `(seq-count PRED SEQ)` — count elements matching predicate.
pub(crate) fn builtin_seq_count(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("seq-count", &args, 2)?;
    let pred = args[0].clone();
    let elems = collect_sequence(&args[1]);
    let mut count = 0i64;
    for e in elems {
        let r = eval.apply(pred.clone(), vec![e])?;
        if r.is_truthy() {
            count += 1;
        }
    }
    Ok(Value::Int(count))
}

/// `(seq-reduce FN SEQ INITIAL)` — reduce with initial value.
pub(crate) fn builtin_seq_reduce(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("seq-reduce", &args, 3)?;
    let func = args[0].clone();
    let elems = collect_sequence(&args[1]);
    let mut acc = args[2].clone();
    for e in elems {
        acc = eval.apply(func.clone(), vec![acc, e])?;
    }
    Ok(acc)
}

/// `(seq-some PRED SEQ)` — some element matches predicate.
pub(crate) fn builtin_seq_some(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("seq-some", &args, 2)?;
    let pred = args[0].clone();
    let elems = collect_sequence(&args[1]);
    for e in elems {
        let r = eval.apply(pred.clone(), vec![e])?;
        if r.is_truthy() {
            return Ok(r);
        }
    }
    Ok(Value::Nil)
}

/// `(seq-every-p PRED SEQ)` — all elements match predicate.
pub(crate) fn builtin_seq_every_p(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("seq-every-p", &args, 2)?;
    let pred = args[0].clone();
    let elems = collect_sequence(&args[1]);
    for e in elems {
        let r = eval.apply(pred.clone(), vec![e])?;
        if r.is_nil() {
            return Ok(Value::Nil);
        }
    }
    Ok(Value::True)
}

/// `(seq-sort PRED SEQ)` — sort with predicate.
pub(crate) fn builtin_seq_sort(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("seq-sort", &args, 2)?;
    let pred = args[0].clone();
    let mut items = collect_sequence(&args[1]);

    // Insertion sort (stable, supports fallible predicates)
    for i in 1..items.len() {
        let mut j = i;
        while j > 0 {
            let result = eval.apply(pred.clone(), vec![items[j].clone(), items[j - 1].clone()])?;
            if result.is_truthy() {
                items.swap(j, j - 1);
                j -= 1;
            } else {
                break;
            }
        }
    }
    Ok(Value::list(items))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- CL-lib pure operations ---

    #[test]
    fn cl_find_in_list() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_cl_find(vec![Value::Int(2), list]).unwrap();
        assert_eq!(result.as_int(), Some(2));
    }

    #[test]
    fn cl_find_not_found() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let result = builtin_cl_find(vec![Value::Int(99), list]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn cl_position_found() {
        let list = Value::list(vec![Value::Int(10), Value::Int(20), Value::Int(30)]);
        let result = builtin_cl_position(vec![Value::Int(20), list]).unwrap();
        assert_eq!(result.as_int(), Some(1));
    }

    #[test]
    fn cl_position_not_found() {
        let list = Value::list(vec![Value::Int(10)]);
        let result = builtin_cl_position(vec![Value::Int(99), list]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn cl_count_occurrences() {
        let list = Value::list(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(1),
            Value::Int(1),
        ]);
        let result = builtin_cl_count(vec![Value::Int(1), list]).unwrap();
        assert_eq!(result.as_int(), Some(3));
    }

    #[test]
    fn cl_remove_items() {
        let list = Value::list(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
            Value::Int(2),
        ]);
        let result = builtin_cl_remove(vec![Value::Int(2), list]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_int(), Some(1));
        assert_eq!(items[1].as_int(), Some(3));
    }

    #[test]
    fn cl_substitute_items() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_cl_substitute(vec![Value::Int(99), Value::Int(2), list]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items[1].as_int(), Some(99));
    }

    #[test]
    fn cl_intersection_test() {
        let l1 = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let l2 = Value::list(vec![Value::Int(2), Value::Int(3), Value::Int(4)]);
        let result = builtin_cl_intersection(vec![l1, l2]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn cl_union_test() {
        let l1 = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let l2 = Value::list(vec![Value::Int(2), Value::Int(3)]);
        let result = builtin_cl_union(vec![l1, l2]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn cl_set_difference_test() {
        let l1 = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let l2 = Value::list(vec![Value::Int(2), Value::Int(4)]);
        let result = builtin_cl_set_difference(vec![l1, l2]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_int(), Some(1));
        assert_eq!(items[1].as_int(), Some(3));
    }

    #[test]
    fn cl_subsetp_test() {
        let l1 = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let l2 = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert!(matches!(
            builtin_cl_subsetp(vec![l1.clone(), l2.clone()]).unwrap(),
            Value::True
        ));
        assert!(builtin_cl_subsetp(vec![l2, l1]).unwrap().is_nil());
    }

    #[test]
    fn cl_adjoin_already_present() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let result = builtin_cl_adjoin(vec![Value::Int(1), list]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn cl_adjoin_new_item() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let result = builtin_cl_adjoin(vec![Value::Int(3), list]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn cl_remove_duplicates_test() {
        let list = Value::list(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(1),
            Value::Int(3),
            Value::Int(2),
        ]);
        let result = builtin_cl_remove_duplicates(vec![list]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn cl_first_through_third() {
        let list = Value::list(vec![Value::Int(10), Value::Int(20), Value::Int(30)]);
        assert_eq!(
            builtin_cl_first(vec![list.clone()]).unwrap().as_int(),
            Some(10)
        );
        assert_eq!(
            builtin_cl_second(vec![list.clone()]).unwrap().as_int(),
            Some(20)
        );
        assert_eq!(
            builtin_cl_third(vec![list.clone()]).unwrap().as_int(),
            Some(30)
        );
        assert!(builtin_cl_fourth(vec![list]).unwrap().is_nil());
    }

    #[test]
    fn cl_rest_test() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_cl_rest(vec![list]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_int(), Some(2));
    }

    #[test]
    fn cl_subseq_test() {
        let list = Value::list(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
            Value::Int(4),
        ]);
        let result = builtin_cl_subseq(vec![list, Value::Int(1), Value::Int(3)]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_int(), Some(2));
        assert_eq!(items[1].as_int(), Some(3));
    }

    #[test]
    fn cl_concatenate_lists() {
        let l1 = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let l2 = Value::list(vec![Value::Int(3), Value::Int(4)]);
        let result = builtin_cl_concatenate(vec![Value::symbol("list"), l1, l2]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn cl_coerce_to_vector() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_cl_coerce(vec![list, Value::symbol("vector")]).unwrap();
        assert!(result.is_vector());
    }

    #[test]
    fn cl_coerce_to_float() {
        let result = builtin_cl_coerce(vec![Value::Int(42), Value::symbol("float")]).unwrap();
        assert!(result.is_float());
    }

    #[test]
    fn cl_member_found() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_cl_member(vec![Value::Int(2), list]).unwrap();
        assert!(result.is_cons());
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_int(), Some(2));
    }

    #[test]
    fn cl_member_not_found() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let result = builtin_cl_member(vec![Value::Int(99), list]).unwrap();
        assert!(result.is_nil());
    }

    // --- Seq.el pure operations ---

    #[test]
    fn seq_reverse_list() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_seq_reverse(vec![list]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items[0].as_int(), Some(3));
        assert_eq!(items[2].as_int(), Some(1));
    }

    #[test]
    fn seq_reverse_string() {
        let s = Value::string("abc");
        let result = builtin_seq_reverse(vec![s]).unwrap();
        assert_eq!(result.as_str(), Some("cba"));
    }

    #[test]
    fn seq_drop_test() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_seq_drop(vec![list, Value::Int(2)]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_int(), Some(3));
    }

    #[test]
    fn seq_take_test() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_seq_take(vec![list, Value::Int(2)]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn seq_subseq_test() {
        let vec = Value::vector(vec![
            Value::Int(10),
            Value::Int(20),
            Value::Int(30),
            Value::Int(40),
        ]);
        let result = builtin_seq_subseq(vec![vec, Value::Int(1), Value::Int(3)]).unwrap();
        if let Value::Vector(v) = result {
            let v = v.lock().unwrap();
            assert_eq!(v.len(), 2);
            assert_eq!(v[0].as_int(), Some(20));
            assert_eq!(v[1].as_int(), Some(30));
        } else {
            panic!("expected vector");
        }
    }

    #[test]
    fn seq_concatenate_test() {
        let l1 = Value::list(vec![Value::Int(1)]);
        let l2 = Value::list(vec![Value::Int(2)]);
        let result = builtin_seq_concatenate(vec![Value::symbol("list"), l1, l2]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn seq_empty_p_test() {
        assert!(matches!(
            builtin_seq_empty_p(vec![Value::Nil]).unwrap(),
            Value::True
        ));
        assert!(matches!(
            builtin_seq_empty_p(vec![Value::string("")]).unwrap(),
            Value::True
        ));
        assert!(builtin_seq_empty_p(vec![Value::list(vec![Value::Int(1)])])
            .unwrap()
            .is_nil());
    }

    #[test]
    fn seq_min_max_test() {
        let list = Value::list(vec![Value::Int(3), Value::Int(1), Value::Int(2)]);
        assert_eq!(
            builtin_seq_min(vec![list.clone()]).unwrap().as_int(),
            Some(1)
        );
        assert_eq!(builtin_seq_max(vec![list]).unwrap().as_int(), Some(3));
    }

    #[test]
    fn seq_into_vector() {
        let list = Value::list(vec![Value::Int(1), Value::Int(2)]);
        let result = builtin_seq_into(vec![list, Value::symbol("vector")]).unwrap();
        assert!(result.is_vector());
    }

    // --- Eval-dependent tests (using Evaluator) ---

    #[test]
    fn cl_every_with_eval() {
        let mut evaluator = super::super::eval::Evaluator::new();
        // Test cl-every with numberp
        evaluator
            .eval_forms(&super::super::parser::parse_forms("(defun my-pos (x) (> x 0))").unwrap());
        let func = Value::Subr("numberp".to_string());
        let seq = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_cl_every(&mut evaluator, vec![func, seq]).unwrap();
        assert!(matches!(result, Value::True));
    }

    #[test]
    fn cl_some_with_eval() {
        let mut evaluator = super::super::eval::Evaluator::new();
        let func = Value::Subr("null".to_string());
        let seq = Value::list(vec![Value::Int(1), Value::Nil, Value::Int(3)]);
        let result = builtin_cl_some(&mut evaluator, vec![func, seq]).unwrap();
        assert!(result.is_truthy());
    }

    #[test]
    fn cl_reduce_with_eval() {
        let mut evaluator = super::super::eval::Evaluator::new();
        let func = Value::Subr("+".to_string());
        let seq = Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result = builtin_cl_reduce(&mut evaluator, vec![func, seq, Value::Int(0)]).unwrap();
        assert_eq!(result.as_int(), Some(6));
    }

    #[test]
    fn seq_reduce_with_eval() {
        let mut evaluator = super::super::eval::Evaluator::new();
        let func = Value::Subr("+".to_string());
        let seq = Value::list(vec![Value::Int(10), Value::Int(20)]);
        let result = builtin_seq_reduce(&mut evaluator, vec![func, seq, Value::Int(0)]).unwrap();
        assert_eq!(result.as_int(), Some(30));
    }

    #[test]
    fn seq_count_with_eval() {
        let mut evaluator = super::super::eval::Evaluator::new();
        let func = Value::Subr("numberp".to_string());
        let seq = Value::list(vec![Value::Int(1), Value::string("a"), Value::Int(2)]);
        let result = builtin_seq_count(&mut evaluator, vec![func, seq]).unwrap();
        assert_eq!(result.as_int(), Some(2));
    }
}
