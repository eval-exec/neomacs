//! CL-lib, seq.el, and JSON built-in functions.
//!
//! Provides Common Lisp compatibility functions, sequence operations,
//! and JSON parsing/serialization for the Elisp interpreter.

use super::error::{EvalResult, Flow, signal};
#[cfg(test)]
use super::intern::intern_uninterned;
use super::intern::{intern, resolve_sym};
use super::value::*;
use crate::emacs_core::error::LispCondition;
#[cfg(test)]
use crate::emacs_core::error::expect_max_args;
use crate::emacs_core::error::{expect_args, expect_min_args};
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use strum::{EnumString, IntoStaticStr};

// ---------------------------------------------------------------------------
// Argument helpers (local copies for this module)
// ---------------------------------------------------------------------------

#[cfg(test)]
static CL_GENSYM_COUNTER: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_int(val: &Value) -> Result<i64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *val],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_number_or_marker(val: &Value) -> Result<f64, Flow> {
    match val.kind() {
        ValueKind::Fixnum(n) => Ok(n as f64),
        ValueKind::Float => Ok(val.xfloat()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("number-or-marker-p"), *val],
        )),
    }
}

/// Collect elements from any sequence type into a Vec.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn lisp_string_elements(value: &Value) -> Vec<Value> {
    let string = value
        .as_lisp_string()
        .expect("ValueKind::String must carry LispString payload");
    super::builtins::lisp_string_char_codes(string)
        .into_iter()
        .map(|cp| Value::fixnum(cp as i64))
        .collect()
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn make_lisp_string_like(
    string: &crate::heap_types::LispString,
    codes: impl IntoIterator<Item = u32>,
) -> Value {
    if !string.is_multibyte() {
        let bytes = codes
            .into_iter()
            .map(|cp| {
                debug_assert!(cp <= 0xFF);
                cp as u8
            })
            .collect();
        return Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes));
    }

    let mut bytes = Vec::new();
    let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    for cp in codes {
        let len = crate::emacs_core::emacs_char::char_string(cp, &mut buf);
        bytes.extend_from_slice(&buf[..len]);
    }
    Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(bytes))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn lisp_string_char_subseq(value: Value, start: usize, end: usize) -> Value {
    let string = value
        .as_lisp_string()
        .expect("ValueKind::String must carry LispString payload");
    if !string.is_multibyte() {
        return Value::heap_string(crate::heap_types::LispString::from_unibyte(
            string.as_bytes()[start..end].to_vec(),
        ));
    }

    let bytes = string.as_bytes();
    let start_byte = crate::emacs_core::emacs_char::char_to_byte_pos(bytes, start);
    let end_byte = crate::emacs_core::emacs_char::char_to_byte_pos(bytes, end);
    Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(
        bytes[start_byte..end_byte].to_vec(),
    ))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn collect_sequence(val: &Value) -> Vec<Value> {
    match val.kind() {
        ValueKind::Nil => Vec::new(),
        ValueKind::Cons => list_to_vec(val).unwrap_or_default(),
        ValueKind::Veclike(VecLikeType::Vector) => val.as_vector_data().unwrap().clone(),
        ValueKind::String => lisp_string_elements(val),
        _ => vec![*val],
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn cl_list_nth(list: &Value, index: usize) -> EvalResult {
    let mut cursor = *list;
    for _ in 0..index {
        match cursor.kind() {
            ValueKind::Nil => return Ok(Value::NIL),
            ValueKind::Cons => {
                let _pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                cursor = pair_cdr;
            }
            _tail => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), cursor],
                ));
            }
        }
    }

    match cursor.kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => Ok(cursor.cons_car()),
        _tail => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), cursor],
        )),
    }
}

/// `(cl-first LIST)` -- return the first element of LIST.
#[cfg(test)]
pub(crate) fn builtin_cl_first(args: Vec<Value>) -> EvalResult {
    expect_args("cl-first", &args, 1)?;
    cl_list_nth(&args[0], 0)
}

/// `(cl-second LIST)` -- return the second element of LIST.
#[cfg(test)]
pub(crate) fn builtin_cl_second(args: Vec<Value>) -> EvalResult {
    expect_args("cl-second", &args, 1)?;
    cl_list_nth(&args[0], 1)
}

