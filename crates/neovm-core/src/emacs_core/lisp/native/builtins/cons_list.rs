use super::*;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_fixnum};
use crate::emacs_core::value::{ValueKind, VecLikeType, eq_value};
use malachite::integer::Integer;

// ===========================================================================
// Cons / List operations
// ===========================================================================

pub(crate) fn builtin_cons_2(
    _eval: &mut super::eval::Context,
    car: Value,
    cdr: Value,
) -> EvalResult {
    builtin_cons_values(car, cdr)
}

fn builtin_cons_values(car: Value, cdr: Value) -> EvalResult {
    Ok(Value::cons(car, cdr))
}

fn for_each_tail_cycle_tail(
    tail: Value,
    tortoise: &mut Value,
    max: &mut i64,
    n: &mut i64,
    q: &mut i64,
) -> Option<Value> {
    *q -= 1;
    let check_against_tortoise = if *q != 0 {
        true
    } else {
        *n -= 1;
        if *n > 0 {
            true
        } else {
            *max = max.saturating_mul(2);
            *q = *max;
            *n = *max >> u16::BITS;
            *tortoise = tail;
            false
        }
    };

    if check_against_tortoise && tail.bits() == tortoise.bits() {
        Some(tail)
    } else {
        None
    }
}

fn for_each_proper_list_tail<F>(
    list: Value,
    improper_error_object: Value,
    mut visit: F,
) -> EvalResult
where
    F: FnMut(Value) -> Result<Option<Value>, Flow>,
{
    let mut tail = list;
    let mut tortoise = list;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2i64;

    while tail.is_cons() {
        if let Some(result) = visit(tail)? {
            return Ok(result);
        }

        tail = tail.cons_cdr();
        if tail.is_cons()
            && let Some(cycle_tail) =
                for_each_tail_cycle_tail(tail, &mut tortoise, &mut max, &mut n, &mut q)
        {
            return Err(signal(LispCondition::CircularList, vec![cycle_tail]));
        }
    }

    if tail.is_nil() {
        Ok(Value::NIL)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), improper_error_object],
        ))
    }
}

pub(crate) fn proper_list_length_or_signal(list: Value) -> Result<usize, Flow> {
    let mut len = 0usize;
    let mut tail = list;
    let mut tortoise = list;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2i64;

    while tail.is_cons() {
        len = len.saturating_add(1);

        tail = tail.cons_cdr();
        if tail.is_cons()
            && let Some(cycle_tail) =
                for_each_tail_cycle_tail(tail, &mut tortoise, &mut max, &mut n, &mut q)
        {
            return Err(signal(LispCondition::CircularList, vec![cycle_tail]));
        }
    }

    if tail.is_nil() {
        Ok(len)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), tail],
        ))
    }
}

pub(crate) fn collect_proper_list_items(list: Value) -> Result<Vec<Value>, Flow> {
    let mut items = Vec::new();
    let mut tail = list;
    let mut tortoise = list;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2i64;

    while tail.is_cons() {
        items.push(tail.cons_car());

        tail = tail.cons_cdr();
        if tail.is_cons()
            && let Some(cycle_tail) =
                for_each_tail_cycle_tail(tail, &mut tortoise, &mut max, &mut n, &mut q)
        {
            return Err(signal(LispCondition::CircularList, vec![cycle_tail]));
        }
    }

    if tail.is_nil() {
        Ok(items)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), tail],
        ))
    }
}

pub(crate) fn lambda_closure_length(value: &Value) -> Option<i64> {
    let slots = value.closure_slots()?;
    Some(slots.len() as i64)
}

/// Convert a Lambda value to the GNU Emacs closure vector layout:
///   [0]=ARGS  [1]=BODY  [2]=ENV  [(3)=nil, (4)=DOCSTRING/TYPE, (5)=INTERACTIVE]
/// Slot count is observable and slot 5's presence is significant even when
/// its value is nil.
pub fn lambda_to_closure_vector(value: &Value) -> Vec<Value> {
    value
        .closure_slots()
        .map(|slots| slots.to_vec())
        .unwrap_or_default()
}

pub(crate) fn bytecode_closure_length(value: &Value) -> Option<i64> {
    let bc = value.get_bytecode_data()?;
    Some(bc.observable_closure_slot_count() as i64)
}

pub(crate) fn closure_vector_length(value: &Value) -> Option<i64> {
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Lambda) => lambda_closure_length(value),
        ValueKind::Veclike(VecLikeType::ByteCode) => bytecode_closure_length(value),
        _ => None,
    }
}

/// One GNU closure slot of a ByteCode value, as `aref` returns it:
///   [0]=ARGLIST [1]=CODE [2]=CONSTANTS/ENV [3]=DEPTH [4]=DOC [5]=INTERACTIVE
///   [6..]=extra slots
/// `None` when `idx` is past the observable slot count (or `value` is not
/// byte code). Slots 1 and 2 are the function's own objects, the same on
/// every read (`Value::bytecode_slot_object`), as GNU's pseudovector slots
/// are; the hot `(aref f 2)` returns without touching the function data.
/// Out of line: `aref`'s vector path must not grow with it.
#[inline(never)]
pub(crate) fn bytecode_closure_slot(value: &Value, idx: usize) -> Option<Value> {
    use crate::tagged::header::{ByteCodeSlotObject, CLOSURE_CODE, CLOSURE_CONSTANTS};
    // Every byte-code object has at least the four mandatory slots, so
    // slots 1 and 2 need no bounds check.
    match idx {
        CLOSURE_CODE => return value.bytecode_slot_object(ByteCodeSlotObject::Code),
        CLOSURE_CONSTANTS => return value.bytecode_slot_object(ByteCodeSlotObject::Constants),
        _ => {}
    }
    let bc = value.get_bytecode_data()?;
    if idx >= bc.observable_closure_slot_count() {
        return None;
    }
    Some(match idx {
        crate::tagged::header::CLOSURE_ARGLIST => bc.arglist,
        crate::tagged::header::CLOSURE_STACK_DEPTH => Value::fixnum(bc.max_stack as i64),
        crate::tagged::header::CLOSURE_DOC_STRING => bc
            .doc_form
            .or_else(|| bc.docstring.as_ref().map(|d| Value::heap_string(d.clone())))
            .unwrap_or(Value::NIL),
        crate::tagged::header::CLOSURE_INTERACTIVE => bc.interactive.unwrap_or(Value::NIL),
        _ => bc
            .extra_slots
            .get(idx - crate::tagged::header::CLOSURE_MIN_SLOTS)
            .copied()
            .unwrap_or(Value::NIL),
    })
}

