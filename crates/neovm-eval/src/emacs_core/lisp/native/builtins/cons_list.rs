use super::*;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodePolicyExt;
use malachite::integer::Integer;
#[doc(hidden)]
pub use neovm_value::error::{expect_args, expect_args_range, expect_fixnum};
use neovm_value::value::{ValueKind, VecLikeType, eq_value};
use std::sync::LazyLock;

// The list-scan substrate (GNU fns.c memq/assq + lisp.h FOR_EACH_TAIL Brent
// cycle check) moved to the value layer with the stage-3 split (C12). The
// names stay re-exported here so `builtins`' glob and every historical
// caller path keep resolving; the subr wrappers that read
// symbols-with-pos-enabled from a Context remain in this layer.
pub(crate) use neovm_value::list_scan::{
    GnuTailCycle, LIST_SCAN_BUDGET, builtin_assq_values, builtin_memq_values,
    builtin_memq_values_from, collect_proper_list_items, eq_bare_symbol_swp,
    eq_bare_symbol_swp_scan, for_each_proper_list_tail, for_each_proper_list_tail_rooting_tortoise,
    for_each_proper_list_tail_scan, for_each_tail_cycle_tail, listp_error, positioned_symbol_scan,
    proper_list_length_or_signal, scan_car, scan_cdr, unobserved_list_scan,
};
// Exact fallbacks are value-private to callers except the regression
// wrappers below (cfg(test) only).
#[cfg(test)]
use neovm_value::list_scan::{assq_exact, assq_swp_exact, memq_exact, memq_swp_exact};

// Assoc callback knob, read once per process:
// | Knob | Values | Default | Effect |
// | NEOVM_ASSOC_RESOLVED | on, off | on | Resolve callable TESTFN once per walk with live function-epoch guards and direct known comparisons. |

/// Process-constant configuration shared by all mutators; contains no Lisp
/// state. LazyLock publishes the initialized immutable value to every thread.
static ASSOC_RESOLVED: LazyLock<bool> =
    LazyLock::new(|| !std::env::var("NEOVM_ASSOC_RESOLVED").is_ok_and(|value| value == "off"));

// ===========================================================================
// Cons / List operations
// ===========================================================================

#[doc(hidden)]
pub fn builtin_cons_2(_eval: &mut super::eval::Context, car: Value, cdr: Value) -> EvalResult {
    builtin_cons_values(car, cdr)
}

fn builtin_cons_values(car: Value, cdr: Value) -> EvalResult {
    Ok(Value::cons(car, cdr))
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
    use neovm_value::tagged::header::{ByteCodeSlotObject, CLOSURE_CODE, CLOSURE_CONSTANTS};
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
        neovm_value::tagged::header::CLOSURE_ARGLIST => bc.arglist,
        neovm_value::tagged::header::CLOSURE_STACK_DEPTH => bc.max_stack.value(),
        neovm_value::tagged::header::CLOSURE_DOC_STRING => bc
            .doc_form
            .or_else(|| bc.docstring.as_ref().map(|d| Value::heap_string(d.clone())))
            .unwrap_or(Value::NIL),
        neovm_value::tagged::header::CLOSURE_INTERACTIVE => bc.interactive.unwrap_or(Value::NIL),
        _ => bc
            .extra_slots
            .get(idx - neovm_value::tagged::header::CLOSURE_MIN_SLOTS)
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

#[doc(hidden)]
pub fn builtin_car_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    car_value(&arg)
}

#[doc(hidden)]
pub fn builtin_cdr_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    cdr_value(&arg)
}

#[doc(hidden)]
pub fn builtin_car_safe_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(car_safe_value(&arg))
}

#[doc(hidden)]
pub fn builtin_cdr_safe_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
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

#[inline(always)]
#[doc(hidden)]
pub fn builtin_setcar_2(
    _eval: &mut super::eval::Context,
    cons: Value,
    new_car: Value,
) -> EvalResult {
    builtin_setcar_values(cons, new_car)
}

