//! Built-in primitive functions.
//!
//! All functions here take pre-evaluated `Vec<Value>` arguments and return `EvalResult`.
//! The evaluator dispatches here after evaluating the argument expressions.

use super::error::{signal, EvalResult, Flow};
use super::value::*;

/// Expect exactly N arguments.
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

/// Expect at least N arguments.
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

/// Extract an integer, signaling wrong-type-argument if not.
fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), other.clone()],
        )),
    }
}

/// Extract a number as f64.
fn expect_number(value: &Value) -> Result<f64, Flow> {
    match value {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        Value::Char(c) => Ok(*c as u32 as f64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("numberp"), other.clone()],
        )),
    }
}

/// True if any arg is a float (triggers float arithmetic).
fn has_float(args: &[Value]) -> bool {
    args.iter().any(|v| matches!(v, Value::Float(_)))
}

/// Return a numeric result: int if all were ints, float otherwise.
fn numeric_result(f: f64, was_float: bool) -> Value {
    if was_float {
        Value::Float(f)
    } else {
        Value::Int(f as i64)
    }
}

// ===========================================================================
// Arithmetic
// ===========================================================================

pub(crate) fn builtin_add(args: Vec<Value>) -> EvalResult {
    if has_float(&args) {
        let mut sum = 0.0f64;
        for a in &args {
            sum += expect_number(a)?;
        }
        Ok(Value::Float(sum))
    } else {
        let mut sum = 0i64;
        for a in &args {
            sum = sum
                .checked_add(expect_int(a)?)
                .ok_or_else(|| signal("overflow-error", vec![]))?;
        }
        Ok(Value::Int(sum))
    }
}

pub(crate) fn builtin_sub(args: Vec<Value>) -> EvalResult {
    expect_min_args("-", &args, 1)?;
    if args.len() == 1 {
        // Unary negation
        if has_float(&args) {
            return Ok(Value::Float(-expect_number(&args[0])?));
        }
        return Ok(Value::Int(-expect_int(&args[0])?));
    }
    if has_float(&args) {
        let mut acc = expect_number(&args[0])?;
        for a in &args[1..] {
            acc -= expect_number(a)?;
        }
        Ok(Value::Float(acc))
    } else {
        let mut acc = expect_int(&args[0])?;
        for a in &args[1..] {
            acc = acc
                .checked_sub(expect_int(a)?)
                .ok_or_else(|| signal("overflow-error", vec![]))?;
        }
        Ok(Value::Int(acc))
    }
}

pub(crate) fn builtin_mul(args: Vec<Value>) -> EvalResult {
    if has_float(&args) {
        let mut prod = 1.0f64;
        for a in &args {
            prod *= expect_number(a)?;
        }
        Ok(Value::Float(prod))
    } else {
        let mut prod = 1i64;
        for a in &args {
            prod = prod
                .checked_mul(expect_int(a)?)
                .ok_or_else(|| signal("overflow-error", vec![]))?;
        }
        Ok(Value::Int(prod))
    }
}

pub(crate) fn builtin_div(args: Vec<Value>) -> EvalResult {
    expect_min_args("/", &args, 2)?;
    if has_float(&args) {
        let mut acc = expect_number(&args[0])?;
        for a in &args[1..] {
            let d = expect_number(a)?;
            if d == 0.0 {
                return Err(signal("arith-error", vec![]));
            }
            acc /= d;
        }
        Ok(Value::Float(acc))
    } else {
        let mut acc = expect_int(&args[0])?;
        for a in &args[1..] {
            let d = expect_int(a)?;
            if d == 0 {
                return Err(signal("arith-error", vec![]));
            }
            acc /= d; // Truncating division like Emacs
        }
        Ok(Value::Int(acc))
    }
}

pub(crate) fn builtin_mod(args: Vec<Value>) -> EvalResult {
    expect_args("%", &args, 2)?;
    if has_float(&args) {
        let a = expect_number(&args[0])?;
        let b = expect_number(&args[1])?;
        if b == 0.0 {
            return Err(signal("arith-error", vec![]));
        }
        // Emacs mod: result has sign of divisor
        let r = a % b;
        let r = if (r < 0.0) != (b < 0.0) { r + b } else { r };
        Ok(Value::Float(r))
    } else {
        let a = expect_int(&args[0])?;
        let b = expect_int(&args[1])?;
        if b == 0 {
            return Err(signal("arith-error", vec![]));
        }
        // Emacs mod: result has sign of divisor
        let r = a % b;
        let r = if (r < 0) != (b < 0) { r + b } else { r };
        Ok(Value::Int(r))
    }
}

pub(crate) fn builtin_add1(args: Vec<Value>) -> EvalResult {
    expect_args("1+", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(n.checked_add(1).ok_or_else(|| signal("overflow-error", vec![]))?)),
        Value::Float(f) => Ok(Value::Float(f + 1.0)),
        other => Err(signal("wrong-type-argument", vec![Value::symbol("number-or-marker-p"), other.clone()])),
    }
}

pub(crate) fn builtin_sub1(args: Vec<Value>) -> EvalResult {
    expect_args("1-", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(n.checked_sub(1).ok_or_else(|| signal("overflow-error", vec![]))?)),
        Value::Float(f) => Ok(Value::Float(f - 1.0)),
        other => Err(signal("wrong-type-argument", vec![Value::symbol("number-or-marker-p"), other.clone()])),
    }
}

pub(crate) fn builtin_max(args: Vec<Value>) -> EvalResult {
    expect_min_args("max", &args, 1)?;
    let float = has_float(&args);
    let mut best = expect_number(&args[0])?;
    for a in &args[1..] {
        let n = expect_number(a)?;
        if n > best { best = n; }
    }
    Ok(numeric_result(best, float))
}

pub(crate) fn builtin_min(args: Vec<Value>) -> EvalResult {
    expect_min_args("min", &args, 1)?;
    let float = has_float(&args);
    let mut best = expect_number(&args[0])?;
    for a in &args[1..] {
        let n = expect_number(a)?;
        if n < best { best = n; }
    }
    Ok(numeric_result(best, float))
}

pub(crate) fn builtin_abs(args: Vec<Value>) -> EvalResult {
    expect_args("abs", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(n.abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        other => Err(signal("wrong-type-argument", vec![Value::symbol("numberp"), other.clone()])),
    }
}

// ===========================================================================
// Logical / bitwise
// ===========================================================================

pub(crate) fn builtin_logand(args: Vec<Value>) -> EvalResult {
    let mut acc = -1i64; // all bits set
    for a in &args {
        acc &= expect_int(a)?;
    }
    Ok(Value::Int(acc))
}

pub(crate) fn builtin_logior(args: Vec<Value>) -> EvalResult {
    let mut acc = 0i64;
    for a in &args {
        acc |= expect_int(a)?;
    }
    Ok(Value::Int(acc))
}

pub(crate) fn builtin_logxor(args: Vec<Value>) -> EvalResult {
    let mut acc = 0i64;
    for a in &args {
        acc ^= expect_int(a)?;
    }
    Ok(Value::Int(acc))
}

pub(crate) fn builtin_lognot(args: Vec<Value>) -> EvalResult {
    expect_args("lognot", &args, 1)?;
    Ok(Value::Int(!expect_int(&args[0])?))
}

pub(crate) fn builtin_ash(args: Vec<Value>) -> EvalResult {
    expect_args("ash", &args, 2)?;
    let n = expect_int(&args[0])?;
    let count = expect_int(&args[1])?;
    if count >= 0 {
        Ok(Value::Int(n.checked_shl(count as u32).unwrap_or(0)))
    } else {
        Ok(Value::Int(n >> (-count).min(63) as u32))
    }
}

// ===========================================================================
// Comparisons
// ===========================================================================

pub(crate) fn builtin_num_eq(args: Vec<Value>) -> EvalResult {
    expect_min_args("=", &args, 2)?;
    let first = expect_number(&args[0])?;
    for a in &args[1..] {
        if expect_number(a)? != first {
            return Ok(Value::Nil);
        }
    }
    Ok(Value::t())
}

pub(crate) fn builtin_num_lt(args: Vec<Value>) -> EvalResult {
    expect_min_args("<", &args, 2)?;
    for pair in args.windows(2) {
        if !(expect_number(&pair[0])? < expect_number(&pair[1])?) {
            return Ok(Value::Nil);
        }
    }
    Ok(Value::t())
}

pub(crate) fn builtin_num_le(args: Vec<Value>) -> EvalResult {
    expect_min_args("<=", &args, 2)?;
    for pair in args.windows(2) {
        if !(expect_number(&pair[0])? <= expect_number(&pair[1])?) {
            return Ok(Value::Nil);
        }
    }
    Ok(Value::t())
}

pub(crate) fn builtin_num_gt(args: Vec<Value>) -> EvalResult {
    expect_min_args(">", &args, 2)?;
    for pair in args.windows(2) {
        if !(expect_number(&pair[0])? > expect_number(&pair[1])?) {
            return Ok(Value::Nil);
        }
    }
    Ok(Value::t())
}

pub(crate) fn builtin_num_ge(args: Vec<Value>) -> EvalResult {
    expect_min_args(">=", &args, 2)?;
    for pair in args.windows(2) {
        if !(expect_number(&pair[0])? >= expect_number(&pair[1])?) {
            return Ok(Value::Nil);
        }
    }
    Ok(Value::t())
}

pub(crate) fn builtin_num_ne(args: Vec<Value>) -> EvalResult {
    expect_args("/=", &args, 2)?;
    let a = expect_number(&args[0])?;
    let b = expect_number(&args[1])?;
    Ok(Value::bool(a != b))
}

// ===========================================================================
// Type predicates
// ===========================================================================

pub(crate) fn builtin_null(args: Vec<Value>) -> EvalResult {
    expect_args("null", &args, 1)?;
    Ok(Value::bool(args[0].is_nil()))
}

pub(crate) fn builtin_atom(args: Vec<Value>) -> EvalResult {
    expect_args("atom", &args, 1)?;
    Ok(Value::bool(!args[0].is_cons()))
}

pub(crate) fn builtin_consp(args: Vec<Value>) -> EvalResult {
    expect_args("consp", &args, 1)?;
    Ok(Value::bool(args[0].is_cons()))
}

pub(crate) fn builtin_listp(args: Vec<Value>) -> EvalResult {
    expect_args("listp", &args, 1)?;
    Ok(Value::bool(args[0].is_list()))
}

pub(crate) fn builtin_nlistp(args: Vec<Value>) -> EvalResult {
    expect_args("nlistp", &args, 1)?;
    Ok(Value::bool(!args[0].is_list()))
}

pub(crate) fn builtin_symbolp(args: Vec<Value>) -> EvalResult {
    expect_args("symbolp", &args, 1)?;
    Ok(Value::bool(args[0].is_symbol()))
}

pub(crate) fn builtin_numberp(args: Vec<Value>) -> EvalResult {
    expect_args("numberp", &args, 1)?;
    Ok(Value::bool(args[0].is_number()))
}

pub(crate) fn builtin_integerp(args: Vec<Value>) -> EvalResult {
    expect_args("integerp", &args, 1)?;
    Ok(Value::bool(args[0].is_integer()))
}

pub(crate) fn builtin_floatp(args: Vec<Value>) -> EvalResult {
    expect_args("floatp", &args, 1)?;
    Ok(Value::bool(args[0].is_float()))
}

pub(crate) fn builtin_stringp(args: Vec<Value>) -> EvalResult {
    expect_args("stringp", &args, 1)?;
    Ok(Value::bool(args[0].is_string()))
}

pub(crate) fn builtin_vectorp(args: Vec<Value>) -> EvalResult {
    expect_args("vectorp", &args, 1)?;
    Ok(Value::bool(args[0].is_vector()))
}

pub(crate) fn builtin_characterp(args: Vec<Value>) -> EvalResult {
    expect_args("characterp", &args, 1)?;
    Ok(Value::bool(args[0].is_char()))
}

pub(crate) fn builtin_functionp(args: Vec<Value>) -> EvalResult {
    expect_args("functionp", &args, 1)?;
    Ok(Value::bool(args[0].is_function()))
}

pub(crate) fn builtin_keywordp(args: Vec<Value>) -> EvalResult {
    expect_args("keywordp", &args, 1)?;
    Ok(Value::bool(args[0].is_keyword()))
}

pub(crate) fn builtin_hash_table_p(args: Vec<Value>) -> EvalResult {
    expect_args("hash-table-p", &args, 1)?;
    Ok(Value::bool(args[0].is_hash_table()))
}

pub(crate) fn builtin_type_of(args: Vec<Value>) -> EvalResult {
    expect_args("type-of", &args, 1)?;
    Ok(Value::symbol(args[0].type_name()))
}

pub(crate) fn builtin_sequencep(args: Vec<Value>) -> EvalResult {
    expect_args("sequencep", &args, 1)?;
    let is_seq = args[0].is_list() || args[0].is_vector() || args[0].is_string();
    Ok(Value::bool(is_seq))
}

pub(crate) fn builtin_arrayp(args: Vec<Value>) -> EvalResult {
    expect_args("arrayp", &args, 1)?;
    let is_arr = args[0].is_vector() || args[0].is_string();
    Ok(Value::bool(is_arr))
}

// ===========================================================================
// Equality
// ===========================================================================

pub(crate) fn builtin_eq(args: Vec<Value>) -> EvalResult {
    expect_args("eq", &args, 2)?;
    Ok(Value::bool(eq_value(&args[0], &args[1])))
}

pub(crate) fn builtin_eql(args: Vec<Value>) -> EvalResult {
    expect_args("eql", &args, 2)?;
    Ok(Value::bool(eql_value(&args[0], &args[1])))
}

pub(crate) fn builtin_equal(args: Vec<Value>) -> EvalResult {
    expect_args("equal", &args, 2)?;
    Ok(Value::bool(equal_value(&args[0], &args[1], 0)))
}

pub(crate) fn builtin_not(args: Vec<Value>) -> EvalResult {
    expect_args("not", &args, 1)?;
    Ok(Value::bool(args[0].is_nil()))
}

// ===========================================================================
// Cons / List operations
// ===========================================================================

pub(crate) fn builtin_cons(args: Vec<Value>) -> EvalResult {
    expect_args("cons", &args, 2)?;
    Ok(Value::cons(args[0].clone(), args[1].clone()))
}

pub(crate) fn builtin_car(args: Vec<Value>) -> EvalResult {
    expect_args("car", &args, 1)?;
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Cons(cell) => Ok(cell.lock().expect("poisoned").car.clone()),
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("listp"), args[0].clone()])),
    }
}

pub(crate) fn builtin_cdr(args: Vec<Value>) -> EvalResult {
    expect_args("cdr", &args, 1)?;
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Cons(cell) => Ok(cell.lock().expect("poisoned").cdr.clone()),
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("listp"), args[0].clone()])),
    }
}

pub(crate) fn builtin_car_safe(args: Vec<Value>) -> EvalResult {
    expect_args("car-safe", &args, 1)?;
    match &args[0] {
        Value::Cons(cell) => Ok(cell.lock().expect("poisoned").car.clone()),
        _ => Ok(Value::Nil),
    }
}

pub(crate) fn builtin_cdr_safe(args: Vec<Value>) -> EvalResult {
    expect_args("cdr-safe", &args, 1)?;
    match &args[0] {
        Value::Cons(cell) => Ok(cell.lock().expect("poisoned").cdr.clone()),
        _ => Ok(Value::Nil),
    }
}

pub(crate) fn builtin_setcar(args: Vec<Value>) -> EvalResult {
    expect_args("setcar", &args, 2)?;
    match &args[0] {
        Value::Cons(cell) => {
            cell.lock().expect("poisoned").car = args[1].clone();
            Ok(args[1].clone())
        }
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("consp"), args[0].clone()])),
    }
}

pub(crate) fn builtin_setcdr(args: Vec<Value>) -> EvalResult {
    expect_args("setcdr", &args, 2)?;
    match &args[0] {
        Value::Cons(cell) => {
            cell.lock().expect("poisoned").cdr = args[1].clone();
            Ok(args[1].clone())
        }
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("consp"), args[0].clone()])),
    }
}

pub(crate) fn builtin_list(args: Vec<Value>) -> EvalResult {
    Ok(Value::list(args))
}

pub(crate) fn builtin_length(args: Vec<Value>) -> EvalResult {
    expect_args("length", &args, 1)?;
    match &args[0] {
        Value::Nil => Ok(Value::Int(0)),
        Value::Cons(_) => {
            match list_length(&args[0]) {
                Some(n) => Ok(Value::Int(n as i64)),
                None => Err(signal("wrong-type-argument", vec![Value::symbol("listp"), args[0].clone()])),
            }
        }
        Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
        Value::Vector(v) => Ok(Value::Int(v.lock().expect("poisoned").len() as i64)),
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("sequencep"), args[0].clone()])),
    }
}

pub(crate) fn builtin_nth(args: Vec<Value>) -> EvalResult {
    expect_args("nth", &args, 2)?;
    let n = expect_int(&args[0])? as usize;
    let mut cursor = args[1].clone();
    for _ in 0..n {
        match cursor {
            Value::Cons(cell) => cursor = cell.lock().expect("poisoned").cdr.clone(),
            _ => return Ok(Value::Nil),
        }
    }
    match cursor {
        Value::Cons(cell) => Ok(cell.lock().expect("poisoned").car.clone()),
        _ => Ok(Value::Nil),
    }
}

pub(crate) fn builtin_nthcdr(args: Vec<Value>) -> EvalResult {
    expect_args("nthcdr", &args, 2)?;
    let n = expect_int(&args[0])? as usize;
    let mut cursor = args[1].clone();
    for _ in 0..n {
        match cursor {
            Value::Cons(cell) => cursor = cell.lock().expect("poisoned").cdr.clone(),
            _ => return Ok(Value::Nil),
        }
    }
    Ok(cursor)
}

pub(crate) fn builtin_append(args: Vec<Value>) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::Nil);
    }
    if args.len() == 1 {
        return Ok(args[0].clone());
    }

    // Collect all elements from all lists except the last, then use last as tail
    let mut elements: Vec<Value> = Vec::new();
    for arg in &args[..args.len() - 1] {
        if let Some(items) = list_to_vec(arg) {
            elements.extend(items);
        }
    }

    let last = &args[args.len() - 1];
    if elements.is_empty() {
        return Ok(last.clone());
    }

    // Build list with last arg as tail (supports improper lists)
    let tail = last.clone();
    Ok(elements.into_iter().rev().fold(tail, |acc, item| {
        Value::cons(item, acc)
    }))
}

pub(crate) fn builtin_reverse(args: Vec<Value>) -> EvalResult {
    expect_args("reverse", &args, 1)?;
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Cons(_) => {
            let items = list_to_vec(&args[0])
                .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("listp"), args[0].clone()]))?;
            let mut reversed = items;
            reversed.reverse();
            Ok(Value::list(reversed))
        }
        Value::Vector(v) => {
            let mut items = v.lock().expect("poisoned").clone();
            items.reverse();
            Ok(Value::vector(items))
        }
        Value::Str(s) => {
            let reversed: String = s.chars().rev().collect();
            Ok(Value::string(reversed))
        }
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("sequencep"), args[0].clone()])),
    }
}

pub(crate) fn builtin_nreverse(args: Vec<Value>) -> EvalResult {
    // For simplicity, same as reverse (we'd need to destructively modify cons cells)
    builtin_reverse(args)
}

pub(crate) fn builtin_member(args: Vec<Value>) -> EvalResult {
    expect_args("member", &args, 2)?;
    let target = &args[0];
    let mut cursor = args[1].clone();
    loop {
        match cursor {
            Value::Nil => return Ok(Value::Nil),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if equal_value(target, &pair.car, 0) {
                    drop(pair);
                    return Ok(Value::Cons(cell));
                }
                cursor = pair.cdr.clone();
            }
            _ => return Ok(Value::Nil),
        }
    }
}

pub(crate) fn builtin_memq(args: Vec<Value>) -> EvalResult {
    expect_args("memq", &args, 2)?;
    let target = &args[0];
    let mut cursor = args[1].clone();
    loop {
        match cursor {
            Value::Nil => return Ok(Value::Nil),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if eq_value(target, &pair.car) {
                    drop(pair);
                    return Ok(Value::Cons(cell));
                }
                cursor = pair.cdr.clone();
            }
            _ => return Ok(Value::Nil),
        }
    }
}

pub(crate) fn builtin_assoc(args: Vec<Value>) -> EvalResult {
    expect_args("assoc", &args, 2)?;
    let key = &args[0];
    let mut cursor = args[1].clone();
    loop {
        match cursor {
            Value::Nil => return Ok(Value::Nil),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if let Value::Cons(ref entry) = pair.car {
                    let entry_pair = entry.lock().expect("poisoned");
                    if equal_value(key, &entry_pair.car, 0) {
                        return Ok(pair.car.clone());
                    }
                }
                cursor = pair.cdr.clone();
            }
            _ => return Ok(Value::Nil),
        }
    }
}

pub(crate) fn builtin_assq(args: Vec<Value>) -> EvalResult {
    expect_args("assq", &args, 2)?;
    let key = &args[0];
    let mut cursor = args[1].clone();
    loop {
        match cursor {
            Value::Nil => return Ok(Value::Nil),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if let Value::Cons(ref entry) = pair.car {
                    let entry_pair = entry.lock().expect("poisoned");
                    if eq_value(key, &entry_pair.car) {
                        return Ok(pair.car.clone());
                    }
                }
                cursor = pair.cdr.clone();
            }
            _ => return Ok(Value::Nil),
        }
    }
}

pub(crate) fn builtin_copy_sequence(args: Vec<Value>) -> EvalResult {
    expect_args("copy-sequence", &args, 1)?;
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Cons(_) => {
            let items = list_to_vec(&args[0])
                .ok_or_else(|| signal("wrong-type-argument", vec![]))?;
            Ok(Value::list(items))
        }
        Value::Str(s) => Ok(Value::string((**s).clone())),
        Value::Vector(v) => Ok(Value::vector(v.lock().expect("poisoned").clone())),
        other => Err(signal("wrong-type-argument", vec![Value::symbol("sequencep"), other.clone()])),
    }
}