/// Convert a ByteCode value to the GNU Emacs closure vector layout: every
/// observable slot, in order, as [`bytecode_closure_slot`] returns it (so
/// `append` and `vconcat` hand out the same slot objects `aref` does).
pub(crate) fn bytecode_to_closure_vector(value: &Value) -> Vec<Value> {
    let Some(slot_count) = value
        .get_bytecode_data()
        .map(|bc| bc.observable_closure_slot_count())
    else {
        return Vec::new();
    };
    let saved_roots = crate::emacs_core::eval::save_scratch_gc_roots();
    let mut result = Vec::with_capacity(slot_count);
    for idx in 0..slot_count {
        let slot = bytecode_closure_slot(value, idx).unwrap_or(Value::NIL);
        // A docstring slot is a fresh string; keep it rooted while the rest
        // are built.
        crate::emacs_core::eval::push_scratch_gc_root(slot);
        result.push(slot);
    }
    crate::emacs_core::eval::restore_scratch_gc_roots(saved_roots);
    result
}

/// Convert LambdaParams to a Lisp list (a b &optional c &rest d).
/// Parse a Lisp arglist Value into LambdaParams.
pub fn parse_lambda_params_from_value(
    arglist: &Value,
) -> Result<LambdaParams, super::super::error::Flow> {
    use crate::emacs_core::intern::intern;
    let items = list_to_vec(arglist).unwrap_or_default();
    let mut required = Vec::new();
    let mut optional = Vec::new();
    let mut rest = None;
    let mut mode = 0; // 0=required, 1=optional, 2=rest
    for item in &items {
        let item = if item.is_symbol_with_pos() {
            item.as_symbol_with_pos_sym().unwrap()
        } else {
            *item
        };
        if let Some(name) = item.as_symbol_name() {
            match name {
                "&optional" => {
                    mode = 1;
                    continue;
                }
                "&rest" => {
                    mode = 2;
                    continue;
                }
                _ => {}
            }
        }
        let sym_id = item.as_symbol_id().unwrap_or_else(|| intern("_"));
        match mode {
            0 => required.push(sym_id),
            1 => optional.push(sym_id),
            2 => {
                rest = Some(sym_id);
                break;
            }
            _ => {}
        }
    }
    Ok(LambdaParams {
        required,
        optional,
        rest,
    })
}

pub fn lambda_params_to_value(params: &LambdaParams) -> Value {
    let mut elements = Vec::new();
    for p in &params.required {
        elements.push(Value::from_sym_id(*p));
    }
    if !params.optional.is_empty() {
        elements.push(Value::symbol("&optional"));
        for p in &params.optional {
            elements.push(Value::from_sym_id(*p));
        }
    }
    if let Some(ref rest) = params.rest {
        elements.push(Value::symbol("&rest"));
        elements.push(Value::from_sym_id(*rest));
    }
    Value::list(elements)
}

/// Semantic view used by Lisp list-cell accessors.
///
/// GNU represents interpreted functions as `PVEC_CLOSURE`, not cons cells.
/// Keep that distinction encoded in one classifier so direct builtins cannot
/// accidentally expose closure slots as a synthetic `(closure ...)` list.
/// Quoted `(lambda ...)` syntax naturally enters through the `Cons` variant.
enum ConsCellView {
    Empty,
    Cell { car: Value, cdr: Value },
    NonList(Value),
}

impl From<Value> for ConsCellView {
    fn from(value: Value) -> Self {
        match value.kind() {
            ValueKind::Nil => Self::Empty,
            ValueKind::Cons => Self::Cell {
                car: value.cons_car(),
                cdr: value.cons_cdr(),
            },
            _ => Self::NonList(value),
        }
    }
}

fn car_value(value: &Value) -> Result<Value, Flow> {
    match ConsCellView::from(*value) {
        ConsCellView::Empty => Ok(Value::NIL),
        ConsCellView::Cell { car, .. } => Ok(car),
        ConsCellView::NonList(value) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), value],
        )),
    }
}

fn cdr_value(value: &Value) -> Result<Value, Flow> {
    match ConsCellView::from(*value) {
        ConsCellView::Empty => Ok(Value::NIL),
        ConsCellView::Cell { cdr, .. } => Ok(cdr),
        ConsCellView::NonList(value) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), value],
        )),
    }
}

pub(crate) fn builtin_car_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    car_value(&arg)
}

pub(crate) fn builtin_cdr_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    cdr_value(&arg)
}

pub(crate) fn builtin_car_safe_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(car_safe_value(&arg))
}

pub(crate) fn builtin_cdr_safe_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(cdr_safe_value(&arg))
}

fn car_safe_value(val: &Value) -> Value {
    match ConsCellView::from(*val) {
        ConsCellView::Cell { car, .. } => car,
        ConsCellView::Empty | ConsCellView::NonList(_) => Value::NIL,
    }
}

fn cdr_safe_value(val: &Value) -> Value {
    match ConsCellView::from(*val) {
        ConsCellView::Cell { cdr, .. } => cdr,
        ConsCellView::Empty | ConsCellView::NonList(_) => Value::NIL,
    }
}

pub(crate) fn builtin_setcar_2(
    _eval: &mut super::eval::Context,
    cons: Value,
    new_car: Value,
) -> EvalResult {
    builtin_setcar_values(cons, new_car)
}

pub(crate) fn builtin_setcar_values(cons: Value, new_car: Value) -> EvalResult {
    match cons.kind() {
        ValueKind::Cons => {
            cons.set_car(new_car);
            Ok(new_car)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), cons],
        )),
    }
}

pub(crate) fn builtin_setcdr_2(
    _eval: &mut super::eval::Context,
    cons: Value,
    new_cdr: Value,
) -> EvalResult {
    builtin_setcdr_values(cons, new_cdr)
}

pub(crate) fn builtin_setcdr_values(cons: Value, new_cdr: Value) -> EvalResult {
    match cons.kind() {
        ValueKind::Cons => {
            cons.set_cdr(new_cdr);
            Ok(new_cdr)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), cons],
        )),
    }
}

pub(crate) fn builtin_list_slice(_eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    Ok(Value::list_from_slice(args))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_length(args: Vec<Value>) -> EvalResult {
    expect_args("length", &args, 1)?;
    builtin_length_value(args[0])
}

pub(crate) fn builtin_length_1(_eval: &mut super::eval::Context, sequence: Value) -> EvalResult {
    builtin_length_value(sequence)
}

pub(crate) fn builtin_length_value(sequence: Value) -> EvalResult {
    match sequence.kind() {
        ValueKind::Nil => Ok(Value::fixnum(0)),
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::ByteCode) => {
            Ok(Value::fixnum(closure_vector_length(&sequence).unwrap()))
        }
        ValueKind::Cons => proper_list_length_or_signal(sequence).map(|n| Value::fixnum(n as i64)),
        ValueKind::String => {
            let s = sequence.as_lisp_string().expect("string");
            Ok(Value::fixnum(s.schars() as i64))
        }
        ValueKind::Veclike(VecLikeType::CharTable) => Ok(Value::fixnum(
            super::chartable::char_table_length(&sequence).unwrap(),
        )),
        ValueKind::Veclike(VecLikeType::Vector) | ValueKind::Veclike(VecLikeType::Record) => {
            Ok(Value::fixnum(vector_sequence_length(&sequence)))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), sequence],
        )),
    }
}