#[inline(never)]
pub(crate) fn builtin_setcar_values(cons: Value, new_car: Value) -> EvalResult {
    if cons.is_cons() {
        cons.set_car(new_car);
        Ok(new_car)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), cons],
        ))
    }
}

#[inline(always)]
#[doc(hidden)]
pub fn builtin_setcdr_2(
    _eval: &mut super::eval::Context,
    cons: Value,
    new_cdr: Value,
) -> EvalResult {
    builtin_setcdr_values(cons, new_cdr)
}

#[inline(never)]
pub(crate) fn builtin_setcdr_values(cons: Value, new_cdr: Value) -> EvalResult {
    if cons.is_cons() {
        cons.set_cdr(new_cdr);
        Ok(new_cdr)
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), cons],
        ))
    }
}

#[doc(hidden)]
pub fn builtin_list_slice(_eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    Ok(Value::list_from_slice(args))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_length(args: Vec<Value>) -> EvalResult {
    expect_args("length", &args, 1)?;
    builtin_length_value(args[0])
}

#[inline(always)]
#[doc(hidden)]
pub fn builtin_length_1(_eval: &mut super::eval::Context, sequence: Value) -> EvalResult {
    builtin_length_value(sequence)
}

#[inline(never)]
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
        ValueKind::Veclike(VecLikeType::BoolVector) => Ok(Value::fixnum(
            super::boolvec::bool_vector_length(&sequence).unwrap_or(0),
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
    super::boolvec::bool_vector_length(sequence).unwrap_or_else(|| {
        sequence
            .as_vector_data()
            .or_else(|| sequence.as_record_data())
            .expect("vector or record")
            .len() as i64
    })
}

fn list_length_internal_for_predicate(sequence: Value, len: i64) -> Result<i64, Flow> {
    if unobserved_list_scan() {
        list_length_internal_for_predicate_scan::<false>(sequence, len)
    } else {
        list_length_internal_for_predicate_scan::<true>(sequence, len)
    }
}

#[inline]
fn list_length_internal_for_predicate_scan<const OBSERVED: bool>(
    mut sequence: Value,
    mut len: i64,
) -> Result<i64, Flow> {
    if len < 0xffff {
        while sequence.is_cons() {
            len -= 1;
            if len <= 0 {
                return Ok(-1);
            }
            sequence = scan_cdr::<OBSERVED>(sequence);
        }
        return Ok(len);
    }

    let mut tortoise = sequence;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2u16;
    while sequence.is_cons() {
        len -= 1;
        if len <= 0 {
            return Ok(-1);
        }

        sequence = scan_cdr::<OBSERVED>(sequence);
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
        ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::Record)
        | ValueKind::Veclike(VecLikeType::BoolVector) => {
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
        ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::Record)
        | ValueKind::Veclike(VecLikeType::BoolVector) => {
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
        ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::Record)
        | ValueKind::Veclike(VecLikeType::BoolVector) => {
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

#[doc(hidden)]
pub fn builtin_length_lt(args: Vec<Value>) -> EvalResult {
    expect_args("length<", &args, 2)?;
    let target = expect_fixnum(&args[1])?;
    Ok(Value::bool_val(sequence_length_less_than(
        &args[0], target,
    )?))
}

#[doc(hidden)]
pub fn builtin_length_eq(args: Vec<Value>) -> EvalResult {
    expect_args("length=", &args, 2)?;
    let target = expect_fixnum(&args[1])?;
    Ok(Value::bool_val(sequence_length_equal(&args[0], target)?))
}

#[doc(hidden)]
pub fn builtin_length_gt(args: Vec<Value>) -> EvalResult {
    expect_args("length>", &args, 2)?;
    let target = expect_fixnum(&args[1])?;
    Ok(Value::bool_val(sequence_length_greater_than(
        &args[0], target,
    )?))
}

#[doc(hidden)]
pub fn builtin_nth_2(_eval: &mut super::eval::Context, n_value: Value, list: Value) -> EvalResult {
    builtin_nth_values(n_value, list)
}

pub(crate) fn builtin_nth_values(n_value: Value, list: Value) -> EvalResult {
    if unobserved_list_scan() {
        builtin_nth_values_scan::<false>(n_value, list)
    } else {
        builtin_nth_values_scan::<true>(n_value, list)
    }
}

#[inline]
fn builtin_nth_values_scan<const OBSERVED: bool>(n_value: Value, list: Value) -> EvalResult {
    let tail = nthcdr_impl_scan::<OBSERVED>(n_value, list)?;
    match tail.kind() {
        ValueKind::Cons => Ok(scan_car::<OBSERVED>(tail)),
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
#[inline(never)]
pub(crate) fn bytecode_nth_values(n_value: Value, list: Value) -> EvalResult {
    if unobserved_list_scan() {
        bytecode_nth_values_scan::<false>(n_value, list)
    } else {
        bytecode_nth_values_scan::<true>(n_value, list)
    }
}

#[inline]
fn bytecode_nth_values_scan<const OBSERVED: bool>(n_value: Value, list: Value) -> EvalResult {
    if let Some(n) = n_value.as_fixnum()
        && (0..=127).contains(&n)
    {
        let mut tail = list;
        for _ in 0..n {
            if !tail.is_cons() {
                break;
            }
            tail = scan_cdr::<OBSERVED>(tail);
        }
        return if tail.is_cons() {
            Ok(scan_car::<OBSERVED>(tail))
        } else if tail.is_nil() {
            Ok(Value::NIL)
        } else {
            Err(listp_error(tail))
        };
    }
    builtin_nth_values_scan::<OBSERVED>(n_value, list)
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

#[inline(always)]
fn nthcdr_impl(n_value: Value, list: Value) -> EvalResult {
    if unobserved_list_scan() {
        nthcdr_impl_scan::<false>(n_value, list)
    } else {
        nthcdr_impl_scan::<true>(n_value, list)
    }
}

#[inline(never)]
fn nthcdr_impl_scan<const OBSERVED: bool>(n_value: Value, list: Value) -> EvalResult {
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
                    tail = scan_cdr::<OBSERVED>(tail);
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

    nthcdr_large_or_bignum_scan::<OBSERVED>(count, tail, list)
}

#[inline]
fn nthcdr_large_or_bignum_scan<const OBSERVED: bool>(
    count: NthcdrCount,
    mut tail: Value,
    list: Value,
) -> EvalResult {
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
    let mut q = 2u16;
    let mut found_cycle = false;

    while tail.is_cons() {
        if tail.bits() == tortoise.bits() {
            tortoise_num = num;
        }

        saved_tail = scan_cdr::<OBSERVED>(tail);
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
        tail = scan_cdr::<OBSERVED>(tail);
    }
    Ok(tail)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_nthcdr(args: Vec<Value>) -> EvalResult {
    expect_args("nthcdr", &args, 2)?;
    builtin_nthcdr_values(args[0], args[1])
}

#[inline(always)]
#[doc(hidden)]
pub fn builtin_nthcdr_2(
    _eval: &mut super::eval::Context,
    n_value: Value,
    list: Value,
) -> EvalResult {
    builtin_nthcdr_values(n_value, list)
}

#[inline(always)]
pub(crate) fn builtin_nthcdr_values(n_value: Value, list: Value) -> EvalResult {
    nthcdr_impl(n_value, list)
}

#[doc(hidden)]
pub fn builtin_append_slice(_eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
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
        if tail.is_cons() {
            append_element(result, last, tail.cons_car());
            tail = tail.cons_cdr();
        }
        let mut cycle = GnuTailCycle::new(tail);
        while tail.is_cons() {
            append_element(result, last, tail.cons_car());
            tail = tail.cons_cdr();
            cycle.check(tail)?;
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
                ValueKind::Veclike(VecLikeType::BoolVector) => {
                    for item in super::boolvec::bool_vector_elements(arg).unwrap_or_default() {
                        append_element(&mut result, &mut last, item);
                    }
                }
                ValueKind::Veclike(VecLikeType::Vector) => {
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

#[doc(hidden)]
pub fn builtin_reverse(args: Vec<Value>) -> EvalResult {
    fn reverse_string(value: Value) -> EvalResult {
        let string = value.as_lisp_string().ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), value],
            )
        })?;

        // GNU Freverse (fns.c:2354): equal character/byte lengths select
        // unibyte output, including ASCII-only multibyte strings.
        if string.schars() == string.sbytes() {
            let mut bytes = string.as_bytes().to_vec();
            bytes.reverse();
            return Ok(Value::heap_string(
                neovm_value::heap_types::LispString::from_unibyte(bytes),
            ));
        }

        let mut codes = super::lisp_string_char_codes(string);
        codes.reverse();

        let mut data = Vec::with_capacity(string.sbytes());
        let mut buf = [0u8; neovm_value::emacs_char::MAX_MULTIBYTE_LENGTH];
        for code in codes {
            let len = neovm_value::emacs_char::char_string(code, &mut buf);
            data.extend_from_slice(&buf[..len]);
        }
        Ok(Value::heap_string(
            neovm_value::heap_types::LispString::from_emacs_bytes(data),
        ))
    }

    fn reverse_bool_vector(value: Value) -> EvalResult {
        super::boolvec::reverse_bool_vector(&value).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("sequencep"), value],
            )
        })
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
            let mut items = args[0].as_vector_data().unwrap().clone();
            items.reverse();
            Ok(Value::vector(items))
        }
        ValueKind::Veclike(VecLikeType::BoolVector) => reverse_bool_vector(args[0]),
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

#[inline(always)]
#[doc(hidden)]
pub fn builtin_nreverse_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    nreverse_value(arg)
}

#[inline(never)]
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
            let mut data = arg
                .as_vector_data()
                .map(|items| items.to_vec())
                .unwrap_or_default();
            data.reverse();
            let _ = arg.replace_vector_data(data);
            Ok(arg)
        }
        ValueKind::Veclike(VecLikeType::BoolVector) => {
            super::boolvec::nreverse_bool_vector(&arg);
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

#[inline(always)]
#[doc(hidden)]
pub fn builtin_member_2(eval: &mut super::eval::Context, target: Value, list: Value) -> EvalResult {
    builtin_member_values(target, list, eval.symbols_with_pos_enabled)
}

#[inline(never)]
pub(crate) fn builtin_member_values(
    target: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    if unobserved_list_scan() {
        builtin_member_values_scan::<false>(target, list, symbols_with_pos_enabled)
    } else {
        builtin_member_values_scan::<true>(target, list, symbols_with_pos_enabled)
    }
}

#[inline]
fn builtin_member_values_scan<const OBSERVED: bool>(
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
    for_each_proper_list_tail_scan::<OBSERVED, _>(list, list, |tail| {
        let pair_car = scan_car::<OBSERVED>(tail);
        if neovm_value::value::try_equal_value_swp(&target, &pair_car, 0, symbols_with_pos_enabled)?
        {
            Ok(Some(tail))
        } else {
            Ok(None)
        }
    })
}

#[inline(always)]
#[doc(hidden)]
pub fn builtin_memq_2(eval: &mut super::eval::Context, target: Value, list: Value) -> EvalResult {
    builtin_memq_values(target, list, eval.symbols_with_pos_enabled)
}

#[cfg(test)]
pub(crate) fn memq_exact_for_test(target: Value, list: Value, swp: bool) -> EvalResult {
    let bare = target.as_symbol_with_pos_sym().unwrap_or(target);
    if swp && bare.is_bare_symbol() {
        memq_swp_exact(bare, list)
    } else {
        memq_exact(target, list)
    }
}

#[doc(hidden)]
pub fn builtin_memql_2(eval: &mut super::eval::Context, target: Value, list: Value) -> EvalResult {
    builtin_memql_values(target, list, eval.symbols_with_pos_enabled)
}

fn builtin_memql_values(target: Value, list: Value, symbols_with_pos_enabled: bool) -> EvalResult {
    if unobserved_list_scan() {
        builtin_memql_values_scan::<false>(target, list, symbols_with_pos_enabled)
    } else {
        builtin_memql_values_scan::<true>(target, list, symbols_with_pos_enabled)
    }
}

#[inline]
fn builtin_memql_values_scan<const OBSERVED: bool>(
    target: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    for_each_proper_list_tail_scan::<OBSERVED, _>(list, list, |tail| {
        let pair_car = scan_car::<OBSERVED>(tail);
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
#[doc(hidden)]
pub fn builtin_assoc_3(
    eval: &mut super::eval::Context,
    key: Value,
    list: Value,
    test_fn: Value,
) -> EvalResult {
    if !test_fn.is_nil() {
        return builtin_assoc_slice(eval, &[key, list, test_fn]);
    }
    assoc_values(key, list, eval.symbols_with_pos_enabled)
}

/// `assoc` without a TESTFN: the first entry whose car is KEY (`eq`, then
/// `equal`). Runs no Lisp; shared with the builtin's leaf.
pub(crate) fn assoc_values(key: Value, list: Value, symbols_with_pos_enabled: bool) -> EvalResult {
    if unobserved_list_scan() {
        assoc_values_scan::<false>(key, list, symbols_with_pos_enabled)
    } else {
        assoc_values_scan::<true>(key, list, symbols_with_pos_enabled)
    }
}

#[inline]
fn assoc_values_scan<const OBSERVED: bool>(
    key: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    for_each_proper_list_tail_scan::<OBSERVED, _>(list, list, |tail| {
        let pair_car = scan_car::<OBSERVED>(tail);
        if pair_car.is_cons() {
            let entry_key = scan_car::<OBSERVED>(pair_car);
            if entry_key.bits() == key.bits()
                || neovm_value::value::try_equal_value_swp(
                    &key,
                    &entry_key,
                    0,
                    symbols_with_pos_enabled,
                )?
            {
                return Ok(Some(pair_car));
            }
        }
        Ok(None)
    })
}

pub(crate) fn builtin_assoc_slice(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    crate::emacs_core::eval::perf::time_op(crate::emacs_core::eval::perf::PerfOp::Assoc, || {
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
            let predicate = if *ASSOC_RESOLVED {
                eval.resolve_assoc_predicate(test_fn)
            } else {
                None
            };
            if let Some(predicate) = &predicate {
                eval.push_specpdl_root(predicate.callable);
            }
            // Root the moving tail across the predicate: TESTFN can setcdr
            // the alist, unlinking the current tail from the rooted head;
            // the slot keeps the remainder alive transitively.
            let cursor_slot = eval.push_specpdl_root_slot(Value::NIL);
            // The matched entry is returned after the predicate runs, and
            // the cycle tortoise is compared by identity after it: TESTFN
            // can unlink either and collect, so root both (GNU Fassoc keeps
            // `car` and the tortoise alive as conservatively scanned C
            // locals).
            let entry_slot = eval.push_specpdl_root_slot(Value::NIL);
            let tortoise_slot = eval.push_specpdl_root_slot(Value::NIL);
            let assoc_result =
                for_each_proper_list_tail_rooting_tortoise(list, list, |tail, tortoise| {
                    eval.set_specpdl_root_slot(&cursor_slot, tail);
                    eval.set_specpdl_root_slot(&tortoise_slot, tortoise);
                    let pair_car = tail.cons_car();
                    if let ValueKind::Cons = pair_car.kind() {
                        eval.set_specpdl_root_slot(&entry_slot, pair_car);
                        let entry_key = pair_car.cons_car();
                        let matches = match &predicate {
                            Some(predicate) => {
                                eval.apply2_assoc_predicate(test_fn, predicate, entry_key, *key)?
                            }
                            None => eval.apply2(test_fn, entry_key, *key)?,
                        }
                        .is_truthy();
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
                if neovm_value::value::try_equal_value_swp(
                    key,
                    &entry_key,
                    0,
                    eval.symbols_with_pos_enabled,
                )? {
                    return Ok(Some(pair_car));
                }
            }
            Ok(None)
        });
        eval.restore_specpdl_roots(roots);
        assoc_result
    })
}

#[inline(always)]
#[doc(hidden)]
pub fn builtin_assq_2(eval: &mut super::eval::Context, key: Value, list: Value) -> EvalResult {
    builtin_assq_values(key, list, eval.symbols_with_pos_enabled)
}

#[cfg(test)]
pub(crate) fn assq_exact_for_test(key: Value, list: Value, swp: bool) -> EvalResult {
    let bare = key.as_symbol_with_pos_sym().unwrap_or(key);
    if swp && bare.is_bare_symbol() {
        assq_swp_exact(bare, list)
    } else {
        assq_exact(key, list)
    }
}

#[doc(hidden)]
pub fn builtin_copy_sequence(args: Vec<Value>) -> EvalResult {
    expect_args("copy-sequence", &args, 1)?;
    copy_sequence_value(args[0])
}

#[doc(hidden)]
pub fn builtin_copy_sequence_1(eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    copy_sequence_value_in_state(eval, arg)
}

pub(crate) fn copy_sequence_value_in_state(
    eval: &mut super::eval::Context,
    arg: Value,
) -> EvalResult {
    neovm_value::sequence_copy::copy_sequence_with_symbols(arg, eval.symbol_mode())
}

#[doc(hidden)]
pub fn copy_sequence_value(arg: Value) -> EvalResult {
    neovm_value::sequence_copy::copy_sequence_with_symbols(
        arg,
        neovm_value::tagged::SymbolMode::Bare,
    )
}

#[cfg(test)]
#[path = "tests/copy_sequence_capture_test.rs"]
mod copy_sequence_capture;

// ===========================================================================
// Extended list operations
// ===========================================================================

fn delete_from_list_in_place_result<F>(seq: &Value, should_delete: F) -> Result<Value, Flow>
where
    F: FnMut(&Value) -> Result<bool, Flow>,
{
    delete_from_list_in_place_result_scan::<true, _>(seq, should_delete)
}

#[inline]
fn delete_from_list_in_place_result_scan<const OBSERVED: bool, F>(
    seq: &Value,
    mut should_delete: F,
) -> Result<Value, Flow>
where
    F: FnMut(&Value) -> Result<bool, Flow>,
{
    let mut list = *seq;
    let mut prev = Value::NIL;
    let mut tail = list;
    let mut tortoise = list;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2u16;

    while tail.is_cons() {
        let remove = {
            let pair_car = scan_car::<OBSERVED>(tail);
            should_delete(&pair_car)?
        };
        let next = scan_cdr::<OBSERVED>(tail);
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

#[doc(hidden)]
pub fn builtin_delete_with_ctx(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_delete_with_symbols(args, eval.symbols_with_pos_enabled)
}

fn builtin_delete_with_symbols(args: Vec<Value>, symbols_with_pos_enabled: bool) -> EvalResult {
    expect_args("delete", &args, 2)?;
    let elt = &args[0];
    match args[1].kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => delete_from_list_in_place_result(&args[1], |item| {
            neovm_value::value::try_equal_value_swp(elt, item, 0, symbols_with_pos_enabled)
        }),
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = args[1].as_vector_data().unwrap().clone();
            let mut changed = false;
            let mut kept = Vec::with_capacity(items.len());
            for item in items.iter() {
                if neovm_value::value::try_equal_value_swp(elt, item, 0, symbols_with_pos_enabled)?
                {
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
            // GNU Fdelete (fns.c:2190-2223) compares character codes, copies
            // their original bytes, and preserves the input storage kind.
            let Some(target) = elt.as_fixnum() else {
                return Ok(args[1]);
            };
            let string = args[1].as_lisp_string().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), args[1]],
                )
            })?;
            let storage = string.storage_kind();
            let bytes = string.as_bytes();
            let mut kept = Vec::with_capacity(bytes.len());
            let mut removed = 0;
            let mut pos = 0;
            // GNU Fdelete traverses SCHARS, unlike symbol printing's SBYTES.
            // Supplied counts may outlive the physical payload; decode their
            // virtual tail without copying bytes outside initialized storage.
            for _ in 0..string.schars() {
                let (code, len) = match storage {
                    neovm_value::heap_types::LispStringStorageKind::Unibyte => {
                        (u32::from(bytes[pos]), 1)
                    }
                    neovm_value::heap_types::LispStringStorageKind::Multibyte => {
                        let (c, n) =
                            neovm_value::heap_types::decode_declared_multibyte_char(&bytes[pos..]);
                        (c.code(), n)
                    }
                };
                let next_pos = pos + len.min(bytes.len() - pos);
                if i64::from(code) == target {
                    removed += 1;
                } else {
                    kept.extend_from_slice(&bytes[pos..next_pos]);
                }
                pos = next_pos;
            }
            if removed == 0 {
                return Ok(args[1]);
            }
            let rebuilt = match storage {
                neovm_value::heap_types::LispStringStorageKind::Unibyte => {
                    neovm_value::heap_types::LispString::from_unibyte(kept)
                }
                neovm_value::heap_types::LispStringStorageKind::Multibyte => {
                    let remaining = string.schars() - removed;
                    match string.character_count_kind() {
                        neovm_value::heap_types::StringCharacterCountKind::Computed => {
                            neovm_value::heap_types::LispString::from_emacs_bytes_with_chars(
                                kept, remaining,
                            )
                        }
                        neovm_value::heap_types::StringCharacterCountKind::Supplied => {
                            let count =
                                neovm_value::heap_types::GnuDeclaredCharacterCount::try_from(
                                    remaining as i128,
                                )
                                .expect("deletion cannot increase a valid supplied count");
                            neovm_value::heap_types::LispString::from_emacs_bytes_with_declared_chars(
                                kept, count,
                            )
                        }
                    }
                }
            };
            Ok(Value::heap_string(rebuilt))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), args[1]],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_delq(args: Vec<Value>) -> EvalResult {
    builtin_delq_with_symbols(args, false)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn builtin_delq_with_symbols(args: Vec<Value>, symbols_with_pos_enabled: bool) -> EvalResult {
    expect_args("delq", &args, 2)?;
    builtin_delq_values(args[0], args[1], symbols_with_pos_enabled)
}

#[doc(hidden)]
pub fn builtin_delq_2(eval: &mut super::eval::Context, elt: Value, list: Value) -> EvalResult {
    builtin_delq_values(elt, list, eval.symbols_with_pos_enabled)
}

pub(crate) fn builtin_delq_values(
    elt: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    match list.kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => {
            if unobserved_list_scan() {
                delete_from_list_in_place_result_scan::<false, _>(&list, |item| {
                    Ok(eq_value_swp(&elt, item, symbols_with_pos_enabled))
                })
            } else {
                delete_from_list_in_place(&list, |item| {
                    eq_value_swp(&elt, item, symbols_with_pos_enabled)
                })
            }
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), list],
        )),
    }
}