/// `(cl-third LIST)` -- return the third element of LIST.
#[cfg(test)]
pub(crate) fn builtin_cl_third(args: Vec<Value>) -> EvalResult {
    expect_args("cl-third", &args, 1)?;
    cl_list_nth(&args[0], 2)
}

/// `(cl-fourth LIST)` -- return the fourth element of LIST.
#[cfg(test)]
pub(crate) fn builtin_cl_fourth(args: Vec<Value>) -> EvalResult {
    expect_args("cl-fourth", &args, 1)?;
    cl_list_nth(&args[0], 3)
}

/// `(cl-fifth LIST)` -- return the fifth element of LIST.
#[cfg(test)]
pub(crate) fn builtin_cl_fifth(args: Vec<Value>) -> EvalResult {
    expect_args("cl-fifth", &args, 1)?;
    cl_list_nth(&args[0], 4)
}

/// `(cl-sixth LIST)` -- return the sixth element of LIST.
#[cfg(test)]
pub(crate) fn builtin_cl_sixth(args: Vec<Value>) -> EvalResult {
    expect_args("cl-sixth", &args, 1)?;
    cl_list_nth(&args[0], 5)
}

/// `(cl-seventh LIST)` -- return the seventh element of LIST.
#[cfg(test)]
pub(crate) fn builtin_cl_seventh(args: Vec<Value>) -> EvalResult {
    expect_args("cl-seventh", &args, 1)?;
    cl_list_nth(&args[0], 6)
}

/// `(cl-eighth LIST)` -- return the eighth element of LIST.
#[cfg(test)]
pub(crate) fn builtin_cl_eighth(args: Vec<Value>) -> EvalResult {
    expect_args("cl-eighth", &args, 1)?;
    cl_list_nth(&args[0], 7)
}

/// `(cl-ninth LIST)` -- return the ninth element of LIST.
#[cfg(test)]
pub(crate) fn builtin_cl_ninth(args: Vec<Value>) -> EvalResult {
    expect_args("cl-ninth", &args, 1)?;
    cl_list_nth(&args[0], 8)
}

/// `(cl-tenth LIST)` -- return the tenth element of LIST.
#[cfg(test)]
pub(crate) fn builtin_cl_tenth(args: Vec<Value>) -> EvalResult {
    expect_args("cl-tenth", &args, 1)?;
    cl_list_nth(&args[0], 9)
}

/// `(cl-rest LIST)` -- return the tail (cdr) of LIST.
#[cfg(test)]
pub(crate) fn builtin_cl_rest(args: Vec<Value>) -> EvalResult {
    expect_args("cl-rest", &args, 1)?;
    match args[0].kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => Ok(args[0].cons_cdr()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), args[0]],
        )),
    }
}

/// `(cl-evenp N)` -- return t if N is even.
#[cfg(test)]
pub(crate) fn builtin_cl_evenp(args: Vec<Value>) -> EvalResult {
    expect_args("cl-evenp", &args, 1)?;
    let n = expect_int(&args[0])?;
    Ok(Value::bool_val(n % 2 == 0))
}

/// `(cl-oddp N)` -- return t if N is odd.
#[cfg(test)]
pub(crate) fn builtin_cl_oddp(args: Vec<Value>) -> EvalResult {
    expect_args("cl-oddp", &args, 1)?;
    let n = expect_int(&args[0])?;
    Ok(Value::bool_val(n % 2 != 0))
}

/// `(cl-plusp N)` -- return t if N is strictly positive.
#[cfg(test)]
pub(crate) fn builtin_cl_plusp(args: Vec<Value>) -> EvalResult {
    expect_args("cl-plusp", &args, 1)?;
    let n = expect_number_or_marker(&args[0])?;
    Ok(Value::bool_val(n > 0.0))
}

/// `(cl-minusp N)` -- return t if N is strictly negative.
#[cfg(test)]
pub(crate) fn builtin_cl_minusp(args: Vec<Value>) -> EvalResult {
    expect_args("cl-minusp", &args, 1)?;
    let n = expect_number_or_marker(&args[0])?;
    Ok(Value::bool_val(n < 0.0))
}

