//! Cons-list plist helpers shared across overlays, strings, images, and
//! symbols.
//!
//! Mirrors GNU `plist-get` / `plist-put` / `plist-member` semantics
//! (`fns.c`). Comparison uses `eq` (via `eq_value`) as GNU does.
//!
//! The value-layer primitives live in the moving `symbol::plist_core` module
//! so symbol and error code can use them after the neovm-value move; this
//! module keeps the Lisp-command layer and re-exports those primitives.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{Flow, signal};
use crate::emacs_core::value::{Value, eq_value};

pub(crate) use crate::emacs_core::symbol::plist_core::{SafeTailGuard, TailCycleCheck};
pub use crate::emacs_core::symbol::plist_core::{
    plist_get, plist_get_swp, plist_put, plist_put_swp,
};
#[cfg(test)]
pub(crate) use crate::emacs_core::symbol::plist_core::{
    plist_get_walks, reset_plist_get_walks, reset_symbol_with_pos_plist_comparisons,
    symbol_with_pos_plist_comparisons,
};

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