#[doc(hidden)]
pub fn builtin_elt_in_state(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("elt", &args, 2)?;
    builtin_elt_values_in_state(eval, args[0], args[1])
}

/// The byte-code `elt` (`Op::Elt`): [`bytecode_elt_values`]. (The Lisp
/// function `elt` is [`builtin_elt_in_state`], `Felt`.)
#[inline(always)]
pub(crate) fn builtin_elt_2(
    eval: &mut super::eval::Context,
    sequence: Value,
    n: Value,
) -> EvalResult {
    bytecode_elt_values_in_state(eval, sequence, n)
}

pub(crate) fn bytecode_elt_values_in_state(
    eval: &super::eval::Context,
    sequence: Value,
    n: Value,
) -> EvalResult {
    bytecode_elt_values(
        super::collections::ArraySymbolSource::Context(eval),
        sequence,
        n,
    )
}

/// GNU `Belt` (`bytecode.c`), the byte-code `elt`: on a cons with a count
/// of 0..127 it walks inline exactly like `Bnth` -- a non-list tail
/// signals with that TAIL -- where `Felt` ([`builtin_elt_values`]) signals
/// with the whole list: byte-compiled `(elt '(1 . 2) 3)` is
/// `(wrong-type-argument listp 2)` in GNU. Everything else is `Felt`'s.
#[inline(never)]
pub(crate) fn bytecode_elt_values(
    source: super::collections::ArraySymbolSource<'_>,
    sequence: Value,
    n: Value,
) -> EvalResult {
    if sequence.is_cons()
        && let Some(count) = n.as_fixnum()
        && (0..=127).contains(&count)
    {
        return bytecode_nth_values(n, sequence);
    }
    builtin_elt_values(source, sequence, n)
}

