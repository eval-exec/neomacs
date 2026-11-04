//! Lifecycle hook table (stage-4 W4/E11).
//!
//! The modules whose lifecycle these hooks drive own their thread-local state
//! in the core/system layer: dynamic-module carrier bookkeeping and module
//! loading, the sqlite/dbusbind/timefns carried World state, and the time-zone
//! reset at pdump reconstruct. The evaluator reaches them through this one
//! cold PC2 table, installed by core at `Context::new_inner` and
//! `Context::from_dump` before any GC-capable allocation.
//!
//! Frequency: bootstrap / pdump / carrier park / per module call — never in a
//! Lisp loop. The table does not enumerate GC roots; the dynamic-module
//! registry's root collection already lives in eval data (E6d) and is not
//! reached through this table. The uninstalled read is a fail-closed
//! assertion: every call site runs only after construction/dump, so a missing
//! descriptor means the construction seam itself is broken.

use super::*;
use neovm_value::error::EvalResult;
use neovm_value::{HookInstallError, HookInstallStatus};
use std::any::Any;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Produce a placeholder for the World's carried thread-local state.
pub type WorldStatePlaceholder = fn() -> Box<dyn Any>;
/// Swap the World's carried thread-local state with a parked slot.
pub type WorldStateSwap = fn(&mut dyn Any);

/// Dynamic-module lifecycle bodies (carrier hooks, panic text, module load).
#[derive(Clone, Copy)]
pub struct DynamicModuleHooks {
    reset: fn(),
    fingerprint: Option<fn(&mut u64)>,
    panic_message: fn(&(dyn Any + Send)) -> String,
    load_module: fn(&mut Context, PathBuf) -> EvalResult,
}

impl DynamicModuleHooks {
    /// Build the dynamic-module group.
    pub const fn new(
        reset: fn(),
        fingerprint: Option<fn(&mut u64)>,
        panic_message: fn(&(dyn Any + Send)) -> String,
        load_module: fn(&mut Context, PathBuf) -> EvalResult,
    ) -> Self {
        Self {
            reset,
            fingerprint,
            panic_message,
            load_module,
        }
    }
}

/// One module's carried World-state pair.
#[derive(Clone, Copy)]
pub struct WorldStateHooks {
    placeholder: WorldStatePlaceholder,
    swap: WorldStateSwap,
}

impl WorldStateHooks {
    /// Build one carried World-state pair.
    pub const fn new(placeholder: WorldStatePlaceholder, swap: WorldStateSwap) -> Self {
        Self { placeholder, swap }
    }
}

/// The cold lifecycle descriptor core publishes at the construction seams.
#[derive(Clone, Copy)]
pub struct LifecycleHooks {
    dynamic_module: DynamicModuleHooks,
    sqlite: WorldStateHooks,
    dbusbind: WorldStateHooks,
    timefns: WorldStateHooks,
    timefns_reset: fn(),
}

impl LifecycleHooks {
    /// Build the one canonical descriptor core publishes.
    pub const fn new(
        dynamic_module: DynamicModuleHooks,
        sqlite: WorldStateHooks,
        dbusbind: WorldStateHooks,
        timefns: WorldStateHooks,
        timefns_reset: fn(),
    ) -> Self {
        Self {
            dynamic_module,
            sqlite,
            dbusbind,
            timefns,
            timefns_reset,
        }
    }

    /// Dynamic-module lifecycle group.
    pub fn dynamic_module(&self) -> DynamicModuleHooks {
        self.dynamic_module
    }

    /// SQLite carried World state.
    pub fn sqlite(&self) -> WorldStateHooks {
        self.sqlite
    }

    /// D-Bus carried World state.
    pub fn dbusbind(&self) -> WorldStateHooks {
        self.dbusbind
    }

    /// Time functions carried World state.
    pub fn timefns(&self) -> WorldStateHooks {
        self.timefns
    }

    /// The pdump-reconstruct time-zone reset.
    pub fn timefns_reset(&self) -> fn() {
        self.timefns_reset
    }
}

impl WorldStateHooks {
    fn placeholder(&self) -> WorldStatePlaceholder {
        self.placeholder
    }

    fn swap(&self) -> WorldStateSwap {
        self.swap
    }
}