fn vector_sequence_length(sequence: &Value) -> i64 {
    super::chartable::bool_vector_length(sequence)
        .or_else(|| super::chartable::char_table_length(sequence))
        .unwrap_or_else(|| {
            sequence
                .as_vector_data()
                .or_else(|| sequence.as_record_data())
                .expect("vector or record")
                .len() as i64
        })
}

fn list_length_internal_for_predicate(mut sequence: Value, mut len: i64) -> Result<i64, Flow> {
    if len < 0xffff {
        while sequence.is_cons() {
            len -= 1;
            if len <= 0 {
                return Ok(-1);
            }
            sequence = sequence.cons_cdr();
        }
        return Ok(len);
    }

    let mut tortoise = sequence;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2i64;
    while sequence.is_cons() {
        len -= 1;
        if len <= 0 {
            return Ok(-1);
        }

        sequence = sequence.cons_cdr();
        if sequence.is_cons()
            && let Some(cycle_tail) =
                for_each_tail_cycle_tail(sequence, &mut tortoise, &mut max, &mut n, &mut q)
        {
            return Err(signal(LispCondition::CircularList, vec![cycle_tail]));
        }
    }
    Ok(len)
}

fn sequence_length_less_than(sequence: &Value, target: i64) -> Result<bool, Flow> {
    match sequence.kind() {
        ValueKind::Nil => Ok(0 < target),
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::ByteCode) => {
            Ok(closure_vector_length(sequence).unwrap() < target)
        }
        ValueKind::String => {
            Ok((sequence.as_lisp_string().expect("string").schars() as i64) < target)
        }
        ValueKind::Veclike(VecLikeType::CharTable) => {
            Ok(super::chartable::char_table_length(sequence).unwrap() < target)
        }
        ValueKind::Veclike(VecLikeType::Vector) | ValueKind::Veclike(VecLikeType::Record) => {
            Ok(vector_sequence_length(sequence) < target)
        }
        ValueKind::Cons => {
            let remaining = list_length_internal_for_predicate(*sequence, target)?;
            Ok(remaining != -1)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), *sequence],
        )),
    }
}

fn sequence_length_equal(sequence: &Value, target: i64) -> Result<bool, Flow> {
    match sequence.kind() {
        ValueKind::Nil => Ok(target == 0),
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::ByteCode) => {
            Ok(closure_vector_length(sequence).unwrap() == target)
        }
        ValueKind::String => {
            Ok((sequence.as_lisp_string().expect("string").schars() as i64) == target)
        }
        ValueKind::Veclike(VecLikeType::CharTable) => {
            Ok(super::chartable::char_table_length(sequence).unwrap() == target)
        }
        ValueKind::Veclike(VecLikeType::Vector) | ValueKind::Veclike(VecLikeType::Record) => {
            Ok(vector_sequence_length(sequence) == target)
        }
        ValueKind::Cons => {
            if target < 0 {
                return Ok(false);
            }
            let remaining =
                list_length_internal_for_predicate(*sequence, target.saturating_add(1))?;
            Ok(remaining == 1)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), *sequence],
        )),
    }
}

fn sequence_length_greater_than(sequence: &Value, target: i64) -> Result<bool, Flow> {
    match sequence.kind() {
        ValueKind::Nil => Ok(0 > target),
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::ByteCode) => {
            Ok(closure_vector_length(sequence).unwrap() > target)
        }
        ValueKind::String => {
            Ok((sequence.as_lisp_string().expect("string").schars() as i64) > target)
        }
        ValueKind::Veclike(VecLikeType::CharTable) => {
            Ok(super::chartable::char_table_length(sequence).unwrap() > target)
        }
        ValueKind::Veclike(VecLikeType::Vector) | ValueKind::Veclike(VecLikeType::Record) => {
            Ok(vector_sequence_length(sequence) > target)
        }
        ValueKind::Cons => {
            let remaining =
                list_length_internal_for_predicate(*sequence, target.saturating_add(1))?;
            Ok(remaining == -1)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), *sequence],
        )),
    }
}

pub(crate) fn builtin_length_lt(args: Vec<Value>) -> EvalResult {
    expect_args("length<", &args, 2)?;
    let target = expect_fixnum(&args[1])?;
    Ok(Value::bool_val(sequence_length_less_than(
        &args[0], target,
    )?))
}

pub(crate) fn builtin_length_eq(args: Vec<Value>) -> EvalResult {
    expect_args("length=", &args, 2)?;
    let target = expect_fixnum(&args[1])?;
    Ok(Value::bool_val(sequence_length_equal(&args[0], target)?))
}

pub(crate) fn builtin_length_gt(args: Vec<Value>) -> EvalResult {
    expect_args("length>", &args, 2)?;
    let target = expect_fixnum(&args[1])?;
    Ok(Value::bool_val(sequence_length_greater_than(
        &args[0], target,
    )?))
}

pub(crate) fn builtin_nth(args: Vec<Value>) -> EvalResult {
    expect_args("nth", &args, 2)?;
    builtin_nth_values(args[0], args[1])
}

pub(crate) fn builtin_nth_2(
    _eval: &mut super::eval::Context,
    n_value: Value,
    list: Value,
) -> EvalResult {
    builtin_nth_values(n_value, list)
}

pub(crate) fn builtin_nth_values(n_value: Value, list: Value) -> EvalResult {
    let tail = nthcdr_impl(n_value, list)?;
    match tail.kind() {
        ValueKind::Cons => Ok(tail.cons_car()),
        ValueKind::Nil => Ok(Value::NIL),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), tail],
        )),
    }
}

/// GNU `Bnth` (`bytecode.c`), the byte-code `nth`: a count of 0..127 walks
/// the list inline and a non-list tail signals with that TAIL, where `Fnth`
/// (a funcall or interpreted `nth`, [`builtin_nth_values`]) signals with the
/// whole list. Byte-compiled `(nth 2 '(a . b))` is `(wrong-type-argument
/// listp b)` in GNU. Any other count is `Fnth`'s.
pub(crate) fn bytecode_nth_values(n_value: Value, list: Value) -> EvalResult {
    if let Some(n) = n_value.as_fixnum()
        && (0..=127).contains(&n)
    {
        let mut tail = list;
        for _ in 0..n {
            if !tail.is_cons() {
                break;
            }
            tail = tail.cons_cdr();
        }
        return if tail.is_cons() {
            Ok(tail.cons_car())
        } else if tail.is_nil() {
            Ok(Value::NIL)
        } else {
            Err(listp_error(tail))
        };
    }
    builtin_nth_values(n_value, list)
}

enum NthcdrCount {
    Fixnum(i64),
    NegativeBignum,
    PositiveBignum(Integer),
}

