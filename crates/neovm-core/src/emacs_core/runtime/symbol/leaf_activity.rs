//! Debug-build leaf-contract state.
//!
//! A leaf builtin runs on a shared `Context` borrow: it may not reach a GC
//! safe point, a Lisp entry, a binding push or a function-binding change.
//! Debug builds track how many leaf bodies are running and panic when one of
//! those sites is reached from inside one; release builds compile all of it
//! away.
//!
//! The depth lives with the symbol/function-binding layer because that layer
//! asserts on it directly (`Obarray::invalidate_all_function_bindings`), while
//! the leaf entry that raises it is an upper-layer concept. Keeping the state
//! here lets the bottom layer state its own invariant without reaching up.

use std::cell::Cell;

thread_local! {
    /// How many leaf bodies are running on this thread.
    static LEAF_ACTIVE: Cell<u32> = const { Cell::new(0) };
}

/// Panic when a leaf body is running. `site` names a GC safe point, a Lisp
/// entry, a binding push or a function-binding change -- a place the leaf
/// contract says no leaf can reach.
#[track_caller]
pub(crate) fn assert_no_leaf_active(site: &'static str) {
    let active = LEAF_ACTIVE.with(Cell::get);
    assert!(
        active == 0,
        "{site} reached from inside a leaf builtin: the leaf contract \
         (no GC, no Lisp, no bindings) is broken"
    );
}

/// Raises the leaf depth for its lifetime, restoring the enclosing value on
/// drop (best effort during teardown, like the caller-side TLS scopes).
#[must_use = "the leaf-active extent ends when this guard drops"]
#[derive(Debug)]
pub(crate) struct LeafActiveScope {
    previous: u32,
}

impl LeafActiveScope {
    pub(crate) fn enter() -> Self {
        Self {
            previous: LEAF_ACTIVE.with(|depth| depth.replace(depth.get().saturating_add(1))),
        }
    }
}

impl Drop for LeafActiveScope {
    fn drop(&mut self) {
        let _ = LEAF_ACTIVE.try_with(|depth| depth.replace(self.previous));
    }
}
