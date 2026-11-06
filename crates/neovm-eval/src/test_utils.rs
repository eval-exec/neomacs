//! Evaluator test support (post-split TEST-SUPPORT-DESIGN.md rev 2).
//!
//! Only fixtures that never construct a `Context` live here: a booted
//! Context belongs to the core boot facade, and tests that need one are
//! re-homed into `neovm-core/core_tests/moved_eval/`. This module is
//! compiled for the crate's own unit tests and, behind `test-support`, for
//! core's test build; the helpers are the single value-level definitions.

pub use neovm_value::test_utils::{
    cargo_workspace_root, init_test_tracing, nextest_workspace_root, workspace_root,
};

/// The evaluator's cfg(test) unit runtime (H5a/T5).
///
/// One set per crate: eval and the layers below it are REAL; every upper
/// layer slot is either an ABSENT-LAYER refusal (PANIC), a recorded
/// absent-by-construction no-op, or one of the three structural ASSERT-EMPTY
/// checks. The proof is minted only by the one production publication entry,
/// [`hooks_proof::install_eval_hooks`], after the full set is installed.
#[cfg(test)]
mod unit_runtime {
    #![allow(unexpected_cfgs)]

    use crate::emacs_core::eval::Context;
    use crate::emacs_core::eval::buffer_kill::BufferKillHooks;
    use crate::emacs_core::eval::call_interactively::CallInteractivelyHooks;
    use crate::emacs_core::eval::case_tables::CaseTableHooks;
    use crate::emacs_core::eval::display_query_hooks::DisplayQueryHooks;
    use crate::emacs_core::eval::hooks_proof::{
        DisplayServiceHooks, EvalHookSet, EvalHooksInstalled, FileNotifyHostQueries,
        FrontendServiceHooks, TimerServiceHooks, WaitServiceHooks, install_eval_hooks,
    };
    use crate::emacs_core::eval::input_hooks::InputHooks;
    use crate::emacs_core::eval::kmacro_hooks::KmacroHooks;
    use crate::emacs_core::eval::lifecycle::{
        BootHooks, CommandSeamHooks, DbusbindHooks, DynamicModuleHooks, FileIoHooks, FontsetHooks,
        LifecycleHooks, TerminalHooks, WindowTreeHooks, WorldStateHooks, XfacesHooks,
    };
    use crate::emacs_core::eval::perf::PerfHooks;
    use crate::emacs_core::eval::process_hooks::{MainWaitHooks, ProcessHooks, SelectHooks};
    use neovm_value::error::EvalResult;
    use neovm_value::value::Value;
    use std::sync::OnceLock;

    crate::eval_absent_layer_slots!();
    use absent_layer_slots as s;

    include!("upper_subrs.generated.rs");

    // The upper layers' bootstrap variable names, from the checked-in
    // snapshot generated from the four canonical registrations (fileio,
    // process, dbusbind, xfaces). T4/H3b: each name is bound to its
    // uninterned ABSENT-LAYER marker, so boundp and special-variable-p
    // shapes match production while any use fails loudly. The core-side
    // snapshot equality test is the P2 gate.
    include!("upper_bootstrap_vars.generated.rs");

    fn boot_register_core_bootstrap_vars(obarray: &mut neovm_value::symbol::Obarray) {
        s::bind_absent_bootstrap_vars(obarray, UPPER_BOOTSTRAP_VAR_NAMES);
    }

    /// H3a: the evaluator's own registrars, from the same production code
    /// core's compatibility sequence calls. The remaining upper subr names
    /// (H3) are appended from the checked-in panicking snapshot. Exact
    /// canonical interleaving with the upper names is the P2 names+order
    /// gate; P1 is compile-only (T7).
    fn register_core_subrs(ctx: &mut Context) {
        crate::emacs_core::builtins::lcms::register_subrs(ctx);
        crate::emacs_core::data::register_subrs(ctx);
        crate::emacs_core::eval::register_subrs(ctx);
        ctx.register_subr(crate::emacs_core::eval::FBOUNDP_SUBR);
        crate::emacs_core::eval::register_public_subrs(ctx);
        std::cfg_select! {
            any(target_os = "linux", target_os = "macos", target_os = "windows") => {
                crate::emacs_core::builtins::file_notify::register_subrs(ctx);
            }
            _ => {}
        }
        for (name, min, max, function) in UPPER_SUBRS {
            ctx.register_subr(crate::emacs_core::subr::SubrSpec::new(
                name,
                crate::emacs_core::subr::NativeFn::ContextVec(*function),
                crate::emacs_core::subr::SubrArity::new(*min, *max),
            ));
        }
    }