fn expect_nthcdr_count(value: Value) -> Result<NthcdrCount, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(NthcdrCount::Fixnum(n)),
        ValueKind::Veclike(VecLikeType::Bignum) => {
            let n = value.as_bignum().expect("bignum kind").clone();
            if n < 0 {
                Ok(NthcdrCount::NegativeBignum)
            } else {
                Ok(NthcdrCount::PositiveBignum(n))
            }
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), value],
        )),
    }
}

fn nthcdr_impl(n_value: Value, list: Value) -> EvalResult {
    let count = expect_nthcdr_count(n_value)?;

    if matches!(count, NthcdrCount::Fixnum(n) if n <= 0)
        || matches!(count, NthcdrCount::NegativeBignum)
    {
        return Ok(list);
    }

    let mut tail = list;

    if let NthcdrCount::Fixnum(n) = &count
        && *n <= 127
    {
        for _ in 0..(*n as usize) {
            match tail.kind() {
                ValueKind::Cons => {
                    tail = tail.cons_cdr();
                }
                ValueKind::Nil => return Ok(Value::NIL),
                _ => {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("listp"), list],
                    ));
                }
            }
        }
        return Ok(tail);
    }

    nthcdr_large_or_bignum(count, tail, list)
}

fn nthcdr_large_or_bignum(count: NthcdrCount, mut tail: Value, list: Value) -> EvalResult {
    let large_num = i64::MAX;
    let (mut num, original_bignum) = match count {
        NthcdrCount::Fixnum(n) => (n, None),
        NthcdrCount::PositiveBignum(n) => (large_num, Some(n)),
        NthcdrCount::NegativeBignum => unreachable!("negative bignum returns before large path"),
    };

    let mut tortoise_num = num;
    let mut saved_tail = tail;
    let mut tortoise = tail;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2i64;
    let mut found_cycle = false;

    while tail.is_cons() {
        if tail.bits() == tortoise.bits() {
            tortoise_num = num;
        }

        saved_tail = tail.cons_cdr();
        num -= 1;
        if num == 0 {
            return Ok(saved_tail);
        }

        tail = saved_tail;
        if tail.is_cons()
            && for_each_tail_cycle_tail(tail, &mut tortoise, &mut max, &mut n, &mut q).is_some()
        {
            found_cycle = true;
            break;
        }
    }

    tail = saved_tail;
    if !found_cycle {
        return if tail.is_nil() {
            Ok(Value::NIL)
        } else {
            Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), list],
            ))
        };
    }

    let cycle_length = tortoise_num - num;
    if let Some(big) = original_bignum.as_ref() {
        let modulus = Integer::from(cycle_length);
        let remainder = big % &modulus;
        num += i64::try_from(&remainder).expect("remainder fits in cycle length");
        num += cycle_length - large_num % cycle_length;
    }
    num %= cycle_length;

    for _ in 0..num {
        tail = tail.cons_cdr();
    }
    Ok(tail)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_nthcdr(args: Vec<Value>) -> EvalResult {
    expect_args("nthcdr", &args, 2)?;
    builtin_nthcdr_values(args[0], args[1])
}

pub(crate) fn builtin_nthcdr_2(
    _eval: &mut super::eval::Context,
    n_value: Value,
    list: Value,
) -> EvalResult {
    builtin_nthcdr_values(n_value, list)
}

pub(crate) fn builtin_nthcdr_values(n_value: Value, list: Value) -> EvalResult {
    nthcdr_impl(n_value, list)
}

pub(crate) fn builtin_append_slice(_eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    builtin_append_slice_impl(args)
}

fn builtin_append_slice_impl(args: &[Value]) -> EvalResult {
    fn append_element(result: &mut Value, last: &mut Value, element: Value) {
        let node = Value::cons(element, Value::NIL);
        if result.is_nil() {
            *result = node;
        } else {
            last.set_cdr(node);
        }
        *last = node;
        crate::emacs_core::eval::push_scratch_gc_root(node);
    }

    fn append_proper_list(result: &mut Value, last: &mut Value, list: Value) -> Result<(), Flow> {
        let mut tail = list;
        let mut tortoise = list;
        let mut max = 2i64;
        let mut n = 0i64;
        let mut q = 2i64;

        while tail.is_cons() {
            append_element(result, last, tail.cons_car());

            tail = tail.cons_cdr();
            if tail.is_cons()
                && let Some(cycle_tail) =
                    for_each_tail_cycle_tail(tail, &mut tortoise, &mut max, &mut n, &mut q)
            {
                return Err(signal(LispCondition::CircularList, vec![cycle_tail]));
            }
        }

        if !tail.is_nil() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), tail],
            ));
        }

        Ok(())
    }

    if args.is_empty() {
        return Ok(Value::NIL);
    }
    if args.len() == 1 {
        return Ok(args[0]);
    }

    let saved_roots = crate::emacs_core::eval::save_scratch_gc_roots();
    for arg in args {
        crate::emacs_core::eval::push_scratch_gc_root(*arg);
    }

    let result = (|| -> EvalResult {
        let mut result = Value::NIL;
        let mut last = Value::NIL;

        for arg in &args[..args.len() - 1] {
            match arg.kind() {
                ValueKind::Nil => {}
                ValueKind::Cons => append_proper_list(&mut result, &mut last, *arg)?,
                ValueKind::Veclike(VecLikeType::Lambda) => {
                    if let Some(slots) = arg.closure_slots() {
                        for item in slots.as_slice().iter().copied() {
                            append_element(&mut result, &mut last, item);
                        }
                    }
                }
                ValueKind::Veclike(VecLikeType::ByteCode) => {
                    for item in bytecode_to_closure_vector(arg) {
                        crate::emacs_core::eval::push_scratch_gc_root(item);
                        append_element(&mut result, &mut last, item);
                    }
                }
                ValueKind::Veclike(VecLikeType::Vector)
                    if super::chartable::is_bool_vector(arg) =>
                {
                    let len = super::chartable::bool_vector_length(arg).unwrap_or_default();
                    for index in 0..usize::try_from(len).unwrap_or_default() {
                        let item = super::chartable::bool_vector_ref_value(arg, index).ok_or_else(
                            || {
                                signal(
                                    LispCondition::WrongTypeArgument,
                                    vec![Value::symbol("bool-vector-p"), *arg],
                                )
                            },
                        )?;
                        append_element(&mut result, &mut last, item);
                    }
                }
                ValueKind::Veclike(VecLikeType::Vector) => {
                    if super::chartable::is_char_table(arg) {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("sequencep"), *arg],
                        ));
                    }
                    if let Some(items) = arg.as_vector_data() {
                        for item in items.as_slice().iter().copied() {
                            append_element(&mut result, &mut last, item);
                        }
                    }
                }
                ValueKind::String => {
                    let string = arg.as_lisp_string().expect("string");
                    super::for_each_lisp_string_char(string, |cp| {
                        append_element(&mut result, &mut last, Value::fixnum(cp as i64));
                    });
                }
                _ => {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("sequencep"), *arg],
                    ));
                }
            }
        }

        let last_tail = args[args.len() - 1];
        if result.is_nil() {
            return Ok(last_tail);
        }

        last.set_cdr(last_tail);
        Ok(result)
    })();
    crate::emacs_core::eval::restore_scratch_gc_roots(saved_roots);
    result
}

