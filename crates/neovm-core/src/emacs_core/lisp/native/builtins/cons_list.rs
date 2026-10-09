use super::*;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_fixnum};
use crate::emacs_core::value::{ValueKind, VecLikeType, eq_value};
use malachite::integer::Integer;
use std::sync::LazyLock;

// Assoc callback knob, read once per process:
// | Knob | Values | Default | Effect |
// | NEOVM_ASSOC_RESOLVED | on, off | on | Resolve callable TESTFN once per walk with live function-epoch guards and direct known comparisons. |

/// Process-constant configuration shared by all mutators; contains no Lisp
/// state. LazyLock publishes the initialized immutable value to every thread.
static ASSOC_RESOLVED: LazyLock<bool> =
    LazyLock::new(|| !std::env::var("NEOVM_ASSOC_RESOLVED").is_ok_and(|value| value == "off"));

/// A scan can keep this choice for its whole duration only when it runs no
/// Lisp callbacks. Captures are thread scoped: another mutator's scope does
/// not change which reads belong to the current thread.
#[inline(always)]
fn unobserved_list_scan() -> bool {
    !crate::tagged::collection_reads::reads_need_observation()
}

#[inline(always)]
fn scan_car<const OBSERVED: bool>(value: Value) -> Value {
    if OBSERVED {
        value.cons_car()
    } else {
        value.cons_car_unobserved()
    }
}

#[inline(always)]
fn scan_cdr<const OBSERVED: bool>(value: Value) -> Value {
    if OBSERVED {
        value.cons_cdr()
    } else {
        value.cons_cdr_unobserved()
    }
}

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

