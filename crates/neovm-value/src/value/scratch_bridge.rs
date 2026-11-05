//! Function-pointer bridge to the evaluator's scratch GC roots.
//!
//! Text-property storage links intermediate conses that must stay rooted while
//! they are built. The root stack itself belongs to the evaluator (its
//! thread-local layout is load-bearing), while the storage lives in the value
//! layer; the storage therefore asks through this typed table, which the
//! evaluator installs once. Until installed every operation is a typed no-op:
//! no `Context` exists yet, so nothing can collect the conses being built.

use super::Value;
use super::hook_publication::{HookInstallError, HookInstallStatus};
use std::sync::OnceLock;

/// The evaluator's scratch-root stack operations.
#[derive(Clone, Copy)]
pub struct ScratchRootHooks {
    save: fn() -> usize,
    push: fn(Value),
    restore: fn(usize),
}

impl ScratchRootHooks {
    /// Declare inert callback data; only the unsafe installer publishes it.
    pub const fn new(save: fn() -> usize, push: fn(Value), restore: fn(usize)) -> Self {
        Self {
            save,
            push,
            restore,
        }
    }
}

// Keep the installed payload at its original callback-only footprint.
const _: () = {
    assert!(size_of::<ScratchRootHooks>() == 3 * size_of::<fn()>());
    assert!(align_of::<ScratchRootHooks>() == align_of::<fn()>());
};

static SCRATCH_ROOT_HOOKS: OnceLock<ScratchRootHooks> = OnceLock::new();
static SCRATCH_ROOT_HOOKS_IDENTITY: OnceLock<&'static ScratchRootHooks> = OnceLock::new();

/// Install the evaluator's one canonical scratch-root stack descriptor.
///
/// # Safety
/// Call only through core's `eval::construct::install_scratch_root_hooks`
/// adapter, reached only from `Context::new_inner` or `Context::from_dump`,
/// before that Context can allocate through text-property storage. `hooks`
/// must borrow that adapter's single named `CANONICAL_SCRATCH_ROOT_HOOKS`
/// static; its save/push/restore must operate on the evaluator's same
/// mutator-local scratch stack and retain each pushed value as a GC root
/// until restoring its saved length. The callbacks live for the process.
/// A different descriptor is refused and must not be ignored.
#[must_use = "handle a conflicting scratch-root publication"]
#[cold]
#[inline(never)]
pub unsafe fn install_scratch_root_hooks(
    hooks: &'static ScratchRootHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    super::hook_publication::publish(&SCRATCH_ROOT_HOOKS, &SCRATCH_ROOT_HOOKS_IDENTITY, hooks)
}

/// Save the evaluator's scratch-root stack length.
///
/// The route for every scratch-root consumer below the evaluator: this
/// crate's own storage and, after the stage-3 split, the edit layer's
/// Context-free bodies, which cannot name the evaluator's free functions.
#[doc(hidden)]
#[inline]
pub fn save_scratch_gc_roots() -> usize {
    SCRATCH_ROOT_HOOKS.get().map_or(0, |hooks| (hooks.save)())
}

/// Root `value` on the evaluator's scratch-root stack.
///
/// See [`save_scratch_gc_roots`]: callers must run only under a live
/// evaluator (the hooks install at `Context` construction); pre-install
/// pushes are typed no-ops, as nothing can collect yet.
#[doc(hidden)]
#[inline]
pub fn push_scratch_gc_root(value: Value) {
    if let Some(hooks) = SCRATCH_ROOT_HOOKS.get() {
        (hooks.push)(value);
    }
}

/// Truncate the scratch-root stack back to a saved length.
///
/// See [`save_scratch_gc_roots`] for the audience and pre-install behavior.
#[doc(hidden)]
#[inline]
pub fn restore_scratch_gc_roots(saved_len: usize) {
    if let Some(hooks) = SCRATCH_ROOT_HOOKS.get() {
        (hooks.restore)(saved_len);
    }
}

#[cfg(test)]
#[path = "scratch_bridge/tests.rs"]
mod tests;