pub(crate) fn builtin_reverse(args: Vec<Value>) -> EvalResult {
    fn reverse_string(value: Value) -> EvalResult {
        let string = value.as_lisp_string().ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), value],
            )
        })?;

        if !string.is_multibyte() {
            let mut bytes = string.as_bytes().to_vec();
            bytes.reverse();
            return Ok(Value::heap_string(
                crate::heap_types::LispString::from_unibyte(bytes),
            ));
        }

        let mut codes = super::lisp_string_char_codes(string);
        codes.reverse();

        let mut data = Vec::with_capacity(string.sbytes());
        let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
        for code in codes {
            let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
            data.extend_from_slice(&buf[..len]);
        }
        Ok(Value::heap_string(
            crate::heap_types::LispString::from_emacs_bytes(data),
        ))
    }

    fn reverse_bool_vector(value: Value) -> EvalResult {
        let Some(mut data) = value.as_vector_data().map(|items| items.to_vec()) else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("sequencep"), value],
            ));
        };
        let logical_len = super::chartable::bool_vector_length(&value).unwrap_or_default() as usize;
        let bits_end = 2 + logical_len;
        if data.len() >= bits_end {
            data[2..bits_end].reverse();
        }
        Ok(Value::vector(data))
    }

    expect_args("reverse", &args, 1)?;
    match args[0].kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => {
            let items = collect_proper_list_items(args[0])?;
            let mut reversed = items;
            reversed.reverse();
            Ok(Value::list(reversed))
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            if super::chartable::is_char_table(&args[0]) {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("sequencep"), args[0]],
                ));
            }
            if super::chartable::is_bool_vector(&args[0]) {
                return reverse_bool_vector(args[0]);
            }
            let mut items = args[0].as_vector_data().unwrap().clone();
            items.reverse();
            Ok(Value::vector(items))
        }
        ValueKind::String => reverse_string(args[0]),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), args[0]],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_nreverse(args: Vec<Value>) -> EvalResult {
    expect_args("nreverse", &args, 1)?;
    nreverse_value(args[0])
}

pub(crate) fn builtin_nreverse_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    nreverse_value(arg)
}

pub(crate) fn nreverse_value(arg: Value) -> EvalResult {
    match arg.kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => {
            let mut prev = Value::NIL;
            let mut current = arg;
            loop {
                match current.kind() {
                    ValueKind::Nil => return Ok(prev),
                    ValueKind::Cons => {
                        let next = current.cons_cdr();
                        if eq_value(&next, &arg) {
                            return Err(signal(LispCondition::CircularList, vec![arg]));
                        }
                        current.set_cdr(prev);
                        prev = current;
                        current = next;
                    }
                    _ => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("listp"), arg],
                        ));
                    }
                }
            }
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            if super::chartable::is_char_table(&arg) {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("arrayp"), arg],
                ));
            }
            if super::chartable::is_bool_vector(&arg) {
                let logical_len =
                    super::chartable::bool_vector_length(&arg).unwrap_or_default() as usize;
                let bits_end = 2 + logical_len;
                let mut data = arg
                    .as_vector_data()
                    .map(|items| items.to_vec())
                    .unwrap_or_default();
                if data.len() >= bits_end {
                    data[2..bits_end].reverse();
                }
                let _ = arg.replace_vector_data(data);
                return Ok(arg);
            }
            let mut data = arg
                .as_vector_data()
                .map(|items| items.to_vec())
                .unwrap_or_default();
            data.reverse();
            let _ = arg.replace_vector_data(data);
            Ok(arg)
        }
        ValueKind::String => builtin_reverse(vec![arg]),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("arrayp"), arg],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_member(args: Vec<Value>) -> EvalResult {
    builtin_member_with_symbols(args, false)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn builtin_member_with_symbols(args: Vec<Value>, symbols_with_pos_enabled: bool) -> EvalResult {
    expect_args("member", &args, 2)?;
    builtin_member_values(args[0], args[1], symbols_with_pos_enabled)
}

pub(crate) fn builtin_member_2(
    eval: &mut super::eval::Context,
    target: Value,
    list: Value,
) -> EvalResult {
    builtin_member_values(target, list, eval.symbols_with_pos_enabled)
}

pub(crate) fn builtin_member_values(
    target: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    if list.is_t() {
        tracing::error!(
            "(member {} t) — list is bare t! target={:?}",
            crate::emacs_core::print::print_value(&target),
            target.kind()
        );
    }
    for_each_proper_list_tail(list, list, |tail| {
        let pair_car = tail.cons_car();
        if equal_value_swp(&target, &pair_car, 0, symbols_with_pos_enabled) {
            Ok(Some(tail))
        } else {
            Ok(None)
        }
    })
}

pub(crate) fn builtin_memq_2(
    eval: &mut super::eval::Context,
    target: Value,
    list: Value,
) -> EvalResult {
    builtin_memq_values(target, list, eval.symbols_with_pos_enabled)
}

/// How many cells a list scan with no cycle check visits before handing the
/// list to the exact algorithm, which starts again from the head. A cycle
/// cannot hide a match from the scan: every distinct cell is visited before
/// the first repeated one, which is the only cell a cycle check reacts to.
/// Only a list with no match that is circular, improper or longer than this
/// pays the replay.
pub(crate) const LIST_SCAN_BUDGET: usize = 1 << 16;

/// `memq`. The scan tests each cell and nothing else, so it needs no stack
/// frame: the tortoise-and-hare bookkeeping it no longer carries was 4 to 5
/// of each cell's instructions, and its state and an inlined error path made
/// the function save five registers on every call (208,797 calls per
/// compile of elb-smie.el).
pub(crate) fn builtin_memq_values(
    target: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    if symbols_with_pos_enabled {
        return builtin_memq_values_swp(target, list);
    }
    let target_bits = target.bits();
    let mut tail = list;
    let mut budget = LIST_SCAN_BUDGET;
    while budget != 0 {
        if !tail.is_cons() {
            if tail.is_nil() {
                return Ok(Value::NIL);
            }
            break;
        }
        if tail.cons_car().bits() == target_bits {
            return Ok(tail);
        }
        tail = tail.cons_cdr();
        budget -= 1;
    }
    memq_exact(target, list)
}

/// [`builtin_memq_values`]'s exact algorithm, from the head: the match, or
/// the circular-list or improper-list signal with GNU's data.
#[cold]
#[inline(never)]
fn memq_exact(target: Value, list: Value) -> EvalResult {
    let target_bits = target.bits();
    for_each_proper_list_tail(list, list, |tail| {
        let pair_car = tail.cons_car();
        if target_bits == pair_car.bits() {
            Ok(Some(tail))
        } else {
            Ok(None)
        }
    })
}

/// `eq` to the bare symbol `bare` while `symbols-with-pos-enabled`: `value`
/// is `bare` itself or a symbol with position whose symbol is `bare` (GNU
/// `slow_eq` with a bare-symbol side). Only a symbol can equal a symbol with
/// position, so a caller whose key is no symbol at all compares bits instead.
#[inline(always)]
pub(crate) fn eq_bare_symbol_swp(value: Value, bare: Value) -> bool {
    value.bits() == bare.bits()
        || (value.is_veclike()
            && value
                .as_symbol_with_pos_sym()
                .is_some_and(|sym| sym.bits() == bare.bits()))
}

#[cold]
#[inline(never)]
pub(crate) fn circular_list_error(tail: Value) -> Flow {
    signal(LispCondition::CircularList, vec![tail])
}

#[cold]
#[inline(never)]
pub(crate) fn listp_error(list: Value) -> Flow {
    signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("listp"), list],
    )
}

