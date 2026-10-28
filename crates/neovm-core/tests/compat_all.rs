//! One binary for the `compat_*` GNU-semantics suites.
//!
//! Cargo auto-discovers one integration target per file, and each target
//! statically links the whole 442K-line engine: 46 near-identical links for
//! ~110 assertions, 6,712 of 7,304 CPU-seconds of a `--tests` build. These are
//! process-isolated by nextest (a process per test), so one binary costs
//! nothing at run time. `autotests = false` in Cargo.toml keeps the remaining
//! integration targets explicit.

#[path = "compat_advice_place_semantics.rs"]
mod compat_advice_place_semantics;

#[path = "compat_advice_stack_semantics.rs"]
mod compat_advice_stack_semantics;

#[path = "compat_batch_kbd_macro_character_input_semantics.rs"]
mod compat_batch_kbd_macro_character_input_semantics;

#[path = "compat_batch_kbd_macro_minibuffer_semantics.rs"]
mod compat_batch_kbd_macro_minibuffer_semantics;

#[path = "compat_bootstrap_gc_smoke.rs"]
mod compat_bootstrap_gc_smoke;

#[path = "compat_bootstrap_runtime_core_surface.rs"]
mod compat_bootstrap_runtime_core_surface;

#[path = "compat_bootstrap_runtime_loaddefs_surface.rs"]
mod compat_bootstrap_runtime_loaddefs_surface;

#[path = "compat_bootstrap_runtime_state.rs"]
mod compat_bootstrap_runtime_state;

#[path = "compat_buffer_locals_semantics.rs"]
mod compat_buffer_locals_semantics;

#[path = "compat_buffer_semantics.rs"]
mod compat_buffer_semantics;

#[path = "compat_buffer_switch_semantics.rs"]
mod compat_buffer_switch_semantics;

#[path = "compat_category_semantics.rs"]
mod compat_category_semantics;

#[path = "compat_command_key_history_semantics.rs"]
mod compat_command_key_history_semantics;

#[path = "compat_default_lexical_binding_semantics.rs"]
mod compat_default_lexical_binding_semantics;

#[path = "compat_defvar_non_special_exceptions.rs"]
mod compat_defvar_non_special_exceptions;

#[path = "compat_delete_frame_semantics.rs"]
mod compat_delete_frame_semantics;

#[path = "compat_eval_after_load_semantics.rs"]
mod compat_eval_after_load_semantics;

#[path = "compat_eval_internal_environment_surface.rs"]
mod compat_eval_internal_environment_surface;

#[path = "compat_eval_special_binding_semantics.rs"]
mod compat_eval_special_binding_semantics;

#[path = "compat_face_semantics.rs"]
mod compat_face_semantics;

#[path = "compat_face_surface.rs"]
mod compat_face_surface;

#[path = "compat_file_lock_semantics.rs"]
mod compat_file_lock_semantics;

#[path = "compat_focus_event_semantics.rs"]
mod compat_focus_event_semantics;

#[path = "compat_function_shape_semantics.rs"]
mod compat_function_shape_semantics;

#[path = "compat_interpreted_closure_runtime_surface.rs"]
mod compat_interpreted_closure_runtime_surface;

#[path = "compat_keymap_semantics.rs"]
mod compat_keymap_semantics;

#[path = "compat_load_semantics.rs"]
mod compat_load_semantics;

#[path = "compat_lread_special_binding_semantics.rs"]
mod compat_lread_special_binding_semantics;

#[path = "compat_marker_semantics.rs"]
mod compat_marker_semantics;

#[path = "compat_module_semantics.rs"]
mod compat_module_semantics;

#[path = "compat_overlay_insert_semantics.rs"]
mod compat_overlay_insert_semantics;

#[path = "compat_overlay_semantics.rs"]
mod compat_overlay_semantics;

#[path = "compat_pcase_semantics.rs"]
mod compat_pcase_semantics;

#[path = "compat_provide_require_semantics.rs"]
mod compat_provide_require_semantics;

#[path = "compat_readevalloop_dynvars_semantics.rs"]
mod compat_readevalloop_dynvars_semantics;

#[path = "compat_source_bootstrap_macro_surface.rs"]
mod compat_source_bootstrap_macro_surface;

#[path = "compat_switch_to_buffer_semantics.rs"]
mod compat_switch_to_buffer_semantics;

#[path = "compat_syntax_table_semantics.rs"]
mod compat_syntax_table_semantics;

#[path = "compat_text_property_semantics.rs"]
mod compat_text_property_semantics;

#[path = "compat_text_quoting_semantics.rs"]
mod compat_text_quoting_semantics;

#[path = "compat_window_batch_display_semantics.rs"]
mod compat_window_batch_display_semantics;

#[path = "compat_window_display_semantics.rs"]
mod compat_window_display_semantics;

#[path = "compat_window_history_semantics.rs"]
mod compat_window_history_semantics;

#[path = "compat_window_position_semantics.rs"]
mod compat_window_position_semantics;

#[path = "compat_window_semantics.rs"]
mod compat_window_semantics;

#[path = "compat_window_surface.rs"]
mod compat_window_surface;

#[path = "compat_with_current_buffer_semantics.rs"]
mod compat_with_current_buffer_semantics;

#[path = "compat_write_coding_semantics.rs"]
mod compat_write_coding_semantics;
