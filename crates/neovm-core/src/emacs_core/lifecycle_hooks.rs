//! Canonical lifecycle hook descriptor (stage-4 W4/E11).
//!
//! The implementations stay in the modules that own their thread-local or
//! process state (dynamic-module carrier bookkeeping and loading, the
//! sqlite/dbusbind/timefns carried World state, the pdump time-zone reset);
//! this module only assembles the one descriptor eval publishes at
//! `Context::new_inner` / `Context::from_dump`.

use crate::emacs_core::eval::lifecycle::{
    BootHooks, CommandSeamHooks, DbusbindHooks, DynamicModuleHooks, FileIoHooks, FontsetHooks,
    LifecycleHooks, TerminalHooks, WindowTreeHooks, WorldStateHooks, XfacesHooks,
};
use neovm_value::display_vocab::FrameId;

#[cfg(debug_assertions)]
const DYNAMIC_MODULE_FINGERPRINT: Option<fn(&mut u64)> =
    Some(crate::emacs_core::dynamic_module::carrier_tls_fingerprint);
#[cfg(not(debug_assertions))]
const DYNAMIC_MODULE_FINGERPRINT: Option<fn(&mut u64)> = None;

#[cfg(debug_assertions)]
const TERMINAL_TLS_FINGERPRINT: Option<fn(&mut u64)> =
    Some(crate::emacs_core::terminal::pure::carrier_tls_fingerprint);
#[cfg(not(debug_assertions))]
const TERMINAL_TLS_FINGERPRINT: Option<fn(&mut u64)> = None;

#[cfg(debug_assertions)]
const XFACES_TLS_FINGERPRINT: Option<fn(&mut u64)> =
    Some(crate::emacs_core::xfaces::carrier_tls_fingerprint);
#[cfg(not(debug_assertions))]
const XFACES_TLS_FINGERPRINT: Option<fn(&mut u64)> = None;

/// Apply the post-image `Lread` constant rows (GNU's startup constant reset).
fn apply_lread_constants(ctx: &mut crate::emacs_core::eval::Context) {
    for row in crate::emacs_core::post_image_init::PostImageInit::Lread.constants() {
        ctx.set_variable(row.name, row.value.value());
    }
}

/// Invoke the display `frame-parameter` Lisp entry for the environment
/// fallback (cold; only when no frame value resolves).
fn frame_parameter_entry(
    eval: &mut crate::emacs_core::eval::Context,
    frame: crate::emacs_core::value::Value,
    key: crate::emacs_core::value::Value,
) -> crate::emacs_core::error::EvalResult {
    crate::emacs_core::frame::builtin_frame_parameter(eval, &[frame, key])
}

/// Seed the initial startup frame before display initialization.
fn seed_batch_startup_frame(ctx: &mut crate::emacs_core::eval::Context) -> FrameId {
    let (frames, buffers) = ctx.frame_manager_and_buffers_mut();
    crate::emacs_core::window_cmds::seed_batch_startup_frame_in_state(frames, buffers)
}

/// Register the bootstrap variables of the core modules that stay above eval.
fn register_core_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    crate::emacs_core::fileio::register_bootstrap_vars(obarray);
    crate::emacs_core::process::register_bootstrap_vars(obarray);
    crate::emacs_core::dbusbind::register_bootstrap_vars(obarray);
    crate::emacs_core::xfaces::register_bootstrap_vars(obarray);
}

/// Register the platform bootstrap variables owned by the system crate.
///
/// GNU registers the w32 family's `syms_of_w32*` DEFVARs/DEFSYMs before
/// loadup reads them. The platform module moved above eval in the split, so
/// the call comes from this core-side boot hook at the same construction
/// point the evaluator used before the move; eval never names the module.
#[cfg(windows)]
fn register_platform_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    crate::emacs_core::w32::register_bootstrap_symbols(obarray);
}

#[cfg(not(windows))]
fn register_platform_bootstrap_vars(_obarray: &mut crate::emacs_core::symbol::Obarray) {}

/// Core-side char-property window lookup (the lookup body itself lives with
/// the window state in eval data; eval receives a plain fn pointer).
fn char_property_window_lookup(
    frames: Option<&crate::window::FrameManager>,
    window: neovm_edit::emacs_core::display_metrics::WindowId,
) -> crate::emacs_core::textprop::WindowBufferLookup {
    match frames {
        Some(frames) => {
            crate::emacs_core::eval::data::window_state::char_property_window_buffer(frames, window)
        }
        None => crate::emacs_core::textprop::WindowBufferLookup::NotAvailable,
    }
}

/// Configure the terminal runtime for a graphical display identity.
fn configure_window_system_runtime(identity: neomacs_display_protocol::GraphicalDisplayIdentity) {
    use crate::emacs_core::terminal::pure::{TerminalRuntimeConfig, configure_terminal_runtime};
    configure_terminal_runtime(TerminalRuntimeConfig::window_system(identity));
}

/// Apply a planned minibuffer resize (display-owned frame layout chain).
fn apply_mini_window_resize(
    frame: &mut crate::window::Frame,
    resize: crate::emacs_core::eval::data::window_state::MiniWindowResize,
) {
    use crate::window::DisplayFrameExt;
    frame.apply_mini_window_resize(resize);
}

fn terminal_park_carrier_tls() {
    crate::emacs_core::terminal::pure::park_carrier_tls()
}

fn terminal_reset_thread_locals() {
    crate::emacs_core::terminal::pure::reset_terminal_thread_locals()
}

fn terminal_reset_handle() {
    crate::emacs_core::terminal::pure::reset_terminal_handle()
}