/// One step of GNU `FOR_EACH_TAIL_INTERNAL`'s Brent cycle check, run after
/// the walk advanced to the cons `tail`: returns `tail` when it closes a
/// cycle (the object GNU signals `circular-list` with).
///
/// `q` is GNU's `unsigned short`: `li.q = li.max` keeps only the low 16 bits
/// and the decrement wraps. With a wider counter the tortoise stops moving
/// once `max` reaches 2^17, and a cycle entered past ~131K steps is never
/// found (the walk spins instead of signaling).
fn for_each_tail_cycle_tail(
    tail: Value,
    tortoise: &mut Value,
    max: &mut i64,
    n: &mut i64,
    q: &mut u16,
) -> Option<Value> {
    *q = q.wrapping_sub(1);
    let check_against_tortoise = if *q != 0 {
        true
    } else {
        *n -= 1;
        if *n > 0 {
            true
        } else {
            *max = max.saturating_mul(2);
            *q = *max as u16;
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

/// Stack-local GNU Brent state, with its counters kept together so callers
/// cannot mix cycle algorithms or initialization phases. The unsigned-short
/// countdown matches GNU lisp.h:5875-5892, including its wrapping semantics.
/// Each traversal belongs to its mutator; this stores no shared or cached
/// Lisp state. Callers that run Lisp must root `tortoise()` across callbacks.
#[derive(Debug)]
pub(crate) struct GnuTailCycle {
    tortoise: Value,
    max: i64,
    n: i64,
    q: u16,
    mutator: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(GnuTailCycle: Send, Sync);

impl GnuTailCycle {
    pub(crate) fn new(head: Value) -> Self {
        Self {
            tortoise: head,
            max: 2,
            n: 0,
            q: 2,
            mutator: std::marker::PhantomData,
        }
    }

    pub(crate) fn tortoise(&self) -> Value {
        self.tortoise
    }

    #[inline]
    pub(crate) fn check(&mut self, advanced_tail: Value) -> Result<(), Flow> {
        if let Some(tail) = for_each_tail_cycle_tail(
            advanced_tail,
            &mut self.tortoise,
            &mut self.max,
            &mut self.n,
            &mut self.q,
        ) {
            Err(circular_list_error(tail))
        } else {
            Ok(())
        }
    }
}

fn for_each_proper_list_tail<F>(list: Value, improper_error_object: Value, visit: F) -> EvalResult
where
    F: FnMut(Value) -> Result<Option<Value>, Flow>,
{
    for_each_proper_list_tail_scan::<true, _>(list, improper_error_object, visit)
}

/// [`for_each_proper_list_tail`] for walks that call Lisp between steps:
/// `visit` also receives the current Brent tortoise so the caller can keep
/// it rooted. The cycle check compares the tortoise by identity; a callback
/// can unlink it, and once collected its address can be reused by a cons
/// spliced into the list, which would raise a false `circular-list` (GNU's
/// `FOR_EACH_TAIL` tortoise is a conservatively scanned C local).
fn for_each_proper_list_tail_rooting_tortoise<F>(
    list: Value,
    improper_error_object: Value,
    mut visit: F,
) -> EvalResult
where
    F: FnMut(Value, Value) -> Result<Option<Value>, Flow>,
{
    let mut tail = list;
    let mut tortoise = list;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2u16;

    while tail.is_cons() {
        if let Some(result) = visit(tail, tortoise)? {
            return Ok(result);
        }

        tail = scan_cdr::<true>(tail);
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

#[inline]
fn for_each_proper_list_tail_scan<const OBSERVED: bool, F>(
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
    let mut q = 2u16;

    while tail.is_cons() {
        if let Some(result) = visit(tail)? {
            return Ok(result);
        }

        tail = scan_cdr::<OBSERVED>(tail);
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
    if unobserved_list_scan() {
        proper_list_length_or_signal_scan::<false>(list)
    } else {
        proper_list_length_or_signal_scan::<true>(list)
    }
}

/// GNU 32 list_length's FOR_EACH_TAIL schedule (lisp.h): keep the
/// tortoise at the head for 4096 steps, then move it at powers of two.
/// Restrict this to list_length and Fapply's fused measurement/copy walk;
/// the existing general list walkers keep their own cycle protocol.
#[derive(Debug)]
pub(crate) struct GnuListLengthCycle {
    tortoise: Value,
    steps: usize,
    mutator: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(GnuListLengthCycle: Send, Sync);

impl GnuListLengthCycle {
    pub(crate) fn new(head: Value) -> Self {
        Self {
            tortoise: head,
            steps: 0,
            mutator: std::marker::PhantomData,
        }
    }

    #[inline]
    pub(crate) fn check(&mut self, advanced_tail: Value) -> Result<(), Flow> {
        const FOR_EACH_TAIL_THRESHOLD: usize = 4096;
        if advanced_tail.bits() == self.tortoise.bits() {
            return Err(circular_list_error(advanced_tail));
        }
        self.steps = self.steps.saturating_add(1);
        if self.steps & (FOR_EACH_TAIL_THRESHOLD - 1) == 0 && self.steps.is_power_of_two() {
            self.tortoise = advanced_tail;
        }
        Ok(())
    }
}

#[inline]
fn proper_list_length_or_signal_scan<const OBSERVED: bool>(list: Value) -> Result<usize, Flow> {
    let mut tail = list;
    let mut cycle = GnuListLengthCycle::new(list);

    while tail.is_cons() {
        tail = scan_cdr::<OBSERVED>(tail);
        cycle.check(tail)?;
    }

    if tail.is_nil() {
        Ok(cycle.steps)
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
    let mut q = 2u16;

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

#[inline(always)]
pub(crate) fn builtin_setcar_2(
    _eval: &mut super::eval::Context,
    cons: Value,
    new_car: Value,
) -> EvalResult {
    builtin_setcar_values(cons, new_car)
}

#[inline(never)]
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

#[inline(always)]
pub(crate) fn builtin_setcdr_2(
    _eval: &mut super::eval::Context,
    cons: Value,
    new_cdr: Value,
) -> EvalResult {
    builtin_setcdr_values(cons, new_cdr)
}

#[inline(never)]
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

#[inline(always)]
pub(crate) fn builtin_length_1(_eval: &mut super::eval::Context, sequence: Value) -> EvalResult {
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
pub(crate) fn builtin_nthcdr_2(
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

pub(crate) fn builtin_reverse(args: Vec<Value>) -> EvalResult {
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
pub(crate) fn builtin_nreverse_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
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
pub(crate) fn builtin_member_2(
    eval: &mut super::eval::Context,
    target: Value,
    list: Value,
) -> EvalResult {
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
        if crate::emacs_core::value::try_equal_value_swp(
            &target,
            &pair_car,
            0,
            symbols_with_pos_enabled,
        )? {
            Ok(Some(tail))
        } else {
            Ok(None)
        }
    })
}

#[inline(always)]
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
#[inline(never)]
pub(crate) fn builtin_memq_values(
    target: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    if unobserved_list_scan() {
        builtin_memq_values_scan::<false>(target, list, symbols_with_pos_enabled)
    } else {
        builtin_memq_values_observed(target, list, symbols_with_pos_enabled)
    }
}

// Keep the recorder's calls and live registers out of the unobserved scan's
// public entry, so they do not impose a stack frame on every invocation.
#[cold]
#[inline(never)]
fn builtin_memq_values_observed(
    target: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    builtin_memq_values_scan::<true>(target, list, symbols_with_pos_enabled)
}

#[inline]
fn builtin_memq_values_scan<const OBSERVED: bool>(
    target: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    if symbols_with_pos_enabled {
        return builtin_memq_values_swp_scan::<OBSERVED>(target, list);
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
        if scan_car::<OBSERVED>(tail).bits() == target_bits {
            return Ok(tail);
        }
        tail = scan_cdr::<OBSERVED>(tail);
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
    eq_bare_symbol_swp_scan::<true>(value, bare)
}

#[inline(always)]
pub(crate) fn positioned_symbol_scan<const OBSERVED: bool>(value: Value) -> Option<Value> {
    if OBSERVED {
        value.as_symbol_with_pos_sym()
    } else {
        value.as_symbol_with_pos_sym_unobserved()
    }
}

#[inline(always)]
pub(crate) fn eq_bare_symbol_swp_scan<const OBSERVED: bool>(value: Value, bare: Value) -> bool {
    value.bits() == bare.bits()
        || (value.is_veclike()
            && positioned_symbol_scan::<OBSERVED>(value)
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
fn builtin_memq_values_swp_scan<const OBSERVED: bool>(target: Value, list: Value) -> EvalResult {
    let bare = positioned_symbol_scan::<OBSERVED>(target).unwrap_or(target);
    if !bare.is_symbol() {
        return builtin_memq_values_scan::<OBSERVED>(target, list, false);
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
        if eq_bare_symbol_swp_scan::<OBSERVED>(scan_car::<OBSERVED>(tail), bare) {
            return Ok(tail);
        }
        tail = scan_cdr::<OBSERVED>(tail);
        budget -= 1;
    }
    memq_swp_exact(bare, list)
}

/// [`builtin_memq_values_swp_scan`]'s exact algorithm, from the head.
#[cold]
#[inline(never)]
fn memq_swp_exact(bare: Value, list: Value) -> EvalResult {
    let mut tail = list;
    let mut tortoise = list;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2u16;
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
pub(crate) fn builtin_assoc_3(
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
                || crate::emacs_core::value::try_equal_value_swp(
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
                if crate::emacs_core::value::try_equal_value_swp(
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
pub(crate) fn builtin_assq_2(
    eval: &mut super::eval::Context,
    key: Value,
    list: Value,
) -> EvalResult {
    builtin_assq_values(key, list, eval.symbols_with_pos_enabled)
}

/// `assq`, scanned like [`builtin_memq_values`]: no cycle bookkeeping until
/// the budget runs out, then the exact algorithm from the head.
#[inline(never)]
pub(crate) fn builtin_assq_values(
    key: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    if unobserved_list_scan() {
        builtin_assq_values_scan::<false>(key, list, symbols_with_pos_enabled)
    } else {
        builtin_assq_values_observed(key, list, symbols_with_pos_enabled)
    }
}

#[cold]
#[inline(never)]
fn builtin_assq_values_observed(
    key: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    builtin_assq_values_scan::<true>(key, list, symbols_with_pos_enabled)
}

#[inline]
fn builtin_assq_values_scan<const OBSERVED: bool>(
    key: Value,
    list: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    if symbols_with_pos_enabled {
        return builtin_assq_values_swp_scan::<OBSERVED>(key, list);
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
        let pair = scan_car::<OBSERVED>(tail);
        if pair.is_cons() && scan_car::<OBSERVED>(pair).bits() == key_bits {
            return Ok(pair);
        }
        tail = scan_cdr::<OBSERVED>(tail);
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
    let mut cycle = GnuTailCycle::new(list);

    while tail.is_cons() {
        let pair_car = tail.cons_car();
        if pair_car.is_cons() {
            let entry_key = pair_car.cons_car();
            if key_bits == entry_key.bits() {
                return Ok(pair_car);
            }
        }

        tail = tail.cons_cdr();
        cycle.check(tail)?;
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

fn builtin_assq_values_swp_scan<const OBSERVED: bool>(key: Value, list: Value) -> EvalResult {
    let bare = positioned_symbol_scan::<OBSERVED>(key).unwrap_or(key);
    if !bare.is_symbol() {
        return builtin_assq_values_scan::<OBSERVED>(key, list, false);
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
        let pair = scan_car::<OBSERVED>(tail);
        if pair.is_cons() && eq_bare_symbol_swp_scan::<OBSERVED>(scan_car::<OBSERVED>(pair), bare) {
            return Ok(pair);
        }
        tail = scan_cdr::<OBSERVED>(tail);
        budget -= 1;
    }
    assq_swp_exact(bare, list)
}

/// [`builtin_assq_values_swp_scan`]'s exact algorithm, from the head.
#[cold]
#[inline(never)]
fn assq_swp_exact(bare: Value, list: Value) -> EvalResult {
    let mut tail = list;
    let mut cycle = GnuTailCycle::new(list);

    while tail.is_cons() {
        let pair_car = tail.cons_car();
        if pair_car.is_cons() && eq_bare_symbol_swp(pair_car.cons_car(), bare) {
            return Ok(pair_car);
        }

        tail = tail.cons_cdr();
        cycle.check(tail)?;
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

pub(crate) fn copy_sequence_value(arg: Value) -> EvalResult {
    match arg.kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => {
            if unobserved_list_scan() {
                copy_list_sequence_scan::<false>(arg)
            } else {
                copy_list_sequence_scan::<true>(arg)
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
        // GNU `Fcopy_sequence`: always a fresh bool-vector.
        ValueKind::Veclike(VecLikeType::BoolVector) => {
            Ok(super::boolvec::copy_bool_vector(&arg).expect("a bool-vector"))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), arg],
        )),
    }
}

/// This source walk runs no Lisp callback or collecting safe point. Each
/// mutator selects its own capture policy once; fresh-result setters keep
/// their normal journals and barriers in both traversal variants.
#[inline]
fn copy_list_sequence_scan<const OBSERVED: bool>(arg: Value) -> EvalResult {
    let copy = Value::cons(scan_car::<OBSERVED>(arg), Value::NIL);
    let mut prev = copy;
    let mut tail = scan_cdr::<OBSERVED>(arg);
    let mut tortoise = tail;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2u16;

    while tail.is_cons() {
        let next = Value::cons(scan_car::<OBSERVED>(tail), Value::NIL);
        prev.set_cdr(next);
        prev = next;

        tail = scan_cdr::<OBSERVED>(tail);
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
        ValueKind::Cons => delete_from_list_in_place_result(&args[1], |item| {
            crate::emacs_core::value::try_equal_value_swp(elt, item, 0, symbols_with_pos_enabled)
        }),
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = args[1].as_vector_data().unwrap().clone();
            let mut changed = false;
            let mut kept = Vec::with_capacity(items.len());
            for item in items.iter() {
                if crate::emacs_core::value::try_equal_value_swp(
                    elt,
                    item,
                    0,
                    symbols_with_pos_enabled,
                )? {
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
            while pos < bytes.len() {
                let (code, len) = match storage {
                    crate::heap_types::LispStringStorageKind::Unibyte => (u32::from(bytes[pos]), 1),
                    crate::heap_types::LispStringStorageKind::Multibyte => {
                        crate::emacs_core::emacs_char::string_char_unchecked(&bytes[pos..])
                    }
                };
                if i64::from(code) == target {
                    removed += 1;
                } else {
                    kept.extend_from_slice(&bytes[pos..pos + len]);
                }
                pos += len;
            }
            if removed == 0 {
                return Ok(args[1]);
            }
            let rebuilt = match storage {
                crate::heap_types::LispStringStorageKind::Unibyte => {
                    crate::heap_types::LispString::from_unibyte(kept)
                }
                crate::heap_types::LispStringStorageKind::Multibyte => {
                    crate::heap_types::LispString::from_emacs_bytes_with_chars(
                        kept,
                        string.schars() - removed,
                    )
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

pub(crate) fn builtin_elt(args: Vec<Value>) -> EvalResult {
    expect_args("elt", &args, 2)?;
    match args[0].kind() {
        ValueKind::Cons | ValueKind::Nil => builtin_nth(vec![args[1], args[0]]),
        ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::CharTable)
        | ValueKind::Veclike(VecLikeType::BoolVector)
        | ValueKind::String => builtin_aref(vec![args[0], args[1]]),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), args[0]],
        )),
    }
}

/// The byte-code `elt` (`Op::Elt`): [`bytecode_elt_values`]. (The Lisp
/// function `elt` is [`builtin_elt`], `Felt`.)
#[inline(always)]
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
#[inline(never)]
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
        | ValueKind::Veclike(VecLikeType::BoolVector)
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