pub(crate) fn builtin_elt_values_in_state(
    eval: &super::eval::Context,
    sequence: Value,
    n: Value,
) -> EvalResult {
    builtin_elt_values(
        super::collections::ArraySymbolSource::Context(eval),
        sequence,
        n,
    )
}

/// GNU fns.c:1855 validates SEQUENCE before Faref checks N. The source is
/// passed through only for valid array shapes; list behavior stays Fnthcdr.
pub(crate) fn builtin_elt_values(
    source: super::collections::ArraySymbolSource<'_>,
    sequence: Value,
    n: Value,
) -> EvalResult {
    match sequence.kind() {
        ValueKind::Cons | ValueKind::Nil => builtin_nth_values(n, sequence),
        ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::CharTable)
        | ValueKind::Veclike(VecLikeType::BoolVector)
        | ValueKind::String => builtin_aref_values(source, sequence, n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), sequence],
        )),
    }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_nconc(args: Vec<Value>) -> EvalResult {
    builtin_nconc_slice_values(&args)
}

#[doc(hidden)]
pub fn builtin_nconc_slice(_eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    builtin_nconc_slice_values(args)
}

#[inline(never)]
pub(crate) fn builtin_nconc_slice_values(args: &[Value]) -> EvalResult {
    if unobserved_list_scan() {
        builtin_nconc_slice_values_scan::<false>(args)
    } else {
        builtin_nconc_slice_values_scan::<true>(args)
    }
}

#[inline]
fn builtin_nconc_slice_values_scan<const OBSERVED: bool>(args: &[Value]) -> EvalResult {
    fn last_cons_for_nconc<const OBSERVED: bool>(list: Value) -> Result<Value, Flow> {
        let mut last = list;
        let mut tail = list;
        let mut cycle = GnuTailCycle::new(list);

        while tail.is_cons() {
            last = tail;
            tail = scan_cdr::<OBSERVED>(tail);
            cycle.check(tail)?;
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

                last_tail = Some(last_cons_for_nconc::<OBSERVED>(*arg)?);
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

#[cfg(test)]
#[path = "tests/collection_scan_capture_test.rs"]
mod collection_scan_capture;

#[cfg(test)]
#[path = "tests/gdn_equality_test.rs"]
mod gdn_equality;

#[cfg(test)]
#[path = "tests/positioned_elt_test.rs"]
mod positioned_elt;