// The byte compiler binds `symbols-with-pos-enabled`, so every `memq` it runs
// comes here: one test per element against the target's bare symbol, not a
// closure unwrapping both sides of every comparison (246 instructions a call,
// 13.5% of compiling elb-smie.el). The same budgeted scan as the plain case.
fn builtin_memq_values_swp(target: Value, list: Value) -> EvalResult {
    let bare = target.as_symbol_with_pos_sym().unwrap_or(target);
    if !bare.is_symbol() {
        return builtin_memq_values(target, list, false);
    }
    let mut tail = list;
    let mut budget = LIST_SCAN_BUDGET;
    while budget != 0 {
        if !tail.is_cons() {
            if tail.is_nil() {
                return Ok(Value::NIL);
            }
            break;
        }
        if eq_bare_symbol_swp(tail.cons_car(), bare) {
            return Ok(tail);
        }
        tail = tail.cons_cdr();
        budget -= 1;
    }
    memq_swp_exact(bare, list)
}

/// [`builtin_memq_values_swp`]'s exact algorithm, from the head.
#[cold]
#[inline(never)]
fn memq_swp_exact(bare: Value, list: Value) -> EvalResult {
    let mut tail = list;
    let mut tortoise = list;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2i64;
    while tail.is_cons() {
        if eq_bare_symbol_swp(tail.cons_car(), bare) {
            return Ok(tail);
        }
        tail = tail.cons_cdr();
        if tail.is_cons()
            && let Some(cycle_tail) =
                for_each_tail_cycle_tail(tail, &mut tortoise, &mut max, &mut n, &mut q)
        {
            return Err(circular_list_error(cycle_tail));
        }
    }
    if tail.is_nil() {
        Ok(Value::NIL)
    } else {
        Err(listp_error(list))
    }
}

#[cfg(test)]
pub(crate) fn memq_exact_for_test(target: Value, list: Value, swp: bool) -> EvalResult {
    let bare = target.as_symbol_with_pos_sym().unwrap_or(target);
    if swp && bare.is_symbol() {
        memq_swp_exact(bare, list)
    } else {
        memq_exact(target, list)
    }
}

pub(crate) fn builtin_memql_2(
    eval: &mut super::eval::Context,
    target: Value,
    list: Value,
) -> EvalResult {
    builtin_memql_values(target, list, eval.symbols_with_pos_enabled)
}

fn builtin_memql_values(target: Value, list: Value, symbols_with_pos_enabled: bool) -> EvalResult {
    for_each_proper_list_tail(list, list, |tail| {
        let pair_car = tail.cons_car();
        if eql_value_swp(&target, &pair_car, symbols_with_pos_enabled) {
            Ok(Some(tail))
        } else {
            Ok(None)
        }
    })
}

/// `assoc` as a three-slot subr (an omitted TESTFN arrives as nil). Without
/// TESTFN nothing it runs can collect or call Lisp, so the list needs no
/// root and a key `eq` to the entry's answers before `equal` is asked.
pub(crate) fn builtin_assoc_3(
    eval: &mut super::eval::Context,
    key: Value,
    list: Value,
    test_fn: Value,
) -> EvalResult {
    if !test_fn.is_nil() {
        return builtin_assoc_slice(eval, &[key, list, test_fn]);
    }
    let symbols_with_pos_enabled = eval.symbols_with_pos_enabled;
    for_each_proper_list_tail(list, list, |tail| {
        let pair_car = tail.cons_car();
        if pair_car.is_cons() {
            let entry_key = pair_car.cons_car();
            if entry_key.bits() == key.bits()
                || equal_value_swp(&key, &entry_key, 0, symbols_with_pos_enabled)
            {
                return Ok(Some(pair_car));
            }
        }
        Ok(None)
    })
}

pub(crate) fn builtin_assoc_slice(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    crate::emacs_core::perf_trace::time_op(crate::emacs_core::perf_trace::HotpathOp::Assoc, || {
        expect_args_range("assoc", args, 2, 3)?;
        let key = &args[0];
        let list = args[1];
        let test_fn = args
            .get(2)
            .and_then(|value| if value.is_nil() { None } else { Some(*value) });
        if let Some(test_fn) = test_fn {
            let roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(*key);
            eval.push_specpdl_root(list);
            eval.push_specpdl_root(test_fn);
            // Root the moving tail across the predicate: TESTFN can setcdr
            // the alist, unlinking the current tail from the rooted head;
            // the slot keeps the remainder alive transitively.
            let cursor_slot = eval.push_specpdl_root_slot(Value::NIL);
            let assoc_result = for_each_proper_list_tail(list, list, |tail| {
                eval.set_specpdl_root_slot(&cursor_slot, tail);
                let pair_car = tail.cons_car();
                if let ValueKind::Cons = pair_car.kind() {
                    let entry_key = pair_car.cons_car();
                    let matches = eval.apply2(test_fn, entry_key, *key)?.is_truthy();
                    if matches {
                        return Ok(Some(pair_car));
                    }
                }
                Ok(None)
            });
            eval.restore_specpdl_roots(roots);
            return assoc_result;
        }
        // No test_fn: simple equal-based traversal (no rooting needed)
        let roots = eval.save_specpdl_roots();
        eval.push_specpdl_root(list);
        let assoc_result = for_each_proper_list_tail(list, list, |tail| {
            let pair_car = tail.cons_car();
            if let ValueKind::Cons = pair_car.kind() {
                let entry_key = pair_car.cons_car();
                if equal_value_swp(key, &entry_key, 0, eval.symbols_with_pos_enabled) {
                    return Ok(Some(pair_car));
                }
            }
            Ok(None)
        });
        eval.restore_specpdl_roots(roots);
        assoc_result
    })
}

pub(crate) fn builtin_assq_2(
    eval: &mut super::eval::Context,
    key: Value,
    list: Value,
) -> EvalResult {
    builtin_assq_values(key, list, eval.symbols_with_pos_enabled)
}

