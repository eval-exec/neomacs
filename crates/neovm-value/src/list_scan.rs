//! List-scan substrate (GNU `fns.c` `memq`/`assq` and `lisp.h`
//! `FOR_EACH_TAIL`).
//!
//! The Brent cycle check, the observed/unobserved scan split, the budgeted
//! `memq`/`assq` scans with their exact fallbacks, and the proper-list
//! walkers they share live here, below the evaluator, next to the values
//! they traverse. The subr wrappers that read `symbols-with-pos-enabled`
//! from a `Context` stay in the upper `builtins` layer and call these
//! entries; `edit`-layer callers (buffer text-property casing) reach the
//! scans directly instead of through the lisp builtins. Moved from
//! `emacs_core/lisp/native/builtins/cons_list.rs` with the stage-3 split
//! (C12).
//!
//! The `#[inline(never)]` entries keep their deliberate stack-frame shape:
//! `builtin_memq_values` and `builtin_assq_values` carry no cycle
//! bookkeeping in the fast path (208,797 `memq` calls per compile of
//! elb-smie.el), and the `#[cold]` exact fallbacks and error constructors
//! stay out of those frames.

use crate::error::{EvalResult, Flow, LispCondition, signal};
use crate::value::Value;

/// A scan can keep this choice for its whole duration only when it runs no
/// Lisp callbacks. Captures are thread scoped: another mutator's scope does
/// not change which reads belong to the current thread.
#[inline(always)]
#[doc(hidden)]
pub fn unobserved_list_scan() -> bool {
    !crate::tagged::collection_reads::reads_need_observation()
}

#[inline(always)]
#[doc(hidden)]
pub fn scan_car<const OBSERVED: bool>(value: Value) -> Value {
    if OBSERVED {
        value.cons_car()
    } else {
        value.cons_car_unobserved()
    }
}

#[inline(always)]
#[doc(hidden)]
pub fn scan_cdr<const OBSERVED: bool>(value: Value) -> Value {
    if OBSERVED {
        value.cons_cdr()
    } else {
        value.cons_cdr_unobserved()
    }
}