// ===========================================================================
// String operations
// ===========================================================================

pub(crate) fn builtin_string_equal(args: Vec<Value>) -> EvalResult {
    expect_args("string-equal", &args, 2)?;
    let a = expect_string(&args[0])?;
    let b = expect_string(&args[1])?;
    Ok(Value::bool(a == b))
}

pub(crate) fn builtin_string_lessp(args: Vec<Value>) -> EvalResult {
    expect_args("string-lessp", &args, 2)?;
    let a = expect_string(&args[0])?;
    let b = expect_string(&args[1])?;
    Ok(Value::bool(a < b))
}

pub(crate) fn builtin_substring(args: Vec<Value>) -> EvalResult {
    expect_min_args("substring", &args, 1)?;
    let s = expect_string(&args[0])?;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;

    let from = if args.len() > 1 {
        let n = expect_int(&args[1])?;
        if n < 0 { (len + n).max(0) as usize } else { n as usize }
    } else {
        0
    };

    let to = if args.len() > 2 {
        if args[2].is_nil() {
            chars.len()
        } else {
            let n = expect_int(&args[2])?;
            if n < 0 { (len + n).max(0) as usize } else { n as usize }
        }
    } else {
        chars.len()
    };

    let from = from.min(chars.len());
    let to = to.min(chars.len());
    if from > to {
        return Ok(Value::string(""));
    }
    let result: String = chars[from..to].iter().collect();
    Ok(Value::string(result))
}

pub(crate) fn builtin_concat(args: Vec<Value>) -> EvalResult {
    let mut result = String::new();
    for arg in &args {
        match arg {
            Value::Str(s) => result.push_str(s),
            Value::Nil => {}
            Value::Cons(_) => {
                if let Some(items) = list_to_vec(arg) {
                    for item in items {
                        if let Value::Char(c) = item {
                            result.push(c);
                        } else if let Value::Int(n) = item {
                            if let Some(c) = char::from_u32(n as u32) {
                                result.push(c);
                            }
                        }
                    }
                }
            }
            Value::Vector(v) => {
                let items = v.lock().expect("poisoned");
                for item in items.iter() {
                    if let Value::Char(c) = item {
                        result.push(*c);
                    } else if let Value::Int(n) = item {
                        if let Some(c) = char::from_u32(*n as u32) {
                            result.push(c);
                        }
                    }
                }
            }
            _ => return Err(signal("wrong-type-argument", vec![Value::symbol("sequencep"), arg.clone()])),
        }
    }
    Ok(Value::string(result))
}

pub(crate) fn builtin_string_to_number(args: Vec<Value>) -> EvalResult {
    expect_min_args("string-to-number", &args, 1)?;
    let s = expect_string(&args[0])?;
    let base = if args.len() > 1 { expect_int(&args[1])? as u32 } else { 10 };

    let s = s.trim();
    if base == 10 {
        if let Ok(n) = s.parse::<i64>() {
            return Ok(Value::Int(n));
        }
        if let Ok(f) = s.parse::<f64>() {
            return Ok(Value::Float(f));
        }
    } else if let Ok(n) = i64::from_str_radix(s, base) {
        return Ok(Value::Int(n));
    }
    Ok(Value::Int(0))
}

pub(crate) fn builtin_number_to_string(args: Vec<Value>) -> EvalResult {
    expect_args("number-to-string", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::string(n.to_string())),
        Value::Float(f) => Ok(Value::string(format!("{}", f))),
        other => Err(signal("wrong-type-argument", vec![Value::symbol("numberp"), other.clone()])),
    }
}

pub(crate) fn builtin_upcase(args: Vec<Value>) -> EvalResult {
    expect_args("upcase", &args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(Value::string(s.to_uppercase())),
        Value::Char(c) => Ok(Value::Char(c.to_uppercase().next().unwrap_or(*c))),
        Value::Int(n) => {
            if let Some(c) = char::from_u32(*n as u32) {
                Ok(Value::Int(c.to_uppercase().next().unwrap_or(c) as i64))
            } else {
                Ok(Value::Int(*n))
            }
        }
        other => Err(signal("wrong-type-argument", vec![Value::symbol("char-or-string-p"), other.clone()])),
    }
}

pub(crate) fn builtin_downcase(args: Vec<Value>) -> EvalResult {
    expect_args("downcase", &args, 1)?;
    match &args[0] {
        Value::Str(s) => Ok(Value::string(s.to_lowercase())),
        Value::Char(c) => Ok(Value::Char(c.to_lowercase().next().unwrap_or(*c))),
        Value::Int(n) => {
            if let Some(c) = char::from_u32(*n as u32) {
                Ok(Value::Int(c.to_lowercase().next().unwrap_or(c) as i64))
            } else {
                Ok(Value::Int(*n))
            }
        }
        other => Err(signal("wrong-type-argument", vec![Value::symbol("char-or-string-p"), other.clone()])),
    }
}

pub(crate) fn builtin_format(args: Vec<Value>) -> EvalResult {
    expect_min_args("format", &args, 1)?;
    let fmt_str = expect_string(&args[0])?;
    let mut result = String::new();
    let mut arg_idx = 1;
    let mut chars = fmt_str.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '%' {
            if let Some(&spec) = chars.peek() {
                chars.next();
                match spec {
                    's' => {
                        if arg_idx < args.len() {
                            match &args[arg_idx] {
                                Value::Str(s) => result.push_str(s),
                                other => result.push_str(&super::print::print_value(other)),
                            }
                            arg_idx += 1;
                        }
                    }
                    'S' => {
                        if arg_idx < args.len() {
                            result.push_str(&super::print::print_value(&args[arg_idx]));
                            arg_idx += 1;
                        }
                    }
                    'd' => {
                        if arg_idx < args.len() {
                            if let Ok(n) = expect_int(&args[arg_idx]) {
                                result.push_str(&n.to_string());
                            }
                            arg_idx += 1;
                        }
                    }
                    'f' => {
                        if arg_idx < args.len() {
                            if let Ok(f) = expect_number(&args[arg_idx]) {
                                result.push_str(&format!("{:.6}", f));
                            }
                            arg_idx += 1;
                        }
                    }
                    'c' => {
                        if arg_idx < args.len() {
                            if let Ok(n) = expect_int(&args[arg_idx]) {
                                if let Some(c) = char::from_u32(n as u32) {
                                    result.push(c);
                                }
                            }
                            arg_idx += 1;
                        }
                    }
                    '%' => result.push('%'),
                    _ => {
                        result.push('%');
                        result.push(spec);
                    }
                }
            } else {
                result.push('%');
            }
        } else {
            result.push(ch);
        }
    }

    Ok(Value::string(result))
}

// ===========================================================================
// Vector operations
// ===========================================================================

pub(crate) fn builtin_make_vector(args: Vec<Value>) -> EvalResult {
    expect_args("make-vector", &args, 2)?;
    let len = expect_int(&args[0])? as usize;
    Ok(Value::vector(vec![args[1].clone(); len]))
}

pub(crate) fn builtin_vector(args: Vec<Value>) -> EvalResult {
    Ok(Value::vector(args))
}

pub(crate) fn builtin_aref(args: Vec<Value>) -> EvalResult {
    expect_args("aref", &args, 2)?;
    let idx = expect_int(&args[1])? as usize;
    match &args[0] {
        Value::Vector(v) => {
            let items = v.lock().expect("poisoned");
            items.get(idx).cloned().ok_or_else(|| {
                signal("args-out-of-range", vec![args[0].clone(), args[1].clone()])
            })
        }
        Value::Str(s) => {
            s.chars().nth(idx).map(Value::Char).ok_or_else(|| {
                signal("args-out-of-range", vec![args[0].clone(), args[1].clone()])
            })
        }
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("arrayp"), args[0].clone()])),
    }
}

pub(crate) fn builtin_aset(args: Vec<Value>) -> EvalResult {
    expect_args("aset", &args, 3)?;
    let idx = expect_int(&args[1])? as usize;
    match &args[0] {
        Value::Vector(v) => {
            let mut items = v.lock().expect("poisoned");
            if idx >= items.len() {
                return Err(signal("args-out-of-range", vec![args[0].clone(), args[1].clone()]));
            }
            items[idx] = args[2].clone();
            Ok(args[2].clone())
        }
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("arrayp"), args[0].clone()])),
    }
}

pub(crate) fn builtin_vconcat(args: Vec<Value>) -> EvalResult {
    let mut result = Vec::new();
    for arg in &args {
        match arg {
            Value::Vector(v) => result.extend(v.lock().expect("poisoned").iter().cloned()),
            Value::Nil => {}
            Value::Cons(_) => {
                if let Some(items) = list_to_vec(arg) {
                    result.extend(items);
                }
            }
            _ => return Err(signal("wrong-type-argument", vec![Value::symbol("sequencep"), arg.clone()])),
        }
    }
    Ok(Value::vector(result))
}

// ===========================================================================
// Hash table operations
// ===========================================================================

pub(crate) fn builtin_make_hash_table(args: Vec<Value>) -> EvalResult {
    let mut test = HashTableTest::Eql;
    let mut i = 0;
    while i < args.len() {
        if let Value::Keyword(ref k) = args[i] {
            if k == ":test" && i + 1 < args.len() {
                test = match args[i + 1].as_symbol_name() {
                    Some("eq") => HashTableTest::Eq,
                    Some("eql") => HashTableTest::Eql,
                    Some("equal") => HashTableTest::Equal,
                    _ => HashTableTest::Eql,
                };
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    Ok(Value::hash_table(test))
}

pub(crate) fn builtin_gethash(args: Vec<Value>) -> EvalResult {
    expect_min_args("gethash", &args, 2)?;
    let default = if args.len() > 2 { args[2].clone() } else { Value::Nil };
    match &args[1] {
        Value::HashTable(ht) => {
            let ht = ht.lock().expect("poisoned");
            let key = args[0].to_hash_key(&ht.test);
            Ok(ht.data.get(&key).cloned().unwrap_or(default))
        }
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("hash-table-p"), args[1].clone()])),
    }
}

pub(crate) fn builtin_puthash(args: Vec<Value>) -> EvalResult {
    expect_args("puthash", &args, 3)?;
    match &args[2] {
        Value::HashTable(ht) => {
            let mut ht = ht.lock().expect("poisoned");
            let key = args[0].to_hash_key(&ht.test);
            ht.data.insert(key, args[1].clone());
            Ok(args[1].clone())
        }
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("hash-table-p"), args[2].clone()])),
    }
}

pub(crate) fn builtin_remhash(args: Vec<Value>) -> EvalResult {
    expect_args("remhash", &args, 2)?;
    match &args[1] {
        Value::HashTable(ht) => {
            let mut ht = ht.lock().expect("poisoned");
            let key = args[0].to_hash_key(&ht.test);
            ht.data.remove(&key);
            Ok(Value::Nil)
        }
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("hash-table-p"), args[1].clone()])),
    }
}

pub(crate) fn builtin_clrhash(args: Vec<Value>) -> EvalResult {
    expect_args("clrhash", &args, 1)?;
    match &args[0] {
        Value::HashTable(ht) => {
            ht.lock().expect("poisoned").data.clear();
            Ok(Value::Nil)
        }
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("hash-table-p"), args[0].clone()])),
    }
}

pub(crate) fn builtin_hash_table_count(args: Vec<Value>) -> EvalResult {
    expect_args("hash-table-count", &args, 1)?;
    match &args[0] {
        Value::HashTable(ht) => Ok(Value::Int(ht.lock().expect("poisoned").data.len() as i64)),
        _ => Err(signal("wrong-type-argument", vec![Value::symbol("hash-table-p"), args[0].clone()])),
    }
}

// ===========================================================================
// Conversion
// ===========================================================================

pub(crate) fn builtin_float(args: Vec<Value>) -> EvalResult {
    expect_args("float", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Float(*n as f64)),
        Value::Float(f) => Ok(Value::Float(*f)),
        other => Err(signal("wrong-type-argument", vec![Value::symbol("numberp"), other.clone()])),
    }
}

pub(crate) fn builtin_truncate(args: Vec<Value>) -> EvalResult {
    expect_args("truncate", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(f) => Ok(Value::Int(*f as i64)),
        other => Err(signal("wrong-type-argument", vec![Value::symbol("numberp"), other.clone()])),
    }
}

pub(crate) fn builtin_floor(args: Vec<Value>) -> EvalResult {
    expect_args("floor", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(f) => Ok(Value::Int(f.floor() as i64)),
        other => Err(signal("wrong-type-argument", vec![Value::symbol("numberp"), other.clone()])),
    }
}

pub(crate) fn builtin_ceiling(args: Vec<Value>) -> EvalResult {
    expect_args("ceiling", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(f) => Ok(Value::Int(f.ceil() as i64)),
        other => Err(signal("wrong-type-argument", vec![Value::symbol("numberp"), other.clone()])),
    }
}

pub(crate) fn builtin_round(args: Vec<Value>) -> EvalResult {
    expect_args("round", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(f) => Ok(Value::Int(f.round() as i64)),
        other => Err(signal("wrong-type-argument", vec![Value::symbol("numberp"), other.clone()])),
    }
}

pub(crate) fn builtin_char_to_string(args: Vec<Value>) -> EvalResult {
    expect_args("char-to-string", &args, 1)?;
    match &args[0] {
        Value::Char(c) => Ok(Value::string(c.to_string())),
        Value::Int(n) => {
            char::from_u32(*n as u32)
                .map(|c| Value::string(c.to_string()))
                .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("characterp"), args[0].clone()]))
        }
        other => Err(signal("wrong-type-argument", vec![Value::symbol("characterp"), other.clone()])),
    }
}

pub(crate) fn builtin_string_to_char(args: Vec<Value>) -> EvalResult {
    expect_args("string-to-char", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::Int(s.chars().next().map(|c| c as i64).unwrap_or(0)))
}

// ===========================================================================
// Property lists
// ===========================================================================

pub(crate) fn builtin_plist_get(args: Vec<Value>) -> EvalResult {
    expect_args("plist-get", &args, 2)?;
    let mut cursor = args[0].clone();
    loop {
        match cursor {
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                if eq_value(&pair.car, &args[1]) {
                    // Next element is the value
                    match &pair.cdr {
                        Value::Cons(val_cell) => {
                            return Ok(val_cell.lock().expect("poisoned").car.clone());
                        }
                        _ => return Ok(Value::Nil),
                    }
                }
                // Skip the value entry
                match &pair.cdr {
                    Value::Cons(val_cell) => {
                        cursor = val_cell.lock().expect("poisoned").cdr.clone();
                    }
                    _ => return Ok(Value::Nil),
                }
            }
            _ => return Ok(Value::Nil),
        }
    }
}

pub(crate) fn builtin_plist_put(args: Vec<Value>) -> EvalResult {
    expect_args("plist-put", &args, 3)?;
    // Simple implementation: rebuild plist with new key/value
    let plist = &args[0];
    let key = &args[1];
    let new_val = &args[2];

    let mut items = Vec::new();
    let mut found = false;
    let mut cursor = plist.clone();
    loop {
        match cursor {
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                let k = pair.car.clone();
                match &pair.cdr {
                    Value::Cons(val_cell) => {
                        let val_pair = val_cell.lock().expect("poisoned");
                        if eq_value(&k, key) {
                            items.push(k);
                            items.push(new_val.clone());
                            found = true;
                        } else {
                            items.push(k);
                            items.push(val_pair.car.clone());
                        }
                        cursor = val_pair.cdr.clone();
                    }
                    _ => break,
                }
            }
            _ => break,
        }
    }

    if !found {
        items.push(key.clone());
        items.push(new_val.clone());
    }

    Ok(Value::list(items))
}

// ===========================================================================
// Misc
// ===========================================================================

pub(crate) fn builtin_identity(args: Vec<Value>) -> EvalResult {
    expect_args("identity", &args, 1)?;
    Ok(args[0].clone())
}

pub(crate) fn builtin_message(args: Vec<Value>) -> EvalResult {
    expect_min_args("message", &args, 1)?;
    let msg = if args.len() == 1 {
        match &args[0] {
            Value::Str(s) => (**s).clone(),
            other => super::print::print_value(other),
        }
    } else {
        // Use format
        match builtin_format(args.clone())? {
            Value::Str(s) => (*s).clone(),
            _ => String::new(),
        }
    };
    eprintln!("{}", msg);
    Ok(Value::string(msg))
}

pub(crate) fn builtin_error(args: Vec<Value>) -> EvalResult {
    expect_min_args("error", &args, 1)?;
    let msg = match builtin_format(args)? {
        Value::Str(s) => (*s).clone(),
        _ => "error".to_string(),
    };
    Err(signal("error", vec![Value::string(msg)]))
}

pub(crate) fn builtin_symbol_name(args: Vec<Value>) -> EvalResult {
    expect_args("symbol-name", &args, 1)?;
    match args[0].as_symbol_name() {
        Some(name) => Ok(Value::string(name)),
        None => Err(signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()])),
    }
}

pub(crate) fn builtin_make_symbol(args: Vec<Value>) -> EvalResult {
    expect_args("make-symbol", &args, 1)?;
    let name = expect_string(&args[0])?;
    Ok(Value::Symbol(name))
}

pub(crate) fn builtin_apply(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    if args.len() < 2 {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("apply"), Value::Int(args.len() as i64)]));
    }
    let func = args[0].clone();
    let last = &args[args.len() - 1];
    let mut call_args: Vec<Value> = args[1..args.len() - 1].to_vec();

    // Last argument must be a list, which gets spread
    match last {
        Value::Nil => {}
        Value::Cons(_) => {
            if let Some(items) = list_to_vec(last) {
                call_args.extend(items);
            }
        }
        _ => return Err(signal("wrong-type-argument", vec![Value::symbol("listp"), last.clone()])),
    }

    eval.apply(func, call_args)
}

// ===========================================================================
// Higher-order
// ===========================================================================

pub(crate) fn builtin_mapcar(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    if args.len() != 2 {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("mapcar"), Value::Int(args.len() as i64)]));
    }
    let func = args[0].clone();
    let mut results = Vec::new();
    let mut cursor = args[1].clone();
    loop {
        match cursor {
            Value::Nil => break,
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                let item = pair.car.clone();
                cursor = pair.cdr.clone();
                drop(pair);
                results.push(eval.apply(func.clone(), vec![item])?);
            }
            _ => return Err(signal("wrong-type-argument", vec![Value::symbol("listp"), cursor])),
        }
    }
    Ok(Value::list(results))
}

pub(crate) fn builtin_mapc(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    if args.len() != 2 {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("mapc"), Value::Int(args.len() as i64)]));
    }
    let func = args[0].clone();
    let list_val = args[1].clone();
    let mut cursor = list_val.clone();
    loop {
        match cursor {
            Value::Nil => break,
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                let item = pair.car.clone();
                cursor = pair.cdr.clone();
                drop(pair);
                eval.apply(func.clone(), vec![item])?;
            }
            _ => return Err(signal("wrong-type-argument", vec![Value::symbol("listp"), cursor])),
        }
    }
    Ok(list_val)
}

pub(crate) fn builtin_sort(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    if args.len() != 2 {
        return Err(signal("wrong-number-of-arguments", vec![Value::symbol("sort"), Value::Int(args.len() as i64)]));
    }
    let pred = args[1].clone();
    let mut items = list_to_vec(&args[0])
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("listp"), args[0].clone()]))?;

    // Simple insertion sort (stable sort with predicate)
    // We can't use sort_by because the predicate can fail
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
// Helpers
// ===========================================================================

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

// ===========================================================================
// Symbol operations (need evaluator for obarray access)
// ===========================================================================

pub(crate) fn builtin_boundp(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("boundp", &args, 1)?;
    let name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    Ok(Value::bool(eval.obarray().boundp(name)))
}

pub(crate) fn builtin_fboundp(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("fboundp", &args, 1)?;
    let name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    Ok(Value::bool(eval.obarray().fboundp(name)))
}

pub(crate) fn builtin_symbol_value(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("symbol-value", &args, 1)?;
    let name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    // Check dynamic bindings first
    for frame in eval.dynamic.iter().rev() {
        if let Some(value) = frame.get(name) {
            return Ok(value.clone());
        }
    }
    eval.obarray().symbol_value(name).cloned()
        .ok_or_else(|| signal("void-variable", vec![Value::symbol(name)]))
}

pub(crate) fn builtin_symbol_function(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("symbol-function", &args, 1)?;
    let name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    eval.obarray().symbol_function(name).cloned()
        .ok_or_else(|| signal("void-function", vec![Value::symbol(name)]))
}

pub(crate) fn builtin_set(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("set", &args, 2)?;
    let name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    let value = args[1].clone();
    eval.assign(name, value.clone());
    Ok(value)
}

pub(crate) fn builtin_fset(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("fset", &args, 2)?;
    let name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    let def = args[1].clone();
    eval.obarray_mut().set_symbol_function(name, def.clone());
    Ok(def)
}

pub(crate) fn builtin_makunbound(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("makunbound", &args, 1)?;
    let name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    eval.obarray_mut().makunbound(name);
    Ok(args[0].clone())
}

pub(crate) fn builtin_fmakunbound(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("fmakunbound", &args, 1)?;
    let name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    eval.obarray_mut().fmakunbound(name);
    Ok(args[0].clone())
}

pub(crate) fn builtin_get(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("get", &args, 2)?;
    let sym = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    let prop = args[1].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[1].clone()]))?;
    Ok(eval.obarray().get_property(sym, prop).cloned().unwrap_or(Value::Nil))
}

pub(crate) fn builtin_put(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("put", &args, 3)?;
    let sym = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    let prop = args[1].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[1].clone()]))?;
    let value = args[2].clone();
    eval.obarray_mut().put_property(sym, prop, value.clone());
    Ok(value)
}