/// `assq`, scanned like [`builtin_memq_values`]: no cycle bookkeeping until
/// the budget runs out, then the exact algorithm from the head.
pub(crate) fn builtin_assq_values(
    key: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    if symbols_with_pos_enabled {
        return builtin_assq_values_swp(key, list);
    }
    let key_bits = key.bits();
    let mut tail = list;
    let mut budget = LIST_SCAN_BUDGET;
    while budget != 0 {
        if !tail.is_cons() {
            if tail.is_nil() {
                return Ok(Value::NIL);
            }
            break;
        }
        let pair = tail.cons_car();
        if pair.is_cons() && pair.cons_car().bits() == key_bits {
            return Ok(pair);
        }
        tail = tail.cons_cdr();
        budget -= 1;
    }
    assq_exact(key, list)
}

/// [`builtin_assq_values`]'s exact algorithm, from the head.
#[cold]
#[inline(never)]
fn assq_exact(key: Value, list: Value) -> EvalResult {
    let key_bits = key.bits();
    let mut tail = list;
    let mut tortoise = list;
    let mut power = 1usize;
    let mut distance = 0usize;

    while tail.is_cons() {
        let pair_car = tail.cons_car();
        if pair_car.is_cons() {
            let entry_key = pair_car.cons_car();
            if key_bits == entry_key.bits() {
                return Ok(pair_car);
            }
        }

        tail = tail.cons_cdr();
        if tail.is_cons() {
            distance = distance.saturating_add(1);
            if tail.bits() == tortoise.bits() {
                return Err(circular_list_error(tail));
            }
            if distance == power {
                tortoise = tail;
                power = power.saturating_mul(2).max(1);
                distance = 0;
            }
        }
    }

    if tail.is_nil() {
        Ok(Value::NIL)
    } else {
        Err(listp_error(list))
    }
}

#[cfg(test)]
pub(crate) fn assq_exact_for_test(key: Value, list: Value, swp: bool) -> EvalResult {
    let bare = key.as_symbol_with_pos_sym().unwrap_or(key);
    if swp && bare.is_symbol() {
        assq_swp_exact(bare, list)
    } else {
        assq_exact(key, list)
    }
}

fn builtin_assq_values_swp(key: Value, list: Value) -> EvalResult {
    let bare = key.as_symbol_with_pos_sym().unwrap_or(key);
    if !bare.is_symbol() {
        return builtin_assq_values(key, list, false);
    }
    let mut tail = list;
    let mut budget = LIST_SCAN_BUDGET;
    while budget != 0 {
        if !tail.is_cons() {
            if tail.is_nil() {
                return Ok(Value::NIL);
            }
            break;
        }
        let pair = tail.cons_car();
        if pair.is_cons() && eq_bare_symbol_swp(pair.cons_car(), bare) {
            return Ok(pair);
        }
        tail = tail.cons_cdr();
        budget -= 1;
    }
    assq_swp_exact(bare, list)
}

/// [`builtin_assq_values_swp`]'s exact algorithm, from the head.
#[cold]
#[inline(never)]
fn assq_swp_exact(bare: Value, list: Value) -> EvalResult {
    let mut tail = list;
    let mut tortoise = list;
    let mut power = 1usize;
    let mut distance = 0usize;

    while tail.is_cons() {
        let pair_car = tail.cons_car();
        if pair_car.is_cons() && eq_bare_symbol_swp(pair_car.cons_car(), bare) {
            return Ok(pair_car);
        }

        tail = tail.cons_cdr();
        if tail.is_cons() {
            distance = distance.saturating_add(1);
            if tail.bits() == tortoise.bits() {
                return Err(circular_list_error(tail));
            }
            if distance == power {
                tortoise = tail;
                power = power.saturating_mul(2).max(1);
                distance = 0;
            }
        }
    }

    if tail.is_nil() {
        Ok(Value::NIL)
    } else {
        Err(listp_error(list))
    }
}

pub(crate) fn builtin_copy_sequence(args: Vec<Value>) -> EvalResult {
    expect_args("copy-sequence", &args, 1)?;
    copy_sequence_value(args[0])
}

pub(crate) fn builtin_copy_sequence_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    copy_sequence_value(arg)
}

fn copy_sequence_value(arg: Value) -> EvalResult {
    match arg.kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => {
            let copy = Value::cons(arg.cons_car(), Value::NIL);
            let mut prev = copy;
            let mut tail = arg.cons_cdr();
            let mut tortoise = tail;
            let mut max = 2i64;
            let mut n = 0i64;
            let mut q = 2i64;

            while tail.is_cons() {
                let next = Value::cons(tail.cons_car(), Value::NIL);
                prev.set_cdr(next);
                prev = next;

                tail = tail.cons_cdr();
                if tail.is_cons()
                    && let Some(cycle_tail) =
                        for_each_tail_cycle_tail(tail, &mut tortoise, &mut max, &mut n, &mut q)
                {
                    return Err(signal(LispCondition::CircularList, vec![cycle_tail]));
                }
            }

            if tail.is_nil() {
                Ok(copy)
            } else {
                Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), tail],
                ))
            }
        }
        ValueKind::String => {
            let string = arg
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload");
            // GNU Emacs: (copy-sequence "") returns "" itself (eq).
            if string.is_empty() {
                return Ok(arg);
            }
            let new_val = Value::heap_string(string.clone());
            // Copy text properties
            if new_val.is_string()
                && let Some(table) = get_string_text_properties_table_for_value(arg)
            {
                set_string_text_properties_table_for_value(
                    new_val,
                    table.copy_interval_plist_spines(),
                );
            }
            Ok(new_val)
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let elems = arg.as_vector_data().unwrap().clone();
            // GNU Emacs: (copy-sequence (vector)) returns the same empty vector (eq).
            if elems.is_empty() {
                return Ok(arg);
            }
            Ok(Value::vector(elems))
        }
        ValueKind::Veclike(VecLikeType::CharTable) => {
            crate::emacs_core::chartable::copy_char_table(arg).ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("sequencep"), arg],
                )
            })
        }
        ValueKind::Veclike(VecLikeType::Record) => {
            let items = arg.as_record_data().unwrap().clone();
            Ok(Value::make_record(items))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), arg],
        )),
    }
}

// ===========================================================================
// Extended list operations
// ===========================================================================

fn delete_from_list_in_place_result<F>(seq: &Value, mut should_delete: F) -> Result<Value, Flow>
where
    F: FnMut(&Value) -> Result<bool, Flow>,
{
    let mut list = *seq;
    let mut prev = Value::NIL;
    let mut tail = list;
    let mut tortoise = list;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2i64;

    while tail.is_cons() {
        let remove = {
            let pair_car = tail.cons_car();
            should_delete(&pair_car)?
        };
        let next = tail.cons_cdr();
        if remove {
            if prev.is_nil() {
                list = next;
            } else {
                prev.set_cdr(next);
            }
        } else {
            prev = tail;
        }

        tail = next;
        if tail.is_cons()
            && let Some(cycle_tail) =
                for_each_tail_cycle_tail(tail, &mut tortoise, &mut max, &mut n, &mut q)
        {
            return Err(signal(LispCondition::CircularList, vec![cycle_tail]));
        }
    }

    if tail.is_nil() {
        Ok(list)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), list],
        ))
    }
}