    static LIFECYCLE: LifecycleHooks = LifecycleHooks::new(
        DynamicModuleHooks::new(
            s::dynamic_module_reset,
            Some(s::dynamic_module_fingerprint),
            s::dynamic_module_panic_message,
            s::dynamic_module_load_module,
            s::dynamic_module_apply_module_function,
            s::dynamic_module_module_call_active,
            s::dynamic_module_collect_gc_roots,
        ),
        WorldStateHooks::new(
            s::sqlite_world_state_placeholder,
            s::sqlite_world_state_swap,
        ),
        DbusbindHooks::new(
            WorldStateHooks::new(
                s::dbusbind_world_state_placeholder,
                s::dbusbind_world_state_swap,
            ),
            s::dbusbind_drain_events,
            s::dbusbind_has_active_watches,
            s::dbusbind_reset_thread_locals,
        ),
        WorldStateHooks::new(
            s::timefns_world_state_placeholder,
            s::timefns_world_state_swap,
        ),
        BootHooks::new(
            s::boot_ensure_startup_compat_variables,
            s::boot_clear_created_lisp_face,
            s::boot_remove_face_new_frame_defaults_entry,
            s::boot_restore_created_faces_from_table,
            s::boot_gui_window_system_symbol,
            s::boot_apply_post_image_init,
            s::boot_apply_lread_constants,
            s::boot_install_host_environment_snapshot,
            s::boot_shell_file_name_install,
            s::boot_shell_file_name_install_dump_default,
            s::boot_seed_batch_startup_frame,
            boot_register_core_bootstrap_vars,
            s::boot_init_frame_lisp_faces,
            s::boot_prepare_runtime_faces,
            s::boot_sync_runtime_face_table,
            s::boot_ensure_lisp_face_id_property,
            s::boot_clear_font_cache_state,
        ),
        FileIoHooks::new(
            s::fileio_expand_file_name_entry,
            s::fileio_dispatch_file_handler,
            s::fileio_resolve_filename,
            s::fileio_lock_file,
            s::fileio_unlock_file,
        ),
        TerminalHooks::new(
            s::terminal_handle_value,
            s::terminal_delete_noelisp,
            s::terminal_frame_terminal,
            s::terminal_print_handle,
            s::terminal_configure_window_system_runtime,
            s::terminal_park_carrier_tls,
            Some(s::terminal_carrier_tls_fingerprint),
            s::terminal_reset_thread_locals,
            s::terminal_reset_handle,
            s::terminal_world_state_placeholder,
            s::terminal_swap_world_state,
            s::terminal_live_terminal_ids,
            s::terminal_current_registry_handle,
            s::terminal_install_registry_handle,
            s::terminal_install_owner_registry_handle,
        ),
        FontsetHooks::new(
            s::fontset_alias_alist_startup_value,
            s::fontset_list_value,
            s::fontset_reset,
            s::fontset_snapshot,
            s::fontset_restore,
        ),
        CommandSeamHooks::new(
            s::command_window_configuration_capture,
            s::command_window_configuration_restore,
            s::command_run_redisplay_window_change_hooks,
            s::command_unwind_minibuffer_session,
            s::command_unwind_minibuffer_buffer,
            s::command_get_char_property,
            s::command_try_completion,
            s::command_all_completions,
            s::command_test_completion,
            s::command_menu_bar_top_level_items,
            s::command_key_binding,
            s::command_where_is_internal,
        ),
        WindowTreeHooks::new(
            s::window_tree_char_property_window_lookup,
            s::window_tree_window_resize_check,
            s::window_tree_window_resize_apply,
            s::window_tree_apply_mini_window_resize,
            s::window_tree_frame_resize_pixelwise,
            s::window_tree_frame_parameter_entry,
        ),
        XfacesHooks::new(
            s::xfaces_park_carrier_tls,
            Some(s::xfaces_carrier_tls_fingerprint),
            s::xfaces_current_registry_handle,
            s::xfaces_install_registry_handle,
        ),
        s::timefns_reset,
        s::sqlite_reset,
    );

    static SELECT: SelectHooks = SelectHooks::new(
        s::select_handle,
        s::select_notify,
        s::select_nudge,
        s::select_control_source,
        s::select_wait_until,
        s::select_arm,
        s::select_wait_registered,
        s::select_cancel_arm,
    );