/// `(cl-member ITEM LIST)` -- CL alias for `member`.
#[cfg(test)]
pub(crate) fn builtin_cl_member(args: Vec<Value>) -> EvalResult {
    super::builtins::builtin_member(args)
}

/// `(cl-coerce OBJECT TYPE)` -- coerce a sequence to TYPE.
#[cfg(test)]
pub(crate) fn builtin_cl_coerce(args: Vec<Value>) -> EvalResult {
    expect_args("cl-coerce", &args, 2)?;
    builtin_seq_concatenate(vec![args[1], args[0]])
}

/// `(cl-adjoin ITEM LIST)` -- add ITEM to LIST if not already present.
#[cfg(test)]
pub(crate) fn builtin_cl_adjoin(args: Vec<Value>) -> EvalResult {
    expect_args("cl-adjoin", &args, 2)?;
    let item = args[0];
    let list = args[1];
    let found = super::builtins::builtin_member(vec![item, list])?;
    if found.is_truthy() {
        Ok(list)
    } else {
        Ok(Value::cons(item, list))
    }
}

/// `(cl-remove ITEM LIST)` -- CL alias for `remove`.
#[cfg(test)]
pub(crate) fn builtin_cl_remove(args: Vec<Value>) -> EvalResult {
    super::builtins_extra::remove_list_equal(args)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn seq_position_list_elements(seq: &Value) -> Result<Vec<Value>, Flow> {
    let mut elements = Vec::new();
    let mut cursor = *seq;
    loop {
        match cursor.kind() {
            ValueKind::Nil => return Ok(elements),
            ValueKind::Cons => {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                elements.push(pair_car);
                cursor = pair_cdr;
            }
            _tail => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), cursor],
                ));
            }
        }
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn seq_position_elements(seq: &Value) -> Result<Vec<Value>, Flow> {
    match seq.kind() {
        ValueKind::Nil => Ok(Vec::new()),
        ValueKind::Cons => seq_position_list_elements(seq),
        ValueKind::Veclike(VecLikeType::Vector) => Ok(seq.as_vector_data().unwrap().clone()),
        ValueKind::String => Ok(lisp_string_elements(seq)),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), *seq],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn seq_default_match(left: &Value, right: &Value) -> bool {
    if equal_value(left, right, 0) {
        return true;
    }
    match (left.kind(), right.kind()) {
        (ValueKind::Fixnum(a), ValueKind::Fixnum(b)) => a == b,
        _ => false,
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn seq_collect_concat_arg(arg: &Value) -> Result<Vec<Value>, Flow> {
    match arg.kind() {
        ValueKind::Nil => Ok(Vec::new()),
        ValueKind::Cons => {
            let mut out = Vec::new();
            let mut cursor = *arg;
            loop {
                match cursor.kind() {
                    ValueKind::Nil => return Ok(out),
                    ValueKind::Cons => {
                        let pair_car = cursor.cons_car();
                        let pair_cdr = cursor.cons_cdr();
                        out.push(pair_car);
                        cursor = pair_cdr;
                    }
                    _tail => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("listp"), cursor],
                        ));
                    }
                }
            }
        }
        ValueKind::Veclike(VecLikeType::Vector) => Ok(arg.as_vector_data().unwrap().clone()),
        ValueKind::String => Ok(lisp_string_elements(arg)),
        _ => Err(signal(
            "error",
            vec![Value::string(format!(
                "Cannot convert {} into a sequence",
                super::print::print_value(arg)
            ))],
        )),
    }
}

// ===========================================================================
// Seq.el pure operations
// ===========================================================================

/// `(seq-reverse SEQ)` — reverse a sequence.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_reverse(args: Vec<Value>) -> EvalResult {
    expect_args("seq-reverse", &args, 1)?;
    let mut elems = seq_position_elements(&args[0])?;
    elems.reverse();
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Vector) => Ok(Value::vector(elems)),
        ValueKind::String => {
            let string = args[0]
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload");
            let mut codes = Vec::with_capacity(elems.len());
            for value in &elems {
                let ValueKind::Fixnum(c) = value.kind() else {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("characterp"), args[0]],
                    ));
                };
                codes.push(c as u32);
            }
            Ok(make_lisp_string_like(string, codes))
        }
        _ => Ok(Value::list(elems)),
    }
}