pub(crate) fn builtin_symbol_plist_fn(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("symbol-plist", &args, 1)?;
    let name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    Ok(eval.obarray().symbol_plist(name))
}

pub(crate) fn builtin_indirect_function(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("indirect-function", &args, 1)?;
    let name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    Ok(eval.obarray().indirect_function(name).unwrap_or(Value::Nil))
}

pub(crate) fn builtin_intern_fn(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("intern", &args, 1)?;
    let name = expect_string(&args[0])?;
    eval.obarray_mut().intern(&name);
    Ok(Value::symbol(name))
}

pub(crate) fn builtin_intern_soft(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("intern-soft", &args, 1)?;
    let name = expect_string(&args[0])?;
    if eval.obarray().intern_soft(&name).is_some() {
        Ok(Value::symbol(name))
    } else {
        Ok(Value::Nil)
    }
}

// ===========================================================================
// Hook system (need evaluator)
// ===========================================================================

pub(crate) fn builtin_add_hook(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("add-hook", &args, 2)?;
    let hook_name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?
        .to_string();
    let function = args[1].clone();
    let append = args.get(2).is_some_and(|v| v.is_truthy());

    // Get current hook value
    let current = eval.obarray().symbol_value(&hook_name).cloned().unwrap_or(Value::Nil);
    let mut items = list_to_vec(&current).unwrap_or_default();

    // Don't add duplicates
    if !items.iter().any(|v| eq_value(v, &function)) {
        if append {
            items.push(function);
        } else {
            items.insert(0, function);
        }
    }

    eval.obarray_mut().set_symbol_value(&hook_name, Value::list(items));
    Ok(Value::Nil)
}

pub(crate) fn builtin_remove_hook(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("remove-hook", &args, 2)?;
    let hook_name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?
        .to_string();
    let function = args[1].clone();

    let current = eval.obarray().symbol_value(&hook_name).cloned().unwrap_or(Value::Nil);
    let items = list_to_vec(&current).unwrap_or_default();
    let filtered: Vec<Value> = items.into_iter().filter(|v| !eq_value(v, &function)).collect();
    eval.obarray_mut().set_symbol_value(&hook_name, Value::list(filtered));
    Ok(Value::Nil)
}

pub(crate) fn builtin_run_hooks(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    for hook_sym in &args {
        let hook_name = hook_sym.as_symbol_name()
            .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), hook_sym.clone()]))?;
        let hook_val = eval.obarray().symbol_value(hook_name).cloned().unwrap_or(Value::Nil);
        let fns = list_to_vec(&hook_val).unwrap_or_default();
        for func in fns {
            eval.apply(func, vec![])?;
        }
    }
    Ok(Value::Nil)
}

pub(crate) fn builtin_run_hook_with_args(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("run-hook-with-args", &args, 1)?;
    let hook_name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    let hook_args: Vec<Value> = args[1..].to_vec();
    let hook_val = eval.obarray().symbol_value(hook_name).cloned().unwrap_or(Value::Nil);
    let fns = list_to_vec(&hook_val).unwrap_or_default();
    for func in fns {
        eval.apply(func, hook_args.clone())?;
    }
    Ok(Value::Nil)
}

pub(crate) fn builtin_featurep(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("featurep", &args, 1)?;
    let name = args[0].as_symbol_name()
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("symbolp"), args[0].clone()]))?;
    Ok(Value::bool(eval.features.contains(&name.to_string())))
}

// ===========================================================================
// Loading / eval
// ===========================================================================

/// Convert an EvalError back to a Flow for builtins that call load_file.
fn eval_error_to_flow(e: super::error::EvalError) -> Flow {
    match e {
        super::error::EvalError::Signal { symbol, data } => {
            Flow::Signal(super::error::SignalData { symbol, data })
        }
        super::error::EvalError::UncaughtThrow { tag, value } => {
            Flow::Throw { tag, value }
        }
    }
}

pub(crate) fn builtin_load(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("load", &args, 1)?;
    let file = expect_string(&args[0])?;
    let noerror = args.get(1).is_some_and(|v| v.is_truthy());

    let load_path = super::load::get_load_path(&eval.obarray);
    match super::load::find_file_in_load_path(&file, &load_path) {
        Some(path) => {
            super::load::load_file(eval, &path).map_err(eval_error_to_flow)
        }
        None => {
            // Try as absolute path
            let path = std::path::Path::new(&file);
            if path.exists() {
                super::load::load_file(eval, path).map_err(eval_error_to_flow)
            } else if noerror {
                Ok(Value::Nil)
            } else {
                Err(signal("file-missing", vec![
                    Value::string(format!("Cannot open load file: {}", file))
                ]))
            }
        }
    }
}

pub(crate) fn builtin_load_file(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("load-file", &args, 1)?;
    let file = expect_string(&args[0])?;
    let path = std::path::Path::new(&file);
    super::load::load_file(eval, path).map_err(eval_error_to_flow)
}

pub(crate) fn builtin_eval(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("eval", &args, 1)?;
    // Convert value back to expr and evaluate
    let expr = super::eval::value_to_expr_pub(&args[0]);
    eval.eval(&expr) // eval.eval() already returns EvalResult = Result<Value, Flow>
}

// ===========================================================================
// Math functions (pure)
// ===========================================================================

pub(crate) fn builtin_sqrt(args: Vec<Value>) -> EvalResult {
    expect_args("sqrt", &args, 1)?;
    Ok(Value::Float(expect_number(&args[0])?.sqrt()))
}

pub(crate) fn builtin_sin(args: Vec<Value>) -> EvalResult {
    expect_args("sin", &args, 1)?;
    Ok(Value::Float(expect_number(&args[0])?.sin()))
}

pub(crate) fn builtin_cos(args: Vec<Value>) -> EvalResult {
    expect_args("cos", &args, 1)?;
    Ok(Value::Float(expect_number(&args[0])?.cos()))
}

pub(crate) fn builtin_tan(args: Vec<Value>) -> EvalResult {
    expect_args("tan", &args, 1)?;
    Ok(Value::Float(expect_number(&args[0])?.tan()))
}

pub(crate) fn builtin_asin(args: Vec<Value>) -> EvalResult {
    expect_args("asin", &args, 1)?;
    Ok(Value::Float(expect_number(&args[0])?.asin()))
}

pub(crate) fn builtin_acos(args: Vec<Value>) -> EvalResult {
    expect_args("acos", &args, 1)?;
    Ok(Value::Float(expect_number(&args[0])?.acos()))
}

pub(crate) fn builtin_atan(args: Vec<Value>) -> EvalResult {
    expect_min_args("atan", &args, 1)?;
    if args.len() == 2 {
        let y = expect_number(&args[0])?;
        let x = expect_number(&args[1])?;
        Ok(Value::Float(y.atan2(x)))
    } else {
        Ok(Value::Float(expect_number(&args[0])?.atan()))
    }
}

pub(crate) fn builtin_exp(args: Vec<Value>) -> EvalResult {
    expect_args("exp", &args, 1)?;
    Ok(Value::Float(expect_number(&args[0])?.exp()))
}

pub(crate) fn builtin_log(args: Vec<Value>) -> EvalResult {
    expect_min_args("log", &args, 1)?;
    let val = expect_number(&args[0])?;
    if args.len() == 2 {
        let base = expect_number(&args[1])?;
        Ok(Value::Float(val.ln() / base.ln()))
    } else {
        Ok(Value::Float(val.ln()))
    }
}

pub(crate) fn builtin_expt(args: Vec<Value>) -> EvalResult {
    expect_args("expt", &args, 2)?;
    if has_float(&args) {
        let base = expect_number(&args[0])?;
        let exp = expect_number(&args[1])?;
        Ok(Value::Float(base.powf(exp)))
    } else {
        let base = expect_int(&args[0])?;
        let exp = expect_int(&args[1])?;
        if exp < 0 {
            Ok(Value::Float((base as f64).powf(exp as f64)))
        } else {
            Ok(Value::Int(base.wrapping_pow(exp as u32)))
        }
    }
}

pub(crate) fn builtin_random(args: Vec<Value>) -> EvalResult {
    if args.is_empty() {
        // Random integer
        Ok(Value::Int(rand_simple()))
    } else {
        let limit = expect_int(&args[0])?;
        if limit <= 0 {
            return Err(signal("args-out-of-range", vec![args[0].clone()]));
        }
        Ok(Value::Int(rand_simple().unsigned_abs() as i64 % limit))
    }
}

/// Simple PRNG (xorshift64) — not cryptographically secure.
fn rand_simple() -> i64 {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u64> = Cell::new(0x12345678_9abcdef0);
    }
    STATE.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        x as i64
    })
}

pub(crate) fn builtin_isnan(args: Vec<Value>) -> EvalResult {
    expect_args("isnan", &args, 1)?;
    match &args[0] {
        Value::Float(f) => Ok(Value::bool(f.is_nan())),
        _ => Ok(Value::Nil),
    }
}

// ===========================================================================
// Extended string operations
// ===========================================================================

pub(crate) fn builtin_string_prefix_p(args: Vec<Value>) -> EvalResult {
    expect_args("string-prefix-p", &args, 2)?;
    let prefix = expect_string(&args[0])?;
    let s = expect_string(&args[1])?;
    Ok(Value::bool(s.starts_with(&prefix)))
}

pub(crate) fn builtin_string_suffix_p(args: Vec<Value>) -> EvalResult {
    expect_args("string-suffix-p", &args, 2)?;
    let suffix = expect_string(&args[0])?;
    let s = expect_string(&args[1])?;
    Ok(Value::bool(s.ends_with(&suffix)))
}

pub(crate) fn builtin_string_join(args: Vec<Value>) -> EvalResult {
    expect_args("string-join", &args, 2)?;
    let strs = list_to_vec(&args[0])
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("listp"), args[0].clone()]))?;
    let sep = expect_string(&args[1])?;
    let parts: Result<Vec<String>, _> = strs.iter().map(expect_string).collect();
    Ok(Value::string(parts?.join(&sep)))
}

pub(crate) fn builtin_split_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("split-string", &args, 1)?;
    let s = expect_string(&args[0])?;
    let sep = if args.len() > 1 {
        expect_string(&args[1])?
    } else {
        "[ \t\n\r]+".to_string()
    };
    // Simple string split (not regex for now)
    let parts: Vec<Value> = s.split(&sep)
        .filter(|p| !p.is_empty())
        .map(|p| Value::string(p.to_string()))
        .collect();
    Ok(Value::list(parts))
}

pub(crate) fn builtin_string_trim(args: Vec<Value>) -> EvalResult {
    expect_args("string-trim", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::string(s.trim().to_string()))
}

pub(crate) fn builtin_string_trim_left(args: Vec<Value>) -> EvalResult {
    expect_args("string-trim-left", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::string(s.trim_start().to_string()))
}

pub(crate) fn builtin_string_trim_right(args: Vec<Value>) -> EvalResult {
    expect_args("string-trim-right", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::string(s.trim_end().to_string()))
}

pub(crate) fn builtin_make_string(args: Vec<Value>) -> EvalResult {
    expect_args("make-string", &args, 2)?;
    let n = expect_int(&args[0])? as usize;
    let ch = match &args[1] {
        Value::Int(c) => char::from_u32(*c as u32).unwrap_or(' '),
        Value::Char(c) => *c,
        other => return Err(signal("wrong-type-argument", vec![Value::symbol("characterp"), other.clone()])),
    };
    Ok(Value::string(std::iter::repeat(ch).take(n).collect::<String>()))
}

pub(crate) fn builtin_string_to_list(args: Vec<Value>) -> EvalResult {
    expect_args("string-to-list", &args, 1)?;
    let s = expect_string(&args[0])?;
    let chars: Vec<Value> = s.chars().map(Value::Char).collect();
    Ok(Value::list(chars))
}

pub(crate) fn builtin_string_width(args: Vec<Value>) -> EvalResult {
    expect_args("string-width", &args, 1)?;
    let s = expect_string(&args[0])?;
    // Simple: count chars (proper Unicode width would need unicode-width crate)
    Ok(Value::Int(s.chars().count() as i64))
}

// ===========================================================================
// Extended list operations
// ===========================================================================

pub(crate) fn builtin_last(args: Vec<Value>) -> EvalResult {
    expect_min_args("last", &args, 1)?;
    let n = if args.len() > 1 { expect_int(&args[1])? as usize } else { 1 };
    let items = list_to_vec(&args[0])
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("listp"), args[0].clone()]))?;
    if n >= items.len() {
        Ok(args[0].clone())
    } else {
        Ok(Value::list(items[items.len() - n..].to_vec()))
    }
}

pub(crate) fn builtin_butlast(args: Vec<Value>) -> EvalResult {
    expect_min_args("butlast", &args, 1)?;
    let n = if args.len() > 1 { expect_int(&args[1])? as usize } else { 1 };
    let items = list_to_vec(&args[0])
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("listp"), args[0].clone()]))?;
    if n >= items.len() {
        Ok(Value::Nil)
    } else {
        Ok(Value::list(items[..items.len() - n].to_vec()))
    }
}

pub(crate) fn builtin_delete(args: Vec<Value>) -> EvalResult {
    expect_args("delete", &args, 2)?;
    let elt = &args[0];
    let items = list_to_vec(&args[1])
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("listp"), args[1].clone()]))?;
    let filtered: Vec<Value> = items.into_iter()
        .filter(|v| !equal_value(elt, v, 0))
        .collect();
    Ok(Value::list(filtered))
}

pub(crate) fn builtin_delq(args: Vec<Value>) -> EvalResult {
    expect_args("delq", &args, 2)?;
    let elt = &args[0];
    let items = list_to_vec(&args[1])
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("listp"), args[1].clone()]))?;
    let filtered: Vec<Value> = items.into_iter()
        .filter(|v| !eq_value(elt, v))
        .collect();
    Ok(Value::list(filtered))
}

pub(crate) fn builtin_elt(args: Vec<Value>) -> EvalResult {
    expect_args("elt", &args, 2)?;
    let idx = expect_int(&args[1])? as usize;
    match &args[0] {
        Value::Cons(_) | Value::Nil => {
            let items = list_to_vec(&args[0]).unwrap_or_default();
            Ok(items.get(idx).cloned().unwrap_or(Value::Nil))
        }
        Value::Vector(v) => {
            let v = v.lock().expect("poisoned");
            Ok(v.get(idx).cloned().unwrap_or(Value::Nil))
        }
        Value::Str(s) => {
            Ok(s.chars().nth(idx).map(Value::Char).unwrap_or(Value::Nil))
        }
        other => Err(signal("wrong-type-argument", vec![Value::symbol("sequencep"), other.clone()])),
    }
}

pub(crate) fn builtin_nconc(args: Vec<Value>) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::Nil);
    }
    let mut all_items: Vec<Value> = Vec::new();
    for arg in &args {
        match arg {
            Value::Nil => {}
            _ => {
                let items = list_to_vec(arg)
                    .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("listp"), arg.clone()]))?;
                all_items.extend(items);
            }
        }
    }
    Ok(Value::list(all_items))
}

pub(crate) fn builtin_alist_get(args: Vec<Value>) -> EvalResult {
    expect_min_args("alist-get", &args, 2)?;
    let key = &args[0];
    let alist = list_to_vec(&args[1])
        .ok_or_else(|| signal("wrong-type-argument", vec![Value::symbol("listp"), args[1].clone()]))?;
    let default = args.get(2).cloned().unwrap_or(Value::Nil);
    let _remove = args.get(3); // not used
    let use_equal = args.get(4).is_some_and(|v| v.is_truthy());

    for entry in &alist {
        if let Value::Cons(cell) = entry {
            let pair = cell.lock().expect("poisoned");
            let matches = if use_equal {
                equal_value(key, &pair.car, 0)
            } else {
                eq_value(key, &pair.car)
            };
            if matches {
                return Ok(pair.cdr.clone());
            }
        }
    }
    Ok(default)
}

pub(crate) fn builtin_number_sequence(args: Vec<Value>) -> EvalResult {
    expect_min_args("number-sequence", &args, 1)?;
    let from = expect_int(&args[0])?;
    let to = if args.len() > 1 {
        match &args[1] {
            Value::Nil => return Ok(Value::list(vec![Value::Int(from)])),
            v => expect_int(v)?,
        }
    } else {
        return Ok(Value::list(vec![Value::Int(from)]));
    };
    let step = if args.len() > 2 { expect_int(&args[2])? } else if from <= to { 1 } else { -1 };

    if step == 0 {
        return Err(signal("args-out-of-range", vec![Value::Int(0)]));
    }

    let mut result = Vec::new();
    let mut i = from;
    if step > 0 {
        while i <= to {
            result.push(Value::Int(i));
            i += step;
        }
    } else {
        while i >= to {
            result.push(Value::Int(i));
            i += step;
        }
    }
    Ok(Value::list(result))
}

// ===========================================================================
// Misc builtins
// ===========================================================================

pub(crate) fn builtin_princ(args: Vec<Value>) -> EvalResult {
    expect_min_args("princ", &args, 1)?;
    // In real Emacs this prints to standard output; here just return the value
    Ok(args[0].clone())
}

pub(crate) fn builtin_prin1(args: Vec<Value>) -> EvalResult {
    expect_min_args("prin1", &args, 1)?;
    Ok(args[0].clone())
}

pub(crate) fn builtin_prin1_to_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("prin1-to-string", &args, 1)?;
    Ok(Value::string(super::print::print_value(&args[0])))
}

pub(crate) fn builtin_print(args: Vec<Value>) -> EvalResult {
    expect_min_args("print", &args, 1)?;
    Ok(args[0].clone())
}

pub(crate) fn builtin_propertize(args: Vec<Value>) -> EvalResult {
    expect_min_args("propertize", &args, 1)?;
    // Stub: ignore properties, return the string
    Ok(args[0].clone())
}

pub(crate) fn builtin_gensym(args: Vec<Value>) -> EvalResult {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let prefix = if !args.is_empty() {
        expect_string(&args[0])?
    } else {
        "g".to_string()
    };
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    Ok(Value::Symbol(format!("{}{}", prefix, n)))
}

pub(crate) fn builtin_string_to_syntax(args: Vec<Value>) -> EvalResult {
    super::syntax::builtin_string_to_syntax(args)
}

pub(crate) fn builtin_current_time(args: Vec<Value>) -> EvalResult {
    let _ = args;
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let usecs = dur.subsec_micros() as i64;
    Ok(Value::list(vec![
        Value::Int(secs >> 16),
        Value::Int(secs & 0xFFFF),
        Value::Int(usecs),
    ]))
}

pub(crate) fn builtin_float_time(args: Vec<Value>) -> EvalResult {
    let _ = args;
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    Ok(Value::Float(dur.as_secs_f64()))
}

// ===========================================================================
// Buffer operations (require evaluator for BufferManager access)
// ===========================================================================

use crate::buffer::BufferId;

fn expect_buffer_id(value: &Value) -> Result<BufferId, Flow> {
    match value {
        Value::Buffer(id) => Ok(*id),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("bufferp"), other.clone()],
        )),
    }
}

/// (get-buffer-create NAME) → buffer
pub(crate) fn builtin_get_buffer_create(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("get-buffer-create", &args, 1)?;
    let name = expect_string(&args[0])?;
    if let Some(id) = eval.buffers.find_buffer_by_name(&name) {
        Ok(Value::Buffer(id))
    } else {
        let id = eval.buffers.create_buffer(&name);
        Ok(Value::Buffer(id))
    }
}

/// (get-buffer NAME-OR-BUFFER) → buffer or nil
pub(crate) fn builtin_get_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("get-buffer", &args, 1)?;
    match &args[0] {
        Value::Buffer(id) => {
            if eval.buffers.get(*id).is_some() {
                Ok(args[0].clone())
            } else {
                Ok(Value::Nil)
            }
        }
        Value::Str(s) => {
            if let Some(id) = eval.buffers.find_buffer_by_name(s) {
                Ok(Value::Buffer(id))
            } else {
                Ok(Value::Nil)
            }
        }
        _ => Ok(Value::Nil),
    }
}

/// (kill-buffer BUFFER-OR-NAME) → t
pub(crate) fn builtin_kill_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("kill-buffer", &args, 1)?;
    let id = match &args[0] {
        Value::Buffer(id) => *id,
        Value::Str(s) => {
            match eval.buffers.find_buffer_by_name(s) {
                Some(id) => id,
                None => return Ok(Value::Nil),
            }
        }
        _ => return Ok(Value::Nil),
    };
    eval.buffers.kill_buffer(id);
    Ok(Value::True)
}

/// (set-buffer BUFFER-OR-NAME) → buffer
pub(crate) fn builtin_set_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-buffer", &args, 1)?;
    let id = match &args[0] {
        Value::Buffer(id) => *id,
        Value::Str(s) => {
            eval.buffers.find_buffer_by_name(s)
                .ok_or_else(|| signal("error", vec![Value::string(format!("No buffer named {s}"))]))?
        }
        other => return Err(signal("wrong-type-argument", vec![Value::symbol("stringp"), other.clone()])),
    };
    eval.buffers.set_current(id);
    Ok(Value::Buffer(id))
}

/// (current-buffer) → buffer
pub(crate) fn builtin_current_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    match eval.buffers.current_buffer() {
        Some(buf) => Ok(Value::Buffer(buf.id)),
        None => Ok(Value::Nil),
    }
}

/// (buffer-name &optional BUFFER) → string
pub(crate) fn builtin_buffer_name(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let id = if args.is_empty() || matches!(args[0], Value::Nil) {
        match eval.buffers.current_buffer() {
            Some(b) => b.id,
            None => return Ok(Value::Nil),
        }
    } else {
        expect_buffer_id(&args[0])?
    };
    match eval.buffers.get(id) {
        Some(buf) => Ok(Value::string(&buf.name)),
        None => Ok(Value::Nil),
    }
}

