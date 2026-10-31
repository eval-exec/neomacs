//! The value layer's function-cell change seam.
//!
//! Every function-cell write either moves the function epoch or records that
//! it stored the same value. The JIT observes both to keep its compiled
//! caches honest: it meters the epoch, evicts leaves that inlined a redefined
//! callee, and pins the previous byte-code value while a compiled activation
//! may still run it. Those actions need the JIT's thread-local caches, so the
//! value layer owns only the observer table and the JIT installs it once.
//!
//! The observer is a typed no-op until installed: function-cell writes before
//! any `Context` exists (bare-`Obarray` tests, early construction) cannot
//! serve compiled code, so there is nothing to observe. `Context::new_inner`
//! installs it with the other one-time hooks, before any compiled leaf can be
//! created.
//!
//! The call sites are function-cell WRITE paths (fset / defalias /
//! redefinition), cold next to calls; the one `OnceLock` load and indirect
//! call there are not on a hot path.

use super::FunctionEpochBump;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value;
use std::sync::OnceLock;

/// The JIT's observations of function-cell writes.
#[derive(Clone, Copy)]
pub(crate) struct FunctionCellObserver {
    /// Count one function-epoch bump; `None` for a bump without a symbol.
    pub epoch_bump: fn(FunctionEpochBump, Option<SymId>),
    /// Count one write that stored the value already there (no bump).
    pub cell_unchanged: fn(),
    /// Retire compiled leaves that inlined `sym` as a callee.
    pub evict_inline_dependents: fn(SymId),
    /// Keep the previous byte-code value alive while compiled frames may run it.
    pub pin_redefined: fn(SymId, Value),
}

static FUNCTION_CELL_OBSERVER: OnceLock<FunctionCellObserver> = OnceLock::new();

/// Install the observer table. Idempotent: every `Context` construction calls
/// it and whichever runs first wins with the same function table.
pub(crate) fn install_function_cell_observer(observer: FunctionCellObserver) {
    let _ = FUNCTION_CELL_OBSERVER.set(observer);
}

/// The installed observer, or `None` before the first `Context` exists (a
/// typed no-op: no compiled leaf can be stale yet).
#[inline(always)]
pub(crate) fn function_cell_observer() -> Option<&'static FunctionCellObserver> {
    FUNCTION_CELL_OBSERVER.get()
}