fn terminal_world_state_placeholder() -> Box<dyn std::any::Any> {
    crate::emacs_core::terminal::pure::world_state_placeholder()
}

fn terminal_swap_world_state(slot: &mut dyn std::any::Any) {
    crate::emacs_core::terminal::pure::swap_world_state(slot)
}

fn live_terminal_ids_in_keyboard_poll_order() -> Vec<u64> {
    crate::emacs_core::terminal::pure::live_terminal_ids_in_keyboard_poll_order()
}

fn xfaces_park_carrier_tls() {
    crate::emacs_core::xfaces::park_carrier_tls()
}

static CANONICAL_LIFECYCLE_HOOKS: LifecycleHooks = LifecycleHooks::new(
    DynamicModuleHooks::new(
        crate::emacs_core::dynamic_module::reset_carrier_tls,
        DYNAMIC_MODULE_FINGERPRINT,
        crate::emacs_core::dynamic_module::panic_message,
        crate::emacs_core::dynamic_module::load_module,
        crate::emacs_core::dynamic_module::apply_module_function,
        crate::emacs_core::dynamic_module::module_call_active,
        crate::emacs_core::dynamic_module::collect_dynamic_module_gc_roots,
    ),
    WorldStateHooks::new(
        crate::emacs_core::sqlite::world_state_placeholder,
        crate::emacs_core::sqlite::swap_world_state,
    ),
    DbusbindHooks::new(
        WorldStateHooks::new(
            crate::emacs_core::dbusbind::world_state_placeholder,
            crate::emacs_core::dbusbind::swap_world_state,
        ),
        crate::emacs_core::dbusbind::drain_events,
        crate::emacs_core::dbusbind::has_active_watches,
        crate::emacs_core::dbusbind::reset_thread_locals,
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
        register_core_bootstrap_vars,
        register_platform_bootstrap_vars,
        crate::emacs_core::xfaces::init_frame_lisp_faces,
        crate::emacs_core::xfaces::realization::prepare_runtime_faces,
        crate::emacs_core::xfaces::sync_runtime_face_table_from_frame_lisp_faces,
        crate::emacs_core::xfaces::ensure_lisp_face_id_property,
        crate::emacs_core::xfaces::clear_font_cache_state,
    ),
    FileIoHooks::new(
        crate::emacs_core::fileio::builtin_expand_file_name,
        crate::emacs_core::fileio::dispatch_file_handler,
        crate::emacs_core::fileio::resolve_filename_lisp_for_eval,
        crate::emacs_core::filelock::lock_file,
        crate::emacs_core::filelock::unlock_file,
    ),
    TerminalHooks::new(
        crate::emacs_core::terminal::pure::terminal_handle_value,
        crate::emacs_core::terminal::pure::delete_terminal_noelisp_owned,
        crate::emacs_core::terminal::pure::builtin_frame_terminal,
        crate::emacs_core::terminal::pure::print_terminal_handle,
        configure_window_system_runtime,
        terminal_park_carrier_tls,
        TERMINAL_TLS_FINGERPRINT,
        terminal_reset_thread_locals,
        terminal_reset_handle,
        terminal_world_state_placeholder,
        terminal_swap_world_state,
        live_terminal_ids_in_keyboard_poll_order,
        crate::emacs_core::terminal::pure::current_terminal_registry_handle,
        crate::emacs_core::terminal::pure::install_terminal_registry_handle,
        crate::emacs_core::terminal::pure::install_owner_terminal_registry_handle,
    ),
    FontsetHooks::new(
        crate::emacs_core::fontset::fontset_alias_alist_startup_value,
        crate::emacs_core::fontset::fontset_list_value,
        crate::emacs_core::fontset::reset_fontset_registry,
        crate::emacs_core::fontset::snapshot_fontset_registry,
        crate::emacs_core::fontset::restore_fontset_registry,
    ),
    CommandSeamHooks::new(
        crate::emacs_core::window_hooks::builtin_current_window_configuration,
        crate::emacs_core::window_hooks::set_window_configuration_with_options,
        crate::emacs_core::window_hooks::run_redisplay_window_change_hooks,
        crate::emacs_core::minibuffer::unwind_minibuffer_session,
        crate::emacs_core::minibuffer::unwind_minibuffer_buffer,
        crate::emacs_core::textprop_entries::builtin_get_char_property,
        crate::emacs_core::minibuffer::builtin_try_completion,
        crate::emacs_core::minibuffer::builtin_all_completions,
        crate::emacs_core::minibuffer::builtin_test_completion,
        crate::emacs_core::keymap::menu_bar_top_level_items,
        crate::emacs_core::interactive::builtin_key_binding,
        crate::emacs_core::interactive::builtin_where_is_internal,
    ),
    WindowTreeHooks::new(
        char_property_window_lookup,
        crate::window::window_resize_check,
        crate::window::window_resize_apply,
        apply_mini_window_resize,
        crate::window::resize_pixelwise_with_buffer_constraints,
        frame_parameter_entry,
    ),
    XfacesHooks::new(
        xfaces_park_carrier_tls,
        XFACES_TLS_FINGERPRINT,
        crate::emacs_core::xfaces::current_font_registry_handle,
        crate::emacs_core::xfaces::install_font_registry_handle,
    ),
    crate::emacs_core::timefns::reset_timefns_thread_locals,
    crate::emacs_core::sqlite::reset_sqlite_thread_locals,
);

/// The one canonical lifecycle descriptor (its publication is E3a's first
/// slot; core no longer exposes a per-table installer, peer follow-up F2).
pub(crate) fn canonical_lifecycle_hooks() -> &'static LifecycleHooks {
    &CANONICAL_LIFECYCLE_HOOKS
}