/// One step of GNU `FOR_EACH_TAIL_INTERNAL`'s Brent cycle check, run after
/// the walk advanced to the cons `tail`: returns `tail` when it closes a
/// cycle (the object GNU signals `circular-list` with).
///
/// `q` is GNU's `unsigned short`: `li.q = li.max` keeps only the low 16 bits
/// and the decrement wraps. With a wider counter the tortoise stops moving
/// once `max` reaches 2^17, and a cycle entered past ~131K steps is never
/// found (the walk spins instead of signaling).
#[doc(hidden)]
pub fn for_each_tail_cycle_tail(
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
#[doc(hidden)]
pub struct GnuTailCycle {
    tortoise: Value,
    max: i64,
    n: i64,
    q: u16,
    mutator: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(GnuTailCycle: Send, Sync);

impl GnuTailCycle {
    #[doc(hidden)]
    pub fn new(head: Value) -> Self {
        Self {
            tortoise: head,
            max: 2,
            n: 0,
            q: 2,
            mutator: std::marker::PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn tortoise(&self) -> Value {
        self.tortoise
    }

    #[inline]
    #[doc(hidden)]
    pub fn check(&mut self, advanced_tail: Value) -> Result<(), Flow> {
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

#[doc(hidden)]
pub fn for_each_proper_list_tail<F>(
    list: Value,
    improper_error_object: Value,
    visit: F,
) -> EvalResult
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
#[doc(hidden)]
pub fn for_each_proper_list_tail_rooting_tortoise<F>(
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
#[doc(hidden)]
pub fn for_each_proper_list_tail_scan<const OBSERVED: bool, F>(
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

#[doc(hidden)]
pub fn proper_list_length_or_signal(list: Value) -> Result<usize, Flow> {
    if unobserved_list_scan() {
        proper_list_length_or_signal_scan::<false>(list)
    } else {
        proper_list_length_or_signal_scan::<true>(list)
    }
}

#[inline]
fn proper_list_length_or_signal_scan<const OBSERVED: bool>(list: Value) -> Result<usize, Flow> {
    let mut len = 0usize;
    let mut tail = list;
    let mut tortoise = list;
    let mut max = 2i64;
    let mut n = 0i64;
    let mut q = 2u16;

    while tail.is_cons() {
        len = len.saturating_add(1);

        tail = scan_cdr::<OBSERVED>(tail);
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

#[doc(hidden)]
pub fn collect_proper_list_items(list: Value) -> Result<Vec<Value>, Flow> {
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

/// How many cells a list scan with no cycle check visits before handing the
/// list to the exact algorithm, which starts again from the head. A cycle
/// cannot hide a match from the scan: every distinct cell is visited before
/// the first repeated one, which is the only cell a cycle check reacts to.
/// Only a list with no match that is circular, improper or longer than this
/// pays the replay.
#[doc(hidden)]
pub const LIST_SCAN_BUDGET: usize = 1 << 16;

/// `memq`. The scan tests each cell and nothing else, so it needs no stack
/// frame: the tortoise-and-hare bookkeeping it no longer carries was 4 to 5
/// of each cell's instructions, and its state and an inlined error path made
/// the function save five registers on every call (208,797 calls per
/// compile of elb-smie.el).
#[inline(never)]
#[doc(hidden)]
pub fn builtin_memq_values(
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
    memq_plain_scan::<OBSERVED>(target, list, list)
}

/// [`builtin_memq_values`]'s budgeted scan continued from `tail`, a cell a
/// bounded caller-side walk has already reached; `list` is the original head
/// and is used only by the exact fallback, so an improper or circular list
/// signals exactly what it would have from the head.
#[doc(hidden)]
pub fn builtin_memq_values_from(
    target: Value,
    list: Value,
    tail: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    if unobserved_list_scan() {
        builtin_memq_values_from_scan::<false>(target, list, tail, symbols_with_pos_enabled)
    } else {
        builtin_memq_values_from_observed(target, list, tail, symbols_with_pos_enabled)
    }
}

// Same split as [`builtin_memq_values_observed`]: the recorder's registers
// stay out of the continuation's entry.
#[cold]
#[inline(never)]
fn builtin_memq_values_from_observed(
    target: Value,
    list: Value,
    tail: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    builtin_memq_values_from_scan::<true>(target, list, tail, symbols_with_pos_enabled)
}

#[inline]
fn builtin_memq_values_from_scan<const OBSERVED: bool>(
    target: Value,
    list: Value,
    tail: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    if symbols_with_pos_enabled {
        return builtin_memq_values_swp_scan_from::<OBSERVED>(target, list, tail);
    }
    memq_plain_scan::<OBSERVED>(target, list, tail)
}

/// The flag-off budgeted scan from `tail`; `list` is the original head for
/// the exact fallback.
#[inline]
fn memq_plain_scan<const OBSERVED: bool>(
    target: Value,
    list: Value,
    mut tail: Value,
) -> EvalResult {
    let target_bits = target.bits();
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
#[doc(hidden)]
pub fn memq_exact(target: Value, list: Value) -> EvalResult {
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
#[doc(hidden)]
pub fn eq_bare_symbol_swp(value: Value, bare: Value) -> bool {
    eq_bare_symbol_swp_scan::<true>(value, bare)
}

#[inline(always)]
#[doc(hidden)]
pub fn positioned_symbol_scan<const OBSERVED: bool>(value: Value) -> Option<Value> {
    if OBSERVED {
        value.as_symbol_with_pos_sym()
    } else {
        value.as_symbol_with_pos_sym_unobserved()
    }
}

#[inline(always)]
#[doc(hidden)]
pub fn eq_bare_symbol_swp_scan<const OBSERVED: bool>(value: Value, bare: Value) -> bool {
    value.bits() == bare.bits()
        || (value.is_veclike()
            && positioned_symbol_scan::<OBSERVED>(value)
                .is_some_and(|sym| sym.bits() == bare.bits()))
}

#[cold]
#[inline(never)]
#[doc(hidden)]
pub fn circular_list_error(tail: Value) -> Flow {
    signal(LispCondition::CircularList, vec![tail])
}

#[cold]
#[inline(never)]
#[doc(hidden)]
pub fn listp_error(list: Value) -> Flow {
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
    builtin_memq_values_swp_scan_from::<OBSERVED>(target, list, list)
}

/// [`builtin_memq_values_swp_scan`] from `tail`; `list` is the original head
/// for the exact fallback.
#[inline]
fn builtin_memq_values_swp_scan_from<const OBSERVED: bool>(
    target: Value,
    list: Value,
    mut tail: Value,
) -> EvalResult {
    let bare = positioned_symbol_scan::<OBSERVED>(target).unwrap_or(target);
    if !bare.is_bare_symbol() {
        return memq_plain_scan::<OBSERVED>(target, list, tail);
    }
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
#[doc(hidden)]
pub fn memq_swp_exact(bare: Value, list: Value) -> EvalResult {
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

/// `assq`, scanned like [`builtin_memq_values`]: no cycle bookkeeping until
/// the budget runs out, then the exact algorithm from the head.
#[inline(never)]
#[doc(hidden)]
pub fn builtin_assq_values(key: Value, list: Value, symbols_with_pos_enabled: bool) -> EvalResult {
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
#[doc(hidden)]
pub fn assq_exact(key: Value, list: Value) -> EvalResult {
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

fn builtin_assq_values_swp_scan<const OBSERVED: bool>(key: Value, list: Value) -> EvalResult {
    let bare = positioned_symbol_scan::<OBSERVED>(key).unwrap_or(key);
    if !bare.is_bare_symbol() {
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
#[doc(hidden)]
pub fn assq_swp_exact(bare: Value, list: Value) -> EvalResult {
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