    static MAIN_WAIT: MainWaitHooks =
        MainWaitHooks::new(s::main_wait_prepare, s::main_wait_wait, s::main_wait_cancel);

    static PROCESS: ProcessHooks = ProcessHooks::new(
        s::process_unlink,
        s::process_delete,
        s::process_open_channel,
        s::process_print_handle,
        s::process_wait_notifier,
        s::process_terminate_and_defer_reaping,
        SELECT,
        MAIN_WAIT,
    );

    static INPUT: InputHooks = InputHooks::new(
        s::input_interrupt_for_input_event_if_requested,
        s::input_os_signal_pending,
        s::input_take_termination_signal,
        s::input_drain_pending_os_signals,
        s::input_signal_wake_pipe_read_fd,
        s::input_drain_signal_wake_pipe,
    );

    static CALL_INTERACTIVELY: CallInteractivelyHooks =
        CallInteractivelyHooks::new(s::call_interactively_resolve);

    static CASE_TABLES: CaseTableHooks = CaseTableHooks::new(
        s::case_tables_canon,
        s::case_tables_upcase,
        s::case_tables_sync,
        s::case_tables_standard,
        s::case_tables_override_for_current_buffer,
    );

    static BUFFER_KILL: BufferKillHooks = BufferKillHooks::new(s::buffer_kill_before);

    static KMACRO: KmacroHooks = KmacroHooks::new(
        s::kmacro_start,
        s::kmacro_end,
        s::kmacro_call_last,
        s::kmacro_execute,
        s::kmacro_store_event,
    );

    static DISPLAY_WINDOW: crate::emacs_core::eval::display_query_hooks::DisplayWindowHooks =
        crate::emacs_core::eval::display_query_hooks::DisplayWindowHooks::new(
            s::display_window_ensure_selected_frame_id,
            s::display_window_remember_selected_window_point,
            s::display_window_window_body_dimensions,
            s::display_window_resize_mini_window,
            s::display_window_update_auto_hscroll,
            s::display_window_fontset_generation,
            s::display_window_window_domain_frame_of,
            s::display_window_window_regions_at,
            s::display_window_motion_resolve,
            s::display_window_window_line_wrap,
            s::display_window_window_body_width,
            s::display_window_window_hscroll,
            s::display_window_invisible_status,
            s::display_window_invisible_source_run_end,
        );

    static DISPLAY_QUERY: DisplayQueryHooks = DisplayQueryHooks::new(
        s::display_query_redisplay_idle_skip,
        s::display_query_mode_line_flow,
        s::display_query_face_realization,
        s::display_query_frame_tty_target,
        DISPLAY_WINDOW,
    );

    static PERF: PerfHooks =
        PerfHooks::new(s::perf_record, s::perf_reset, Some(s::perf_fingerprint));

    static WAIT: WaitServiceHooks = WaitServiceHooks {
        service_input_pending_with_timers: s::wait_service_input_pending_with_timers,
        service_read_char_preliminary_redisplay: s::wait_service_read_char_preliminary_redisplay,
        service_timers_without_redisplay: s::wait_service_timers_without_redisplay,
        wait_for_command_input_with_idle_policy: s::wait_for_command_input_with_idle_policy,
        wait_for_resize_ack_until: s::wait_for_resize_ack_until,
    };

    static TIMER: TimerServiceHooks = TimerServiceHooks {
        next_ordinary_gnu_timer_timeout_before: s::timer_next_ordinary_gnu_timer_timeout_before,
        next_idle_gnu_timer_timeout: s::timer_next_idle_gnu_timer_timeout,
    };

    static FRONTEND: FrontendServiceHooks = FrontendServiceHooks {
        apply_xwidget_frontend_event: s::frontend_apply_xwidget_frontend_event,
    };

    static DISPLAY_SERVICE: DisplayServiceHooks = DisplayServiceHooks {
        compositor_scrolling_enabled: s::display_service_compositor_scrolling_enabled,
        permits_compositor_pixel_scroll: s::display_service_permits_compositor_pixel_scroll,
        redisplay_buffer_layout_inputs: s::display_service_redisplay_buffer_layout_inputs,
    };

    static FILE_NOTIFY: FileNotifyHostQueries = FileNotifyHostQueries {
        expand_file_name: s::file_notify_expand_file_name,
        directory_file_name: s::file_notify_directory_file_name,
        file_exists_p: s::file_notify_file_exists_p,
    };

