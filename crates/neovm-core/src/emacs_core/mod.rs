//! Elisp interpreter module.
//!
//! Provides a full Elisp evaluator with:
//! - Value types: nil, t, int, float, string, symbol, keyword, char, cons, vector, hash-table
//! - Complete parser: strings, floats, chars, vectors, dotted pairs, quasiquote, reader macros
//! - Special forms: quote, function, let, let*, setq, if, and, or, cond, while, progn, prog1,
//!   lambda, defun, defvar, defconst, defmacro, funcall, catch, throw, unwind-protect,
//!   condition-case, when, unless
//! - 100+ built-in functions: arithmetic, comparisons, type predicates, list ops, string ops,
//!   vector ops, hash tables, higher-order functions, conversion, property lists

// This file is the stable module-path seam. Physical ownership is grouped by
// domain below while callers continue to use `crate::emacs_core::<module>`.

// Command input and invocation.
#[path = "commands/abbrev/mod.rs"]
pub mod abbrev;
#[path = "commands/buffer_ops/mod.rs"]
pub mod buffer_ops;
#[path = "commands/casefiddle_entries/mod.rs"]
pub mod casefiddle_entries;
#[path = "commands/casetab_entries/mod.rs"]
pub mod casetab_entries;
#[path = "commands/chartable_entries/mod.rs"]
pub mod chartable_entries;
#[path = "commands/command_observation/mod.rs"]
pub(crate) mod command_observation;
#[path = "commands/display_entries/mod.rs"]
pub mod display_entries;
#[path = "commands/format_time_entries/mod.rs"]
pub mod format_time_entries;
#[path = "commands/gnutls_entries/mod.rs"]
pub(crate) mod gnutls_entries;
#[path = "commands/identity_commands/mod.rs"]
pub mod identity_commands;
pub(crate) mod input_hooks;
#[path = "commands/interactive/mod.rs"]
pub mod interactive;
#[path = "commands/kbd/mod.rs"]
pub mod kbd;
#[path = "commands/keyboard/mod.rs"]
pub mod keyboard;
#[path = "commands/keymap/mod.rs"]
pub mod keymap;
#[path = "commands/kill_buffer/mod.rs"]
pub mod kill_buffer;
#[path = "commands/kmacro/mod.rs"]
pub mod kmacro;
pub(crate) mod lifecycle_hooks;
#[path = "commands/minibuffer/mod.rs"]
pub mod minibuffer;
pub(crate) mod process_hooks;
#[path = "commands/window_hooks/mod.rs"]
pub mod window_hooks;
pub(crate) use neovm_value::prefix;
#[path = "commands/register/mod.rs"]
pub mod register;
#[path = "commands/syntax_entries/mod.rs"]
pub mod syntax_entries;
#[path = "commands/system_entries/mod.rs"]
pub(crate) mod system_entries;
#[path = "commands/textprop_entries/mod.rs"]
pub mod textprop_entries;

// Display model, redisplay, and graphical/terminal surfaces.
pub use neovm_display::emacs_core::chrome_dirty;
pub use neovm_display::emacs_core::display;
pub use neovm_display::emacs_core::display_host;
// The display-property decode moved into the eval manifest (stage-4 K2
// display_prop TD); the historical emacs_core path stays as a re-export for
// the display engine and window input code above eval.
pub use crate::emacs_core::eval::data::display_spec;
pub use neovm_display::emacs_core::display_subrs;
pub use neovm_display::emacs_core::display_when;
pub use neovm_display::emacs_core::dispnew;
pub use neovm_display::emacs_core::effect_profile;
pub use neovm_display::emacs_core::font;
pub use neovm_display::emacs_core::fontset;
pub use neovm_display::emacs_core::frame;
pub use neovm_display::emacs_core::frame_vars;
pub use neovm_display::emacs_core::hscroll;
pub use neovm_display::emacs_core::image;
pub use neovm_display::emacs_core::image_catalog;
pub use neovm_display::emacs_core::image_path;
pub use neovm_display::emacs_core::invisibility;
pub(crate) use neovm_display::emacs_core::menu;
pub use neovm_display::emacs_core::neo;
pub use neovm_display::emacs_core::shader_surface;
pub(crate) use neovm_display::emacs_core::sound;
pub use neovm_display::emacs_core::terminal;
pub use neovm_display::emacs_core::video;
pub use neovm_display::emacs_core::window_cmds;
pub use neovm_display::emacs_core::xdisp;
pub use neovm_display::emacs_core::xfaces;
pub use neovm_display::emacs_core::xwidget;
pub use neovm_edit::emacs_core::edit_host;

