//! Round 3w (plan-3w.md section 3): Context is `repr(C)` in the field order
//! rustc chose for main at e5fd507ac9, so moving GNU-global state into the
//! World moves no field. This pins every offset; a failure means a field
//! changed size or was added out of place, which shifts the hot set and the
//! JIT's Context ABI.

use super::Context;
use crate::emacs_core::eval::data::process_state::NativeConnectObservationSlot;
use neovm_text::emacs_core::regex::MatchData;
use std::mem::{offset_of, size_of};

#[test]
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
fn every_context_offset_is_mains() {
    // The release layout is the measured, shipped one. MatchData carries the
    // only debug/release difference: `read_mask` (`#[cfg(debug_assertions)]
    // read_mask: Cell<u64>`, text/regex/mod.rs:807, consumed by
    // `match_stats::count_first_some_read` at :1541/:1568). Context embeds
    // MatchData inline at `match_data` (offset 2752); MatchData is 232 bytes in
    // debug and 224 in release, so every later field moves by that word in a
    // debug build. Pinning the size of MatchData makes a future layout change
    // fail here instead of being absorbed by the offset adjustment.
    #[cfg(debug_assertions)]
    assert_eq!(size_of::<MatchData>(), 232, "MatchData size (debug)");
    #[cfg(not(debug_assertions))]
    assert_eq!(size_of::<MatchData>(), 224, "MatchData size (release)");
    // Native-connect observation adds a separately typed, cfg(test)-only slot
    // inside ProcessManager. Its owner pins the slot size at compile time, so
    // a future change cannot be absorbed into the production layout contract.
    let observation_extra = size_of::<NativeConnectObservationSlot>();
    #[cfg(debug_assertions)]
    assert_eq!(
        size_of::<Context>(),
        44632 + observation_extra,
        "Context size (debug with test observer)"
    );
    #[cfg(not(debug_assertions))]
    assert_eq!(
        size_of::<Context>(),
        44624 + observation_extra,
        "Context size (release with test observer)"
    );
    let table: &[(usize, usize, &str)] = &[
        (
            2312,
            offset_of!(Context, edit.combine_after_change_buffer),
            "combine_after_change_buffer",
        ),
        (
            0,
            offset_of!(Context, selected_global_map),
            "selected_global_map",
        ),
        (16, offset_of!(Context, current_message), "current_message"),
        (
            64,
            offset_of!(Context, echo_area_buffers),
            "echo_area_buffers",
        ),
        (
            96,
            offset_of!(Context, minibuffer_selected_window),
            "minibuffer_selected_window",
        ),
        (
            112,
            offset_of!(Context, active_minibuffer_window),
            "active_minibuffer_window",
        ),
        (128, offset_of!(Context, threads), "threads"),
        (464, offset_of!(Context, command_loop), "command_loop"),
        (1616, offset_of!(Context, scroll_goal), "scroll_goal"),
        (
            1664,
            offset_of!(Context, materialized_face_table_source),
            "materialized_face_table_source",
        ),
        (
            1688,
            offset_of!(Context, gnu_redisplay_hooks),
            "gnu_redisplay_hooks",
        ),
        (
            2144,
            offset_of!(Context, gc_runtime_settings_cache),
            "gc_runtime_settings_cache",
        ),
        (
            2192,
            offset_of!(Context, interpreted_closure_filter_fn),
            "interpreted_closure_filter_fn",
        ),
        (
            2208,
            offset_of!(Context, in_flight_registry),
            "in_flight_registry",
        ),
        (
            2232,
            offset_of!(Context, cached_standard_case_table),
            "cached_standard_case_table",
        ),
        (2248, offset_of!(Context, edit.buffers), "buffers"),
        (2248, offset_of!(Context, edit), "edit"),
        (2248, offset_of!(Context, edit.buffers), "edit.buffers"),
        (
            2312,
            offset_of!(Context, edit.combine_after_change_buffer),
            "edit.combine_after_change_buffer",
        ),
        (
            2328,
            offset_of!(Context, edit.combine_after_change_list),
            "edit.combine_after_change_list",
        ),
        (
            2352,
            offset_of!(Context, edit.last_overlay_modification_hooks),
            "edit.last_overlay_modification_hooks",
        ),
        (
            2376,
            offset_of!(Context, edit.interval_insert_behind_hooks),
            "edit.interval_insert_behind_hooks",
        ),
        (
            2384,
            offset_of!(Context, edit.interval_insert_in_front_hooks),
            "edit.interval_insert_in_front_hooks",
        ),
        (
            2392,
            offset_of!(Context, window_layout_query_adapter),
            "window_layout_query_adapter",
        ),
        (
            2416,
            offset_of!(Context, pending_menu_bar_popup_anchor),
            "pending_menu_bar_popup_anchor",
        ),
        (
            2504,
            offset_of!(Context, code_conversion_workspace),
            "code_conversion_workspace",
        ),
        (
            2520,
            offset_of!(Context, last_redisplay_signature),
            "last_redisplay_signature",
        ),
        (2816, offset_of!(Context, match_data), "match_data"),
        (3040, offset_of!(Context, input_rx), "input_rx"),
        (3056, offset_of!(Context, eval_task_rx), "eval_task_rx"),
        (3072, offset_of!(Context, specpdl), "specpdl"),
        (
            3096,
            offset_of!(Context, suspended_thread_bindings),
            "suspended_thread_bindings",
        ),
        (3120, offset_of!(Context, require_stack), "require_stack"),
        (
            3144,
            offset_of!(Context, loads_in_progress),
            "loads_in_progress",
        ),
        (
            3168,
            offset_of!(Context, load_read_cursors),
            "load_read_cursors",
        ),
        (
            2352,
            offset_of!(Context, edit.last_overlay_modification_hooks),
            "last_overlay_modification_hooks",
        ),
        (
            2328,
            offset_of!(Context, edit.combine_after_change_list),
            "combine_after_change_list",
        ),
        (3192, offset_of!(Context, bookmarks), "bookmarks"),
        (3272, offset_of!(Context, rectangle), "rectangle"),
        (3296, offset_of!(Context, interactive), "interactive"),
        (3568, offset_of!(Context, minibuffers), "minibuffers"),
        (3648, offset_of!(Context, modes), "modes"),
        (3992, offset_of!(Context, kmacro), "kmacro"),
        (4064, offset_of!(Context, visual_config), "visual_config"),
        (8208, offset_of!(Context, vm_root_frames), "vm_root_frames"),
        (
            8232,
            offset_of!(Context, backtrace_args_stack),
            "backtrace_args_stack",
        ),
        (
            8256,
            offset_of!(Context, eval_temp_roots),
            "eval_temp_roots",
        ),
        (
            8280,
            offset_of!(Context, eval_call_roots),
            "eval_call_roots",
        ),
        (
            8304,
            offset_of!(Context, sequence_temp_root_frames),
            "sequence_temp_root_frames",
        ),
        (8328, offset_of!(Context, bc_buf), "bc_buf"),
        (8352, offset_of!(Context, jit_root_stack), "jit_root_stack"),
        (8376, offset_of!(Context, bc_frames), "bc_frames"),
        (
            8400,
            offset_of!(Context, condition_stack),
            "condition_stack",
        ),
        (
            8424,
            offset_of!(Context, pending_safe_funcalls),
            "pending_safe_funcalls",
        ),
        (
            8448,
            offset_of!(Context, interpreter_stacks),
            "interpreter_stacks",
        ),
        (8472, offset_of!(Context, jit_bind_stack), "jit_bind_stack"),
        (8496, offset_of!(Context, cconv_memo), "cconv_memo"),
        (8896, offset_of!(Context, tier_i), "tier_i"),
        (
            9080,
            offset_of!(Context, last_uncaught_signal_backtrace),
            "last_uncaught_signal_backtrace",
        ),
        (9104, offset_of!(Context, daemon), "daemon"),
        (9152, offset_of!(Context, processes), "processes"),
        (9352, offset_of!(Context, abbrevs), "abbrevs"),
        (9408, offset_of!(Context, profiler), "profiler"),
        (
            9576,
            offset_of!(Context, shutdown_request),
            "shutdown_request",
        ),
        (
            9584,
            offset_of!(Context, window_edge_drag),
            "window_edge_drag",
        ),
        (
            9600,
            offset_of!(Context, integer_width_context),
            "integer_width_context",
        ),
        (9608, offset_of!(Context, owned_roots), "owned_roots"),
        (9624, offset_of!(Context, tagged_heap), "tagged_heap"),
        (9640, offset_of!(Context, obarray), "obarray"),
        (
            9680,
            offset_of!(Context, runtime_projection_mask),
            "runtime_projection_mask",
        ),
        (9696, offset_of!(Context, features), "features"),
        (9712, offset_of!(Context, xwidgets), "xwidgets"),
        (9840, offset_of!(Context, watchers), "watchers"),
        (
            9856,
            offset_of!(Context, active_variable_watchers),
            "active_variable_watchers",
        ),
        (9872, offset_of!(Context, registers), "registers"),
        (9920, offset_of!(Context, autoloads), "autoloads"),
        (9936, offset_of!(Context, treesit), "treesit"),
        (9952, offset_of!(Context, frames), "frames"),
        (9968, offset_of!(Context, quit_requested), "quit_requested"),
        (
            9976,
            offset_of!(Context, gstring_shape_cache),
            "gstring_shape_cache",
        ),
        (10024, offset_of!(Context, coding_systems), "coding_systems"),
        (10040, offset_of!(Context, face_table), "face_table"),
        (10064, offset_of!(Context, input_progress), "input_progress"),
        (
            10096,
            offset_of!(Context, body_redisplay_by_window),
            "body_redisplay_by_window",
        ),
        (
            10128,
            offset_of!(Context, body_redisplay_by_buffer),
            "body_redisplay_by_buffer",
        ),
        (10160, offset_of!(Context, chrome_dirty), "chrome_dirty"),
        (
            10264,
            offset_of!(Context, symbol_bytecode_call_cache),
            "symbol_bytecode_call_cache",
        ),
        (
            10272,
            offset_of!(Context, named_call_cache),
            "named_call_cache",
        ),
        (
            10304,
            offset_of!(Context, macro_perf_stats),
            "macro_perf_stats",
        ),
        (10320, offset_of!(Context, fringe_bitmaps), "fringe_bitmaps"),
        (10392, offset_of!(Context, ccl_registry), "ccl_registry"),
        (
            10408,
            offset_of!(Context, charset_registry),
            "charset_registry",
        ),
        (10424, offset_of!(Context, font_registry), "font_registry"),
        (
            10440,
            offset_of!(Context, terminal_registry),
            "terminal_registry",
        ),
        (
            10456,
            offset_of!(Context, hash_table_test_registry),
            "hash_table_test_registry",
        ),
        (
            10472,
            offset_of!(Context, file_notify_registry),
            "file_notify_registry",
        ),
        (
            10488,
            offset_of!(Context, window_configuration_registry),
            "window_configuration_registry",
        ),
        (
            10504,
            offset_of!(Context, dynamic_module_registry),
            "dynamic_module_registry",
        ),
        (10520, offset_of!(Context, pdump_image), "pdump_image"),
        (
            10528,
            offset_of!(Context, cached_system_name),
            "cached_system_name",
        ),
        (10536, offset_of!(Context, lexenv), "lexenv"),
        (10544, offset_of!(Context, quit_flag), "quit_flag"),
        (10552, offset_of!(Context, inhibit_quit), "inhibit_quit"),
        (10560, offset_of!(Context, throw_on_input), "throw_on_input"),
        (
            10568,
            offset_of!(Context, unwind_cleanup_depth),
            "unwind_cleanup_depth",
        ),
        (
            2376,
            offset_of!(Context, edit.interval_insert_behind_hooks),
            "interval_insert_behind_hooks",
        ),
        (
            2384,
            offset_of!(Context, edit.interval_insert_in_front_hooks),
            "interval_insert_in_front_hooks",
        ),
        (
            10576,
            offset_of!(Context, standard_syntax_table),
            "standard_syntax_table",
        ),
        (
            10584,
            offset_of!(Context, syntax_code_objects),
            "syntax_code_objects",
        ),
        (
            10592,
            offset_of!(Context, standard_category_table),
            "standard_category_table",
        ),
        (
            10600,
            offset_of!(Context, current_local_map),
            "current_local_map",
        ),
        (
            10608,
            offset_of!(Context, interactive_minibuffer_read_count),
            "interactive_minibuffer_read_count",
        ),
        (10616, offset_of!(Context, quit_char), "quit_char"),
        (10624, offset_of!(Context, redisplay_fn), "redisplay_fn"),
        (
            10640,
            offset_of!(Context, mode_line_display_flow),
            "mode_line_display_flow",
        ),
        (
            10648,
            offset_of!(Context, redisplay_prepare_fn),
            "redisplay_prepare_fn",
        ),
        (10664, offset_of!(Context, font_shape_fn), "font_shape_fn"),
        (
            10680,
            offset_of!(Context, scroll_preview_fn),
            "scroll_preview_fn",
        ),
        (
            10696,
            offset_of!(Context, display_idle_maintenance_fn),
            "display_idle_maintenance_fn",
        ),
        (
            10712,
            offset_of!(Context, frame_snapshot_fn),
            "frame_snapshot_fn",
        ),
        (10728, offset_of!(Context, display_host), "display_host"),
        (
            10744,
            offset_of!(Context, tty_frame_host_factory),
            "tty_frame_host_factory",
        ),
        (
            10760,
            offset_of!(Context, face_change_count),
            "face_change_count",
        ),
        (
            10768,
            offset_of!(Context, display_var_change_count),
            "display_var_change_count",
        ),
        (
            10776,
            offset_of!(Context, redisplay_generation),
            "redisplay_generation",
        ),
        (
            10784,
            offset_of!(Context, body_redisplay_all),
            "body_redisplay_all",
        ),
        (
            10792,
            offset_of!(Context, menu_bar_rebuild_generation),
            "menu_bar_rebuild_generation",
        ),
        (
            10800,
            offset_of!(Context, context_instance_id),
            "context_instance_id",
        ),
        (
            10808,
            offset_of!(Context, media_generation),
            "media_generation",
        ),
        (10816, offset_of!(Context, depth), "depth"),
        (10824, offset_of!(Context, eval_counter), "eval_counter"),
        (10832, offset_of!(Context, max_depth), "max_depth"),
        (10840, offset_of!(Context, gc_count), "gc_count"),
        (
            10848,
            offset_of!(Context, last_activation_thread),
            "last_activation_thread",
        ),
        (
            10856,
            offset_of!(Context, gc_inhibit_depth),
            "gc_inhibit_depth",
        ),
        (
            10864,
            offset_of!(Context, jit_root_stack_ptr),
            "jit_root_stack_ptr",
        ),
        (
            10872,
            offset_of!(Context, jit_root_stack_top),
            "jit_root_stack_top",
        ),
        (
            10880,
            offset_of!(Context, jit_root_stack_cap),
            "jit_root_stack_cap",
        ),
        (
            10888,
            offset_of!(Context, jit_stack_limit),
            "jit_stack_limit",
        ),
        (
            10896,
            offset_of!(Context, jit_stack_scratch),
            "jit_stack_scratch",
        ),
        (10952, offset_of!(Context, next_resume_id), "next_resume_id"),
        (10960, offset_of!(Context, owner), "owner"),
        (
            10968,
            offset_of!(Context, apply_fast_path_epoch),
            "apply_fast_path_epoch",
        ),
        (
            10976,
            offset_of!(Context, form_head_cache),
            "form_head_cache",
        ),
        (
            43744,
            offset_of!(Context, lexenv_assq_cache),
            "lexenv_assq_cache",
        ),
        (
            44256,
            offset_of!(Context, lexenv_special_cache),
            "lexenv_special_cache",
        ),
        (
            44512,
            offset_of!(Context, macro_expansion_scope_depth),
            "macro_expansion_scope_depth",
        ),
        (
            44520,
            offset_of!(Context, macro_expansion_mutation_epoch),
            "macro_expansion_mutation_epoch",
        ),
        (
            44528,
            offset_of!(Context, macro_expand_calls),
            "macro_expand_calls",
        ),
        (
            44536,
            offset_of!(Context, macro_expand_total_us),
            "macro_expand_total_us",
        ),
        (
            44544,
            offset_of!(Context, internal_interpreter_environment_symbol),
            "internal_interpreter_environment_symbol",
        ),
        (
            44548,
            offset_of!(Context, quit_flag_symbol),
            "quit_flag_symbol",
        ),
        (
            44552,
            offset_of!(Context, inhibit_quit_symbol),
            "inhibit_quit_symbol",
        ),
        (
            44556,
            offset_of!(Context, throw_on_input_symbol),
            "throw_on_input_symbol",
        ),
        (
            44560,
            offset_of!(Context, kill_emacs_symbol),
            "kill_emacs_symbol",
        ),
        (44564, offset_of!(Context, attention), "attention"),
        (
            44568,
            offset_of!(Context, noninteractive_symbol),
            "noninteractive_symbol",
        ),
        (
            44572,
            offset_of!(Context, symbols_with_pos_enabled_symbol),
            "symbols_with_pos_enabled_symbol",
        ),
        (
            44576,
            offset_of!(Context, print_symbols_bare_symbol),
            "print_symbols_bare_symbol",
        ),
        (
            44580,
            offset_of!(Context, load_read_stream_token),
            "load_read_stream_token",
        ),
        (
            44584,
            offset_of!(Context, _reserved_debugging_output_file),
            "_reserved_debugging_output_file",
        ),
        (
            44588,
            offset_of!(Context, compiler_function_overrides_symbol),
            "compiler_function_overrides_symbol",
        ),
        (
            44592,
            offset_of!(Context, after_pdump_load_hook_pending),
            "after_pdump_load_hook_pending",
        ),
        (44593, offset_of!(Context, noninteractive), "noninteractive"),
        (
            44594,
            offset_of!(Context, symbols_with_pos_enabled),
            "symbols_with_pos_enabled",
        ),
        (
            44595,
            offset_of!(Context, print_symbols_bare),
            "print_symbols_bare",
        ),
        (
            44596,
            offset_of!(Context, _reserved_message_log_need_newline),
            "_reserved_message_log_need_newline",
        ),
        (
            44597,
            offset_of!(Context, echo_area_resize_exact_pending),
            "echo_area_resize_exact_pending",
        ),
        (
            44598,
            offset_of!(Context, _reserved_message_buf_print),
            "_reserved_message_buf_print",
        ),
        (44599, offset_of!(Context, shutdown_phase), "shutdown_phase"),
        (
            44600,
            offset_of!(Context, _reserved_input_mode_interrupt),
            "_reserved_input_mode_interrupt",
        ),
        (
            44601,
            offset_of!(Context, waiting_for_user_input),
            "waiting_for_user_input",
        ),
        (44602, offset_of!(Context, gc_pending), "gc_pending"),
        (
            44603,
            offset_of!(Context, gc_driver_active),
            "gc_driver_active",
        ),
        (44604, offset_of!(Context, gc_stress), "gc_stress"),
        (
            44605,
            offset_of!(Context, compiler_function_overrides_active),
            "compiler_function_overrides_active",
        ),
        (
            44606,
            offset_of!(Context, macro_perf_enabled),
            "macro_perf_enabled",
        ),
        (44607, offset_of!(Context, custom), "custom"),
        (
            44608,
            offset_of!(Context, frame_and_buffer_state),
            "frame_and_buffer_state",
        ),
        (
            44607,
            offset_of!(Context, loadup_dump_mode),
            "loadup_dump_mode",
        ),
    ];
    // The table pins the shipped release layout. A debug build adds MatchData's
    // extra word to every field after `match_data`; derive that shift from the
    // measured type sizes rather than hard-coding it. The cfg(test) native
    // connect slot shifts only fields after ProcessManager; every original
    // production offset remains pinned in the table.
    let match_data = offset_of!(Context, match_data);
    let processes = offset_of!(Context, processes);
    let debug_extra = size_of::<MatchData>() - 224;
    let adjusted = |pinned: usize| {
        let debug_shift = if pinned > match_data { debug_extra } else { 0 };
        let observation_shift = if pinned > processes {
            observation_extra
        } else {
            0
        };
        pinned + debug_shift + observation_shift
    };
    let moved: Vec<String> = table
        .iter()
        .filter(|(expected, actual, _)| adjusted(*expected) != *actual)
        .map(|(expected, actual, name)| format!("{name}: {} -> {actual}", adjusted(*expected)))
        .collect();
    assert!(moved.is_empty(), "Context fields moved: {moved:#?}");
}