/// (buffer-file-name &optional BUFFER) → string or nil
pub(crate) fn builtin_buffer_file_name(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let id = if args.is_empty() || matches!(args[0], Value::Nil) {
        match eval.buffers.current_buffer() {
            Some(b) => b.id,
            None => return Ok(Value::Nil),
        }
    } else {
        expect_buffer_id(&args[0])?
    };
    match eval.buffers.get(id) {
        Some(buf) => match &buf.file_name {
            Some(f) => Ok(Value::string(f)),
            None => Ok(Value::Nil),
        },
        None => Ok(Value::Nil),
    }
}

/// (buffer-string) → string
pub(crate) fn builtin_buffer_string(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::string(buf.buffer_string()))
}

/// (buffer-substring START END) → string
pub(crate) fn builtin_buffer_substring(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("buffer-substring", &args, 2)?;
    let start = expect_int(&args[0])? as usize;
    let end = expect_int(&args[1])? as usize;
    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    // Emacs uses 1-based positions, convert to 0-based byte positions
    let s = if start > 0 { start - 1 } else { 0 };
    let e = if end > 0 { end - 1 } else { 0 };
    // Convert char positions to byte positions
    let byte_start = buf.text.char_to_byte(s);
    let byte_end = buf.text.char_to_byte(e);
    Ok(Value::string(buf.buffer_substring(byte_start, byte_end)))
}

/// (point) → integer
pub(crate) fn builtin_point(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    let buf = eval.buffers.current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    // Return 1-based char position
    Ok(Value::Int(buf.point_char() as i64 + 1))
}

/// (point-min) → integer
pub(crate) fn builtin_point_min(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    let buf = eval.buffers.current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::Int(buf.text.byte_to_char(buf.point_min()) as i64 + 1))
}

/// (point-max) → integer
pub(crate) fn builtin_point_max(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    let buf = eval.buffers.current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::Int(buf.text.byte_to_char(buf.point_max()) as i64 + 1))
}

/// (goto-char POS) → POS
pub(crate) fn builtin_goto_char(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("goto-char", &args, 1)?;
    let pos = expect_int(&args[0])?;
    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    // Convert 1-based char pos to 0-based byte pos
    let char_pos = if pos > 0 { pos as usize - 1 } else { 0 };
    let byte_pos = buf.text.char_to_byte(char_pos.min(buf.text.char_count()));
    buf.goto_char(byte_pos);
    Ok(args[0].clone())
}

/// (insert &rest ARGS) → nil
pub(crate) fn builtin_insert(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    for arg in &args {
        match arg {
            Value::Str(s) => buf.insert(s),
            Value::Char(c) => {
                let mut tmp = [0u8; 4];
                buf.insert(c.encode_utf8(&mut tmp));
            }
            Value::Int(n) => {
                if let Some(c) = char::from_u32(*n as u32) {
                    let mut tmp = [0u8; 4];
                    buf.insert(c.encode_utf8(&mut tmp));
                }
            }
            other => return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("char-or-string-p"), other.clone()],
            )),
        }
    }
    Ok(Value::Nil)
}

/// (delete-region START END) → nil
pub(crate) fn builtin_delete_region(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("delete-region", &args, 2)?;
    let start = expect_int(&args[0])? as usize;
    let end = expect_int(&args[1])? as usize;
    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    // Convert 1-based to 0-based char positions, then to byte positions
    let s = if start > 0 { start - 1 } else { 0 };
    let e = if end > 0 { end - 1 } else { 0 };
    let byte_start = buf.text.char_to_byte(s);
    let byte_end = buf.text.char_to_byte(e);
    buf.delete_region(byte_start, byte_end);
    Ok(Value::Nil)
}

/// (erase-buffer) → nil
pub(crate) fn builtin_erase_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let len = buf.text.len();
    buf.delete_region(0, len);
    buf.widen();
    Ok(Value::Nil)
}

/// (buffer-size &optional BUFFER) → integer
pub(crate) fn builtin_buffer_size(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let buf = if args.is_empty() || matches!(args[0], Value::Nil) {
        eval.buffers.current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
    } else {
        let id = expect_buffer_id(&args[0])?;
        eval.buffers.get(id)
            .ok_or_else(|| signal("error", vec![Value::string("No such buffer")]))?
    };
    Ok(Value::Int(buf.text.char_count() as i64))
}

/// (narrow-to-region START END) → nil
pub(crate) fn builtin_narrow_to_region(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("narrow-to-region", &args, 2)?;
    let start = expect_int(&args[0])? as usize;
    let end = expect_int(&args[1])? as usize;
    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let s = if start > 0 { start - 1 } else { 0 };
    let e = if end > 0 { end - 1 } else { 0 };
    let byte_start = buf.text.char_to_byte(s);
    let byte_end = buf.text.char_to_byte(e);
    buf.narrow_to_region(byte_start, byte_end);
    Ok(Value::Nil)
}

/// (widen) → nil
pub(crate) fn builtin_widen(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.widen();
    Ok(Value::Nil)
}

/// (buffer-modified-p &optional BUFFER) → t or nil
pub(crate) fn builtin_buffer_modified_p(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let buf = if args.is_empty() || matches!(args[0], Value::Nil) {
        eval.buffers.current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
    } else {
        let id = expect_buffer_id(&args[0])?;
        eval.buffers.get(id)
            .ok_or_else(|| signal("error", vec![Value::string("No such buffer")]))?
    };
    Ok(Value::bool(buf.is_modified()))
}

/// (set-buffer-modified-p FLAG) → FLAG
pub(crate) fn builtin_set_buffer_modified_p(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-buffer-modified-p", &args, 1)?;
    let flag = args[0].is_truthy();
    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.set_modified(flag);
    Ok(args[0].clone())
}

/// (buffer-list) → list of buffers
pub(crate) fn builtin_buffer_list(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let _ = args;
    let ids = eval.buffers.buffer_list();
    let vals: Vec<Value> = ids.into_iter().map(Value::Buffer).collect();
    Ok(Value::list(vals))
}

/// (generate-new-buffer-name BASE) → string
pub(crate) fn builtin_generate_new_buffer_name(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("generate-new-buffer-name", &args, 1)?;
    let base = expect_string(&args[0])?;
    Ok(Value::string(eval.buffers.generate_new_buffer_name(&base)))
}

/// (generate-new-buffer NAME) → buffer
pub(crate) fn builtin_generate_new_buffer(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("generate-new-buffer", &args, 1)?;
    let base = expect_string(&args[0])?;
    let name = eval.buffers.generate_new_buffer_name(&base);
    let id = eval.buffers.create_buffer(&name);
    Ok(Value::Buffer(id))
}

/// (bufferp OBJECT) → t or nil
pub(crate) fn builtin_bufferp(args: Vec<Value>) -> EvalResult {
    expect_args("bufferp", &args, 1)?;
    Ok(Value::bool(matches!(args[0], Value::Buffer(_))))
}

/// (char-after &optional POS) → integer or nil
pub(crate) fn builtin_char_after(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let buf = eval.buffers.current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let byte_pos = if args.is_empty() || matches!(args[0], Value::Nil) {
        buf.point()
    } else {
        let pos = expect_int(&args[0])? as usize;
        let char_pos = if pos > 0 { pos - 1 } else { 0 };
        buf.text.char_to_byte(char_pos.min(buf.text.char_count()))
    };
    match buf.char_after(byte_pos) {
        Some(c) => Ok(Value::Int(c as i64)),
        None => Ok(Value::Nil),
    }
}

/// (char-before &optional POS) → integer or nil
pub(crate) fn builtin_char_before(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let buf = eval.buffers.current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let byte_pos = if args.is_empty() || matches!(args[0], Value::Nil) {
        buf.point()
    } else {
        let pos = expect_int(&args[0])? as usize;
        let char_pos = if pos > 0 { pos - 1 } else { 0 };
        buf.text.char_to_byte(char_pos.min(buf.text.char_count()))
    };
    match buf.char_before(byte_pos) {
        Some(c) => Ok(Value::Int(c as i64)),
        None => Ok(Value::Nil),
    }
}

/// (buffer-local-value VARIABLE BUFFER) → value
pub(crate) fn builtin_buffer_local_value(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("buffer-local-value", &args, 2)?;
    let name = match &args[0] {
        Value::Symbol(s) => s.clone(),
        other => return Err(signal("wrong-type-argument", vec![Value::symbol("symbolp"), other.clone()])),
    };
    let id = expect_buffer_id(&args[1])?;
    let buf = eval.buffers.get(id)
        .ok_or_else(|| signal("error", vec![Value::string("No such buffer")]))?;
    match buf.get_buffer_local(&name) {
        Some(v) => Ok(v.clone()),
        None => Ok(Value::Nil),
    }
}

/// (with-current-buffer BUFFER-OR-NAME &rest BODY) is a special form handled
/// in eval.rs, but we provide the utility of switching and restoring here.

// Search / regex builtins are defined at the end of this file.

// ===========================================================================
// Keymap builtins
// ===========================================================================

use super::keymap::{KeyBinding, KeyEvent, KeymapManager};

/// Extract a keymap id from a Value, signaling wrong-type-argument if invalid.
fn expect_keymap_id(
    eval: &super::eval::Evaluator,
    value: &Value,
) -> Result<u64, Flow> {
    match value {
        Value::Int(n) => {
            let id = *n as u64;
            if eval.keymaps.is_keymap(id) {
                Ok(id)
            } else {
                Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("keymapp"), value.clone()],
                ))
            }
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("keymapp"), other.clone()],
        )),
    }
}

/// Convert a KeyBinding to a Value for returning to Lisp.
fn key_binding_to_value(binding: &KeyBinding) -> Value {
    match binding {
        KeyBinding::Command(name) => Value::symbol(name.clone()),
        KeyBinding::Prefix(id) => Value::Int(*id as i64),
        KeyBinding::LispValue(v) => v.clone(),
    }
}

/// Convert a Value to a KeyBinding.
fn value_to_key_binding(
    eval: &super::eval::Evaluator,
    value: &Value,
) -> KeyBinding {
    match value {
        Value::Symbol(name) => KeyBinding::Command(name.clone()),
        Value::Nil => KeyBinding::Command("nil".to_string()),
        Value::Int(n) => {
            let id = *n as u64;
            if eval.keymaps.is_keymap(id) {
                KeyBinding::Prefix(id)
            } else {
                KeyBinding::LispValue(value.clone())
            }
        }
        other => KeyBinding::LispValue(other.clone()),
    }
}

/// Parse a key description from a Value (must be a string).
fn expect_key_description(value: &Value) -> Result<Vec<KeyEvent>, Flow> {
    let desc = match value {
        Value::Str(s) => s.as_str(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ));
        }
    };
    KeymapManager::parse_key_description(desc).map_err(|msg| {
        signal("error", vec![Value::string(msg)])
    })
}

/// Helper: define a key in a keymap, auto-creating prefix maps for multi-key sequences.
fn define_key_in_map(
    eval: &mut super::eval::Evaluator,
    map_id: u64,
    keys: Vec<KeyEvent>,
    binding: KeyBinding,
) {
    if keys.len() == 1 {
        eval.keymaps
            .define_key(map_id, keys.into_iter().next().unwrap(), binding);
    } else {
        let mut current_map = map_id;
        for (i, key) in keys.iter().enumerate() {
            if i == keys.len() - 1 {
                eval.keymaps.define_key(current_map, key.clone(), binding.clone());
            } else {
                match eval.keymaps.lookup_key(current_map, key).cloned() {
                    Some(KeyBinding::Prefix(next_map)) => {
                        current_map = next_map;
                    }
                    _ => {
                        let prefix_map = eval.keymaps.make_sparse_keymap(None);
                        eval.keymaps.define_key(
                            current_map,
                            key.clone(),
                            KeyBinding::Prefix(prefix_map),
                        );
                        current_map = prefix_map;
                    }
                }
            }
        }
    }
}

/// (make-keymap) -> keymap-id
fn builtin_make_keymap(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    let _ = &args;
    let id = eval.keymaps.make_keymap();
    Ok(Value::Int(id as i64))
}

/// (make-sparse-keymap &optional NAME) -> keymap-id
fn builtin_make_sparse_keymap(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    let name = if !args.is_empty() {
        match &args[0] {
            Value::Str(s) => Some((**s).clone()),
            Value::Nil => None,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), other.clone()],
                ));
            }
        }
    } else {
        None
    };
    let id = eval.keymaps.make_sparse_keymap(name);
    Ok(Value::Int(id as i64))
}

/// (define-key KEYMAP KEY DEF) -> DEF
fn builtin_define_key(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("define-key", &args, 3)?;
    let keymap_id = expect_keymap_id(eval, &args[0])?;
    let keys = expect_key_description(&args[1])?;
    let binding = value_to_key_binding(eval, &args[2]);
    define_key_in_map(eval, keymap_id, keys, binding);
    Ok(args[2].clone())
}

/// (lookup-key KEYMAP KEY) -> binding or nil
fn builtin_lookup_key(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("lookup-key", &args, 2)?;
    let keymap_id = expect_keymap_id(eval, &args[0])?;
    let keys = expect_key_description(&args[1])?;
    let result = if keys.len() == 1 {
        eval.keymaps.lookup_key(keymap_id, &keys[0])
    } else {
        eval.keymaps.lookup_key_sequence(keymap_id, &keys)
    };
    match result {
        Some(binding) => Ok(key_binding_to_value(binding)),
        None => Ok(Value::Nil),
    }
}

/// (global-set-key KEY COMMAND)
fn builtin_global_set_key(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("global-set-key", &args, 2)?;
    let global_id = match eval.keymaps.global_map() {
        Some(id) => id,
        None => {
            let id = eval.keymaps.make_keymap();
            eval.keymaps.set_global_map(id);
            id
        }
    };
    let keys = expect_key_description(&args[0])?;
    let binding = value_to_key_binding(eval, &args[1]);
    define_key_in_map(eval, global_id, keys, binding);
    Ok(args[1].clone())
}

/// (local-set-key KEY COMMAND)
fn builtin_local_set_key(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("local-set-key", &args, 2)?;
    let local_id = match eval.current_local_map {
        Some(id) => id,
        None => {
            let id = eval.keymaps.make_sparse_keymap(None);
            eval.current_local_map = Some(id);
            id
        }
    };
    let keys = expect_key_description(&args[0])?;
    let binding = value_to_key_binding(eval, &args[1]);
    define_key_in_map(eval, local_id, keys, binding);
    Ok(args[1].clone())
}

/// (use-local-map KEYMAP)
fn builtin_use_local_map(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("use-local-map", &args, 1)?;
    if args[0].is_nil() {
        eval.current_local_map = None;
    } else {
        let id = expect_keymap_id(eval, &args[0])?;
        eval.current_local_map = Some(id);
    }
    Ok(Value::Nil)
}

/// (use-global-map KEYMAP)
fn builtin_use_global_map(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("use-global-map", &args, 1)?;
    let id = expect_keymap_id(eval, &args[0])?;
    eval.keymaps.set_global_map(id);
    Ok(Value::Nil)
}

/// (current-local-map) -> keymap-id or nil
fn builtin_current_local_map(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("current-local-map", &args, 0)?;
    match eval.current_local_map {
        Some(id) => Ok(Value::Int(id as i64)),
        None => Ok(Value::Nil),
    }
}

/// (current-global-map) -> keymap-id or nil
fn builtin_current_global_map(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("current-global-map", &args, 0)?;
    match eval.keymaps.global_map() {
        Some(id) => Ok(Value::Int(id as i64)),
        None => Ok(Value::Nil),
    }
}

/// (keymap-parent KEYMAP) -> keymap-id or nil
fn builtin_keymap_parent(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("keymap-parent", &args, 1)?;
    let id = expect_keymap_id(eval, &args[0])?;
    match eval.keymaps.keymap_parent(id) {
        Some(parent_id) => Ok(Value::Int(parent_id as i64)),
        None => Ok(Value::Nil),
    }
}

/// (set-keymap-parent KEYMAP PARENT) -> PARENT
fn builtin_set_keymap_parent(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("set-keymap-parent", &args, 2)?;
    let id = expect_keymap_id(eval, &args[0])?;
    let parent = if args[1].is_nil() {
        None
    } else {
        Some(expect_keymap_id(eval, &args[1])?)
    };
    eval.keymaps.set_keymap_parent(id, parent);
    Ok(args[1].clone())
}

/// (keymapp OBJ) -> t or nil
fn builtin_keymapp(eval: &mut super::eval::Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("keymapp", &args, 1)?;
    match &args[0] {
        Value::Int(n) => Ok(Value::bool(eval.keymaps.is_keymap(*n as u64))),
        _ => Ok(Value::Nil),
    }
}

/// (kbd STRING) -> STRING
/// Validates and normalizes the key description string.
fn builtin_kbd(args: Vec<Value>) -> EvalResult {
    expect_args("kbd", &args, 1)?;
    let desc = match &args[0] {
        Value::Str(s) => s.as_str(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), other.clone()],
            ));
        }
    };
    let events = KeymapManager::parse_key_description(desc)
        .map_err(|msg| signal("error", vec![Value::string(msg)]))?;
    Ok(Value::string(KeymapManager::format_key_sequence(&events)))
}

// ===========================================================================
// Dispatch table
// ===========================================================================

