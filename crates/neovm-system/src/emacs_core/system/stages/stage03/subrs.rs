//! W-REG R1 stage 03: core manifest lines 344-655 (set-visited-file-modtime .. process-buffer).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-visited-file-modtime",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_visited_file_modtime),
            SubrArity::new(0, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "print--preprocess",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_print_preprocess),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "format-network-address",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_format_network_address),
            SubrArity::new(1, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "network-interface-list",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_network_interface_list),
            SubrArity::new(0, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "network-interface-info",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_network_interface_info),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "signal-names",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_signal_names),
            SubrArity::new(0, Some(0)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "accept-process-output",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_accept_process_output),
            SubrArity::new(0, Some(4)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "list-system-processes",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_list_system_processes),
            SubrArity::new(0, Some(0)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "num-processors",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_num_processors),
            SubrArity::new(0, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "make-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_make_process),
            SubrArity::new(0, None),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "make-network-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_make_network_process),
            SubrArity::new(0, None),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "neomacs-open-tls-stream",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_neomacs_open_tls_stream),
            SubrArity::new(4, Some(4)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "neomacs-tls-available-p",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::tls::builtin_neomacs_tls_available_p(args)
            }),
            SubrArity::new(0, Some(0)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "make-pipe-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_make_pipe_process),
            SubrArity::new(0, None),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "gnutls-boot",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_boot),
            SubrArity::new(3, Some(3)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "make-serial-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_make_serial_process),
            SubrArity::new(0, None),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "serial-process-configure",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_serial_process_configure),
            SubrArity::new(0, None),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "call-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_call_process),
            SubrArity::new(1, None),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "call-process-region",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_call_process_region),
            SubrArity::new(3, None),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "continue-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_continue_process),
            SubrArity::new(0, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
                "delete-process",
                NativeFn::ContextVec(crate::emacs_core::process::builtin_delete_process),
                SubrArity::new(0, Some(1)),
            ))
            .interactive(
                neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::Form(
                    neovm_eval::emacs_core::builtins::delete_process_interactive_spec,
                ),
            ),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "interrupt-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_interrupt_process),
            SubrArity::new(0, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
                "kill-process",
                NativeFn::ContextVec(crate::emacs_core::process::builtin_kill_process),
                SubrArity::new(0, Some(2)),
            ))
            .interactive(
                neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::Form(
                    neovm_eval::emacs_core::builtins::kill_process_interactive_spec,
                ),
            ),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "quit-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_quit_process),
            SubrArity::new(0, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
                "signal-process",
                NativeFn::ContextVec(crate::emacs_core::process::builtin_signal_process),
                SubrArity::new(2, Some(3)),
            ))
            .interactive(
                neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::Form(
                    neovm_eval::emacs_core::builtins::signal_process_interactive_spec,
                ),
            ),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "stop-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_stop_process),
            SubrArity::new(0, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "get-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_get_process),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "get-buffer-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_get_buffer_process),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-attributes",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_attributes),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "processp",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_processp),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-id",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_id),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-command",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_command),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-contact",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_contact),
            SubrArity::new(1, Some(3)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-filter",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_filter),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-process-filter",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_filter),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-sentinel",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_sentinel),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-process-sentinel",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_sentinel),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-coding-system",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_coding_system),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-datagram-address",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_datagram_address),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-process-buffer",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_buffer),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-process-thread",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_thread),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-process-window-size",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_window_size),
            SubrArity::new(3, Some(3)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-tty-name",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_tty_name),
            SubrArity::new(1, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-plist",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_plist),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-process-plist",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_plist),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-mark",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_mark),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-type",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_type),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-thread",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_thread),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-running-child-p",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_running_child_p),
            SubrArity::new(0, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-send-region",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_send_region),
            SubrArity::new(3, Some(3)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-send-eof",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_send_eof),
            SubrArity::new(0, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-send-string",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_send_string),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-status",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_status),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-exit-status",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_exit_status),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-list",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_list),
            SubrArity::new(0, Some(0)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-name",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_name),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-buffer",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_buffer),
            SubrArity::new(1, Some(1)),
        )),
}
