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
use crate::intern::SymId;
use crate::value::Value;
use crate::value::hook_publication::{self, HookInstallError, HookInstallStatus};
use std::sync::OnceLock;

/// The JIT's observations of function-cell writes.
#[derive(Clone, Copy)]
pub struct FunctionCellObserver {
    /// Count one function-epoch bump; `None` for a bump without a symbol.
    pub(super) epoch_bump: fn(FunctionEpochBump, Option<SymId>),
    /// Count one write that stored the value already there (no bump).
    pub(super) cell_unchanged: fn(),
    /// Retire compiled leaves that inlined `sym` as a callee.
    pub(super) evict_inline_dependents: fn(SymId),
    /// Keep the previous byte-code value alive while compiled frames may run it.
    pub(super) pin_redefined: fn(SymId, Value),
}

impl FunctionCellObserver {
    /// Build an inert descriptor. Only the unsafe installer publishes it.
    pub const fn new(
        epoch_bump: fn(FunctionEpochBump, Option<SymId>),
        cell_unchanged: fn(),
        evict_inline_dependents: fn(SymId),
        pin_redefined: fn(SymId, Value),
    ) -> Self {
        Self {
            epoch_bump,
            cell_unchanged,
            evict_inline_dependents,
            pin_redefined,
        }
    }
}

// Readers keep their original callback-only payload; descriptor identity is
// consulted only by the cold installer.
static FUNCTION_CELL_OBSERVER: OnceLock<FunctionCellObserver> = OnceLock::new();
static FUNCTION_CELL_OBSERVER_IDENTITY: OnceLock<&'static FunctionCellObserver> = OnceLock::new();

const _: () = {
    assert!(size_of::<FunctionCellObserver>() == 4 * size_of::<fn()>());
    assert!(align_of::<FunctionCellObserver>() == align_of::<fn()>());
};

/// Publish the canonical observer, accepting repeated installation of that
/// exact table and refusing any different table without replacing the first.
///
/// # Safety
/// The table must be the core JIT adapter's process-lifetime canonical table:
/// it meters function epochs, evicts inline dependents and retains redefined
/// byte code using the calling mutator's existing JIT state. Only that adapter
/// may call this installer, and only `Context::new_inner` and
/// `Context::from_dump` may call the adapter, before compiled code can run.
/// A refusal must be surfaced at the construction or image-restore seam.
#[must_use = "handle a conflicting function-cell observer publication"]
#[cold]
#[inline(never)]
pub unsafe fn install_function_cell_observer(
    observer: &'static FunctionCellObserver,
) -> Result<HookInstallStatus, HookInstallError> {
    hook_publication::publish(
        &FUNCTION_CELL_OBSERVER,
        &FUNCTION_CELL_OBSERVER_IDENTITY,
        observer,
    )
}

/// The installed observer, or `None` before the first `Context` exists (a
/// typed no-op: no compiled leaf can be stale yet).
#[inline(always)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn function_cell_observer() -> Option<&'static FunctionCellObserver> {
    FUNCTION_CELL_OBSERVER.get()
}

/// The evaluator's observed-run log for function-epoch moves
/// (`eval/cconv_memo`). Beside the cell observer and installed at the same
/// two one-time hooks; a typed no-op until then, so the function-cell write
/// paths never name the evaluator.
pub struct FunctionEpochMoveSink {
    callback: fn(FunctionEpochBump, Option<SymId>),
}

impl FunctionEpochMoveSink {
    /// Build inert callback metadata; this does not install or invoke it.
    pub const fn new(callback: fn(FunctionEpochBump, Option<SymId>)) -> Self {
        Self { callback }
    }
}

// The descriptor's callback field is nonzero-sized immutable data. Its data
// address gives the canonical table's identity without comparing code
// addresses or changing the existing callback-only reader payload.
static FUNCTION_EPOCH_MOVE: OnceLock<fn(FunctionEpochBump, Option<SymId>)> = OnceLock::new();
static FUNCTION_EPOCH_MOVE_IDENTITY: OnceLock<&'static fn(FunctionEpochBump, Option<SymId>)> =
    OnceLock::new();

const _: () = {
    assert!(size_of::<FunctionEpochMoveSink>() == size_of::<fn()>());
    assert!(align_of::<FunctionEpochMoveSink>() == align_of::<fn()>());
};

/// Publish the evaluator's canonical move-log table, accepting the same table
/// again and returning a typed refusal for a different one.
///
/// # Safety
/// The sink must be the core cconv adapter's process-lifetime canonical table,
/// routing to the existing calling-thread function-write log. Only that
/// adapter may call this installer, and only `Context::new_inner` and
/// `Context::from_dump` may call the adapter, before Lisp execution or restored
/// compiled code. A refusal must be surfaced at the corresponding seam.
#[must_use = "handle a conflicting function-epoch move sink publication"]
#[cold]
#[inline(never)]
pub unsafe fn install_function_epoch_move_sink(
    sink: &'static FunctionEpochMoveSink,
) -> Result<HookInstallStatus, HookInstallError> {
    hook_publication::publish(
        &FUNCTION_EPOCH_MOVE,
        &FUNCTION_EPOCH_MOVE_IDENTITY,
        &sink.callback,
    )
}

/// Report one function-epoch move to the evaluator's observed-run log when
/// one is installed.
#[inline(always)]
pub(crate) fn note_function_epoch_move(why: FunctionEpochBump, sym: Option<SymId>) {
    if let Some(sink) = FUNCTION_EPOCH_MOVE.get() {
        sink(why, sym);
    }
}