/// Try to dispatch a builtin function by name. Returns None if not a known builtin.
pub(crate) fn dispatch_builtin(
    eval: &mut super::eval::Evaluator,
    name: &str,
    args: Vec<Value>,
) -> Option<EvalResult> {
    // Functions that need the evaluator (higher-order / obarray access)
    match name {
        "apply" => return Some(builtin_apply(eval, args)),
        "mapcar" => return Some(builtin_mapcar(eval, args)),
        "mapc" => return Some(builtin_mapc(eval, args)),
        "sort" => return Some(builtin_sort(eval, args)),
        // Symbol/obarray
        "boundp" => return Some(builtin_boundp(eval, args)),
        "fboundp" => return Some(builtin_fboundp(eval, args)),
        "symbol-value" => return Some(builtin_symbol_value(eval, args)),
        "symbol-function" => return Some(builtin_symbol_function(eval, args)),
        "set" => return Some(builtin_set(eval, args)),
        "fset" => return Some(builtin_fset(eval, args)),
        "makunbound" => return Some(builtin_makunbound(eval, args)),
        "fmakunbound" => return Some(builtin_fmakunbound(eval, args)),
        "get" => return Some(builtin_get(eval, args)),
        "put" => return Some(builtin_put(eval, args)),
        "symbol-plist" => return Some(builtin_symbol_plist_fn(eval, args)),
        "indirect-function" => return Some(builtin_indirect_function(eval, args)),
        "intern" => return Some(builtin_intern_fn(eval, args)),
        "intern-soft" => return Some(builtin_intern_soft(eval, args)),
        // Hooks
        "add-hook" => return Some(builtin_add_hook(eval, args)),
        "remove-hook" => return Some(builtin_remove_hook(eval, args)),
        "run-hooks" => return Some(builtin_run_hooks(eval, args)),
        "run-hook-with-args" => return Some(builtin_run_hook_with_args(eval, args)),
        "featurep" => return Some(builtin_featurep(eval, args)),
        // Loading
        "load" => return Some(builtin_load(eval, args)),
        "load-file" => return Some(builtin_load_file(eval, args)),
        "eval" => return Some(builtin_eval(eval, args)),
        // Buffer operations
        "get-buffer-create" => return Some(builtin_get_buffer_create(eval, args)),
        "get-buffer" => return Some(builtin_get_buffer(eval, args)),
        "kill-buffer" => return Some(builtin_kill_buffer(eval, args)),
        "set-buffer" => return Some(builtin_set_buffer(eval, args)),
        "current-buffer" => return Some(builtin_current_buffer(eval, args)),
        "buffer-name" => return Some(builtin_buffer_name(eval, args)),
        "buffer-file-name" => return Some(builtin_buffer_file_name(eval, args)),
        "buffer-string" => return Some(builtin_buffer_string(eval, args)),
        "buffer-substring" => return Some(builtin_buffer_substring(eval, args)),
        "point" => return Some(builtin_point(eval, args)),
        "point-min" => return Some(builtin_point_min(eval, args)),
        "point-max" => return Some(builtin_point_max(eval, args)),
        "goto-char" => return Some(builtin_goto_char(eval, args)),
        "insert" => return Some(builtin_insert(eval, args)),
        "delete-region" => return Some(builtin_delete_region(eval, args)),
        "erase-buffer" => return Some(builtin_erase_buffer(eval, args)),
        "buffer-size" => return Some(builtin_buffer_size(eval, args)),
        "narrow-to-region" => return Some(builtin_narrow_to_region(eval, args)),
        "widen" => return Some(builtin_widen(eval, args)),
        // set-mark and mark are now in navigation module (below)
        "buffer-modified-p" => return Some(builtin_buffer_modified_p(eval, args)),
        "set-buffer-modified-p" => return Some(builtin_set_buffer_modified_p(eval, args)),
        "buffer-list" => return Some(builtin_buffer_list(eval, args)),
        "generate-new-buffer-name" => return Some(builtin_generate_new_buffer_name(eval, args)),
        "generate-new-buffer" => return Some(builtin_generate_new_buffer(eval, args)),
        "char-after" => return Some(builtin_char_after(eval, args)),
        "char-before" => return Some(builtin_char_before(eval, args)),
        "buffer-local-value" => return Some(builtin_buffer_local_value(eval, args)),
        // Search / regex operations
        "search-forward" => return Some(builtin_search_forward(eval, args)),
        "search-backward" => return Some(builtin_search_backward(eval, args)),
        "re-search-forward" => return Some(builtin_re_search_forward(eval, args)),
        "re-search-backward" => return Some(builtin_re_search_backward(eval, args)),
        "looking-at" => return Some(builtin_looking_at(eval, args)),
        "string-match" => return Some(builtin_string_match_eval(eval, args)),
        "match-string" => return Some(builtin_match_string(eval, args)),
        "match-beginning" => return Some(builtin_match_beginning(eval, args)),
        "match-end" => return Some(builtin_match_end(eval, args)),
        "replace-match" => return Some(builtin_replace_match(eval, args)),
        // File I/O (evaluator-dependent)
        "insert-file-contents" => return Some(super::fileio::builtin_insert_file_contents(eval, args)),
        "write-region" => return Some(super::fileio::builtin_write_region(eval, args)),
        "find-file-noselect" => return Some(super::fileio::builtin_find_file_noselect(eval, args)),
        // Keymap operations
        "make-keymap" => return Some(builtin_make_keymap(eval, args)),
        "make-sparse-keymap" => return Some(builtin_make_sparse_keymap(eval, args)),
        "define-key" => return Some(builtin_define_key(eval, args)),
        "lookup-key" => return Some(builtin_lookup_key(eval, args)),
        "global-set-key" => return Some(builtin_global_set_key(eval, args)),
        "local-set-key" => return Some(builtin_local_set_key(eval, args)),
        "use-local-map" => return Some(builtin_use_local_map(eval, args)),
        "use-global-map" => return Some(builtin_use_global_map(eval, args)),
        "current-local-map" => return Some(builtin_current_local_map(eval, args)),
        "current-global-map" => return Some(builtin_current_global_map(eval, args)),
        "keymap-parent" => return Some(builtin_keymap_parent(eval, args)),
        "set-keymap-parent" => return Some(builtin_set_keymap_parent(eval, args)),
        "keymapp" => return Some(builtin_keymapp(eval, args)),
        // Process operations (evaluator-dependent)
        "start-process" => return Some(super::process::builtin_start_process(eval, args)),
        "call-process" => return Some(super::process::builtin_call_process(eval, args)),
        "call-process-region" => return Some(super::process::builtin_call_process_region(eval, args)),
        "delete-process" => return Some(super::process::builtin_delete_process(eval, args)),
        "process-send-string" => return Some(super::process::builtin_process_send_string(eval, args)),
        "process-status" => return Some(super::process::builtin_process_status(eval, args)),
        "process-exit-status" => return Some(super::process::builtin_process_exit_status(eval, args)),
        "process-list" => return Some(super::process::builtin_process_list(eval, args)),
        "process-name" => return Some(super::process::builtin_process_name(eval, args)),
        "process-buffer" => return Some(super::process::builtin_process_buffer(eval, args)),
        // Timer operations (evaluator-dependent)
        "run-at-time" => return Some(super::timer::builtin_run_at_time(eval, args)),
        "run-with-timer" => return Some(super::timer::builtin_run_with_timer(eval, args)),
        "run-with-idle-timer" => return Some(super::timer::builtin_run_with_idle_timer(eval, args)),
        "cancel-timer" => return Some(super::timer::builtin_cancel_timer(eval, args)),
        "timer-activate" => return Some(super::timer::builtin_timer_activate(eval, args)),
        // Advice system
        "advice-add" => return Some(super::advice::builtin_advice_add(eval, args)),
        "advice-remove" => return Some(super::advice::builtin_advice_remove(eval, args)),
        "advice-member-p" => return Some(super::advice::builtin_advice_member_p(eval, args)),
        // Variable watchers
        "add-variable-watcher" => return Some(super::advice::builtin_add_variable_watcher(eval, args)),
        "remove-variable-watcher" => return Some(super::advice::builtin_remove_variable_watcher(eval, args)),
        // Syntax table operations (evaluator-dependent)
        "modify-syntax-entry" => return Some(super::syntax::builtin_modify_syntax_entry(eval, args)),
        "char-syntax" => return Some(super::syntax::builtin_char_syntax(eval, args)),
        "forward-word" => return Some(super::syntax::builtin_forward_word(eval, args)),
        "backward-word" => return Some(super::syntax::builtin_backward_word(eval, args)),
        "forward-sexp" => return Some(super::syntax::builtin_forward_sexp(eval, args)),
        "backward-sexp" => return Some(super::syntax::builtin_backward_sexp(eval, args)),
        "skip-syntax-forward" => return Some(super::syntax::builtin_skip_syntax_forward(eval, args)),
        "skip-syntax-backward" => return Some(super::syntax::builtin_skip_syntax_backward(eval, args)),
        // Register operations (evaluator-dependent)
        "copy-to-register" => return Some(super::register::builtin_copy_to_register(eval, args)),
        "insert-register" => return Some(super::register::builtin_insert_register(eval, args)),
        "point-to-register" => return Some(super::register::builtin_point_to_register(eval, args)),
        "number-to-register" => return Some(super::register::builtin_number_to_register(eval, args)),
        "increment-register" => return Some(super::register::builtin_increment_register(eval, args)),
        "view-register" => return Some(super::register::builtin_view_register(eval, args)),
        "get-register" => return Some(super::register::builtin_get_register(eval, args)),
        "set-register" => return Some(super::register::builtin_set_register(eval, args)),
        // Keyboard macro operations (evaluator-dependent)
        "start-kbd-macro" => return Some(super::kmacro::builtin_start_kbd_macro(eval, args)),
        "end-kbd-macro" => return Some(super::kmacro::builtin_end_kbd_macro(eval, args)),
        "call-last-kbd-macro" => return Some(super::kmacro::builtin_call_last_kbd_macro(eval, args)),
        "execute-kbd-macro" => return Some(super::kmacro::builtin_execute_kbd_macro(eval, args)),
        "name-last-kbd-macro" => return Some(super::kmacro::builtin_name_last_kbd_macro(eval, args)),
        "insert-kbd-macro" => return Some(super::kmacro::builtin_insert_kbd_macro(eval, args)),
        "kbd-macro-query" => return Some(super::kmacro::builtin_kbd_macro_query(eval, args)),
        "store-kbd-macro-event" => return Some(super::kmacro::builtin_store_kbd_macro_event(eval, args)),
        "kmacro-set-counter" => return Some(super::kmacro::builtin_kmacro_set_counter(eval, args)),
        "kmacro-add-counter" => return Some(super::kmacro::builtin_kmacro_add_counter(eval, args)),
        "kmacro-set-format" => return Some(super::kmacro::builtin_kmacro_set_format(eval, args)),
        "defining-kbd-macro-p" => return Some(super::kmacro::builtin_defining_kbd_macro_p_eval(eval, args)),
        "last-kbd-macro" => return Some(super::kmacro::builtin_last_kbd_macro_eval(eval, args)),
        // Bookmark operations (evaluator-dependent)
        "bookmark-set" => return Some(super::bookmark::builtin_bookmark_set(eval, args)),
        "bookmark-jump" => return Some(super::bookmark::builtin_bookmark_jump(eval, args)),
        "bookmark-delete" => return Some(super::bookmark::builtin_bookmark_delete(eval, args)),
        "bookmark-rename" => return Some(super::bookmark::builtin_bookmark_rename(eval, args)),
        "bookmark-all-names" => return Some(super::bookmark::builtin_bookmark_all_names(eval, args)),
        "bookmark-get-filename" => return Some(super::bookmark::builtin_bookmark_get_filename(eval, args)),
        "bookmark-get-position" => return Some(super::bookmark::builtin_bookmark_get_position(eval, args)),
        "bookmark-get-annotation" => return Some(super::bookmark::builtin_bookmark_get_annotation(eval, args)),
        "bookmark-set-annotation" => return Some(super::bookmark::builtin_bookmark_set_annotation(eval, args)),
        "bookmark-save" => return Some(super::bookmark::builtin_bookmark_save(eval, args)),
        "bookmark-load" => return Some(super::bookmark::builtin_bookmark_load(eval, args)),
        // Abbreviation operations (evaluator-dependent)
        "define-abbrev" => return Some(super::abbrev::builtin_define_abbrev(eval, args)),
        "expand-abbrev" => return Some(super::abbrev::builtin_expand_abbrev(eval, args)),
        "abbrev-mode" => return Some(super::abbrev::builtin_abbrev_mode(eval, args)),
        "define-abbrev-table" => return Some(super::abbrev::builtin_define_abbrev_table(eval, args)),
        "clear-abbrev-table" => return Some(super::abbrev::builtin_clear_abbrev_table(eval, args)),
        "abbrev-expansion" => return Some(super::abbrev::builtin_abbrev_expansion(eval, args)),
        "insert-abbrev-table-description" => return Some(super::abbrev::builtin_insert_abbrev_table_description(eval, args)),
        "abbrev-table-p" => return Some(super::abbrev::builtin_abbrev_table_p(eval, args)),

        // Text property operations (evaluator-dependent — buffer access)
        "put-text-property" => return Some(super::textprop::builtin_put_text_property(eval, args)),
        "get-text-property" => return Some(super::textprop::builtin_get_text_property(eval, args)),
        "get-char-property" => return Some(super::textprop::builtin_get_char_property(eval, args)),
        "add-text-properties" => return Some(super::textprop::builtin_add_text_properties(eval, args)),
        "remove-text-properties" => return Some(super::textprop::builtin_remove_text_properties(eval, args)),
        "text-properties-at" => return Some(super::textprop::builtin_text_properties_at(eval, args)),
        "next-single-property-change" => return Some(super::textprop::builtin_next_single_property_change(eval, args)),
        "previous-single-property-change" => return Some(super::textprop::builtin_previous_single_property_change(eval, args)),
        "next-property-change" => return Some(super::textprop::builtin_next_property_change(eval, args)),
        "text-property-any" => return Some(super::textprop::builtin_text_property_any(eval, args)),
        "make-overlay" => return Some(super::textprop::builtin_make_overlay(eval, args)),
        "delete-overlay" => return Some(super::textprop::builtin_delete_overlay(eval, args)),
        "overlay-put" => return Some(super::textprop::builtin_overlay_put(eval, args)),
        "overlay-get" => return Some(super::textprop::builtin_overlay_get(eval, args)),
        "overlays-at" => return Some(super::textprop::builtin_overlays_at(eval, args)),
        "overlays-in" => return Some(super::textprop::builtin_overlays_in(eval, args)),
        "move-overlay" => return Some(super::textprop::builtin_move_overlay(eval, args)),
        "overlay-start" => return Some(super::textprop::builtin_overlay_start(eval, args)),
        "overlay-end" => return Some(super::textprop::builtin_overlay_end(eval, args)),
        "overlay-buffer" => return Some(super::textprop::builtin_overlay_buffer(eval, args)),
        "overlay-properties" => return Some(super::textprop::builtin_overlay_properties(eval, args)),
        "remove-overlays" => return Some(super::textprop::builtin_remove_overlays(eval, args)),
        "overlayp" => return Some(super::textprop::builtin_overlayp(eval, args)),

        // Navigation / mark / region (evaluator-dependent — buffer access)
        "bobp" => return Some(super::navigation::builtin_bobp(eval, args)),
        "eobp" => return Some(super::navigation::builtin_eobp(eval, args)),
        "bolp" => return Some(super::navigation::builtin_bolp(eval, args)),
        "eolp" => return Some(super::navigation::builtin_eolp(eval, args)),
        "line-beginning-position" => return Some(super::navigation::builtin_line_beginning_position(eval, args)),
        "line-end-position" => return Some(super::navigation::builtin_line_end_position(eval, args)),
        "line-number-at-pos" => return Some(super::navigation::builtin_line_number_at_pos(eval, args)),
        "count-lines" => return Some(super::navigation::builtin_count_lines(eval, args)),
        "forward-line" => return Some(super::navigation::builtin_forward_line(eval, args)),
        "beginning-of-line" => return Some(super::navigation::builtin_beginning_of_line(eval, args)),
        "end-of-line" => return Some(super::navigation::builtin_end_of_line(eval, args)),
        "goto-line" => return Some(super::navigation::builtin_goto_line(eval, args)),
        "forward-char" => return Some(super::navigation::builtin_forward_char(eval, args)),
        "backward-char" => return Some(super::navigation::builtin_backward_char(eval, args)),
        "skip-chars-forward" => return Some(super::navigation::builtin_skip_chars_forward(eval, args)),
        "skip-chars-backward" => return Some(super::navigation::builtin_skip_chars_backward(eval, args)),
        "push-mark" => return Some(super::navigation::builtin_push_mark(eval, args)),
        "pop-mark" => return Some(super::navigation::builtin_pop_mark(eval, args)),
        "set-mark" => return Some(super::navigation::builtin_set_mark_nav(eval, args)),
        "mark" => return Some(super::navigation::builtin_mark_nav(eval, args)),
        "mark-marker" => return Some(super::marker::builtin_mark_marker(eval, args)),
        "region-beginning" => return Some(super::navigation::builtin_region_beginning(eval, args)),
        "region-end" => return Some(super::navigation::builtin_region_end(eval, args)),
        "use-region-p" => return Some(super::navigation::builtin_use_region_p(eval, args)),
        "deactivate-mark" => return Some(super::navigation::builtin_deactivate_mark(eval, args)),
        "exchange-point-and-mark" => return Some(super::navigation::builtin_exchange_point_and_mark(eval, args)),
        "transient-mark-mode" => return Some(super::navigation::builtin_transient_mark_mode(eval, args)),

        // Custom system (evaluator-dependent)
        "custom-variable-p" => return Some(super::custom::builtin_custom_variable_p(eval, args)),
        "custom-group-p" => return Some(super::custom::builtin_custom_group_p(eval, args)),
        "custom-set-variables" => return Some(super::custom::builtin_custom_set_variables(eval, args)),
        "make-variable-buffer-local" => return Some(super::custom::builtin_make_variable_buffer_local(eval, args)),
        "make-local-variable" => return Some(super::custom::builtin_make_local_variable(eval, args)),
        "local-variable-p" => return Some(super::custom::builtin_local_variable_p(eval, args)),
        "buffer-local-variables" => return Some(super::custom::builtin_buffer_local_variables(eval, args)),
        "kill-local-variable" => return Some(super::custom::builtin_kill_local_variable(eval, args)),
        "default-value" => return Some(super::custom::builtin_default_value(eval, args)),
        "set-default" => return Some(super::custom::builtin_set_default(eval, args)),

        // Autoload (evaluator-dependent)
        "autoload-do-load" => return Some(super::autoload::builtin_autoload_do_load(eval, args)),

        // Kill ring / text editing (evaluator-dependent — buffer access)
        "kill-new" => return Some(super::kill_ring::builtin_kill_new(eval, args)),
        "kill-append" => return Some(super::kill_ring::builtin_kill_append(eval, args)),
        "current-kill" => return Some(super::kill_ring::builtin_current_kill(eval, args)),
        "kill-region" => return Some(super::kill_ring::builtin_kill_region(eval, args)),
        "kill-ring-save" => return Some(super::kill_ring::builtin_kill_ring_save(eval, args)),
        "copy-region-as-kill" => return Some(super::kill_ring::builtin_copy_region_as_kill(eval, args)),
        "kill-line" => return Some(super::kill_ring::builtin_kill_line(eval, args)),
        "kill-whole-line" => return Some(super::kill_ring::builtin_kill_whole_line(eval, args)),
        "kill-word" => return Some(super::kill_ring::builtin_kill_word(eval, args)),
        "backward-kill-word" => return Some(super::kill_ring::builtin_backward_kill_word(eval, args)),
        "yank" => return Some(super::kill_ring::builtin_yank(eval, args)),
        "yank-pop" => return Some(super::kill_ring::builtin_yank_pop(eval, args)),
        "downcase-region" => return Some(super::kill_ring::builtin_downcase_region(eval, args)),
        "upcase-region" => return Some(super::kill_ring::builtin_upcase_region(eval, args)),
        "capitalize-region" => return Some(super::kill_ring::builtin_capitalize_region(eval, args)),
        "downcase-word" => return Some(super::kill_ring::builtin_downcase_word(eval, args)),
        "upcase-word" => return Some(super::kill_ring::builtin_upcase_word(eval, args)),
        "capitalize-word" => return Some(super::kill_ring::builtin_capitalize_word(eval, args)),
        "transpose-chars" => return Some(super::kill_ring::builtin_transpose_chars(eval, args)),
        "transpose-words" => return Some(super::kill_ring::builtin_transpose_words(eval, args)),
        "transpose-lines" => return Some(super::kill_ring::builtin_transpose_lines(eval, args)),
        "indent-line-to" => return Some(super::kill_ring::builtin_indent_line_to(eval, args)),
        "indent-to" => return Some(super::kill_ring::builtin_indent_to(eval, args)),
        "newline" => return Some(super::kill_ring::builtin_newline(eval, args)),
        "newline-and-indent" => return Some(super::kill_ring::builtin_newline_and_indent(eval, args)),
        "delete-indentation" => return Some(super::kill_ring::builtin_delete_indentation(eval, args)),
        "tab-to-tab-stop" => return Some(super::kill_ring::builtin_tab_to_tab_stop(eval, args)),
        "indent-rigidly" => return Some(super::kill_ring::builtin_indent_rigidly(eval, args)),

        // Rectangle operations (evaluator-dependent — buffer access)
        "extract-rectangle-line" => return Some(super::rect::builtin_extract_rectangle_line(eval, args)),
        "extract-rectangle" => return Some(super::rect::builtin_extract_rectangle(eval, args)),
        "delete-rectangle" => return Some(super::rect::builtin_delete_rectangle(eval, args)),
        "kill-rectangle" => return Some(super::rect::builtin_kill_rectangle(eval, args)),
        "yank-rectangle" => return Some(super::rect::builtin_yank_rectangle(eval, args)),
        "insert-rectangle" => return Some(super::rect::builtin_insert_rectangle(eval, args)),
        "open-rectangle" => return Some(super::rect::builtin_open_rectangle(eval, args)),
        "clear-rectangle" => return Some(super::rect::builtin_clear_rectangle(eval, args)),
        "string-rectangle" => return Some(super::rect::builtin_string_rectangle(eval, args)),
        "delete-extract-rectangle" => return Some(super::rect::builtin_delete_extract_rectangle(eval, args)),
        "replace-rectangle" => return Some(super::rect::builtin_replace_rectangle(eval, args)),

        // Window/frame operations (evaluator-dependent)
        "selected-window" => return Some(super::window_cmds::builtin_selected_window(eval, args)),
        "window-buffer" => return Some(super::window_cmds::builtin_window_buffer(eval, args)),
        "window-start" => return Some(super::window_cmds::builtin_window_start(eval, args)),
        "window-end" => return Some(super::window_cmds::builtin_window_end(eval, args)),
        "window-point" => return Some(super::window_cmds::builtin_window_point(eval, args)),
        "window-height" => return Some(super::window_cmds::builtin_window_height(eval, args)),
        "window-width" => return Some(super::window_cmds::builtin_window_width(eval, args)),
        "window-body-height" => return Some(super::window_cmds::builtin_window_body_height(eval, args)),
        "window-body-width" => return Some(super::window_cmds::builtin_window_body_width(eval, args)),
        "window-list" => return Some(super::window_cmds::builtin_window_list(eval, args)),
        "window-dedicated-p" => return Some(super::window_cmds::builtin_window_dedicated_p(eval, args)),
        "window-live-p" => return Some(super::window_cmds::builtin_window_live_p(eval, args)),
        "set-window-start" => return Some(super::window_cmds::builtin_set_window_start(eval, args)),
        "set-window-point" => return Some(super::window_cmds::builtin_set_window_point(eval, args)),
        "set-window-dedicated-p" => return Some(super::window_cmds::builtin_set_window_dedicated_p(eval, args)),
        "split-window" => return Some(super::window_cmds::builtin_split_window(eval, args)),
        "delete-window" => return Some(super::window_cmds::builtin_delete_window(eval, args)),
        "delete-other-windows" => return Some(super::window_cmds::builtin_delete_other_windows(eval, args)),
        "select-window" => return Some(super::window_cmds::builtin_select_window(eval, args)),
        "other-window" => return Some(super::window_cmds::builtin_other_window(eval, args)),
        "next-window" => return Some(super::window_cmds::builtin_next_window(eval, args)),
        "previous-window" => return Some(super::window_cmds::builtin_previous_window(eval, args)),
        "set-window-buffer" => return Some(super::window_cmds::builtin_set_window_buffer(eval, args)),
        "switch-to-buffer" => return Some(super::window_cmds::builtin_switch_to_buffer(eval, args)),
        "display-buffer" => return Some(super::window_cmds::builtin_display_buffer(eval, args)),
        "pop-to-buffer" => return Some(super::window_cmds::builtin_pop_to_buffer(eval, args)),
        "selected-frame" => return Some(super::window_cmds::builtin_selected_frame(eval, args)),
        "frame-list" => return Some(super::window_cmds::builtin_frame_list(eval, args)),
        "make-frame" => return Some(super::window_cmds::builtin_make_frame(eval, args)),
        "delete-frame" => return Some(super::window_cmds::builtin_delete_frame(eval, args)),
        "frame-parameter" => return Some(super::window_cmds::builtin_frame_parameter(eval, args)),
        "frame-parameters" => return Some(super::window_cmds::builtin_frame_parameters(eval, args)),
        "modify-frame-parameters" => return Some(super::window_cmds::builtin_modify_frame_parameters(eval, args)),
        "frame-visible-p" => return Some(super::window_cmds::builtin_frame_visible_p(eval, args)),
        "frame-live-p" => return Some(super::window_cmds::builtin_frame_live_p(eval, args)),
        "windowp" => return Some(super::window_cmds::builtin_windowp(eval, args)),
        "framep" => return Some(super::window_cmds::builtin_framep(eval, args)),

        // Interactive / command system (evaluator-dependent)
        "call-interactively" => return Some(super::interactive::builtin_call_interactively(eval, args)),
        "interactive-p" => return Some(super::interactive::builtin_interactive_p(eval, args)),
        "called-interactively-p" => return Some(super::interactive::builtin_called_interactively_p(eval, args)),
        "commandp" => return Some(super::interactive::builtin_commandp_interactive(eval, args)),
        "command-execute" => return Some(super::interactive::builtin_command_execute(eval, args)),
        "execute-extended-command" => return Some(super::interactive::builtin_execute_extended_command(eval, args)),
        "key-binding" => return Some(super::interactive::builtin_key_binding(eval, args)),
        "local-key-binding" => return Some(super::interactive::builtin_local_key_binding(eval, args)),
        "global-key-binding" => return Some(super::interactive::builtin_global_key_binding(eval, args)),
        "minor-mode-key-binding" => return Some(super::interactive::builtin_minor_mode_key_binding(eval, args)),
        "where-is-internal" => return Some(super::interactive::builtin_where_is_internal(eval, args)),
        "substitute-command-keys" => return Some(super::interactive::builtin_substitute_command_keys(eval, args)),
        "describe-key-briefly" => return Some(super::interactive::builtin_describe_key_briefly(eval, args)),
        "this-command-keys" => return Some(super::interactive::builtin_this_command_keys(eval, args)),
        "this-command-keys-vector" => return Some(super::interactive::builtin_this_command_keys_vector(eval, args)),
        "thing-at-point" => return Some(super::interactive::builtin_thing_at_point(eval, args)),
        "bounds-of-thing-at-point" => return Some(super::interactive::builtin_bounds_of_thing_at_point(eval, args)),
        "word-at-point" => return Some(super::interactive::builtin_word_at_point(eval, args)),
        "symbol-at-point" => return Some(super::interactive::builtin_symbol_at_point(eval, args)),

        // Error hierarchy (evaluator-dependent — reads obarray)
        "error-message-string" => return Some(super::errors::builtin_error_message_string(eval, args)),

        // Reader/printer (evaluator-dependent)
        "read-from-string" => return Some(super::reader::builtin_read_from_string(eval, args)),
        "read" => return Some(super::reader::builtin_read(eval, args)),
        "read-from-minibuffer" => return Some(super::reader::builtin_read_from_minibuffer(eval, args)),
        "read-string" => return Some(super::reader::builtin_read_string(eval, args)),
        "read-number" => return Some(super::reader::builtin_read_number(eval, args)),
        "completing-read" => return Some(super::reader::builtin_completing_read(eval, args)),
        "read-char" => return Some(super::reader::builtin_read_char(eval, args)),
        "read-key-sequence" => return Some(super::reader::builtin_read_key_sequence(eval, args)),

        // Misc (evaluator-dependent)
        "backtrace-frame" => return Some(super::misc::builtin_backtrace_frame(eval, args)),
        "recursion-depth" => return Some(super::misc::builtin_recursion_depth(eval, args)),
        "abort-recursive-edit" => return Some(super::misc::builtin_abort_recursive_edit(eval, args)),

        // Threading (evaluator-dependent)
        "make-thread" => return Some(super::threads::builtin_make_thread(eval, args)),
        "thread-join" => return Some(super::threads::builtin_thread_join(eval, args)),
        "thread-yield" => return Some(super::threads::builtin_thread_yield(eval, args)),
        "thread-name" => return Some(super::threads::builtin_thread_name(eval, args)),
        "thread-alive-p" => return Some(super::threads::builtin_thread_alive_p(eval, args)),
        "thread-signal" => return Some(super::threads::builtin_thread_signal(eval, args)),
        "current-thread" => return Some(super::threads::builtin_current_thread(eval, args)),
        "all-threads" => return Some(super::threads::builtin_all_threads(eval, args)),
        "thread-last-error" => return Some(super::threads::builtin_thread_last_error(eval, args)),
        "make-mutex" => return Some(super::threads::builtin_make_mutex(eval, args)),
        "mutex-lock" => return Some(super::threads::builtin_mutex_lock(eval, args)),
        "mutex-unlock" => return Some(super::threads::builtin_mutex_unlock(eval, args)),
        "make-condition-variable" => return Some(super::threads::builtin_make_condition_variable(eval, args)),
        "condition-wait" => return Some(super::threads::builtin_condition_wait(eval, args)),
        "condition-notify" => return Some(super::threads::builtin_condition_notify(eval, args)),

        // Undo system (evaluator-dependent)
        "undo" => return Some(super::undo::builtin_undo(eval, args)),

        // Hash-table / obarray (evaluator-dependent)
        "maphash" => return Some(super::hashtab::builtin_maphash(eval, args)),
        "mapatoms" => return Some(super::hashtab::builtin_mapatoms(eval, args)),
        "unintern" => return Some(super::hashtab::builtin_unintern(eval, args)),

        // Marker (evaluator-dependent)
        "set-marker" => return Some(super::marker::builtin_set_marker(eval, args)),
        "point-marker" => return Some(super::marker::builtin_point_marker(eval, args)),
        "point-min-marker" => return Some(super::marker::builtin_point_min_marker(eval, args)),
        "point-max-marker" => return Some(super::marker::builtin_point_max_marker(eval, args)),

        // Category (evaluator-dependent)
        "modify-category-entry" => return Some(super::category::builtin_modify_category_entry(eval, args)),
        "char-category-set" => return Some(super::category::builtin_char_category_set(eval, args)),

        // Char-table (evaluator-dependent — applies function)
        "map-char-table" => return Some(super::chartable::builtin_map_char_table(eval, args)),

        // Coding system (evaluator-dependent — uses coding_systems manager)
        "coding-system-list" => return Some(super::coding::builtin_coding_system_list(&eval.coding_systems, args)),
        "coding-system-aliases" => return Some(super::coding::builtin_coding_system_aliases(&eval.coding_systems, args)),
        "coding-system-get" => return Some(super::coding::builtin_coding_system_get(&eval.coding_systems, args)),
        "coding-system-put" => return Some(super::coding::builtin_coding_system_put(&mut eval.coding_systems, args)),
        "coding-system-base" => return Some(super::coding::builtin_coding_system_base(&eval.coding_systems, args)),
        "coding-system-eol-type" => return Some(super::coding::builtin_coding_system_eol_type(&eval.coding_systems, args)),
        "coding-system-type" => return Some(super::coding::builtin_coding_system_type(&eval.coding_systems, args)),
        "coding-system-change-eol-conversion" => return Some(super::coding::builtin_coding_system_change_eol_conversion(&eval.coding_systems, args)),
        "coding-system-change-text-conversion" => return Some(super::coding::builtin_coding_system_change_text_conversion(&eval.coding_systems, args)),
        "find-coding-system" => return Some(super::coding::builtin_find_coding_system(&eval.coding_systems, args)),
        "detect-coding-string" => return Some(super::coding::builtin_detect_coding_string(&eval.coding_systems, args)),
        "detect-coding-region" => return Some(super::coding::builtin_detect_coding_region(&eval.coding_systems, args)),
        "keyboard-coding-system" => return Some(super::coding::builtin_keyboard_coding_system(&eval.coding_systems, args)),
        "terminal-coding-system" => return Some(super::coding::builtin_terminal_coding_system(&eval.coding_systems, args)),
        "set-keyboard-coding-system" => return Some(super::coding::builtin_set_keyboard_coding_system(&mut eval.coding_systems, args)),
        "set-terminal-coding-system" => return Some(super::coding::builtin_set_terminal_coding_system(&mut eval.coding_systems, args)),
        "coding-system-priority-list" => return Some(super::coding::builtin_coding_system_priority_list(&eval.coding_systems, args)),

        // CL-lib higher-order (evaluator-dependent — applies functions)
        "cl-map" => return Some(super::cl_lib::builtin_cl_map(eval, args)),
        "cl-every" => return Some(super::cl_lib::builtin_cl_every(eval, args)),
        "cl-some" => return Some(super::cl_lib::builtin_cl_some(eval, args)),
        "cl-notevery" => return Some(super::cl_lib::builtin_cl_notevery(eval, args)),
        "cl-notany" => return Some(super::cl_lib::builtin_cl_notany(eval, args)),
        "cl-reduce" => return Some(super::cl_lib::builtin_cl_reduce(eval, args)),
        "cl-remove-if" => return Some(super::cl_lib::builtin_cl_remove_if(eval, args)),
        "cl-remove-if-not" => return Some(super::cl_lib::builtin_cl_remove_if_not(eval, args)),
        "cl-find-if" => return Some(super::cl_lib::builtin_cl_find_if(eval, args)),
        "cl-count-if" => return Some(super::cl_lib::builtin_cl_count_if(eval, args)),
        "cl-sort" => return Some(super::cl_lib::builtin_cl_sort(eval, args)),
        "cl-stable-sort" => return Some(super::cl_lib::builtin_cl_stable_sort(eval, args)),
        "seq-mapn" => return Some(super::cl_lib::builtin_seq_mapn(eval, args)),
        "seq-do" => return Some(super::cl_lib::builtin_seq_do(eval, args)),
        "seq-count" => return Some(super::cl_lib::builtin_seq_count(eval, args)),
        "seq-reduce" => return Some(super::cl_lib::builtin_seq_reduce(eval, args)),
        "seq-some" => return Some(super::cl_lib::builtin_seq_some(eval, args)),
        "seq-every-p" => return Some(super::cl_lib::builtin_seq_every_p(eval, args)),
        "seq-sort" => return Some(super::cl_lib::builtin_seq_sort(eval, args)),
        "json-parse-buffer" => return Some(super::cl_lib::builtin_json_parse_buffer(eval, args)),
        "json-insert" => return Some(super::cl_lib::builtin_json_insert(eval, args)),

        // Documentation/help (evaluator-dependent)
        "documentation" => return Some(super::doc::builtin_documentation(eval, args)),
        "describe-function" => return Some(super::doc::builtin_describe_function(eval, args)),
        "describe-variable" => return Some(super::doc::builtin_describe_variable(eval, args)),

        // Indentation (evaluator-dependent)
        "indent-region" => return Some(super::indent::builtin_indent_region(eval, args)),
        "reindent-then-newline-and-indent" => return Some(super::indent::builtin_reindent_then_newline_and_indent(eval, args)),
        "indent-for-tab-command" => return Some(super::indent::builtin_indent_for_tab_command(eval, args)),
        "indent-according-to-mode" => return Some(super::indent::builtin_indent_according_to_mode(eval, args)),
        "back-to-indentation" => return Some(super::indent::builtin_back_to_indentation(eval, args)),

        // Case/char (evaluator-dependent)
        "upcase-initials-region" => return Some(super::casefiddle::builtin_upcase_initials_region(eval, args)),

        // Search (evaluator-dependent)
        "posix-search-forward" => return Some(super::search::builtin_posix_search_forward(eval, args)),
        "posix-search-backward" => return Some(super::search::builtin_posix_search_backward(eval, args)),
        "word-search-forward" => return Some(super::search::builtin_word_search_forward(eval, args)),
        "word-search-backward" => return Some(super::search::builtin_word_search_backward(eval, args)),

        // Lread (evaluator-dependent)
        "eval-buffer" => return Some(super::lread::builtin_eval_buffer(eval, args)),
        "eval-region" => return Some(super::lread::builtin_eval_region(eval, args)),
        "read-event" => return Some(super::lread::builtin_read_event(eval, args)),
        "read-char-exclusive" => return Some(super::lread::builtin_read_char_exclusive(eval, args)),

        // Editfns (evaluator-dependent)
        "insert-before-markers" => return Some(super::editfns::builtin_insert_before_markers(eval, args)),
        "delete-char" => return Some(super::editfns::builtin_delete_char(eval, args)),
        "buffer-substring-no-properties" => return Some(super::editfns::builtin_buffer_substring_no_properties(eval, args)),
        "following-char" => return Some(super::editfns::builtin_following_char(eval, args)),
        "preceding-char" => return Some(super::editfns::builtin_preceding_char(eval, args)),

        _ => {}
    }

    // Pure builtins (no evaluator needed)
    Some(match name {
        // Arithmetic
        "+" => builtin_add(args),
        "-" => builtin_sub(args),
        "*" => builtin_mul(args),
        "/" => builtin_div(args),
        "%" | "mod" => builtin_mod(args),
        "1+" => builtin_add1(args),
        "1-" => builtin_sub1(args),
        "max" => builtin_max(args),
        "min" => builtin_min(args),
        "abs" => builtin_abs(args),

        // Logical / bitwise
        "logand" => builtin_logand(args),
        "logior" => builtin_logior(args),
        "logxor" => builtin_logxor(args),
        "lognot" => builtin_lognot(args),
        "ash" => builtin_ash(args),

        // Numeric comparisons
        "=" => builtin_num_eq(args),
        "<" => builtin_num_lt(args),
        "<=" => builtin_num_le(args),
        ">" => builtin_num_gt(args),
        ">=" => builtin_num_ge(args),
        "/=" => builtin_num_ne(args),

        // Type predicates
        "null" => builtin_null(args),
        "not" => builtin_not(args),
        "atom" => builtin_atom(args),
        "consp" => builtin_consp(args),
        "listp" => builtin_listp(args),
        "nlistp" => builtin_nlistp(args),
        "symbolp" => builtin_symbolp(args),
        "numberp" => builtin_numberp(args),
        "integerp" => builtin_integerp(args),
        "floatp" => builtin_floatp(args),
        "stringp" => builtin_stringp(args),
        "vectorp" => builtin_vectorp(args),
        "characterp" => builtin_characterp(args),
        "functionp" => builtin_functionp(args),
        "keywordp" => builtin_keywordp(args),
        "hash-table-p" => builtin_hash_table_p(args),
        "bufferp" => builtin_bufferp(args),
        "type-of" => builtin_type_of(args),
        "sequencep" => builtin_sequencep(args),
        "arrayp" => builtin_arrayp(args),

        // Equality
        "eq" => builtin_eq(args),
        "eql" => builtin_eql(args),
        "equal" => builtin_equal(args),

        // Cons / List
        "cons" => builtin_cons(args),
        "car" => builtin_car(args),
        "cdr" => builtin_cdr(args),
        "car-safe" => builtin_car_safe(args),
        "cdr-safe" => builtin_cdr_safe(args),
        "setcar" => builtin_setcar(args),
        "setcdr" => builtin_setcdr(args),
        "list" => builtin_list(args),
        "length" => builtin_length(args),
        "nth" => builtin_nth(args),
        "nthcdr" => builtin_nthcdr(args),
        "append" => builtin_append(args),
        "reverse" => builtin_reverse(args),
        "nreverse" => builtin_nreverse(args),
        "member" => builtin_member(args),
        "memq" => builtin_memq(args),
        "assoc" => builtin_assoc(args),
        "assq" => builtin_assq(args),
        "copy-sequence" => builtin_copy_sequence(args),

        // String
        "string-equal" | "string=" => builtin_string_equal(args),
        "string-lessp" | "string<" => builtin_string_lessp(args),
        "substring" => builtin_substring(args),
        "concat" => builtin_concat(args),
        "string-to-number" => builtin_string_to_number(args),
        "number-to-string" => builtin_number_to_string(args),
        "upcase" => builtin_upcase(args),
        "downcase" => builtin_downcase(args),
        "format" => builtin_format(args),

        // Vector
        "make-vector" => builtin_make_vector(args),
        "vector" => builtin_vector(args),
        "aref" => builtin_aref(args),
        "aset" => builtin_aset(args),
        "vconcat" => builtin_vconcat(args),

        // Hash table
        "make-hash-table" => builtin_make_hash_table(args),
        "gethash" => builtin_gethash(args),
        "puthash" => builtin_puthash(args),
        "remhash" => builtin_remhash(args),
        "clrhash" => builtin_clrhash(args),
        "hash-table-count" => builtin_hash_table_count(args),

        // Conversion
        "float" => builtin_float(args),
        "truncate" => builtin_truncate(args),
        "floor" => builtin_floor(args),
        "ceiling" => builtin_ceiling(args),
        "round" => builtin_round(args),
        "char-to-string" => builtin_char_to_string(args),
        "string-to-char" => builtin_string_to_char(args),

        // Property lists
        "plist-get" => builtin_plist_get(args),
        "plist-put" => builtin_plist_put(args),

        // Symbol (pure)
        "symbol-name" => builtin_symbol_name(args),
        "make-symbol" => builtin_make_symbol(args),

        // Math
        "sqrt" => builtin_sqrt(args),
        "sin" => builtin_sin(args),
        "cos" => builtin_cos(args),
        "tan" => builtin_tan(args),
        "asin" => builtin_asin(args),
        "acos" => builtin_acos(args),
        "atan" => builtin_atan(args),
        "exp" => builtin_exp(args),
        "log" => builtin_log(args),
        "expt" => builtin_expt(args),
        "random" => builtin_random(args),
        "isnan" => builtin_isnan(args),

        // Extended string
        "string-prefix-p" => builtin_string_prefix_p(args),
        "string-suffix-p" => builtin_string_suffix_p(args),
        "string-join" => builtin_string_join(args),
        "split-string" => builtin_split_string(args),
        "string-trim" => builtin_string_trim(args),
        "string-trim-left" => builtin_string_trim_left(args),
        "string-trim-right" => builtin_string_trim_right(args),
        "make-string" => builtin_make_string(args),
        "string-to-list" => builtin_string_to_list(args),
        "string-width" => builtin_string_width(args),

        // Extended list
        "last" => builtin_last(args),
        "butlast" => builtin_butlast(args),
        "delete" => builtin_delete(args),
        "delq" => builtin_delq(args),
        "elt" => builtin_elt(args),
        "nconc" => builtin_nconc(args),
        "alist-get" => builtin_alist_get(args),
        "number-sequence" => builtin_number_sequence(args),

        // Output / misc
        "identity" => builtin_identity(args),
        "message" => builtin_message(args),
        "error" => builtin_error(args),
        "princ" => builtin_princ(args),
        "prin1" => builtin_prin1(args),
        "prin1-to-string" => builtin_prin1_to_string(args),
        "print" => builtin_print(args),
        "propertize" => builtin_propertize(args),
        "gensym" => builtin_gensym(args),
        "string-to-syntax" => builtin_string_to_syntax(args),
        "make-syntax-table" => super::syntax::builtin_make_syntax_table(args),
        "current-time" => builtin_current_time(args),
        "float-time" => builtin_float_time(args),

        // File I/O (pure)
        "expand-file-name" => super::fileio::builtin_expand_file_name(args),
        "file-name-directory" => super::fileio::builtin_file_name_directory(args),
        "file-name-nondirectory" => super::fileio::builtin_file_name_nondirectory(args),
        "file-name-extension" => super::fileio::builtin_file_name_extension(args),
        "file-name-sans-extension" => super::fileio::builtin_file_name_sans_extension(args),
        "file-exists-p" => super::fileio::builtin_file_exists_p(args),
        "file-readable-p" => super::fileio::builtin_file_readable_p(args),
        "file-writable-p" => super::fileio::builtin_file_writable_p(args),
        "file-directory-p" => super::fileio::builtin_file_directory_p(args),
        "file-regular-p" => super::fileio::builtin_file_regular_p(args),
        "file-symlink-p" => super::fileio::builtin_file_symlink_p(args),
        "delete-file" => super::fileio::builtin_delete_file(args),
        "rename-file" => super::fileio::builtin_rename_file(args),
        "copy-file" => super::fileio::builtin_copy_file(args),
        "make-directory" => super::fileio::builtin_make_directory(args),
        "directory-files" => super::fileio::builtin_directory_files(args),
        "file-attributes" => super::fileio::builtin_file_attributes(args),

        // Keymap (pure — no evaluator needed)
        "kbd" => builtin_kbd(args),

        // Process (pure — no evaluator needed)
        "shell-command-to-string" => super::process::builtin_shell_command_to_string(args),
        "getenv" => super::process::builtin_getenv(args),
        "setenv" => super::process::builtin_setenv(args),

        // Timer (pure — no evaluator needed)
        "timerp" => super::timer::builtin_timerp(args),
        "sit-for" => super::timer::builtin_sit_for(args),

        // Register (pure — no evaluator needed)
        "register-to-string" => super::register::builtin_register_to_string(args),

        // Undo system (pure — no evaluator needed)
        "undo-boundary" => super::undo::builtin_undo_boundary(args),
        "primitive-undo" => super::undo::builtin_primitive_undo(args),

        // Keyboard macro (pure — no evaluator needed)
        "executing-kbd-macro-p" => super::kmacro::builtin_executing_kbd_macro_p(args),
        "kmacro-p" => super::kmacro::builtin_kmacro_p(args),

        // Case table (pure)
        "case-table-p" => super::casetab::builtin_case_table_p(args),
        "current-case-table" => super::casetab::builtin_current_case_table(args),
        "standard-case-table" => super::casetab::builtin_standard_case_table(args),
        "set-case-table" => super::casetab::builtin_set_case_table(args),
        "set-standard-case-table" => super::casetab::builtin_set_standard_case_table(args),
        "upcase-char" => super::casetab::builtin_upcase_char(args),
        "downcase-char" => super::casetab::builtin_downcase_char(args),

        // Category (pure)
        "define-category" => super::category::builtin_define_category(args),
        "category-docstring" => super::category::builtin_category_docstring(args),
        "get-unused-category" => super::category::builtin_get_unused_category(args),
        "category-table-p" => super::category::builtin_category_table_p(args),
        "category-table" => super::category::builtin_category_table(args),
        "standard-category-table" => super::category::builtin_standard_category_table(args),
        "make-category-table" => super::category::builtin_make_category_table(args),
        "set-category-table" => super::category::builtin_set_category_table(args),
        "make-category-set" => super::category::builtin_make_category_set(args),

        // Display/terminal (pure)
        "display-graphic-p" => super::display::builtin_display_graphic_p(args),
        "display-color-p" => super::display::builtin_display_color_p(args),
        "display-pixel-width" => super::display::builtin_display_pixel_width(args),
        "display-pixel-height" => super::display::builtin_display_pixel_height(args),
        "display-mm-width" => super::display::builtin_display_mm_width(args),
        "display-mm-height" => super::display::builtin_display_mm_height(args),
        "display-screens" => super::display::builtin_display_screens(args),
        "display-color-cells" => super::display::builtin_display_color_cells(args),
        "display-planes" => super::display::builtin_display_planes(args),
        "display-visual-class" => super::display::builtin_display_visual_class(args),
        "display-backing-store" => super::display::builtin_display_backing_store(args),
        "x-display-list" => super::display::builtin_x_display_list(args),
        "x-open-connection" => super::display::builtin_x_open_connection(args),
        "x-close-connection" => super::display::builtin_x_close_connection(args),
        "x-display-pixel-width" => super::display::builtin_x_display_pixel_width(args),
        "x-display-pixel-height" => super::display::builtin_x_display_pixel_height(args),
        "x-display-color-p" => super::display::builtin_x_display_color_p(args),
        "terminal-name" => super::display::builtin_terminal_name(args),
        "terminal-list" => super::display::builtin_terminal_list(args),
        "frame-terminal" => super::display::builtin_frame_terminal(args),
        "selected-terminal" => super::display::builtin_selected_terminal(args),
        "terminal-live-p" => super::display::builtin_terminal_live_p(args),
        "terminal-parameter" => super::display::builtin_terminal_parameter(args),
        "set-terminal-parameter" => super::display::builtin_set_terminal_parameter(args),
        "tty-type" => super::display::builtin_tty_type(args),
        "tty-top-frame" => super::display::builtin_tty_top_frame(args),
        "controlling-tty-p" => super::display::builtin_controlling_tty_p(args),
        "suspend-tty" => super::display::builtin_suspend_tty(args),
        "resume-tty" => super::display::builtin_resume_tty(args),
        "display-monitor-attributes-list" => super::display::builtin_display_monitor_attributes_list(args),
        "frame-monitor-attributes" => super::display::builtin_frame_monitor_attributes(args),
        "display-images-p" | "display-supports-face-attributes-p" => Ok(Value::True),

        // Image (pure)
        "image-type-available-p" => super::image::builtin_image_type_available_p(args),
        "create-image" => super::image::builtin_create_image(args),
        "image-size" => super::image::builtin_image_size(args),
        "image-mask-p" => super::image::builtin_image_mask_p(args),
        "put-image" => super::image::builtin_put_image(args),
        "insert-image" => super::image::builtin_insert_image(args),
        "remove-images" => super::image::builtin_remove_images(args),
        "image-flush" => super::image::builtin_image_flush(args),
        "clear-image-cache" => super::image::builtin_clear_image_cache(args),
        "image-type" => super::image::builtin_image_type(args),
        "image-transforms-p" => super::image::builtin_image_transforms_p(args),

        // Character encoding
        "char-width" => crate::encoding::builtin_char_width(args),
        "multibyte-string-p" => crate::encoding::builtin_multibyte_string_p(args),
        "unibyte-string-p" => crate::encoding::builtin_unibyte_string_p(args),
        "encode-coding-string" => crate::encoding::builtin_encode_coding_string(args),
        "decode-coding-string" => crate::encoding::builtin_decode_coding_string(args),
        "char-or-string-p" => crate::encoding::builtin_char_or_string_p(args),
        "max-char" => crate::encoding::builtin_max_char(args),

        // Extra builtins
        "remove" => super::builtins_extra::builtin_remove(args),
        "remq" => super::builtins_extra::builtin_remq(args),
        "flatten-tree" => super::builtins_extra::builtin_flatten_tree(args),
        "take" => super::builtins_extra::builtin_take(args),
        "seq-position" => super::builtins_extra::builtin_seq_position(args),
        "seq-uniq" => super::builtins_extra::builtin_seq_uniq(args),
        "seq-contains-p" => super::builtins_extra::builtin_seq_contains_p(args),
        "seq-length" => super::builtins_extra::builtin_seq_length(args),
        "seq-into" => super::builtins_extra::builtin_seq_into(args),
        "string-empty-p" => super::builtins_extra::builtin_string_empty_p(args),
        "string-blank-p" => super::builtins_extra::builtin_string_blank_p(args),
        "string-chop-newline" => super::builtins_extra::builtin_string_chop_newline(args),
        "string-pad" => super::builtins_extra::builtin_string_pad(args),
        "string-repeat" => super::builtins_extra::builtin_string_repeat(args),
        "string-replace" => super::builtins_extra::builtin_string_replace(args),
        "string-search" => super::builtins_extra::builtin_string_search(args),
        "string-to-vector" => super::builtins_extra::builtin_string_to_vector(args),
        "proper-list-p" => super::builtins_extra::builtin_proper_list_p(args),
        "subrp" => super::builtins_extra::builtin_subrp(args),
        "byte-code-function-p" => super::builtins_extra::builtin_byte_code_function_p(args),
        "compiled-function-p" => super::builtins_extra::builtin_compiled_function_p(args),
        "closurep" => super::builtins_extra::builtin_closurep(args),
        "natnump" => super::builtins_extra::builtin_natnump(args),
        "fixnump" => super::builtins_extra::builtin_fixnump(args),
        "bignump" => super::builtins_extra::builtin_bignump(args),
        "wholenump" => super::builtins_extra::builtin_wholenump(args),
        "zerop" => super::builtins_extra::builtin_zerop(args),
        "cl-oddp" => super::builtins_extra::builtin_cl_oddp(args),
        "cl-evenp" => super::builtins_extra::builtin_cl_evenp(args),
        "cl-plusp" => super::builtins_extra::builtin_cl_plusp(args),
        "cl-minusp" => super::builtins_extra::builtin_cl_minusp(args),
        "user-login-name" => super::builtins_extra::builtin_user_login_name(args),
        "user-real-login-name" => super::builtins_extra::builtin_user_real_login_name(args),
        "user-full-name" => super::builtins_extra::builtin_user_full_name(args),
        "system-name" => super::builtins_extra::builtin_system_name(args),
        "emacs-version" => super::builtins_extra::builtin_emacs_version(args),
        "emacs-pid" => super::builtins_extra::builtin_emacs_pid(args),
        "garbage-collect" => super::builtins_extra::builtin_garbage_collect(args),
        "cl-gensym" => super::builtins_extra::builtin_cl_gensym(args),
        // Note: overlayp is in the eval-dependent section above

        // Autoload (pure)
        "autoloadp" => super::autoload::builtin_autoloadp(args),
        "symbol-file" => super::autoload::builtin_symbol_file(args),

        // Time/date (pure)
        "time-add" => super::timefns::builtin_time_add(args),
        "time-subtract" => super::timefns::builtin_time_subtract(args),
        "time-less-p" => super::timefns::builtin_time_less_p(args),
        "time-equal-p" => super::timefns::builtin_time_equal_p(args),
        "current-time-string" => super::timefns::builtin_current_time_string(args),
        "current-time-zone" => super::timefns::builtin_current_time_zone(args),
        "encode-time" => super::timefns::builtin_encode_time(args),
        "decode-time" => super::timefns::builtin_decode_time(args),
        "time-convert" => super::timefns::builtin_time_convert(args),
        "set-time-zone-rule" => super::timefns::builtin_set_time_zone_rule(args),
        "safe-date-to-time" => super::timefns::builtin_safe_date_to_time(args),

        // Float/math (pure)
        "copysign" => super::floatfns::builtin_copysign(args),
        "frexp" => super::floatfns::builtin_frexp(args),
        "ldexp" => super::floatfns::builtin_ldexp(args),
        "logb" => super::floatfns::builtin_logb(args),
        "fceiling" => super::floatfns::builtin_fceiling(args),
        "ffloor" => super::floatfns::builtin_ffloor(args),
        "fround" => super::floatfns::builtin_fround(args),
        "ftruncate" => super::floatfns::builtin_ftruncate(args),

        // Case/char (pure)
        "capitalize" => super::casefiddle::builtin_capitalize(args),
        "upcase-initials" => super::casefiddle::builtin_upcase_initials(args),
        "char-resolve-modifiers" => super::casefiddle::builtin_char_resolve_modifiers(args),
        "get-byte" => super::casefiddle::builtin_get_byte(args),

        // Font/face (pure)
        "fontp" => super::font::builtin_fontp(args),
        "font-spec" => super::font::builtin_font_spec(args),
        "font-get" => super::font::builtin_font_get(args),
        "font-put" => super::font::builtin_font_put(args),
        "list-fonts" => super::font::builtin_list_fonts(args),
        "find-font" => super::font::builtin_find_font(args),
        "clear-font-cache" => super::font::builtin_clear_font_cache(args),
        "font-family-list" => super::font::builtin_font_family_list(args),
        "font-xlfd-name" => super::font::builtin_font_xlfd_name(args),
        "internal-lisp-face-p" => super::font::builtin_internal_lisp_face_p(args),
        "internal-copy-lisp-face" => super::font::builtin_internal_copy_lisp_face(args),
        "internal-set-lisp-face-attribute" => super::font::builtin_internal_set_lisp_face_attribute(args),
        "internal-get-lisp-face-attribute" => super::font::builtin_internal_get_lisp_face_attribute(args),
        "internal-merge-in-global-face" => super::font::builtin_internal_merge_in_global_face(args),
        "face-attribute-relative-p" => super::font::builtin_face_attribute_relative_p(args),
        "merge-face-attribute" => super::font::builtin_merge_face_attribute(args),
        "face-list" => super::font::builtin_face_list(args),
        "color-defined-p" => super::font::builtin_color_defined_p(args),
        "color-values" => super::font::builtin_color_values(args),
        "defined-colors" => super::font::builtin_defined_colors(args),
        "face-id" => super::font::builtin_face_id(args),
        "face-font" => super::font::builtin_face_font(args),
        "internal-face-x-get-resource" => super::font::builtin_internal_face_x_get_resource(args),
        "internal-set-font-selection-order" => super::font::builtin_internal_set_font_selection_order(args),
        "internal-set-alternative-font-family-alist" => super::font::builtin_internal_set_alternative_font_family_alist(args),
        "internal-set-alternative-font-registry-alist" => super::font::builtin_internal_set_alternative_font_registry_alist(args),

        // Directory/file attributes (pure)
        "directory-files-and-attributes" => super::dired::builtin_directory_files_and_attributes(args),
        "file-name-completion" => super::dired::builtin_file_name_completion(args),
        "file-name-all-completions" => super::dired::builtin_file_name_all_completions(args),
        "file-attributes-lessp" => super::dired::builtin_file_attributes_lessp(args),
        "system-users" => super::dired::builtin_system_users(args),
        "system-groups" => super::dired::builtin_system_groups(args),

        // Display engine (pure)
        "format-mode-line" => super::xdisp::builtin_format_mode_line(args),
        "invisible-p" => super::xdisp::builtin_invisible_p(args),
        "line-pixel-height" => super::xdisp::builtin_line_pixel_height(args),
        "window-text-pixel-size" => super::xdisp::builtin_window_text_pixel_size(args),
        "pos-visible-in-window-p" => super::xdisp::builtin_pos_visible_in_window_p(args),
        "move-point-visually" => super::xdisp::builtin_move_point_visually(args),
        "lookup-image-map" => super::xdisp::builtin_lookup_image_map(args),
        "current-bidi-paragraph-direction" => super::xdisp::builtin_current_bidi_paragraph_direction(args),
        "move-to-window-line" => super::xdisp::builtin_move_to_window_line(args),
        "tool-bar-height" => super::xdisp::builtin_tool_bar_height(args),
        "tab-bar-height" => super::xdisp::builtin_tab_bar_height(args),
        "display-line-numbers-update-width" => super::xdisp::builtin_display_line_numbers_update_width(args),
        "line-number-display-width" => super::xdisp::builtin_line_number_display_width(args),
        "long-line-optimizations-p" => super::xdisp::builtin_long_line_optimizations_p(args),

        // Charset (pure)
        "charsetp" => super::charset::builtin_charsetp(args),
        "charset-list" => super::charset::builtin_charset_list(args),
        "charset-priority-list" => super::charset::builtin_charset_priority_list(args),
        "set-charset-priority" => super::charset::builtin_set_charset_priority(args),
        "char-charset" => super::charset::builtin_char_charset(args),
        "charset-plist" => super::charset::builtin_charset_plist(args),
        "charset-id-internal" => super::charset::builtin_charset_id_internal(args),
        "define-charset-internal" => super::charset::builtin_define_charset_internal(args),
        "find-charset-region" => super::charset::builtin_find_charset_region(args),
        "find-charset-string" => super::charset::builtin_find_charset_string(args),
        "decode-char" => super::charset::builtin_decode_char(args),
        "encode-char" => super::charset::builtin_encode_char(args),
        "clear-charset-maps" => super::charset::builtin_clear_charset_maps(args),
        "charset-after" => super::charset::builtin_charset_after(args),
        "unibyte-charset" => super::charset::builtin_unibyte_charset(args),

        // CCL (pure)
        "ccl-program-p" => super::ccl::builtin_ccl_program_p(args),
        "ccl-execute" => super::ccl::builtin_ccl_execute(args),
        "ccl-execute-on-string" => super::ccl::builtin_ccl_execute_on_string(args),
        "register-ccl-program" => super::ccl::builtin_register_ccl_program(args),
        "register-code-conversion-map" => super::ccl::builtin_register_code_conversion_map(args),

        // XML/decompress (pure)
        "libxml-parse-html-region" => super::xml::builtin_libxml_parse_html_region(args),
        "libxml-parse-xml-region" => super::xml::builtin_libxml_parse_xml_region(args),
        "libxml-available-p" => super::xml::builtin_libxml_available_p(args),
        "zlib-available-p" => super::xml::builtin_zlib_available_p(args),
        "zlib-decompress-region" => super::xml::builtin_zlib_decompress_region(args),

        // Custom system (pure)
        "custom-set-faces" => super::custom::builtin_custom_set_faces(args),

        // Documentation/help (pure)
        "documentation-property" => super::doc::builtin_documentation_property(args),
        "Snarf-documentation" => super::doc::builtin_snarf_documentation(args),
        "substitute-command-keys" => super::doc::builtin_substitute_command_keys(args),
        "help-function-arglist" => super::doc::builtin_help_function_arglist(args),

        // JSON (pure)
        "json-serialize" => super::json::builtin_json_serialize(args),
        "json-parse-string" => super::json::builtin_json_parse_string(args),

        // Subr introspection (pure)
        "subrp" => super::subr_info::builtin_subrp(args),
        "subr-name" => super::subr_info::builtin_subr_name(args),
        "subr-arity" => super::subr_info::builtin_subr_arity(args),
        "functionp" => super::subr_info::builtin_functionp(args),
        "byte-code-function-p" => super::subr_info::builtin_byte_code_function_p(args),
        "closurep" => super::subr_info::builtin_closurep(args),
        "interpreted-function-p" => super::subr_info::builtin_interpreted_function_p(args),
        "special-form-p" => super::subr_info::builtin_special_form_p(args),
        "macrop" => super::subr_info::builtin_macrop(args),
        "commandp" => super::subr_info::builtin_commandp(args),
        "func-arity" => super::subr_info::builtin_func_arity(args),

        // Format/string utilities (pure)
        "format-spec" => super::format::builtin_format_spec(args),
        "format-time-string" => super::format::builtin_format_time_string(args),
        "format-seconds" => super::format::builtin_format_seconds(args),
        "string-pad" => super::format::builtin_string_pad(args),
        "string-chop-newline" => super::format::builtin_string_chop_newline(args),
        "string-lines" => super::format::builtin_string_lines(args),
        "string-clean-whitespace" => super::format::builtin_string_clean_whitespace(args),
        "string-fill" => super::format::builtin_string_fill(args),
        "string-limit" => super::format::builtin_string_limit(args),
        "string-pixel-width" => super::format::builtin_string_pixel_width(args),
        "string-glyph-split" => super::format::builtin_string_glyph_split(args),
        "string-equal-ignore-case" => super::format::builtin_string_equal_ignore_case(args),

        // Marker (pure)
        "markerp" => super::marker::builtin_markerp(args),
        "marker-position" => super::marker::builtin_marker_position(args),
        "marker-buffer" => super::marker::builtin_marker_buffer(args),
        "marker-insertion-type" => super::marker::builtin_marker_insertion_type(args),
        "set-marker-insertion-type" => super::marker::builtin_set_marker_insertion_type(args),
        "copy-marker" => super::marker::builtin_copy_marker(args),
        "make-marker" => super::marker::builtin_make_marker(args),

        // Composite (pure)
        "compose-region-internal" => super::composite::builtin_compose_region_internal(args),
        "compose-string-internal" => super::composite::builtin_compose_string_internal(args),
        "find-composition-internal" => super::composite::builtin_find_composition_internal(args),
        "composition-get-gstring" => super::composite::builtin_composition_get_gstring(args),
        "clear-composition-cache" => super::composite::builtin_clear_composition_cache(args),
        "composition-sort-rules" => super::composite::builtin_composition_sort_rules(args),
        "auto-composition-mode" => super::composite::builtin_auto_composition_mode(args),

        // Indentation (pure)
        "current-indentation" => super::indent::builtin_current_indentation(args),
        "indent-to" => super::indent::builtin_indent_to(args),
        "current-column" => super::indent::builtin_current_column(args),
        "move-to-column" => super::indent::builtin_move_to_column(args),

        // Error hierarchy (pure)
        "signal" => super::errors::builtin_signal(args),

        // Hash-table extended (pure)
        "hash-table-keys" => super::hashtab::builtin_hash_table_keys(args),
        "hash-table-values" => super::hashtab::builtin_hash_table_values(args),
        "hash-table-test" => super::hashtab::builtin_hash_table_test(args),
        "hash-table-size" => super::hashtab::builtin_hash_table_size(args),
        "hash-table-rehash-size" => super::hashtab::builtin_hash_table_rehash_size(args),
        "hash-table-rehash-threshold" => super::hashtab::builtin_hash_table_rehash_threshold(args),
        "hash-table-weakness" => super::hashtab::builtin_hash_table_weakness(args),
        "copy-hash-table" => super::hashtab::builtin_copy_hash_table(args),

        // Threading (pure)
        "threadp" => super::threads::builtin_threadp(args),
        "mutexp" => super::threads::builtin_mutexp(args),
        "condition-variable-p" => super::threads::builtin_condition_variable_p(args),

        // Misc (pure)
        "copy-alist" => super::misc::builtin_copy_alist(args),
        "rassoc" => super::misc::builtin_rassoc(args),
        "rassq" => super::misc::builtin_rassq(args),
        "assoc-default" => super::misc::builtin_assoc_default(args),
        "make-list" => super::misc::builtin_make_list(args),
        "safe-length" => super::misc::builtin_safe_length(args),
        "subst-char-in-string" => super::misc::builtin_subst_char_in_string(args),
        "string-to-multibyte" => super::misc::builtin_string_to_multibyte(args),
        "string-to-unibyte" => super::misc::builtin_string_to_unibyte(args),
        "string-as-unibyte" => super::misc::builtin_string_as_unibyte(args),
        "string-as-multibyte" => super::misc::builtin_string_as_multibyte(args),
        "unibyte-char-to-multibyte" => super::misc::builtin_unibyte_char_to_multibyte(args),
        "multibyte-char-to-unibyte" => super::misc::builtin_multibyte_char_to_unibyte(args),
        "decode-char" => super::misc::builtin_decode_char(args),
        "encode-char" => super::misc::builtin_encode_char(args),
        "define-coding-system-alias" => super::misc::builtin_define_coding_system_alias(args),
        "coding-system-p" => super::misc::builtin_coding_system_p(args),
        "check-coding-system" => super::misc::builtin_check_coding_system(args),
        "set-coding-system-priority" => super::misc::builtin_set_coding_system_priority(args),
        "locale-info" => super::misc::builtin_locale_info(args),

        // Reader/printer (pure)
        "prin1-to-string" => super::reader::builtin_prin1_to_string_full(args),
        "format-spec" => super::reader::builtin_format_spec(args),
        "y-or-n-p" => super::reader::builtin_y_or_n_p(args),
        "yes-or-no-p" => super::reader::builtin_yes_or_no_p(args),

        // Char-table / bool-vector (pure)
        "make-char-table" => super::chartable::builtin_make_char_table(args),
        "char-table-p" => super::chartable::builtin_char_table_p(args),
        "set-char-table-range" => super::chartable::builtin_set_char_table_range(args),
        "char-table-range" => super::chartable::builtin_char_table_range(args),
        "char-table-parent" => super::chartable::builtin_char_table_parent(args),
        "set-char-table-parent" => super::chartable::builtin_set_char_table_parent(args),
        "char-table-extra-slot" => super::chartable::builtin_char_table_extra_slot(args),
        "set-char-table-extra-slot" => super::chartable::builtin_set_char_table_extra_slot(args),
        "char-table-subtype" => super::chartable::builtin_char_table_subtype(args),
        "make-bool-vector" => super::chartable::builtin_make_bool_vector(args),
        "bool-vector-p" => super::chartable::builtin_bool_vector_p(args),
        "bool-vector-count-population" => super::chartable::builtin_bool_vector_count_population(args),
        "bool-vector-intersection" => super::chartable::builtin_bool_vector_intersection(args),
        "bool-vector-union" => super::chartable::builtin_bool_vector_union(args),
        "bool-vector-exclusive-or" => super::chartable::builtin_bool_vector_exclusive_or(args),
        "bool-vector-complement" => super::chartable::builtin_bool_vector_complement(args),
        "bool-vector-subsetp" => super::chartable::builtin_bool_vector_subsetp(args),

        // Note: windowp and framep are in the eval-dependent section above

        // CL-lib (pure)
        "cl-find" => super::cl_lib::builtin_cl_find(args),
        "cl-position" => super::cl_lib::builtin_cl_position(args),
        "cl-count" => super::cl_lib::builtin_cl_count(args),
        "cl-remove" => super::cl_lib::builtin_cl_remove(args),
        "cl-substitute" => super::cl_lib::builtin_cl_substitute(args),
        "cl-intersection" => super::cl_lib::builtin_cl_intersection(args),
        "cl-union" => super::cl_lib::builtin_cl_union(args),
        "cl-set-difference" => super::cl_lib::builtin_cl_set_difference(args),
        "cl-subsetp" => super::cl_lib::builtin_cl_subsetp(args),
        "cl-adjoin" => super::cl_lib::builtin_cl_adjoin(args),
        "cl-remove-duplicates" => super::cl_lib::builtin_cl_remove_duplicates(args),
        "cl-first" => super::cl_lib::builtin_cl_first(args),
        "cl-second" => super::cl_lib::builtin_cl_second(args),
        "cl-third" => super::cl_lib::builtin_cl_third(args),
        "cl-fourth" => super::cl_lib::builtin_cl_fourth(args),
        "cl-fifth" => super::cl_lib::builtin_cl_fifth(args),
        "cl-sixth" => super::cl_lib::builtin_cl_sixth(args),
        "cl-seventh" => super::cl_lib::builtin_cl_seventh(args),
        "cl-eighth" => super::cl_lib::builtin_cl_eighth(args),
        "cl-ninth" => super::cl_lib::builtin_cl_ninth(args),
        "cl-tenth" => super::cl_lib::builtin_cl_tenth(args),
        "cl-rest" => super::cl_lib::builtin_cl_rest(args),
        "cl-subseq" => super::cl_lib::builtin_cl_subseq(args),
        "cl-concatenate" => super::cl_lib::builtin_cl_concatenate(args),
        "cl-coerce" => super::cl_lib::builtin_cl_coerce(args),
        "cl-member" => super::cl_lib::builtin_cl_member(args),
        "seq-reverse" => super::cl_lib::builtin_seq_reverse(args),
        "seq-drop" => super::cl_lib::builtin_seq_drop(args),
        "seq-take" => super::cl_lib::builtin_seq_take(args),
        "seq-subseq" => super::cl_lib::builtin_seq_subseq(args),
        "seq-concatenate" => super::cl_lib::builtin_seq_concatenate(args),
        "seq-empty-p" => super::cl_lib::builtin_seq_empty_p(args),
        "seq-min" => super::cl_lib::builtin_seq_min(args),
        "seq-max" => super::cl_lib::builtin_seq_max(args),
        "json-parse-string" => super::cl_lib::builtin_json_parse_string(args),
        "json-serialize" => super::cl_lib::builtin_json_serialize(args),

        // Search (pure)
        "string-match" => super::search::builtin_string_match(args),
        "string-match-p" => super::search::builtin_string_match_p(args),
        "regexp-quote" => super::search::builtin_regexp_quote(args),
        "match-beginning" => super::search::builtin_match_beginning(args),
        "match-end" => super::search::builtin_match_end(args),
        "match-data" => super::search::builtin_match_data(args),
        "set-match-data" => super::search::builtin_set_match_data(args),
        "looking-at" => super::search::builtin_looking_at(args),
        "replace-regexp-in-string" => super::search::builtin_replace_regexp_in_string(args),

        // Lread (pure)
        "get-load-suffixes" => super::lread::builtin_get_load_suffixes(args),
        "locate-file" => super::lread::builtin_locate_file(args),
        "locate-file-internal" => super::lread::builtin_locate_file_internal(args),
        "read-coding-system" => super::lread::builtin_read_coding_system(args),
        "read-non-nil-coding-system" => super::lread::builtin_read_non_nil_coding_system(args),

        // Editfns (pure)
        "user-login-name" => super::editfns::builtin_user_login_name(args),
        "user-real-login-name" => super::editfns::builtin_user_real_login_name(args),
        "user-uid" => super::editfns::builtin_user_uid(args),
        "user-real-uid" => super::editfns::builtin_user_real_uid(args),
        "user-full-name" => super::editfns::builtin_user_full_name(args),
        "system-name" => super::editfns::builtin_system_name(args),
        "emacs-pid" => super::editfns::builtin_emacs_pid(args),
        "group-gid" => super::editfns::builtin_group_gid(args),
        "group-real-gid" => super::editfns::builtin_group_real_gid(args),

        // Fns (pure)
        "base64-encode-string" => super::fns::builtin_base64_encode_string(args),
        "base64-decode-string" => super::fns::builtin_base64_decode_string(args),
        "base64url-encode-string" => super::fns::builtin_base64url_encode_string(args),
        "base64url-decode-string" => super::fns::builtin_base64url_decode_string(args),
        "md5" => super::fns::builtin_md5(args),
        "secure-hash" => super::fns::builtin_secure_hash(args),
        "buffer-hash" => super::fns::builtin_buffer_hash(args),
        "equal-including-properties" => super::fns::builtin_equal_including_properties(args),
        "widget-get" => super::fns::builtin_widget_get(args),
        "widget-put" => super::fns::builtin_widget_put(args),
        "widget-apply" => super::fns::builtin_widget_apply(args),
        "string-make-multibyte" => super::fns::builtin_string_make_multibyte(args),
        "string-make-unibyte" => super::fns::builtin_string_make_unibyte(args),
        "compare-strings" => super::fns::builtin_compare_strings(args),
        "string-version-lessp" => super::fns::builtin_string_version_lessp(args),
        "string-collate-lessp" => super::fns::builtin_string_collate_lessp(args),
        "string-collate-equalp" => super::fns::builtin_string_collate_equalp(args),

        _ => return None,
    })
}