fn delete_from_list_in_place<F>(seq: &Value, should_delete: F) -> Result<Value, Flow>
where
    F: Fn(&Value) -> bool,
{
    delete_from_list_in_place_result(seq, |value| Ok(should_delete(value)))
}

pub(crate) fn builtin_delete_with_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_delete_with_symbols(args, eval.symbols_with_pos_enabled)
}

fn builtin_delete_with_symbols(args: Vec<Value>, symbols_with_pos_enabled: bool) -> EvalResult {
    expect_args("delete", &args, 2)?;
    let elt = &args[0];
    match args[1].kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => delete_from_list_in_place(&args[1], |item| {
            equal_value_swp(elt, item, 0, symbols_with_pos_enabled)
        }),
        ValueKind::Veclike(VecLikeType::Vector) if !super::chartable::is_bool_vector(&args[1]) => {
            let items = args[1].as_vector_data().unwrap().clone();
            let mut changed = false;
            let mut kept = Vec::with_capacity(items.len());
            for item in items.iter() {
                if equal_value_swp(elt, item, 0, symbols_with_pos_enabled) {
                    changed = true;
                } else {
                    kept.push(*item);
                }
            }
            if changed {
                Ok(Value::vector(kept))
            } else {
                Ok(args[1])
            }
        }
        ValueKind::String => {
            let mut changed = false;
            let mut kept = Vec::new();
            let string = args[1].as_lisp_string().expect("string");
            for cp in super::lisp_string_char_codes(string) {
                let ch = Value::fixnum(cp as i64);
                if equal_value_swp(elt, &ch, 0, symbols_with_pos_enabled) {
                    changed = true;
                } else {
                    kept.push(ch);
                }
            }
            if !changed {
                return Ok(args[1]);
            }
            builtin_concat(vec![Value::list(kept)])
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), args[1]],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_delq(args: Vec<Value>) -> EvalResult {
    builtin_delq_with_symbols(args, false)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn builtin_delq_with_symbols(args: Vec<Value>, symbols_with_pos_enabled: bool) -> EvalResult {
    expect_args("delq", &args, 2)?;
    builtin_delq_values(args[0], args[1], symbols_with_pos_enabled)
}

pub(crate) fn builtin_delq_2(
    eval: &mut super::eval::Context,
    elt: Value,
    list: Value,
) -> EvalResult {
    builtin_delq_values(elt, list, eval.symbols_with_pos_enabled)
}

fn builtin_delq_values(elt: Value, list: Value, symbols_with_pos_enabled: bool) -> EvalResult {
    match list.kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => delete_from_list_in_place(&list, |item| {
            eq_value_swp(&elt, item, symbols_with_pos_enabled)
        }),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), list],
        )),
    }
}

pub(crate) fn builtin_elt(args: Vec<Value>) -> EvalResult {
    expect_args("elt", &args, 2)?;
    match args[0].kind() {
        ValueKind::Cons | ValueKind::Nil => builtin_nth(vec![args[1], args[0]]),
        ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::CharTable)
        | ValueKind::String => builtin_aref(vec![args[0], args[1]]),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), args[0]],
        )),
    }
}

/// The byte-code `elt` (`Op::Elt`): [`bytecode_elt_values`]. (The Lisp
/// function `elt` is [`builtin_elt`], `Felt`.)
pub(crate) fn builtin_elt_2(
    _eval: &mut super::eval::Context,
    sequence: Value,
    n: Value,
) -> EvalResult {
    bytecode_elt_values(sequence, n)
}

/// GNU `Belt` (`bytecode.c`), the byte-code `elt`: on a cons with a count
/// of 0..127 it walks inline exactly like `Bnth` -- a non-list tail
/// signals with that TAIL -- where `Felt` ([`builtin_elt_values`]) signals
/// with the whole list: byte-compiled `(elt '(1 . 2) 3)` is
/// `(wrong-type-argument listp 2)` in GNU. Everything else is `Felt`'s.
pub(crate) fn bytecode_elt_values(sequence: Value, n: Value) -> EvalResult {
    if sequence.is_cons()
        && let Some(count) = n.as_fixnum()
        && (0..=127).contains(&count)
    {
        return bytecode_nth_values(n, sequence);
    }
    builtin_elt_values(sequence, n)
}

/// `elt` over values alone: its list arm is `nth` and its array arm `aref`,
/// neither of which touches the evaluator.
pub(crate) fn builtin_elt_values(sequence: Value, n: Value) -> EvalResult {
    match sequence.kind() {
        ValueKind::Cons | ValueKind::Nil => builtin_nth_values(n, sequence),
        ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::CharTable)
        | ValueKind::String => builtin_aref_values(sequence, n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), sequence],
        )),
    }
}

pub(crate) fn builtin_nconc(args: Vec<Value>) -> EvalResult {
    builtin_nconc_slice_values(&args)
}

pub(crate) fn builtin_nconc_slice(_eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    builtin_nconc_slice_values(args)
}

pub(crate) fn builtin_nconc_slice_values(args: &[Value]) -> EvalResult {
    fn last_cons_for_nconc(list: Value) -> Result<Value, Flow> {
        let mut last = list;
        let mut tail = list;
        let mut tortoise = list;
        let mut power = 1usize;
        let mut distance = 0usize;

        while tail.is_cons() {
            last = tail;
            tail = tail.cons_cdr();
            if tail.is_cons() {
                distance = distance.saturating_add(1);
                if tail.bits() == tortoise.bits() {
                    return Err(signal(LispCondition::CircularList, vec![tail]));
                }
                if distance == power {
                    tortoise = tail;
                    power = power.saturating_mul(2).max(1);
                    distance = 0;
                }
            }
        }

        Ok(last)
    }

    if args.is_empty() {
        return Ok(Value::NIL);
    }

    let mut result_head: Option<Value> = None;
    let mut last_tail: Option<Value> = None;

    for (index, arg) in args.iter().enumerate() {
        let is_last = index + 1 == args.len();

        if is_last {
            if let Some(prev) = last_tail {
                prev.set_cdr(*arg);
                return Ok(result_head.unwrap_or(*arg));
            }
            return Ok(*arg);
        }

        match arg.kind() {
            ValueKind::Nil => continue,
            ValueKind::Cons => {
                if result_head.is_none() {
                    result_head = Some(*arg);
                }
                if let Some(prev) = last_tail {
                    prev.set_cdr(*arg);
                }

                last_tail = Some(last_cons_for_nconc(*arg)?);
            }
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("consp"), *arg],
                ));
            }
        }
    }

    Ok(result_head.unwrap_or(Value::NIL))
}

// ===========================================================================
