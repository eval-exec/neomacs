//! Tests re-homed from neovm-eval under the post-split option A ruling
//! (TEST-SUPPORT-DESIGN.md rev 2, lead 01:1xZ).
//!
//! File names, test names and assertions are byte-identical to the origin;
//! only `use` paths changed (public crate paths plus the documented
//! API-drift shims). Origin crate: neovm-eval.

// The load oracle was a child of eval's load module before the re-home, so
// its `super::` paths named load items. Keep those paths resolving here
// without rewriting the test bodies: this module is the re-home's `super`.
#[cfg(test)]
pub use crate::emacs_core::load::{
    LoadSuffixRequirement, default_load_suffixes, default_load_suffixes_for_os,
    exec_path_dirs_from_env, exec_path_dirs_from_os, find_file_in_load_path_with_requirement,
    load_file, skip_elc_header,
};

#[cfg(test)]
#[path = "emacs_core/lisp/advice/tests/advice_test.rs"]
mod emacs_core_lisp_advice_tests_advice_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/custom/tests/custom_test.rs"]
mod emacs_core_lisp_custom_tests_custom_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/doc/tests/doc_test.rs"]
mod emacs_core_lisp_doc_tests_doc_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/lread/tests/lread_test.rs"]
mod emacs_core_lisp_lread_tests_lread_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/provide_coupled_vars/tests/runtime_surface_test.rs"]
mod emacs_core_lisp_provide_coupled_vars_tests_runtime_surface_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/quit_regression_test.rs"]
mod emacs_core_runtime_eval_tests_quit_regression_test_rs;

// Option-A re-home of eval's main test file (P1 eval-scope ruling 16:5xZ:
// the file needs core/display/system items, so it re-homes byte-identically
// except imports). Eval loses these 836 tests; core gains them.
#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/eval_test.rs"]
mod emacs_core_runtime_eval_tests_eval_test_rs;

#[cfg(test)]
#[path = "emacs_core/lisp/load/tests/stale_bytecode_test.rs"]
mod emacs_core_lisp_load_tests_stale_bytecode_test_rs;

#[cfg(test)]
#[path = "emacs_core/lisp/load/tests/load_test.rs"]
mod emacs_core_lisp_load_tests_load_test_rs;

#[path = "emacs_core/lisp/autoload/tests/autoload_test.rs"]
mod emacs_core_lisp_autoload_tests_autoload_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/reader/tests/reader_test.rs"]
mod emacs_core_lisp_reader_tests_reader_test_rs;
#[path = "emacs_core/runtime/eval/tests/redisplay_mini_preparer_context_test.rs"]
mod emacs_core_runtime_eval_tests_redisplay_mini_preparer_context_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/redisplay_mini_only_test.rs"]
mod emacs_core_runtime_eval_tests_redisplay_mini_only_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/redisplay_hooks/tests/mode_line_flow_test.rs"]
mod emacs_core_runtime_eval_redisplay_hooks_tests_mode_line_flow_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/builtin_vars_test.rs"]
mod emacs_core_runtime_eval_tests_builtin_vars_test_rs;

// Per-host platform tables (MACOS_PRODUCT_DIVERGENCES rows 87-99): the
// DEFVAR_BOOL surface is keyed by the GNU reference build; the signed fence
// installs a scoped surface override for both-host assertions.
#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/platform_surface_tests.rs"]
mod emacs_core_runtime_eval_tests_platform_surface_tests_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/attention_test.rs"]
mod emacs_core_runtime_eval_tests_attention_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/coherence_hooks_test.rs"]
mod emacs_core_runtime_eval_tests_coherence_hooks_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/gc_tls_migration_proofs_test.rs"]
mod emacs_core_runtime_eval_tests_gc_tls_migration_proofs_test_rs;

#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/builtins_test.rs"]
mod emacs_core_lisp_native_builtins_tests_builtins_test_rs;