/// Dispatch to pure builtins that don't need evaluator access.
/// Used by the bytecode VM.
pub(crate) fn dispatch_builtin_pure(name: &str, args: Vec<Value>) -> Option<EvalResult> {
    Some(match name {
        // Arithmetic
        "+" => builtin_add(args),
        "-" => builtin_sub(args),
        "*" => builtin_mul(args),
        "/" => builtin_div(args),
        "%" | "mod" => builtin_mod(args),
        "1+" => builtin_add1(args),
        "1-" => builtin_sub1(args),
        "max" => builtin_max(args),
        "min" => builtin_min(args),
        "abs" => builtin_abs(args),
        "logand" => builtin_logand(args),
        "logior" => builtin_logior(args),
        "logxor" => builtin_logxor(args),
        "lognot" => builtin_lognot(args),
        "ash" => builtin_ash(args),
        "=" => builtin_num_eq(args),
        "<" => builtin_num_lt(args),
        "<=" => builtin_num_le(args),
        ">" => builtin_num_gt(args),
        ">=" => builtin_num_ge(args),
        "/=" => builtin_num_ne(args),
        // Type predicates
        "null" => builtin_null(args),
        "not" => builtin_not(args),
        "atom" => builtin_atom(args),
        "consp" => builtin_consp(args),
        "listp" => builtin_listp(args),
        "nlistp" => builtin_nlistp(args),
        "symbolp" => builtin_symbolp(args),
        "numberp" => builtin_numberp(args),
        "integerp" => builtin_integerp(args),
        "floatp" => builtin_floatp(args),
        "stringp" => builtin_stringp(args),
        "vectorp" => builtin_vectorp(args),
        "characterp" => builtin_characterp(args),
        "functionp" => builtin_functionp(args),
        "keywordp" => builtin_keywordp(args),
        "hash-table-p" => builtin_hash_table_p(args),
        "bufferp" => builtin_bufferp(args),
        "type-of" => builtin_type_of(args),
        "sequencep" => builtin_sequencep(args),
        "arrayp" => builtin_arrayp(args),
        // Equality
        "eq" => builtin_eq(args),
        "eql" => builtin_eql(args),
        "equal" => builtin_equal(args),
        // Cons/List
        "cons" => builtin_cons(args),
        "car" => builtin_car(args),
        "cdr" => builtin_cdr(args),
        "car-safe" => builtin_car_safe(args),
        "cdr-safe" => builtin_cdr_safe(args),
        "setcar" => builtin_setcar(args),
        "setcdr" => builtin_setcdr(args),
        "list" => builtin_list(args),
        "length" => builtin_length(args),
        "nth" => builtin_nth(args),
        "nthcdr" => builtin_nthcdr(args),
        "append" => builtin_append(args),
        "reverse" => builtin_reverse(args),
        "nreverse" => builtin_nreverse(args),
        "member" => builtin_member(args),
        "memq" => builtin_memq(args),
        "assoc" => builtin_assoc(args),
        "assq" => builtin_assq(args),
        "copy-sequence" => builtin_copy_sequence(args),
        // String
        "string-equal" | "string=" => builtin_string_equal(args),
        "string-lessp" | "string<" => builtin_string_lessp(args),
        "substring" => builtin_substring(args),
        "concat" => builtin_concat(args),
        "string-to-number" => builtin_string_to_number(args),
        "number-to-string" => builtin_number_to_string(args),
        "upcase" => builtin_upcase(args),
        "downcase" => builtin_downcase(args),
        "format" => builtin_format(args),
        // Vector
        "make-vector" => builtin_make_vector(args),
        "vector" => builtin_vector(args),
        "aref" => builtin_aref(args),
        "aset" => builtin_aset(args),
        "vconcat" => builtin_vconcat(args),
        // Hash table
        "make-hash-table" => builtin_make_hash_table(args),
        "gethash" => builtin_gethash(args),
        "puthash" => builtin_puthash(args),
        "remhash" => builtin_remhash(args),
        "clrhash" => builtin_clrhash(args),
        "hash-table-count" => builtin_hash_table_count(args),
        // Conversion
        "float" => builtin_float(args),
        "truncate" => builtin_truncate(args),
        "floor" => builtin_floor(args),
        "ceiling" => builtin_ceiling(args),
        "round" => builtin_round(args),
        "char-to-string" => builtin_char_to_string(args),
        "string-to-char" => builtin_string_to_char(args),
        // Plist
        "plist-get" => builtin_plist_get(args),
        "plist-put" => builtin_plist_put(args),
        // Symbol
        "symbol-name" => builtin_symbol_name(args),
        "make-symbol" => builtin_make_symbol(args),
        // Math
        "sqrt" => builtin_sqrt(args),
        "sin" => builtin_sin(args),
        "cos" => builtin_cos(args),
        "tan" => builtin_tan(args),
        "asin" => builtin_asin(args),
        "acos" => builtin_acos(args),
        "atan" => builtin_atan(args),
        "exp" => builtin_exp(args),
        "log" => builtin_log(args),
        "expt" => builtin_expt(args),
        "random" => builtin_random(args),
        "isnan" => builtin_isnan(args),
        // Extended string
        "string-prefix-p" => builtin_string_prefix_p(args),
        "string-suffix-p" => builtin_string_suffix_p(args),
        "string-join" => builtin_string_join(args),
        "split-string" => builtin_split_string(args),
        "string-trim" => builtin_string_trim(args),
        "string-trim-left" => builtin_string_trim_left(args),
        "string-trim-right" => builtin_string_trim_right(args),
        "make-string" => builtin_make_string(args),
        "string-to-list" => builtin_string_to_list(args),
        "string-width" => builtin_string_width(args),
        // Extended list
        "last" => builtin_last(args),
        "butlast" => builtin_butlast(args),
        "delete" => builtin_delete(args),
        "delq" => builtin_delq(args),
        "elt" => builtin_elt(args),
        "nconc" => builtin_nconc(args),
        "alist-get" => builtin_alist_get(args),
        "number-sequence" => builtin_number_sequence(args),
        // Output / misc
        "identity" => builtin_identity(args),
        "message" => builtin_message(args),
        "error" => builtin_error(args),
        "princ" => builtin_princ(args),
        "prin1" => builtin_prin1(args),
        "prin1-to-string" => builtin_prin1_to_string(args),
        "print" => builtin_print(args),
        "propertize" => builtin_propertize(args),
        "gensym" => builtin_gensym(args),
        "string-to-syntax" => builtin_string_to_syntax(args),
        "make-syntax-table" => super::syntax::builtin_make_syntax_table(args),
        "current-time" => builtin_current_time(args),
        "float-time" => builtin_float_time(args),
        // File I/O (pure)
        "expand-file-name" => super::fileio::builtin_expand_file_name(args),
        "file-name-directory" => super::fileio::builtin_file_name_directory(args),
        "file-name-nondirectory" => super::fileio::builtin_file_name_nondirectory(args),
        "file-name-extension" => super::fileio::builtin_file_name_extension(args),
        "file-name-sans-extension" => super::fileio::builtin_file_name_sans_extension(args),
        "file-exists-p" => super::fileio::builtin_file_exists_p(args),
        "file-readable-p" => super::fileio::builtin_file_readable_p(args),
        "file-writable-p" => super::fileio::builtin_file_writable_p(args),
        "file-directory-p" => super::fileio::builtin_file_directory_p(args),
        "file-regular-p" => super::fileio::builtin_file_regular_p(args),
        "file-symlink-p" => super::fileio::builtin_file_symlink_p(args),
        "delete-file" => super::fileio::builtin_delete_file(args),
        "rename-file" => super::fileio::builtin_rename_file(args),
        "copy-file" => super::fileio::builtin_copy_file(args),
        "make-directory" => super::fileio::builtin_make_directory(args),
        "directory-files" => super::fileio::builtin_directory_files(args),
        "file-attributes" => super::fileio::builtin_file_attributes(args),
        // Keymap (pure)
        "kbd" => builtin_kbd(args),
        // Process (pure)
        "shell-command-to-string" => super::process::builtin_shell_command_to_string(args),
        "getenv" => super::process::builtin_getenv(args),
        "setenv" => super::process::builtin_setenv(args),
        // Timer (pure)
        "timerp" => super::timer::builtin_timerp(args),
        "sit-for" => super::timer::builtin_sit_for(args),
        // Register (pure)
        "register-to-string" => super::register::builtin_register_to_string(args),
        // Undo system (pure)
        "undo-boundary" => super::undo::builtin_undo_boundary(args),
        "primitive-undo" => super::undo::builtin_primitive_undo(args),
        // Keyboard macro (pure)
        "executing-kbd-macro-p" => super::kmacro::builtin_executing_kbd_macro_p(args),
        "kmacro-p" => super::kmacro::builtin_kmacro_p(args),
        // Character encoding (pure)
        "char-width" => crate::encoding::builtin_char_width(args),
        "multibyte-string-p" => crate::encoding::builtin_multibyte_string_p(args),
        "unibyte-string-p" => crate::encoding::builtin_unibyte_string_p(args),
        "encode-coding-string" => crate::encoding::builtin_encode_coding_string(args),
        "decode-coding-string" => crate::encoding::builtin_decode_coding_string(args),
        "char-or-string-p" => crate::encoding::builtin_char_or_string_p(args),
        "max-char" => crate::encoding::builtin_max_char(args),
        // Display/terminal (pure)
        "display-graphic-p" => super::display::builtin_display_graphic_p(args),
        "display-color-p" => super::display::builtin_display_color_p(args),
        "display-pixel-width" => super::display::builtin_display_pixel_width(args),
        "display-pixel-height" => super::display::builtin_display_pixel_height(args),
        "terminal-name" => super::display::builtin_terminal_name(args),
        "terminal-live-p" => super::display::builtin_terminal_live_p(args),
        _ => return None,
    })
}