/// `(seq-drop SEQ N)` — drop first n elements.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_drop(args: Vec<Value>) -> EvalResult {
    expect_args("seq-drop", &args, 2)?;
    let n = expect_int(&args[1])?;

    match args[0].kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Veclike(VecLikeType::Vector) => {
            let elems = args[0].as_vector_data().unwrap().clone();
            if n <= 0 {
                return Ok(Value::vector(elems.clone()));
            }
            let n = (n as usize).min(elems.len());
            Ok(Value::vector(elems[n..].to_vec()))
        }
        ValueKind::String => {
            let char_len = args[0]
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload")
                .schars();
            if n <= 0 {
                return Ok(args[0]);
            }
            let start = (n as usize).min(char_len);
            Ok(lisp_string_char_subseq(args[0], start, char_len))
        }
        ValueKind::Cons => {
            if n <= 0 {
                return Ok(args[0]);
            }
            let mut cursor = args[0];
            let mut remaining = n as usize;
            while remaining > 0 {
                match cursor.kind() {
                    ValueKind::Nil => return Ok(Value::NIL),
                    ValueKind::Cons => {
                        let _pair_car = cursor.cons_car();
                        let pair_cdr = cursor.cons_cdr();
                        cursor = pair_cdr;
                        remaining -= 1;
                    }
                    _ => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("listp"), args[0]],
                        ));
                    }
                }
            }
            Ok(cursor)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), args[0]],
        )),
    }
}

/// `(seq-take SEQ N)` — take first n elements.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_take(args: Vec<Value>) -> EvalResult {
    expect_args("seq-take", &args, 2)?;
    let n = expect_int(&args[1])?;

    match args[0].kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Veclike(VecLikeType::Vector) => {
            let elems = args[0].as_vector_data().unwrap().clone();
            if n <= 0 {
                return Ok(Value::vector(Vec::new()));
            }
            let n = (n as usize).min(elems.len());
            Ok(Value::vector(elems[..n].to_vec()))
        }
        ValueKind::String => {
            let char_len = args[0]
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload")
                .schars();
            if n <= 0 {
                return Ok(Value::string(""));
            }
            let end = (n as usize).min(char_len);
            Ok(lisp_string_char_subseq(args[0], 0, end))
        }
        ValueKind::Cons => {
            if n <= 0 {
                return Ok(Value::NIL);
            }
            let mut out = Vec::new();
            let mut cursor = args[0];
            let mut remaining = n as usize;
            while remaining > 0 {
                match cursor.kind() {
                    ValueKind::Nil => break,
                    ValueKind::Cons => {
                        let pair_car = cursor.cons_car();
                        let pair_cdr = cursor.cons_cdr();
                        out.push(pair_car);
                        cursor = pair_cdr;
                        remaining -= 1;
                    }
                    _tail => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("listp"), cursor],
                        ));
                    }
                }
            }
            Ok(Value::list(out))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), args[0]],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
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
        return Ok(Value::NIL);
    }
    let result: Vec<Value> = elems[start..end].to_vec();
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Vector) => Ok(Value::vector(result)),
        _ => Ok(Value::list(result)),
    }
}

/// `(seq-subseq SEQ START &optional END)` — subsequence.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
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

    match args[0].kind() {
        ValueKind::Nil
        | ValueKind::Cons
        | ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::String => {
            let dropped = builtin_seq_drop(vec![args[0], Value::fixnum(start)])?;
            if let Some(end_idx) = end {
                let span = end_idx - start;
                builtin_seq_take(vec![dropped, Value::fixnum(span)])
            } else {
                Ok(dropped)
            }
        }
        _ => Err(signal(
            "error",
            vec![Value::string(format!(
                "Unsupported sequence: {}",
                super::print::print_value(&args[0])
            ))],
        )),
    }
}