#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/random_seed_test.rs"]
mod emacs_core_lisp_native_builtins_tests_random_seed_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/keyboard/tests/help_echo_substitution.rs"]
mod emacs_core_runtime_eval_keyboard_tests_help_echo_substitution_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/keyboard/tests/gnu_help_echo_direct_oracle.rs"]
mod emacs_core_runtime_eval_keyboard_tests_gnu_help_echo_direct_oracle_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/keyboard/tests/gnu_read_timer_redisplay_oracle.rs"]
mod emacs_core_runtime_eval_keyboard_tests_gnu_read_timer_redisplay_oracle_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/keyboard/tests/gnu_timed_read_redisplay_oracle.rs"]
mod emacs_core_runtime_eval_keyboard_tests_gnu_timed_read_redisplay_oracle_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/chrome_transitions_test.rs"]
mod emacs_core_runtime_eval_tests_chrome_transitions_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/keyboard/tests/extended_keysyms_test.rs"]
mod emacs_core_runtime_eval_keyboard_tests_extended_keysyms_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/keyboard/tests/frame_focus_input_test.rs"]
mod emacs_core_runtime_eval_keyboard_tests_frame_focus_input_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/jit_crash_repro_test.rs"]
mod emacs_core_runtime_eval_tests_jit_crash_repro_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/rows_observation_test.rs"]
mod emacs_core_runtime_eval_tests_rows_observation_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/sort_stack_boundary_test.rs"]
mod emacs_core_runtime_eval_tests_sort_stack_boundary_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/bytecode/tests/primitive_error_backtrace_test.rs"]
mod emacs_core_runtime_bytecode_tests_primitive_error_backtrace_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/eval/keyboard/tests/keyboard_test.rs"]
mod emacs_core_runtime_eval_keyboard_tests_keyboard_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/idle_redisplay_test.rs"]
mod emacs_core_runtime_eval_tests_idle_redisplay_test_rs;

// Tier-I Context suites re-homed byte-identically except API drift (the
// private tier_i state is reached through the signed eval test-support
// fences; the context-free parser tests stay in eval).
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/gd_e_sort.rs"]
mod emacs_core_lisp_native_builtins_tests_gd_e_sort_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/eval/tier_i/tests/analyzer_test.rs"]
mod emacs_core_runtime_eval_tier_i_tests_analyzer_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/eval/tier_i/tests/census_test.rs"]
mod emacs_core_runtime_eval_tier_i_tests_census_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/eval/tier_i/tests/differential_test.rs"]
mod emacs_core_runtime_eval_tier_i_tests_differential_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/eval/tier_i/tests/knob_fresh_context_test.rs"]
mod emacs_core_runtime_eval_tier_i_tests_knob_fresh_context_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/eval/tier_i/tests/soak_test.rs"]
mod emacs_core_runtime_eval_tier_i_tests_soak_test_rs;

#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/gd_e_sort_stack_temporary.rs"]
mod emacs_core_lisp_native_builtins_tests_gd_e_sort_stack_temporary_rs;

#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/gdh_format_width_scan_test.rs"]
mod emacs_core_lisp_native_builtins_tests_gdh_format_width_scan_test_rs;

#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/gd_e_sort_native_storage.rs"]
mod emacs_core_lisp_native_builtins_tests_gd_e_sort_native_storage_rs;

#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/format_float_gnu_test.rs"]
mod emacs_core_lisp_native_builtins_tests_format_float_gnu_test_rs;

#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/keymaps_full_tests.rs"]
mod emacs_core_lisp_native_builtins_tests_keymaps_full_tests_rs;
#[cfg(all(test, feature = "jit"))]
#[path = "emacs_core/runtime/jit/tests/car_cdr_split_full_test.rs"]
mod emacs_core_runtime_jit_tests_car_cdr_split_full_test_rs;
#[cfg(all(test, feature = "jit"))]
#[path = "emacs_core/runtime/jit/tests/hof_credit_test.rs"]
mod emacs_core_runtime_jit_tests_hof_credit_test_rs;
#[cfg(all(test, feature = "jit"))]
#[path = "emacs_core/runtime/jit/tests/switch_dispatch_test.rs"]
mod emacs_core_runtime_jit_tests_switch_dispatch_test_rs;