// Editor state and editing operations.
pub use neovm_edit::emacs_core::bookmark;
pub use neovm_edit::emacs_core::buffer;
pub use neovm_edit::emacs_core::buffer_vars;
pub use neovm_edit::emacs_core::case_encoding;
#[path = "editing/dired/mod.rs"]
pub mod dired;
pub use neovm_edit::emacs_core::display_metrics;
pub use neovm_edit::emacs_core::editfns;
pub(crate) use neovm_edit::emacs_core::file_name;
pub mod indent {
    pub use neovm_edit::emacs_core::indent::*;
    #[path = "subrs.rs"]
    mod subrs;
    #[cfg(test)]
    pub(crate) use subrs::SUBRS;
    pub(crate) use subrs::register_subrs;
}
pub use neovm_edit::emacs_core::marker;
pub use neovm_edit::emacs_core::mode;
pub use neovm_edit::emacs_core::navigation;
pub use neovm_edit::emacs_core::rect;
pub(crate) use neovm_edit::emacs_core::redisplay_policy;
pub use neovm_edit::emacs_core::undo;
pub use neovm_edit::emacs_core::word_wrap;

// Lisp loading, documentation, and language-level facilities.
pub use neovm_eval::emacs_core::advice;
pub use neovm_eval::emacs_core::autoload;
pub use neovm_eval::emacs_core::cl_lib;
pub use neovm_eval::emacs_core::custom;
pub use neovm_eval::emacs_core::doc;
pub(crate) use neovm_eval::emacs_core::hook_runtime;
pub use neovm_eval::emacs_core::load;
pub use neovm_eval::emacs_core::lread;
pub use neovm_eval::emacs_core::print;
pub use neovm_eval::emacs_core::provide_coupled_vars;
pub use neovm_eval::emacs_core::reader;

// Native Lisp subroutines and their metadata.
pub mod builtins {
    pub use neovm_eval::emacs_core::builtins::*;
    // Split-era path restoration: the C14 compatibility manifest and the
    // core-owned tests reach these builtins through the historical
    // `emacs_core::builtins::<name>` path. Their implementations moved to
    // the owning lower crate; re-export them crate-internally (no public
    // surface change).
    pub(crate) use neovm_display::emacs_core::frame::builtin_frame_bottom_divider_width;
    pub(crate) use neovm_display::emacs_core::window_cmds::builtin_window_bottom_divider_width;
    pub(crate) use neovm_display::emacs_core::xfaces::builtin_face_attributes_as_vector;
    pub(crate) use neovm_system::emacs_core::filelock::{builtin_lock_file, builtin_unlock_file};
    pub(crate) use neovm_value::strings_core::{
        lisp_string_char_at, lisp_string_char_codes, plain_str_to_lisp_string,
    };
    #[path = "subrs.rs"]
    mod subrs;
    // The localized builtins batch no longer exists in this compatibility
    // manifest (its declarations register through world_subr!); only the
    // catalog remains for the startup-order test.
    #[cfg(test)]
    pub(crate) use subrs::localized_subr_catalog;
    pub(crate) use subrs::register_subrs as init_builtins;
    pub(crate) use subrs::register_subrs;
}
pub use neovm_eval::emacs_core::builtins_extra;
pub use neovm_eval::emacs_core::floatfns;
pub use neovm_eval::emacs_core::fns;
pub use neovm_eval::emacs_core::misc;
pub(crate) use neovm_eval::emacs_core::subr;
#[cfg(doctest)]
/// Compile-time contracts for native subroutine declarations.
///
/// This facade exists only while rustdoc compiles the examples below. Normal
/// builds keep the declaration machinery crate-private.
///
/// A two-slot constructor cannot accept a one-slot Rust entrypoint:
///
/// ```compile_fail,E0308
/// use neovm_core::{Context, Value};
/// use neovm_core::emacs_core::subr_compile_contract::{FixedMin2, SubrSpec};
/// use neovm_core::emacs_core::error::EvalResult;
///
/// fn one(_ctx: &mut Context, value: Value) -> EvalResult {
///     Ok(value)
/// }
///
/// const BAD: SubrSpec = SubrSpec::fixed2("bad", one, FixedMin2::One);
/// ```
///
/// A minimum for a different fixed-slot width is a different Rust type:
///
/// ```compile_fail,E0308
/// use neovm_core::{Context, Value};
/// use neovm_core::emacs_core::subr_compile_contract::{
///     FixedMin2, FixedMin3, SubrSpec,
/// };
/// use neovm_core::emacs_core::error::EvalResult;
///
/// fn two(_ctx: &mut Context, left: Value, _right: Value) -> EvalResult {
///     Ok(left)
/// }
///
/// const BAD: SubrSpec = SubrSpec::fixed2("bad", two, FixedMin3::Two);
/// ```
///
/// A localized declaration batch cannot be declared outside `subrs.rs`:
///
/// ```compile_fail,E0080
/// use neovm_core::{Context, Value};
/// use neovm_core::emacs_core::subr_compile_contract::{SubrBatch, SubrSpec};
/// use neovm_core::emacs_core::error::EvalResult;
///
/// fn zero(_ctx: &mut Context) -> EvalResult {
///     Ok(Value::NIL)
/// }
///
/// const BAD: SubrBatch = SubrBatch::new(
///     module_path!(),
///     &[SubrSpec::fixed0("bad", zero)],
/// );
/// ```
#[doc(hidden)]
pub mod subr_compile_contract {
    pub use super::subr::{
        FixedMin1, FixedMin2, FixedMin3, FixedMin4, FixedMin5, FixedMin6, SubrBatch, SubrSpec,
    };
}
pub use neovm_eval::emacs_core::subr_info;