/// `(seq-concatenate TYPE &rest SEQS)` — concatenate sequences into target type.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_concatenate(args: Vec<Value>) -> EvalResult {
    expect_min_args("seq-concatenate", &args, 1)?;
    let target = match args[0].kind() {
        ValueKind::Symbol(id) => resolve_sym(id),
        _ => {
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "Not a sequence type name: {}",
                    super::print::print_value(&args[0])
                ))],
            ));
        }
    };
    if target != "list" && target != "vector" && target != "string" {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Not a sequence type name: {}",
                target
            ))],
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
            let mut bytes = Vec::new();
            let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
            for value in &combined {
                let code = super::builtins::expect_character_code(value)? as u32;
                let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
                bytes.extend_from_slice(&buf[..len]);
            }
            Ok(Value::heap_string(
                crate::heap_types::LispString::from_emacs_bytes(bytes),
            ))
        }
        _ => unreachable!(),
    }
}

/// `(seq-empty-p SEQ)` — is sequence empty?
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_empty_p(args: Vec<Value>) -> EvalResult {
    expect_args("seq-empty-p", &args, 1)?;
    match args[0].kind() {
        ValueKind::Nil => Ok(Value::T),
        ValueKind::Cons => Ok(Value::NIL),
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::ByteCode) => {
            Ok(Value::NIL)
        }
        ValueKind::String => Ok(Value::bool_val(
            args[0]
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload")
                .is_empty(),
        )),
        ValueKind::Veclike(VecLikeType::Vector) => Ok(Value::bool_val(
            args[0].as_vector_data().unwrap().is_empty(),
        )),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), args[0]],
        )),
    }
}

/// `(seq-min SEQ)` — minimum element (numeric).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_min(args: Vec<Value>) -> EvalResult {
    expect_args("seq-min", &args, 1)?;
    let elems = seq_position_elements(&args[0])?;
    if elems.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::subr_from_sym_id(intern("min")), Value::fixnum(0)],
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
    Ok(*min_val)
}

/// `(seq-max SEQ)` — maximum element (numeric).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_max(args: Vec<Value>) -> EvalResult {
    expect_args("seq-max", &args, 1)?;
    let elems = seq_position_elements(&args[0])?;
    if elems.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::subr_from_sym_id(intern("max")), Value::fixnum(0)],
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
    Ok(*max_val)
}

// ===========================================================================
// Seq.el eval-dependent operations
// ===========================================================================

/// `(seq-position SEQ ELT &optional TESTFN)` — return first matching index.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_position(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("seq-position", &args, 2)?;
    let seq = &args[0];
    if matches!(
        seq.kind(),
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::ByteCode)
    ) {
        return Ok(Value::NIL);
    }
    let target = args[1];
    let test_fn = if args.len() > 2 && !args[2].is_nil() {
        Some(args[2])
    } else {
        None
    };
    let elements = seq_position_elements(seq)?;

    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(target);
    if let Some(tf) = &test_fn {
        eval.push_specpdl_root(*tf);
    }
    for e in &elements {
        eval.push_specpdl_root(*e);
    }
    let result = (|| {
        for (idx, element) in elements.into_iter().enumerate() {
            let matches = if let Some(test) = &test_fn {
                eval.apply2(*test, element, target)?.is_truthy()
            } else {
                seq_default_match(&element, &target)
            };
            if matches {
                return Ok(Value::fixnum(idx as i64));
            }
        }
        Ok(Value::NIL)
    })();
    eval.restore_specpdl_roots(roots);
    result
}

/// `(cl-position ITEM SEQ &optional TESTFN)` -- CL argument order wrapper.
#[cfg(test)]
pub(crate) fn builtin_cl_position(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("cl-position", &args, 2)?;
    expect_max_args("cl-position", &args, 3)?;

    let mut forwarded = Vec::with_capacity(args.len());
    forwarded.push(args[1]);
    forwarded.push(args[0]);
    if args.len() == 3 {
        forwarded.push(args[2]);
    }

    builtin_seq_position(eval, forwarded)
}

/// `(cl-notany PREDICATE SEQ)` -- true when no element satisfies PREDICATE.
#[cfg(test)]
pub(crate) fn builtin_cl_notany(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let found = builtin_seq_some(eval, args)?;
    Ok(Value::bool_val(found.is_nil()))
}

/// `(cl-notevery PREDICATE SEQ)` -- true when not all elements satisfy PREDICATE.
#[cfg(test)]
pub(crate) fn builtin_cl_notevery(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let every = builtin_seq_every_p(eval, args)?;
    Ok(Value::bool_val(!every.is_truthy()))
}