#[path = "emacs_core/runtime/eval/tests/cconv_memo_test.rs"]
mod emacs_core_runtime_eval_tests_cconv_memo_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/eval/tests/gc_forced_first_cycle_test.rs"]
mod emacs_core_runtime_eval_tests_gc_forced_first_cycle_test_rs;
#[cfg(all(test, feature = "jit"))]
#[path = "emacs_core/runtime/jit/tests/spec_frames_test.rs"]
mod emacs_core_runtime_jit_tests_spec_frames_test_rs;
#[cfg(all(test, feature = "jit"))]
#[path = "emacs_core/runtime/jit/tests/spec_rest_calls_test.rs"]
mod emacs_core_runtime_jit_tests_spec_rest_calls_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/pdump/tests/bool_vectors_test.rs"]
mod emacs_core_runtime_pdump_tests_bool_vectors_test_rs;
#[path = "emacs_core/runtime/pdump/tests/bytecode_function_params_full_test.rs"]
mod emacs_core_runtime_pdump_tests_bytecode_function_params_full_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/pdump/tests/bytecode_slot_objects_test.rs"]
mod emacs_core_runtime_pdump_tests_bytecode_slot_objects_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/pdump/tests/pdump_first_cycle_full_test.rs"]
mod emacs_core_runtime_pdump_tests_pdump_first_cycle_full_test_rs;

#[cfg(test)]
#[path = "emacs_core/runtime/bytecode/tests/resumed_chain_test.rs"]
mod emacs_core_runtime_bytecode_tests_resumed_chain_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/pdump/tests/pdump_bytecode_image_full_test.rs"]
mod emacs_core_runtime_pdump_tests_pdump_bytecode_image_full_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/pdump/tests/pdump_mmap_image_readers_full_test.rs"]
mod emacs_core_runtime_pdump_tests_pdump_mmap_image_readers_full_test_rs;
#[cfg(test)]
#[path = "emacs_core/runtime/pdump/tests/pdump_section_inventory_full_test.rs"]
mod emacs_core_runtime_pdump_tests_pdump_section_inventory_full_test_rs;

// Builtins/printer boot-oracle re-homes of the P1 EVAL wave F1 batch 3.
// File names, test names and assertions byte-identical; imports and the
// UPDATE_EXPECT fixture paths moved.
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/gdl_format_test.rs"]
mod emacs_core_lisp_native_builtins_tests_gdl_format_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/lisp_only_misc_names_test.rs"]
mod emacs_core_lisp_native_builtins_tests_lisp_only_misc_names_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/lisp_only_predicates_and_aliases_test.rs"]
mod emacs_core_lisp_native_builtins_tests_lisp_only_predicates_and_aliases_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/lisp_only_undo_commands_test.rs"]
mod emacs_core_lisp_native_builtins_tests_lisp_only_undo_commands_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/lisp_only_window_frame_names_test.rs"]
mod emacs_core_lisp_native_builtins_tests_lisp_only_window_frame_names_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/process_launchers_are_lisp_only_test.rs"]
mod emacs_core_lisp_native_builtins_tests_process_launchers_are_lisp_only_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/rust_subrs_shadowed_by_lisp_test.rs"]
mod emacs_core_lisp_native_builtins_tests_rust_subrs_shadowed_by_lisp_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/search_frontend_test.rs"]
mod emacs_core_lisp_native_builtins_tests_search_frontend_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/width_policy.rs"]
mod emacs_core_lisp_native_builtins_tests_width_policy_rs;
// P1 EVAL wave F1 (peer class ruling 12:57Z (a)/(b)): the sort root-domain
// oracle and the captured-predicate resolver oracle re-homed from eval.
// Pre-split gates at c957557a40: sort.rs:12-14 `#[cfg(test)]` and
// builtins/mod.rs:710-712 `#[cfg(test)]`, so both are plain cfg(test) here
// (both ran in every pre-split test build, no-default included).
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/gd_e_sort_root_domains_test.rs"]
mod emacs_core_lisp_native_builtins_tests_gd_e_sort_root_domains_test_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/tests/sort_capture_test.rs"]
mod emacs_core_lisp_native_builtins_tests_sort_capture_test_rs;
// P1 EVAL wave F1: debug-on-call arming oracle re-homed from eval
// (booted Context through core test_utils; body byte-identical apart from
// imports and the unit-runtime constructor).
#[cfg(test)]
#[path = "emacs_core/runtime/debug_on_call/tests/debug_on_call_test.rs"]
mod emacs_core_runtime_debug_on_call_tests_debug_on_call_test_rs;