impl DynamicModuleHooks {
    fn reset(&self) -> fn() {
        self.reset
    }

    fn fingerprint(&self) -> Option<fn(&mut u64)> {
        self.fingerprint
    }

    fn panic_message(&self) -> fn(&(dyn Any + Send)) -> String {
        self.panic_message
    }

    fn load_module(&self) -> fn(&mut Context, PathBuf) -> EvalResult {
        self.load_module
    }
}

static LIFECYCLE_HOOKS: OnceLock<LifecycleHooks> = OnceLock::new();
static LIFECYCLE_HOOKS_IDENTITY: OnceLock<&'static LifecycleHooks> = OnceLock::new();

/// Publish the lifecycle descriptor.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Core calls
/// this at `Context::new_inner` and `Context::from_dump`, first in the
/// one-time hook block so it precedes any GC-capable allocation.
#[must_use = "handle a conflicting lifecycle publication"]
#[cold]
#[inline(never)]
pub fn install_lifecycle_hooks(
    hooks: &'static LifecycleHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    let selected = LIFECYCLE_HOOKS_IDENTITY.get_or_init(|| hooks);
    if !std::ptr::eq(*selected, hooks) {
        return Err(HookInstallError::DifferentTable);
    }
    match LIFECYCLE_HOOKS.set(**selected) {
        Ok(()) => Ok(HookInstallStatus::Installed),
        Err(_same_descriptor) => Ok(HookInstallStatus::AlreadyInstalled),
    }
}

/// The installed descriptor.
///
/// Fail-closed: every call site (carrier park, JIT panic path, module load,
/// pdump reconstruct) runs after `new_inner`/`from_dump`, so a missing table
/// means the construction seam is broken and the state swap must not be
/// silently skipped.
fn lifecycle_hooks() -> &'static LifecycleHooks {
    LIFECYCLE_HOOKS
        .get()
        .expect("lifecycle hooks install at Context construction")
}

/// Clear the dynamic-module carrier thread-local state on a carrier park.
pub fn reset_dynamic_module_carrier_tls() {
    (lifecycle_hooks().dynamic_module().reset())();
}

/// Debug-only fingerprint of the dynamic-module carrier state.
#[cfg(debug_assertions)]
pub fn dynamic_module_carrier_tls_fingerprint(h: &mut u64) {
    if let Some(fingerprint) = lifecycle_hooks().dynamic_module().fingerprint() {
        fingerprint(h);
    }
}

/// Text of a caught dynamic-module panic payload.
pub fn dynamic_module_panic_message(payload: &(dyn Any + Send)) -> String {
    (lifecycle_hooks().dynamic_module().panic_message())(payload)
}

/// Load a dynamic module from PATH through the core loader.
pub fn load_module(ctx: &mut Context, path: PathBuf) -> EvalResult {
    (lifecycle_hooks().dynamic_module().load_module())(ctx, path)
}

/// Placeholder for SQLite's carried World state.
pub fn sqlite_world_state_placeholder() -> Box<dyn Any> {
    (lifecycle_hooks().sqlite().placeholder())()
}

/// Swap SQLite's carried World state with a parked slot.
pub fn sqlite_swap_world_state(slot: &mut dyn Any) {
    (lifecycle_hooks().sqlite().swap())(slot)
}

/// Placeholder for D-Bus's carried World state.
pub fn dbusbind_world_state_placeholder() -> Box<dyn Any> {
    (lifecycle_hooks().dbusbind().placeholder())()
}

/// Swap D-Bus's carried World state with a parked slot.
pub fn dbusbind_swap_world_state(slot: &mut dyn Any) {
    (lifecycle_hooks().dbusbind().swap())(slot)
}

/// Placeholder for the time-zone rule's carried World state.
pub fn timefns_world_state_placeholder() -> Box<dyn Any> {
    (lifecycle_hooks().timefns().placeholder())()
}

/// Swap the time-zone rule's carried World state with a parked slot.
pub fn timefns_swap_world_state(slot: &mut dyn Any) {
    (lifecycle_hooks().timefns().swap())(slot)
}

/// Reset the time-zone thread-local rule at pdump reconstruct.
pub fn reset_timefns_thread_locals() {
    (lifecycle_hooks().timefns_reset())()
}