// Generated Lisp-facing documentation tables.
pub use neovm_eval::emacs_core::subr_docs;
pub use neovm_eval::emacs_core::var_docs;

// Evaluator runtime, object model, VM, and persistence.
#[cfg(not(test))]
pub use neovm_value::alloc;
#[cfg(test)]
#[path = "../../core_test_facades/emacs_core/alloc.rs"]
pub mod alloc;
pub use neovm_eval::emacs_core::bytecode;
pub mod data {
    // The data batch declarations live beside their implementations in
    // neovm-eval; the glob carries SUBRS/register_subrs to the catalog and
    // to the compatibility sequence.
    pub use neovm_eval::emacs_core::data::*;
}
pub use neovm_eval::emacs_core::debug;
pub use neovm_eval::emacs_core::debug_on_call;
pub use neovm_eval::emacs_core::defvar_bool;
pub use neovm_eval::emacs_core::defvar_object;
#[cfg(not(test))]
pub use neovm_value::error;
#[cfg(test)]
#[path = "../../core_test_facades/emacs_core/error.rs"]
pub mod error;
pub use neovm_eval::emacs_core::eval;
#[cfg(test)]
#[path = "../../core_test_facades/emacs_core/eval.rs"]
mod eval_tests;
pub use neovm_value::errors;
#[cfg(not(test))]
pub use neovm_value::forward;
#[cfg(test)]
#[path = "../../core_test_facades/emacs_core/forward.rs"]
pub mod forward;
pub use neovm_eval::emacs_core::gc_stats;
#[cfg(not(test))]
pub use neovm_value::hashtab;
#[cfg(test)]
#[path = "../../core_test_facades/emacs_core/hashtab.rs"]
pub mod hashtab;
#[cfg(not(test))]
pub use neovm_value::intern;
#[cfg(test)]
#[path = "../../core_test_facades/emacs_core/intern.rs"]
pub mod intern;
pub(crate) use neovm_edit::emacs_core::position;
pub use neovm_eval::emacs_core::jit;
pub use neovm_eval::emacs_core::owned_roots;
pub use neovm_eval::emacs_core::pdump;
pub(crate) use neovm_eval::emacs_core::runtime_identity;
pub use neovm_value::plist;
#[cfg(not(test))]
pub use neovm_value::symbol;
#[cfg(test)]
#[path = "../../core_test_facades/emacs_core/symbol.rs"]
pub mod symbol;
pub use neovm_eval::emacs_core::threads;
#[cfg(not(test))]
pub use neovm_value::value;
#[cfg(test)]
#[path = "../../core_test_facades/emacs_core/value.rs"]
pub mod value;
pub use neovm_eval::emacs_core::value_reader;

