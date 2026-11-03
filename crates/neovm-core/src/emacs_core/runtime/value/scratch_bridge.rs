//! Function-pointer bridge to the evaluator's scratch GC roots.
//!
//! Text-property storage links intermediate conses that must stay rooted while
//! they are built. The root stack itself belongs to the evaluator (its
//! thread-local layout is load-bearing), while the storage lives in the value
//! layer; the storage therefore asks through this typed table, which the
//! evaluator installs once. Until installed every operation is a typed no-op:
//! no `Context` exists yet, so nothing can collect the conses being built.

use super::Value;
use std::sync::OnceLock;

/// The evaluator's scratch-root stack operations.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScratchRootHooks {
    pub save: fn() -> usize,
    pub push: fn(Value),
    pub restore: fn(usize),
}

static SCRATCH_ROOT_HOOKS: OnceLock<ScratchRootHooks> = OnceLock::new();

/// A different scratch-root hook table is already installed. Installs are
/// one-way and idempotent: re-offering the installed table is accepted, and
/// offering a different one is a typed refusal rather than a silent skip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScratchHooksAlreadyInstalled;

/// Install the evaluator's stack operations. Idempotent like the other
/// one-time hooks: every `Context` construction calls it.
///
/// # Safety (peer seal N9, with the string-bytes stamp)
///
/// The table must route the value layer's intermediate text-property conses
/// to the evaluator's scratch-root stack on the calling thread, and it may
/// be installed only at the two evaluator seams: `Context` construction
/// (`eval/construct.rs` `new_inner`) and image restore
/// (`eval/pdump_reconstruct.rs`). The exact call-site guard in
/// `emacs_core/tests/architecture/mint_seal_guard_test.rs` pins the seams.
pub(crate) unsafe fn install_scratch_root_hooks(
    hooks: ScratchRootHooks,
) -> Result<(), ScratchHooksAlreadyInstalled> {
    match SCRATCH_ROOT_HOOKS.set(hooks) {
        Ok(()) => Ok(()),
        Err(existing) if existing == hooks => Ok(()),
        Err(_) => Err(ScratchHooksAlreadyInstalled),
    }
}

#[inline]
pub(crate) fn save_scratch_gc_roots() -> usize {
    SCRATCH_ROOT_HOOKS.get().map_or(0, |hooks| (hooks.save)())
}

#[inline]
pub(crate) fn push_scratch_gc_root(value: Value) {
    if let Some(hooks) = SCRATCH_ROOT_HOOKS.get() {
        (hooks.push)(value);
    }
}

#[inline]
pub(crate) fn restore_scratch_gc_roots(saved_len: usize) {
    if let Some(hooks) = SCRATCH_ROOT_HOOKS.get() {
        (hooks.restore)(saved_len);
    }
}