// ===========================================================================
// Search / Regex builtins (evaluator-dependent)
// ===========================================================================

pub(crate) fn builtin_search_forward(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("search-forward", &args, 1)?;
    let pattern = expect_string(&args[0])?;
    let bound = if args.len() > 1 && !args[1].is_nil() {
        Some(expect_int(&args[1])? as usize)
    } else {
        None
    };
    let noerror = args.len() > 2 && args[2].is_truthy();

    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    match super::regex::search_forward(buf, &pattern, bound, noerror, &mut eval.match_data) {
        Ok(Some(pos)) => {
            // Return point position (1-based char position in Emacs convention)
            let char_pos = buf.text.byte_to_char(pos);
            Ok(Value::Int((char_pos + 1) as i64))
        }
        Ok(None) => Ok(Value::Nil),
        Err(msg) => Err(signal("search-failed", vec![Value::string(msg)])),
    }
}

pub(crate) fn builtin_search_backward(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("search-backward", &args, 1)?;
    let pattern = expect_string(&args[0])?;
    let bound = if args.len() > 1 && !args[1].is_nil() {
        Some(expect_int(&args[1])? as usize)
    } else {
        None
    };
    let noerror = args.len() > 2 && args[2].is_truthy();

    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    match super::regex::search_backward(buf, &pattern, bound, noerror, &mut eval.match_data) {
        Ok(Some(pos)) => {
            let char_pos = buf.text.byte_to_char(pos);
            Ok(Value::Int((char_pos + 1) as i64))
        }
        Ok(None) => Ok(Value::Nil),
        Err(msg) => Err(signal("search-failed", vec![Value::string(msg)])),
    }
}