// Host integration, files, processes, networking, and platform support.
pub use neovm_system::emacs_core::c_features;
pub use neovm_system::emacs_core::callproc;
pub use neovm_system::emacs_core::cus_start_platform_vars;
pub(crate) use neovm_system::emacs_core::dbusbind;
pub use neovm_system::emacs_core::dynamic_module;
pub(crate) use neovm_system::emacs_core::environment;
/// GNU's errno-to-text boundary ([`errno::emacs_strerror`]).  Not a mirror
/// module -- there is no GNU `src/errno.c`; the function it ports lives in
/// `src/emacs.c`.  See the module docs for why the text cannot be Rust's
/// `io::Error` Display.
pub use neovm_system::emacs_core::errno;
#[path = "eval_hook_installers.rs"]
pub mod eval_hook_installers;
pub use neovm_system::emacs_core::fileio;
pub use neovm_system::emacs_core::filelock;
pub use neovm_system::emacs_core::host_input;
#[path = "load_boot.rs"]
pub mod load_boot;
pub use neovm_system::emacs_core::network;
pub(crate) use neovm_system::emacs_core::os_signal;
pub use neovm_system::emacs_core::path_exec;
pub use neovm_system::emacs_core::perf_trace;
pub(crate) use neovm_system::emacs_core::platform_facts;
pub(crate) use neovm_system::emacs_core::post_image_init;
pub use neovm_system::emacs_core::process;
pub(crate) use neovm_system::emacs_core::shell_file_name;
pub(crate) use neovm_system::emacs_core::sqlite;
pub use neovm_system::emacs_core::timefns;
pub use neovm_system::emacs_core::timer;
pub(crate) use neovm_system::emacs_core::tls;
pub(crate) use neovm_system::emacs_core::w32;
pub(crate) use neovm_system::emacs_core::wait;

// Character representation, coding, syntax, search, and structured text.
#[cfg(not(test))]
pub use neovm_value::boolvec;
#[cfg(test)]
#[path = "../../core_test_facades/emacs_core/boolvec.rs"]
pub mod boolvec;
pub use neovm_text::emacs_core::casefiddle;
pub use neovm_text::emacs_core::casetab;
pub use neovm_text::emacs_core::category;
pub use neovm_text::emacs_core::ccl;
pub use neovm_text::emacs_core::character;
pub use neovm_text::emacs_core::charset;
pub use neovm_text::emacs_core::chartable;
pub use neovm_text::emacs_core::coding;
pub mod composite {
    pub use neovm_text::emacs_core::composite::*;
    #[path = "subrs.rs"]
    mod subrs;
    #[cfg(test)]
    pub(crate) use subrs::SUBRS;
    pub(crate) use subrs::register_subrs;
}
pub use neovm_text::emacs_core::format;
pub use neovm_text::emacs_core::json;
pub use neovm_text::emacs_core::regex;
pub use neovm_text::emacs_core::regex_emacs;
pub use neovm_text::emacs_core::search;
pub(crate) use neovm_text::emacs_core::string_escape;
pub(crate) use neovm_text::emacs_core::substrate_hooks;
pub use neovm_value::emacs_char;
pub(crate) use neovm_value::heap_registry;
// The character<->byte position cache (GNU fns.c string_char_to_byte)
// moved into neovm-value with the stage-3 split (C12); the historical
// `emacs_core::string_pos_cache` paths keep resolving through this facade.
pub use neovm_edit::emacs_core::textprop;
pub use neovm_text::emacs_core::syntax;
pub use neovm_text::emacs_core::text_host;
pub use neovm_text::emacs_core::treesit;
pub use neovm_text::emacs_core::xml;
pub(crate) use neovm_text::emacs_core::zlib;
pub(crate) use neovm_value::string_pos_cache;

// Cross-subsystem and externally-shaped regression suites retain the
// `emacs_core` lexical parent while living beside the subsystem they cover.
#[path = "tests/compat_regressions/mod.rs"]
pub mod compat_regressions;
#[cfg(test)]
#[path = "tests/build_support/compile_main_rule_test.rs"]
mod compile_main_rule_test;
#[cfg(test)]
#[path = "tests/build_support/generated_lisp_test.rs"]
mod generated_lisp_test;
#[cfg(test)]
#[path = "tests/gnu_surface/defvar_special_test.rs"]
mod gnu_defvar_special_test;
#[cfg(test)]
#[path = "tests/gnu_surface/subr_test.rs"]
mod gnu_subr_surface_test;
#[cfg(test)]
#[path = "tests/gnu_thread_live_p_test.rs"]
mod gnu_thread_live_p_test;
#[cfg(test)]
#[path = "tests/gnu_thread_release_point_test.rs"]
mod gnu_thread_release_point_test;
#[cfg(test)]
#[path = "tests/architecture/layout_test.rs"]
mod layout_test;
#[cfg(test)]
#[cfg(test)]
#[path = "tests/moved_system/mod.rs"]
mod moved_system;
#[cfg(test)]
#[path = "tests/architecture/runtime_string_guard_test.rs"]
mod runtime_string_guard_test;