// Printer tests re-homed from neovm-eval (P1 EVAL wave F1): they construct a
// booted Context through this crate's runtime_startup_context / probe
// helpers.  File names, test names and assertions are byte-identical to the
// origin; only the use paths and the two UPDATE_EXPECT fixture paths moved.
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/graph_closure_units.rs"]
mod emacs_core_lisp_print_tests_graph_closure_units_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/graph_gnu.rs"]
mod emacs_core_lisp_print_tests_graph_gnu_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/graph_level_units.rs"]
mod emacs_core_lisp_print_tests_graph_level_units_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/graph_recursive_closure_units.rs"]
mod emacs_core_lisp_print_tests_graph_recursive_closure_units_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/graph_tail_units.rs"]
mod emacs_core_lisp_print_tests_graph_tail_units_rs;

// Remaining printer re-homes of the same P1 EVAL wave F1 batch (boot/oracle
// helpers live in this crate).  Bodies byte-identical; only use paths and
// fixture paths changed.
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/compiled_settings_gnu.rs"]
mod emacs_core_lisp_print_tests_compiled_settings_gnu_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/diagnostic_symbols.rs"]
mod emacs_core_lisp_print_tests_diagnostic_symbols_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/error_string_leaf_gnu.rs"]
mod emacs_core_lisp_print_tests_error_string_leaf_gnu_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/float_format_units.rs"]
mod emacs_core_lisp_print_tests_float_format_units_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/float_output_gnu.rs"]
mod emacs_core_lisp_print_tests_float_output_gnu_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/princ_octal_spans_gnu.rs"]
mod emacs_core_lisp_print_tests_princ_octal_spans_gnu_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/raw_hash_test_names.rs"]
mod emacs_core_lisp_print_tests_raw_hash_test_names_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/settings_gnu.rs"]
mod emacs_core_lisp_print_tests_settings_gnu_rs;
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/symbol_noescape_gnu.rs"]
mod emacs_core_lisp_print_tests_symbol_noescape_gnu_rs;
// Core-authored regression test (memq oracle-divergence fix): the evaluator
// frame handle prints the frame's NAME, not an ordinal derived from FrameId.
// It lives here because this crate's test tree reaches both the live frame
// registry and the Lisp `format` builtin.
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/frame_name_print_units.rs"]
mod emacs_core_lisp_print_tests_frame_name_print_units_rs;
// Core-authored regression pin (print_buffer F1): a stale `Copy` prepare
// token reused after its recorded unwind must panic loudly, not drop output
// silently in release. Calls the signed eval test-support authority fence.
#[cfg(test)]
#[path = "emacs_core/lisp/print/tests/print_accumulator_units.rs"]
mod emacs_core_lisp_print_tests_print_accumulator_units_rs;
// P1 EVAL wave F1: file_notify Linux driver oracle re-homed from eval
// (booted Context through core test_utils; driver calls re-pointed to the
// inotify-* Lisp subrs, TLS reset to the eval authority fence).  The
// pre-split module gate was cfg(all(test, target_os = "linux")); preserved.
#[cfg(all(test, target_os = "linux"))]
#[path = "emacs_core/lisp/native/builtins/file_notify/tests/linux_test.rs"]
mod emacs_core_lisp_native_builtins_file_notify_tests_linux_test_rs;
// P1 EVAL wave F1: file_notify fixture/event-queue oracle re-homed from
// eval (booted Context through core test_utils; drivers through the Lisp
// subrs, event injection and the thread-local registry through the eval
// authority fences).  The pre-split module gate was cfg(test); preserved.
#[cfg(test)]
#[path = "emacs_core/lisp/native/builtins/file_notify/tests/file_notify_test.rs"]
mod emacs_core_lisp_native_builtins_file_notify_tests_file_notify_test_rs;
// P1 EVAL wave F1: the two inotify evaluator-encoding oracles re-homed from
// eval's platform/linux tests (the event fixtures are forged through the
// eval authority fence; the remaining pure backend tests stay in eval).
// The pre-split gate was cfg(test) inside the linux-only platform module,
// so this mirror is Linux-only.
#[cfg(all(test, target_os = "linux"))]
#[path = "emacs_core/lisp/native/builtins/file_notify/platform/linux/tests/event_lisp_test.rs"]
mod emacs_core_lisp_native_builtins_file_notify_platform_linux_tests_event_lisp_test_rs;