/// `(cl-gensym &optional PREFIX)` -- generate an uninterned-style symbol name.
#[cfg(test)]
pub(crate) fn builtin_cl_gensym(args: Vec<Value>) -> EvalResult {
    expect_max_args("cl-gensym", &args, 1)?;
    let (prefix, n) = match args.first() {
        None => (
            "G".to_string(),
            CL_GENSYM_COUNTER.fetch_add(1, Ordering::Relaxed),
        ),
        Some(v) if v.is_nil() => (
            "G".to_string(),
            CL_GENSYM_COUNTER.fetch_add(1, Ordering::Relaxed),
        ),
        Some(v) if v.is_string() => (
            v.as_lisp_string()
                .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
                .expect("ValueKind::String must carry LispString payload"),
            CL_GENSYM_COUNTER.fetch_add(1, Ordering::Relaxed),
        ),
        Some(v) if matches!(v.kind(), ValueKind::Fixnum(_)) => {
            ("G".to_string(), v.as_int().unwrap() as u64)
        }
        Some(_) => (
            "G".to_string(),
            CL_GENSYM_COUNTER.fetch_add(1, Ordering::Relaxed),
        ),
    };
    Ok(Value::from_sym_id(intern_uninterned(&format!(
        "{prefix}{n}"
    ))))
}

/// `(cl-find ITEM SEQ)` -- return first element in SEQ equal to ITEM.
#[cfg(test)]
pub(crate) fn builtin_cl_find(args: Vec<Value>) -> EvalResult {
    expect_args("cl-find", &args, 2)?;
    let target = &args[0];
    let elements = seq_position_elements(&args[1])?;
    for element in elements {
        if equal_value(&element, target, 0) {
            return Ok(element);
        }
    }
    Ok(Value::NIL)
}

/// `(cl-find-if PREDICATE SEQ)` -- return first element satisfying PREDICATE.
#[cfg(test)]
pub(crate) fn builtin_cl_find_if(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("cl-find-if", &args, 2)?;
    let pred = args[0];
    let elements = seq_position_elements(&args[1])?;
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(pred);
    for e in &elements {
        eval.push_specpdl_root(*e);
    }
    let result = (|| {
        for element in elements {
            let matched = eval.apply1(pred, element)?;
            if matched.is_truthy() {
                return Ok(element);
            }
        }
        Ok(Value::NIL)
    })();
    eval.restore_specpdl_roots(roots);
    result
}

/// `(cl-subsetp LIST1 LIST2)` -- return t if every element of LIST1 appears in LIST2.
#[cfg(test)]
pub(crate) fn builtin_cl_subsetp(args: Vec<Value>) -> EvalResult {
    expect_args("cl-subsetp", &args, 2)?;
    let left = seq_position_elements(&args[0])?;
    let right = seq_position_elements(&args[1])?;

    for item in left {
        if !right
            .iter()
            .any(|candidate| equal_value(&item, candidate, 0))
        {
            return Ok(Value::NIL);
        }
    }
    Ok(Value::T)
}

/// `(cl-intersection LIST1 LIST2)` -- set-style intersection preserving LIST1 order.
#[cfg(test)]
pub(crate) fn builtin_cl_intersection(args: Vec<Value>) -> EvalResult {
    expect_args("cl-intersection", &args, 2)?;
    let left = seq_position_elements(&args[0])?;
    let right = seq_position_elements(&args[1])?;

    let mut out = Vec::new();
    for item in left {
        let in_right = right
            .iter()
            .any(|candidate| equal_value(&item, candidate, 0));
        let already_in_out = out.iter().any(|seen| equal_value(&item, seen, 0));
        if in_right && !already_in_out {
            out.push(item);
        }
    }
    Ok(Value::list(out))
}

/// `(cl-set-difference LIST1 LIST2)` -- set-style difference preserving LIST1 order.
#[cfg(test)]
pub(crate) fn builtin_cl_set_difference(args: Vec<Value>) -> EvalResult {
    expect_args("cl-set-difference", &args, 2)?;
    let left = seq_position_elements(&args[0])?;
    let right = seq_position_elements(&args[1])?;

    let mut out = Vec::new();
    for item in left {
        let in_right = right
            .iter()
            .any(|candidate| equal_value(&item, candidate, 0));
        let already_in_out = out.iter().any(|seen| equal_value(&item, seen, 0));
        if !in_right && !already_in_out {
            out.push(item);
        }
    }
    Ok(Value::list(out))
}

