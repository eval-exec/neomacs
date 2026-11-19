//! W-REG R1 stage 16: core manifest lines 1850-1924 (file-accessible-directory-p .. set-process-datagram-address).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-accessible-directory-p",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_accessible_directory_p),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-name-case-insensitive-p",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_case_insensitive_p),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-newer-than-file-p",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_newer_than_file_p),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "verify-visited-file-modtime",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_verify_visited_file_modtime),
            SubrArity::new(0, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "internal-default-interrupt-process",
            NativeFn::ContextVec(
                crate::emacs_core::process::builtin_internal_default_interrupt_process,
            ),
            SubrArity::new(0, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "internal-default-process-filter",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_internal_default_process_filter),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "internal-default-process-sentinel",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_internal_default_process_sentinel),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "internal-default-signal-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_internal_default_signal_process),
            SubrArity::new(2, Some(3)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "network-lookup-address-info",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_network_lookup_address_info),
            SubrArity::new(1, Some(3)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-network-process-option",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_set_network_process_option),
            SubrArity::new(3, Some(4)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-query-on-exit-flag",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_process_query_on_exit_flag),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-process-query-on-exit-flag",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_query_on_exit_flag),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "process-inherit-coding-system-flag",
            NativeFn::ContextVec(
                crate::emacs_core::process::builtin_process_inherit_coding_system_flag,
            ),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-process-coding-system",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_coding_system),
            SubrArity::new(1, Some(3)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-process-datagram-address",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_datagram_address),
            SubrArity::new(2, Some(2)),
        )),
}
