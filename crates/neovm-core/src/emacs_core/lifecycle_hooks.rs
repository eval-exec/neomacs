//! Canonical lifecycle hook descriptor (stage-4 W4/E11).
//!
//! The implementations stay in the modules that own their thread-local or
//! process state (dynamic-module carrier bookkeeping and loading, the
//! sqlite/dbusbind/timefns carried World state, the pdump time-zone reset);
//! this module only assembles the one descriptor eval publishes at
//! `Context::new_inner` / `Context::from_dump`.

use crate::emacs_core::eval::lifecycle::{
    BootHooks, DynamicModuleHooks, LifecycleHooks, WorldStateHooks,
};
use neovm_value::display_vocab::FrameId;

#[cfg(debug_assertions)]
const DYNAMIC_MODULE_FINGERPRINT: Option<fn(&mut u64)> =
    Some(crate::emacs_core::dynamic_module::carrier_tls_fingerprint);
#[cfg(not(debug_assertions))]
const DYNAMIC_MODULE_FINGERPRINT: Option<fn(&mut u64)> = None;

/// Apply the post-image `Lread` constant rows (GNU's startup constant reset).
fn apply_lread_constants(ctx: &mut crate::emacs_core::eval::Context) {
    for row in crate::emacs_core::post_image_init::PostImageInit::Lread.constants() {
        ctx.set_variable(row.name, row.value.value());
    }
}

/// Seed the initial startup frame before display initialization.
fn seed_batch_startup_frame(ctx: &mut crate::emacs_core::eval::Context) -> FrameId {
    crate::emacs_core::window_cmds::seed_batch_startup_frame_in_state(
        &mut ctx.frames,
        &mut ctx.edit.buffers,
    )
}

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
    BootHooks::new(
        crate::emacs_core::xfaces::ensure_startup_compat_variables,
        crate::emacs_core::xfaces::clear_created_lisp_face,
        crate::emacs_core::xfaces::remove_face_new_frame_defaults_entry,
        crate::emacs_core::xfaces::restore_created_faces_from_table,
        crate::emacs_core::display::gui_window_system_symbol,
        crate::emacs_core::post_image_init::apply_post_image_init,
        apply_lread_constants,
        crate::emacs_core::environment::install_host_environment_snapshot,
        crate::emacs_core::shell_file_name::install,
        crate::emacs_core::shell_file_name::install_dump_default,
        seed_batch_startup_frame,
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