/// `(cl-union LIST1 LIST2)` -- set-style union preserving left-to-right discovery order.
#[cfg(test)]
pub(crate) fn builtin_cl_union(args: Vec<Value>) -> EvalResult {
    expect_args("cl-union", &args, 2)?;
    let left = seq_position_elements(&args[0])?;
    let right = seq_position_elements(&args[1])?;

    let mut out = Vec::new();
    for item in left.into_iter().chain(right.into_iter()) {
        let already_in_out = out.iter().any(|seen| equal_value(&item, seen, 0));
        if !already_in_out {
            out.push(item);
        }
    }
    Ok(Value::list(out))
}

/// `(cl-substitute NEW OLD SEQ)` -- replace OLD with NEW across SEQ.
#[cfg(test)]
pub(crate) fn builtin_cl_substitute(args: Vec<Value>) -> EvalResult {
    expect_args("cl-substitute", &args, 3)?;
    let new_value = args[0];
    let old_value = &args[1];
    let elements = seq_position_elements(&args[2])?;

    let replaced = elements
        .into_iter()
        .map(|item| {
            if equal_value(&item, old_value, 0) {
                new_value
            } else {
                item
            }
        })
        .collect::<Vec<_>>();
    Ok(Value::list(replaced))
}

/// `(cl-sort SEQ PREDICATE)` -- CL alias for `sort`.
#[cfg(test)]
pub(crate) fn builtin_cl_sort(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    super::builtins::builtin_sort(eval, args)
}

/// `(cl-stable-sort SEQ PREDICATE)` -- CL alias for stable `sort`.
#[cfg(test)]
pub(crate) fn builtin_cl_stable_sort(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::builtins::builtin_sort(eval, args)
}

/// `(cl-remove-if PREDICATE SEQ)` -- remove elements satisfying PREDICATE.
#[cfg(test)]
pub(crate) fn builtin_cl_remove_if(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("cl-remove-if", &args, 2)?;
    let pred = args[0];
    let elements = seq_position_elements(&args[1])?;
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(pred);
    for e in &elements {
        eval.push_specpdl_root(*e);
    }
    let result = (|| {
        let mut out = Vec::new();
        for element in elements {
            let matched = eval.apply1(pred, element)?;
            if !matched.is_truthy() {
                out.push(element);
            }
        }
        Ok(Value::list(out))
    })();
    eval.restore_specpdl_roots(roots);
    result
}

/// `(cl-remove-if-not PREDICATE SEQ)` -- keep elements satisfying PREDICATE.
#[cfg(test)]
pub(crate) fn builtin_cl_remove_if_not(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("cl-remove-if-not", &args, 2)?;
    let pred = args[0];
    let elements = seq_position_elements(&args[1])?;
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(pred);
    for e in &elements {
        eval.push_specpdl_root(*e);
    }
    let result = (|| {
        let mut out = Vec::new();
        for element in elements {
            let matched = eval.apply1(pred, element)?;
            if matched.is_truthy() {
                out.push(element);
            }
        }
        Ok(Value::list(out))
    })();
    eval.restore_specpdl_roots(roots);
    result
}

/// Concrete result-type symbols supported by the local `cl-map` test helper.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum ClMapResultType {
    List,
    Vector,
    String,
}

#[cfg(test)]
impl ClMapResultType {
    fn from_lisp_value(value: &Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    fn name(self) -> &'static str {
        self.into()
    }
}

