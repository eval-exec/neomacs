//! Canonical lifecycle hook descriptor (stage-4 W4/E11).
//!
//! The implementations stay in the modules that own their thread-local or
//! process state (dynamic-module carrier bookkeeping and loading, the
//! sqlite/dbusbind/timefns carried World state, the pdump time-zone reset);
//! this module only assembles the one descriptor eval publishes at
//! `Context::new_inner` / `Context::from_dump`.

use crate::emacs_core::eval::lifecycle::{DynamicModuleHooks, LifecycleHooks, WorldStateHooks};

#[cfg(debug_assertions)]
const DYNAMIC_MODULE_FINGERPRINT: Option<fn(&mut u64)> =
    Some(crate::emacs_core::dynamic_module::carrier_tls_fingerprint);
#[cfg(not(debug_assertions))]
const DYNAMIC_MODULE_FINGERPRINT: Option<fn(&mut u64)> = None;

static CANONICAL_LIFECYCLE_HOOKS: LifecycleHooks = LifecycleHooks::new(
    DynamicModuleHooks::new(
        crate::emacs_core::dynamic_module::reset_carrier_tls,
        DYNAMIC_MODULE_FINGERPRINT,
        crate::emacs_core::dynamic_module::panic_message,
        crate::emacs_core::dynamic_module::load_module,
    ),
    WorldStateHooks::new(
        crate::emacs_core::sqlite::world_state_placeholder,
        crate::emacs_core::sqlite::swap_world_state,
    ),
    WorldStateHooks::new(
        crate::emacs_core::dbusbind::world_state_placeholder,
        crate::emacs_core::dbusbind::swap_world_state,
    ),
    WorldStateHooks::new(
        crate::emacs_core::timefns::world_state_placeholder,
        crate::emacs_core::timefns::swap_world_state,
    ),
    crate::emacs_core::timefns::reset_timefns_thread_locals,
);

/// Publish the lifecycle table; first in the one-time hook block (E3).
pub(crate) fn install_lifecycle_hooks() {
    let installed =
        crate::emacs_core::eval::lifecycle::install_lifecycle_hooks(&CANONICAL_LIFECYCLE_HOOKS);
    debug_assert!(
        installed.is_ok(),
        "a different lifecycle hook table is already installed"
    );
}
