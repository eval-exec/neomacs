//! Cons-list plist helpers shared across overlays, strings, images, and
//! (after P2) symbols.
//!
//! Mirrors GNU `plist-get` / `plist-put` / `plist-member` semantics
//! (`fns.c`). Comparison uses `eq` (via `eq_value`) as GNU does.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{Flow, signal};
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::value::{Value, eq_value, eq_value_swp};

#[cfg(test)]
thread_local! {
    static SYMBOL_WITH_POS_PLIST_COMPARISONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PLIST_GET_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn note_symbol_with_pos_plist_comparison() {
    SYMBOL_WITH_POS_PLIST_COMPARISONS.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
pub(crate) fn reset_symbol_with_pos_plist_comparisons() {
    SYMBOL_WITH_POS_PLIST_COMPARISONS.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn symbol_with_pos_plist_comparisons() -> usize {
    SYMBOL_WITH_POS_PLIST_COMPARISONS.with(std::cell::Cell::get)
}

#[cfg(test)]
fn note_plist_get_walk() {
    PLIST_GET_WALKS.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
pub(crate) fn reset_plist_get_walks() {
    PLIST_GET_WALKS.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn plist_get_walks() -> usize {
    PLIST_GET_WALKS.with(std::cell::Cell::get)
}

/// Compile-time key-comparison policy for a plist walk.
///
/// GNU's ordinary `plist_get` is an identity-only loop. Position-symbol
/// transparency is a separate runtime mode; selecting a monomorphized policy
/// before walking keeps that mode check out of every plist entry comparison.
trait PlistKeyComparison {
    fn matches<const OBSERVE: bool>(left: &Value, right: &Value) -> bool;
}

struct ExactIdentity;

impl PlistKeyComparison for ExactIdentity {
    #[inline]
    fn matches<const OBSERVE: bool>(left: &Value, right: &Value) -> bool {
        eq_value(left, right)
    }
}

struct SymbolWithPositionTransparent;

impl PlistKeyComparison for SymbolWithPositionTransparent {
    #[inline]
    fn matches<const OBSERVE: bool>(left: &Value, right: &Value) -> bool {
        #[cfg(test)]
        note_symbol_with_pos_plist_comparison();
        compare_swp::<OBSERVE>(left, right, true)
    }
}

/// Plist walks make no Lisp callbacks, so an inactive scope stays inactive
/// through both the cons traversal and its comparisons. Other mutators'
/// captures do not include this thread's reads. Preserve the existing
/// observing comparator whenever this mutator's capture is active.
#[inline(always)]
fn compare_swp<const OBSERVE: bool>(
    left: &Value,
    right: &Value,
    symbols_with_pos_enabled: bool,
) -> bool {
    if OBSERVE {
        return eq_value_swp(left, right, symbols_with_pos_enabled);
    }
    if left.bits() == right.bits() {
        return true;
    }
    if !symbols_with_pos_enabled {
        return false;
    }
    let left = left.as_symbol_with_pos_sym_unobserved().unwrap_or(*left);
    let right = right.as_symbol_with_pos_sym_unobserved().unwrap_or(*right);
    left.bits() == right.bits()
}

fn plist_entry(prop: Value, value: Value, tail: Value) -> Value {
    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(prop);
    push_scratch_gc_root(value);
    push_scratch_gc_root(tail);

    let value_cell = Value::cons(value, tail);
    push_scratch_gc_root(value_cell);
    let entry = Value::cons(prop, value_cell);

    restore_scratch_gc_roots(saved);
    entry
}

/// GNU `FOR_EACH_TAIL_INTERNAL`'s Brent cycle state (lisp.h), for plist
/// walks that must signal `circular-list` like GNU's `FOR_EACH_TAIL`: the
/// same tail after the same number of steps. `q` is GNU's `unsigned short`
/// (it keeps the low 16 bits of `max` and wraps); a wider counter stops the
/// tortoise once `max` reaches 2^17 and never finds a late cycle.
pub(crate) struct TailCycleCheck {
    tortoise: Value,
    max: i64,
    n: i64,
    q: u16,
}

impl TailCycleCheck {
    #[inline]
    pub(crate) fn new(list: Value) -> Self {
        Self {
            tortoise: list,
            max: 2,
            n: 0,
            q: 2,
        }
    }

    /// The current tortoise, for walks that call Lisp between steps and must
    /// keep it rooted (it is compared by identity).
    #[inline]
    pub(crate) fn tortoise(&self) -> Value {
        self.tortoise
    }

    /// The macro's step after the walk advanced to `tail`: returns `tail`
    /// when it closes a cycle (the object GNU signals with).
    #[inline]
    pub(crate) fn step(&mut self, tail: Value) -> Option<Value> {
        if !tail.is_cons() {
            return None;
        }
        self.q = self.q.wrapping_sub(1);
        if self.q == 0 {
            self.n -= 1;
            if self.n <= 0 {
                self.max = self.max.saturating_mul(2);
                self.q = self.max as u16;
                self.n = self.max >> u16::BITS;
                self.tortoise = tail;
                return None;
            }
        }
        (tail.bits() == self.tortoise.bits()).then_some(tail)
    }
}

pub(crate) struct SafeTailGuard {
    tortoise: Value,
    power: usize,
    distance: usize,
}

impl SafeTailGuard {
    pub(crate) fn new(tail: Value) -> Self {
        Self {
            tortoise: tail,
            power: 1,
            distance: 0,
        }
    }

    /// Mirrors GNU's `FOR_EACH_TAIL_SAFE` cycle arm: after the caller
    /// advances to the next tail, return true if a cycle was found.
    pub(crate) fn found_cycle_after_advance(&mut self, tail: Value) -> bool {
        if !tail.is_cons() {
            return false;
        }
        self.distance = self.distance.saturating_add(1);
        if tail.bits() == self.tortoise.bits() {
            return true;
        }
        if self.distance == self.power {
            self.tortoise = tail;
            self.power = self.power.saturating_mul(2).max(1);
            self.distance = 0;
        }
        false
    }
}

/// Walk `plist` looking for `prop`. Returns the associated value or None.
/// Matches GNU `Fplist_get` when keys compare by eq.
pub fn plist_get(plist: Value, prop: &Value) -> Option<Value> {
    plist_get_select::<ExactIdentity>(plist, prop)
}

/// Walk `plist` looking for `prop`, using GNU's symbol-with-position aware
/// `EQ` semantics when `symbols_with_pos_enabled` is true.
pub fn plist_get_swp(plist: Value, prop: &Value, symbols_with_pos_enabled: bool) -> Option<Value> {
    if symbols_with_pos_enabled {
        plist_get_select::<SymbolWithPositionTransparent>(plist, prop)
    } else {
        plist_get_select::<ExactIdentity>(plist, prop)
    }
}

#[inline]
fn plist_get_select<Comparison: PlistKeyComparison>(plist: Value, prop: &Value) -> Option<Value> {
    if !crate::tagged::collection_reads::reads_need_observation() {
        plist_get_with::<Comparison, false>(plist, prop)
    } else {
        plist_get_with::<Comparison, true>(plist, prop)
    }
}

#[inline(always)]
fn read_car<const OBSERVE: bool>(cell: Value) -> Value {
    if OBSERVE {
        cell.cons_car()
    } else {
        cell.cons_car_unobserved()
    }
}

#[inline(always)]
fn read_cdr<const OBSERVE: bool>(cell: Value) -> Value {
    if OBSERVE {
        cell.cons_cdr()
    } else {
        cell.cons_cdr_unobserved()
    }
}

#[inline]
fn plist_get_with<Comparison: PlistKeyComparison, const OBSERVE: bool>(
    plist: Value,
    prop: &Value,
) -> Option<Value> {
    #[cfg(test)]
    note_plist_get_walk();
    let mut tail = plist;
    let mut safe_tail = SafeTailGuard::new(tail);
    while tail.is_cons() {
        let key = read_car::<OBSERVE>(tail);
        let rest = read_cdr::<OBSERVE>(tail);
        if !rest.is_cons() {
            return None;
        }
        if Comparison::matches::<OBSERVE>(&key, prop) {
            return Some(read_car::<OBSERVE>(rest));
        }
        tail = read_cdr::<OBSERVE>(rest);
        if safe_tail.found_cycle_after_advance(tail) {
            return None;
        }
    }
    None
}

/// Put `value` under `prop` in `plist`. If `prop` is already in the list,
/// mutate the existing value cell in place (matching GNU `Fplist_put`).
/// Otherwise append `(prop value)` to the end of the list (also matching
/// GNU, which walks to the tail and splices). Returns `(new_plist, changed)`
/// where `changed` indicates whether the effective binding changed (for
/// modification-tick bookkeeping).
///
/// On a malformed plist (walk runs off a non-cons non-nil tail), signals
/// `wrong-type-argument plistp plist`. Matches GNU `Fplist_put`
/// (`fns.c:2703-2727`).
pub fn plist_put(plist: Value, prop: Value, value: Value) -> Result<(Value, bool), Flow> {
    plist_put_swp(plist, prop, value, false)
}

/// `plist_put` variant whose key comparison mirrors GNU `EQ` while
/// `symbols-with-pos-enabled` is non-nil.
pub fn plist_put_swp(
    plist: Value,
    prop: Value,
    value: Value,
    symbols_with_pos_enabled: bool,
) -> Result<(Value, bool), Flow> {
    if !crate::tagged::collection_reads::reads_need_observation() {
        plist_put_walk::<false>(plist, prop, value, symbols_with_pos_enabled)
    } else {
        plist_put_walk::<true>(plist, prop, value, symbols_with_pos_enabled)
    }
}

#[inline]
fn plist_put_walk<const OBSERVE: bool>(
    plist: Value,
    prop: Value,
    value: Value,
    symbols_with_pos_enabled: bool,
) -> Result<(Value, bool), Flow> {
    // Empty plist: create a fresh two-element list.
    if !plist.is_cons() {
        if !plist.is_nil() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("plistp"), plist],
            ));
        }
        let changed = !value.is_nil();
        return Ok((plist_entry(prop, value, Value::NIL), changed));
    }
    let mut tail = plist;
    let mut last_value_cell: Option<Value> = None;
    let mut cycle = TailCycleCheck::new(plist);
    loop {
        if !tail.is_cons() {
            // End of walk. If it's nil, append. If not, malformed plist.
            if !tail.is_nil() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("plistp"), plist],
                ));
            }
            // Append (prop value) to the tail of `plist`.
            let new_tail = plist_entry(prop, value, Value::NIL);
            if let Some(lvc) = last_value_cell {
                lvc.set_cdr(new_tail);
            }
            return Ok((plist, !value.is_nil()));
        }
        let key = read_car::<OBSERVE>(tail);
        let rest = read_cdr::<OBSERVE>(tail);
        if !rest.is_cons() {
            // Odd-length plist (non-cons tail after key). Signal as malformed.
            if !rest.is_nil() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("plistp"), plist],
                ));
            }
            // rest is nil — odd-length plist. GNU treats as malformed too — signal.
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("plistp"), plist],
            ));
        }
        if compare_swp::<OBSERVE>(&key, &prop, symbols_with_pos_enabled) {
            let changed = !compare_swp::<OBSERVE>(
                &read_car::<OBSERVE>(rest),
                &value,
                symbols_with_pos_enabled,
            );
            rest.set_car(value);
            return Ok((plist, changed));
        }
        last_value_cell = Some(rest);
        tail = read_cdr::<OBSERVE>(rest);
        // GNU plist_put walks with FOR_EACH_TAIL: a circular plist signals.
        if let Some(cycle_tail) = cycle.step(tail) {
            return Err(signal(LispCondition::CircularList, vec![cycle_tail]));
        }
    }
}