#[cfg(test)]
#[path = "tests/architecture/split_cfg_test_guard_test.rs"]
mod split_cfg_test_guard_test;
#[cfg(test)]
#[path = "tests/architecture/split_shared_guards.rs"]
mod split_shared_guards;
#[cfg(test)]
#[path = "tests/architecture/split_test_support_manifest_test.rs"]
mod split_test_support_manifest_test;
#[cfg(test)]
#[path = "../../core_tests/emacs_core/runtime/symbol/tests/function_regression_test.rs"]
mod symbol_function_regression_test;
#[cfg(test)]
#[cfg(test)]
#[path = "tests/architecture/symbol_id_guard_test.rs"]
mod symbol_id_guard_test;
#[cfg(test)]
#[path = "../../core_tests/emacs_core/runtime/symbol/tests/plist_regression_test.rs"]
mod symbol_plist_regression_test;
#[cfg(test)]
#[path = "../../core_tests/emacs_core/runtime/symbol/tests/redirect_regression_test.rs"]
mod symbol_redirect_regression_test;

#[cfg(test)]
#[path = "tests/moved_eval/mod.rs"]
mod moved_eval_tests;

#[cfg(test)]
#[path = "tests/moved_display/mod.rs"]
mod moved_display_tests;

// Moved down with the evaluator (stage-4 W5/E13); the historical
// `crate::emacs_core::MenuBarPopupAnchor` path stays as a re-export.
pub(crate) use neovm_eval::emacs_core::MenuBarPopupAnchor;

// Re-export the main public API
pub use bytecode::{ByteCodeFunction, Vm as ByteVm};
pub use display_host::{DisplayHost, GraphicalFaceAttribute, SelectionOwner};
pub use edit_host::EditHost;
pub use error::EvalError;
pub use eval::{
    Context, GuiFrameHostRequest, MenuBarRebuildGeneration, PopupMenuEntry, PopupMenuRequest,
};
pub use intern::SymId;
pub use print::{
    format_eval_result, format_eval_result_bytes_with_eval, format_eval_result_with_eval,
    print_value, print_value_bytes, print_value_bytes_with_eval, print_value_with_buffers,
    print_value_with_eval,
};
pub use symbol::Obarray;
pub use text_host::TextHost;
pub use value::{LambdaData, LambdaParams, Value, ValueKind, VecLikeType};

/// Convenience: parse and evaluate source code, returning the last form's value.
pub fn eval_source(input: &str) -> Result<Value, EvalError> {
    let mut evaluator = eval_hook_installers::new_context();
    evaluator.eval_str(input)
}

#[cfg(test)]
#[path = "tests/architecture/transmute_guard_test.rs"]
mod architecture_transmute_guard_test;

#[cfg(test)]
#[path = "tests/architecture/layering_guard_test.rs"]
mod architecture_layering_guard_test;

#[cfg(test)]
#[path = "tests/architecture/moving_set.rs"]
mod architecture_moving_set;

#[cfg(test)]
#[path = "tests/architecture/plain_pub_surface_guard_test.rs"]
mod architecture_plain_pub_surface_guard_test;

#[cfg(test)]
#[path = "tests/architecture/production_source_roots.rs"]
mod architecture_production_source_roots;

#[cfg(test)]
#[path = "tests/architecture/vis_ledger_guard_test.rs"]
mod architecture_vis_ledger_guard_test;

#[cfg(test)]
#[path = "tests/architecture/test_support_surface_guard_test.rs"]
mod architecture_test_support_surface_guard_test;

#[cfg(test)]
#[path = "tests/architecture/upper_subrs_generated_test.rs"]
mod architecture_upper_subrs_generated_test;

#[cfg(test)]
#[path = "tests/architecture/workspace_dependency_features_guard_test.rs"]
mod architecture_workspace_dependency_features_guard_test;

#[cfg(test)]
#[path = "tests/architecture/test_profile_codegen_guard_test.rs"]
mod architecture_test_profile_codegen_guard_test;

#[cfg(test)]
#[path = "tests/architecture/host_cfg_agreement_guard_test.rs"]
mod architecture_host_cfg_agreement_guard_test;

#[cfg(test)]
#[path = "tests/architecture/canonical_registration_trace_test.rs"]
mod architecture_canonical_registration_trace_test;