    /// The one eval unit set. `perf_enabled` is false, so the PANIC perf
    /// slots are unreachable; the sqlite installer is the recorded
    /// ASSERT-EMPTY publication (H2a).
    static SET: EvalHookSet = EvalHookSet::new(
        &LIFECYCLE,
        &PROCESS,
        &INPUT,
        &CALL_INTERACTIVELY,
        &CASE_TABLES,
        &BUFFER_KILL,
        &KMACRO,
        &DISPLAY_QUERY,
        false,
        &PERF,
        register_core_subrs,
        s::absent_sqlite_installer,
        &WAIT,
        &TIMER,
        &FRONTEND,
        &DISPLAY_SERVICE,
        &FILE_NOTIFY,
    );

    /// The one cfg(test) unit runtime (H5a). The proof is minted only by the
    /// production publication entry, after the full set is installed, and
    /// cached: every constructor switch shares this one proof.
    pub(crate) fn unit_runtime() -> EvalHooksInstalled {
        static PROOF: OnceLock<EvalHooksInstalled> = OnceLock::new();
        *PROOF.get_or_init(|| {
            install_eval_hooks(&SET).expect("the eval unit hook set publishes at first use")
        })
    }

    /// Drift-gate views over the compiled registration tables (H3/H3b): the
    /// checked-in snapshots and these tables must agree row for row.
    pub(crate) fn upper_subrs_for_snapshot_test() -> Vec<(&'static str, u16, Option<u16>)> {
        UPPER_SUBRS
            .iter()
            .map(|row| (row.0, row.1, row.2))
            .collect()
    }

    pub(crate) fn upper_bootstrap_vars_for_snapshot_test() -> &'static [&'static str] {
        UPPER_BOOTSTRAP_VAR_NAMES
    }
}

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use unit_runtime::unit_runtime;

/// Drift gate for the H3/H3b snapshots (peer r2 (d)): the checked-in TSVs,
/// the generated registration source and the compiled tables must agree row
/// for row. Same parser as `cargo xtask upper-snapshots`.
#[cfg(test)]
mod snapshot_consistency {
    use crate::upper_snapshots_build_support as generator;

    use super::unit_runtime::{
        upper_bootstrap_vars_for_snapshot_test, upper_subrs_for_snapshot_test,
    };

    const SUBRS_SOURCE: &str = include_str!("upper_subrs.generated.rs");
    const BOOTSTRAP_SOURCE: &str = include_str!("upper_bootstrap_vars.generated.rs");
    const SUBRS_SNAPSHOT: &str = include_str!("upper_subrs.snapshot.tsv");
    const BOOTSTRAP_SNAPSHOT: &str = include_str!("upper_bootstrap_vars.snapshot.tsv");

    #[test]
    fn the_snapshots_match_the_generated_source_and_the_compiled_tables() {
        let rows = generator::parse_upper_subrs(SUBRS_SOURCE).expect("generated upper subrs");
        assert_eq!(
            generator::render_upper_subrs(&rows),
            SUBRS_SNAPSHOT,
            "upper_subrs.snapshot.tsv is stale; run `cargo xtask upper-snapshots`"
        );
        let bootstrap =
            generator::parse_bootstrap_vars(BOOTSTRAP_SOURCE).expect("generated bootstrap vars");
        assert_eq!(
            generator::render_bootstrap_vars(&bootstrap),
            BOOTSTRAP_SNAPSHOT,
            "upper_bootstrap_vars.snapshot.tsv is stale; run `cargo xtask upper-snapshots`"
        );

        // The host-active slice of the snapshot is exactly the compiled
        // registration table, in the same order.
        let compiled = upper_subrs_for_snapshot_test();
        let active: Vec<_> = rows
            .iter()
            .filter(|row| generator::cfg_active(&row.cfg))
            .collect();
        assert_eq!(active.len(), compiled.len());
        for (row, compiled) in active.iter().zip(compiled.iter()) {
            assert_eq!(
                (row.name.as_str(), row.min, row.max),
                (compiled.0, compiled.1, compiled.2),
                "compiled registration order/arity diverges from the snapshot"
            );
        }
        let active_names: Vec<&str> = bootstrap
            .iter()
            .filter(|row| generator::cfg_active(&row.cfg))
            .map(|row| row.name.as_str())
            .collect();
        assert_eq!(
            active_names.as_slice(),
            upper_bootstrap_vars_for_snapshot_test()
        );
    }
}