/// Validate that `plist` is a proper plist (NIL or an even-length cons
/// chain with a NIL tail). Signals `(wrong-type-argument plistp plist)`
/// on any malformed tail.
///
/// Used by callers that must fail on a malformed plist BEFORE performing
/// unrelated side effects (e.g. allocating a registration ID), so the
/// error path leaves no partial state behind. GNU does equivalent
/// validation at the top of many plist-mutating operations.
pub fn plist_check(plist: Value) -> Result<(), Flow> {
    let mut tail = plist;
    loop {
        if tail.is_nil() {
            return Ok(());
        }
        if !tail.is_cons() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("plistp"), plist],
            ));
        }
        let rest = tail.cons_cdr();
        if !rest.is_cons() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("plistp"), plist],
            ));
        }
        tail = rest.cons_cdr();
    }
}

/// Return the sub-list of `plist` starting at the first match for `prop`,
/// or NIL if not found. Matches GNU `Fplist_member`.
pub fn plist_member(plist: Value, prop: &Value) -> Value {
    let mut tail = plist;
    loop {
        if !tail.is_cons() {
            return Value::NIL;
        }
        let key = tail.cons_car();
        let rest = tail.cons_cdr();
        if !rest.is_cons() {
            return Value::NIL;
        }
        if eq_value(&key, prop) {
            return tail;
        }
        tail = rest.cons_cdr();
    }
}

#[cfg(test)]
#[path = "tests/collection_capture_test.rs"]
mod collection_capture;