#[cfg(test)]
#[path = "tests/architecture/host_cfg_elisp_regression_test.rs"]
mod architecture_host_cfg_elisp_regression_test;

#[cfg(test)]
#[path = "tests/architecture/value_ref_guard_test.rs"]
mod architecture_value_ref_guard_test;

#[cfg(test)]
#[path = "tests/architecture/inline_boundary_test.rs"]
mod architecture_inline_boundary_test;

#[cfg(test)]
#[path = "tests/architecture/carrier_tls_class_test.rs"]
mod architecture_carrier_tls_class_test;

#[cfg(all(test, feature = "jit"))]
#[path = "tests/architecture/native_depth_reset_guard_test.rs"]
mod architecture_native_depth_reset_guard_test;

#[cfg(test)]
#[path = "tests/architecture/carrier_tls_reset_probe_test.rs"]
mod architecture_carrier_tls_reset_probe_test;

#[cfg(test)]
#[path = "tests/architecture/thread_local_owner_guard_test.rs"]
mod architecture_thread_local_owner_guard_test;

#[cfg(test)]
#[path = "tests/architecture/host_effect_guard_test.rs"]
mod architecture_host_effect_guard_test;

#[cfg(test)]
#[path = "tests/architecture/unsafe_shim_guard_test.rs"]
mod architecture_unsafe_shim_guard_test;

#[cfg(test)]
#[path = "tests/architecture/debug_assert_side_effect_guard_test.rs"]
mod architecture_debug_assert_side_effect_guard_test;

#[cfg(test)]
#[path = "tests/architecture/leaf_slot_writer_guard_test.rs"]
mod architecture_leaf_slot_writer_guard_test;

#[cfg(test)]
#[path = "tests/architecture/value_display_hook_guard_test.rs"]
mod architecture_value_display_hook_guard_test;

#[cfg(test)]
#[path = "tests/architecture/hook_walker_guard_test.rs"]
mod architecture_hook_walker_guard_test;

#[cfg(test)]
#[path = "tests/architecture/in_flight_registry_guard_test.rs"]
mod architecture_in_flight_registry_guard_test;

#[cfg(test)]
#[path = "tests/architecture/tagged_compile_seam_guard_test.rs"]
mod architecture_tagged_compile_seam_guard_test;

#[cfg(test)]
#[path = "tests/architecture/coherence_upper_impl_ratchet_test.rs"]
mod architecture_coherence_upper_impl_ratchet_test;

#[cfg(test)]
#[path = "tests/architecture/default_value_only_ratchet_test.rs"]
mod architecture_default_value_only_ratchet_test;

#[cfg(test)]
#[path = "tests/architecture/mac_x11_subrs_ratchet_test.rs"]
mod architecture_mac_x11_subrs_ratchet_test;

#[cfg(test)]
#[path = "tests/architecture/plist_head_seam_guard_test.rs"]
mod architecture_plist_head_seam_guard_test;

#[cfg(test)]
#[path = "tests/heap_registry_cross_heap_test.rs"]
mod heap_registry_cross_heap_test;
#[cfg(test)]
#[path = "tests/plist_collection_capture_test.rs"]
mod plist_collection_capture_test;

#[cfg(test)]
#[path = "tests/architecture/aot_abi_tag_history_guard_test.rs"]
mod architecture_aot_abi_tag_history_guard_test;

// Booted-Context tests re-homed from neovm-value (TEST-SUPPORT-DESIGN 1a,
// lead ruling 01:1xZ option A). Bodies/test names/assertions unchanged.
#[cfg(test)]
#[path = "tests/moved_value/mod.rs"]
mod moved_value;

// The string-position cache's regression children stayed in core with the
// stage-3 C12 move: they drive the evaluator's heap and activation machinery
// (Context, TaggedHeap, tls_scope), which the value layer cannot name.

#[cfg(test)]
#[path = "tests/architecture/parallel_exec_substrate_test.rs"]
mod architecture_parallel_exec_substrate_test;

#[cfg(test)]
#[path = "tests/architecture/subr_storage_boundary_test.rs"]
mod architecture_subr_storage_boundary_test;

#[cfg(test)]
#[path = "tests/architecture/promotion_ledger_guard_test.rs"]
mod architecture_promotion_ledger_guard_test;

#[cfg(test)]
#[path = "tests/moved_text/mod.rs"]
mod moved_text;

#[cfg(test)]
#[path = "tests/moved_edit/mod.rs"]
mod moved_edit;