pub(crate) fn builtin_re_search_forward(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("re-search-forward", &args, 1)?;
    let pattern = expect_string(&args[0])?;
    let bound = if args.len() > 1 && !args[1].is_nil() {
        Some(expect_int(&args[1])? as usize)
    } else {
        None
    };
    let noerror = args.len() > 2 && args[2].is_truthy();

    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    match super::regex::re_search_forward(buf, &pattern, bound, noerror, &mut eval.match_data) {
        Ok(Some(pos)) => {
            let char_pos = buf.text.byte_to_char(pos);
            Ok(Value::Int((char_pos + 1) as i64))
        }
        Ok(None) => Ok(Value::Nil),
        Err(msg) => Err(signal("search-failed", vec![Value::string(msg)])),
    }
}

pub(crate) fn builtin_re_search_backward(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("re-search-backward", &args, 1)?;
    let pattern = expect_string(&args[0])?;
    let bound = if args.len() > 1 && !args[1].is_nil() {
        Some(expect_int(&args[1])? as usize)
    } else {
        None
    };
    let noerror = args.len() > 2 && args[2].is_truthy();

    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    match super::regex::re_search_backward(buf, &pattern, bound, noerror, &mut eval.match_data) {
        Ok(Some(pos)) => {
            let char_pos = buf.text.byte_to_char(pos);
            Ok(Value::Int((char_pos + 1) as i64))
        }
        Ok(None) => Ok(Value::Nil),
        Err(msg) => Err(signal("search-failed", vec![Value::string(msg)])),
    }
}

pub(crate) fn builtin_looking_at(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("looking-at", &args, 1)?;
    let pattern = expect_string(&args[0])?;

    let buf = eval.buffers.current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    match super::regex::looking_at(buf, &pattern, &mut eval.match_data) {
        Ok(matched) => Ok(Value::bool(matched)),
        Err(msg) => Err(signal("invalid-regexp", vec![Value::string(msg)])),
    }
}

/// Evaluator-dependent `string-match`: updates match data on the evaluator.
pub(crate) fn builtin_string_match_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("string-match", &args, 2)?;
    let pattern = expect_string(&args[0])?;
    let s = expect_string(&args[1])?;
    let start = if args.len() > 2 {
        expect_int(&args[2])? as usize
    } else {
        0
    };

    match super::regex::string_match_full(&pattern, &s, start, &mut eval.match_data) {
        Ok(Some(pos)) => Ok(Value::Int(pos as i64)),
        Ok(None) => Ok(Value::Nil),
        Err(msg) => Err(signal("invalid-regexp", vec![Value::string(msg)])),
    }
}

pub(crate) fn builtin_match_string(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("match-string", &args, 1)?;
    let group = expect_int(&args[0])? as usize;

    let md = match &eval.match_data {
        Some(md) => md,
        None => return Ok(Value::Nil),
    };

    let (start, end) = match md.groups.get(group) {
        Some(Some(pair)) => *pair,
        _ => return Ok(Value::Nil),
    };

    // If the match was against a string, use that string
    if let Some(ref searched) = md.searched_string {
        if end <= searched.len() {
            return Ok(Value::string(&searched[start..end]));
        }
        return Ok(Value::Nil);
    }

    // Otherwise use current buffer
    // If an optional second arg is a string, use that
    if args.len() > 1 {
        if let Some(s) = args[1].as_str() {
            if end <= s.len() {
                return Ok(Value::string(&s[start..end]));
            }
            return Ok(Value::Nil);
        }
    }

    let buf = match eval.buffers.current_buffer() {
        Some(b) => b,
        None => return Ok(Value::Nil),
    };
    if end <= buf.text.len() {
        Ok(Value::string(buf.text.text_range(start, end)))
    } else {
        Ok(Value::Nil)
    }
}

pub(crate) fn builtin_match_beginning(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("match-beginning", &args, 1)?;
    let group = expect_int(&args[0])? as usize;

    let md = match &eval.match_data {
        Some(md) => md,
        None => return Err(signal("error", vec![Value::string("No match data")])),
    };

    match md.groups.get(group) {
        Some(Some((start, _end))) => Ok(Value::Int(*start as i64)),
        Some(None) => Ok(Value::Nil),  // group exists but didn't participate
        None => Err(signal("error", vec![
            Value::string(format!("match-beginning: invalid group {}", group))
        ])),
    }
}

pub(crate) fn builtin_match_end(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("match-end", &args, 1)?;
    let group = expect_int(&args[0])? as usize;

    let md = match &eval.match_data {
        Some(md) => md,
        None => return Err(signal("error", vec![Value::string("No match data")])),
    };

    match md.groups.get(group) {
        Some(Some((_start, end))) => Ok(Value::Int(*end as i64)),
        Some(None) => Ok(Value::Nil),
        None => Err(signal("error", vec![
            Value::string(format!("match-end: invalid group {}", group))
        ])),
    }
}

pub(crate) fn builtin_replace_match(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("replace-match", &args, 1)?;
    let newtext = expect_string(&args[0])?;
    let fixedcase = args.len() > 1 && args[1].is_truthy();
    let literal = args.len() > 2 && args[2].is_truthy();

    // Clone match_data to avoid borrow conflict
    let md = eval.match_data.clone();

    let buf = eval.buffers.current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    match super::regex::replace_match(buf, &newtext, fixedcase, literal, &md) {
        Ok(true) => Ok(Value::Nil),  // Emacs returns nil on success
        Ok(false) => Err(signal("error", vec![Value::string("replace-match: no match")])),
        Err(msg) => Err(signal("error", vec![Value::string(msg)])),
    }
}