/// `(cl-map RESULT-TYPE FUNCTION SEQ...)` -- CL map with explicit result type.
#[cfg(test)]
pub(crate) fn builtin_cl_map(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("cl-map", &args, 3)?;
    let result_type = args[0];
    let func = args[1];
    let seqs = args[2..].to_vec();

    let mut forwarded = Vec::with_capacity(1 + seqs.len());
    forwarded.push(func);
    forwarded.extend(seqs);
    let mapped = builtin_seq_mapn(eval, forwarded)?;

    match ClMapResultType::from_lisp_value(&result_type) {
        Some(ClMapResultType::List) => Ok(mapped),
        Some(ClMapResultType::Vector) => {
            let items = list_to_vec(&mapped).ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), mapped],
                )
            })?;
            Ok(Value::vector(items))
        }
        Some(ClMapResultType::String) => {
            let items = list_to_vec(&mapped).ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), mapped],
                )
            })?;
            let mut bytes = Vec::new();
            let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
            for item in items {
                let code = super::builtins::expect_character_code(&item)? as u32;
                let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
                bytes.extend_from_slice(&buf[..len]);
            }
            Ok(Value::heap_string(
                crate::heap_types::LispString::from_emacs_bytes(bytes),
            ))
        }
        _ => Err(signal(
            "error",
            vec![Value::string(format!(
                "Unsupported cl-map result type: {}",
                super::print::print_value(&result_type)
            ))],
        )),
    }
}

/// `(seq-mapn FN &rest SEQS)` — map over multiple sequences.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_mapn(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("seq-mapn", &args, 2)?;
    let func = args[0];
    let seqs: Vec<Vec<Value>> = args[1..].iter().map(collect_sequence).collect();
    if seqs.is_empty() {
        return Ok(Value::NIL);
    }
    let min_len = seqs.iter().map(|s| s.len()).min().unwrap_or(0);
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(func);
    for seq in &seqs {
        for e in seq {
            eval.push_specpdl_root(*e);
        }
    }
    let result = (|| {
        let mut results = Vec::new();
        for i in 0..min_len {
            let call_args: Vec<Value> = seqs.iter().map(|s| s[i]).collect();
            let val = eval.apply(func, call_args)?;
            eval.push_specpdl_root(val);
            results.push(val);
        }
        Ok(Value::list(results))
    })();
    eval.restore_specpdl_roots(roots);
    result
}

/// `(seq-count PRED SEQ)` — count elements matching predicate.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_count(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("seq-count", &args, 2)?;
    let pred = args[0];
    let elems = collect_sequence(&args[1]);
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(pred);
    for e in &elems {
        eval.push_specpdl_root(*e);
    }
    let result = (|| {
        let mut count = 0i64;
        for e in elems {
            let r = eval.apply1(pred, e)?;
            if r.is_truthy() {
                count += 1;
            }
        }
        Ok(Value::fixnum(count))
    })();
    eval.restore_specpdl_roots(roots);
    result
}

/// `(seq-reduce FN SEQ INITIAL)` — reduce with initial value.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_reduce(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("seq-reduce", &args, 3)?;
    let func = args[0];
    let elems = collect_sequence(&args[1]);
    let mut acc = args[2];
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(func);
    eval.push_specpdl_root(acc);
    for e in &elems {
        eval.push_specpdl_root(*e);
    }
    let result = (|| {
        for e in elems {
            acc = eval.apply2(func, acc, e)?;
            eval.push_specpdl_root(acc);
        }
        Ok(acc)
    })();
    eval.restore_specpdl_roots(roots);
    result
}

/// `(seq-some PRED SEQ)` — some element matches predicate.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_some(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("seq-some", &args, 2)?;
    let pred = args[0];
    let elems = collect_sequence(&args[1]);
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(pred);
    for e in &elems {
        eval.push_specpdl_root(*e);
    }
    let result = (|| {
        for e in elems {
            let r = eval.apply1(pred, e)?;
            if r.is_truthy() {
                return Ok(r);
            }
        }
        Ok(Value::NIL)
    })();
    eval.restore_specpdl_roots(roots);
    result
}

/// `(seq-every-p PRED SEQ)` — all elements match predicate.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_seq_every_p(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("seq-every-p", &args, 2)?;
    let pred = args[0];
    let elems = collect_sequence(&args[1]);
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(pred);
    for e in &elems {
        eval.push_specpdl_root(*e);
    }
    let result = (|| {
        for e in elems {
            let r = eval.apply1(pred, e)?;
            if r.is_nil() {
                return Ok(Value::NIL);
            }
        }
        Ok(Value::T)
    })();
    eval.restore_specpdl_roots(roots);
    result
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
