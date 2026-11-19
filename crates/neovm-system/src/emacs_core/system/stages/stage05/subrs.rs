//! W-REG R1 stage 05: core manifest lines 1088-1168 (file-exists-p .. set-process-inherit-coding-system-flag).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-exists-p",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_exists_p),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-readable-p",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_readable_p),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-writable-p",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_writable_p),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-acl",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_acl),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-executable-p",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_executable_p),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-locked-p",
            NativeFn::ContextVec(crate::emacs_core::filelock::builtin_file_locked_p),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-selinux-context",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_selinux_context),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-system-info",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_system_info),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-directory-p",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_directory_p),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-regular-p",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_regular_p),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-symlink-p",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_symlink_p),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-modes",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_modes),
            SubrArity::new(1, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
                "set-file-modes",
                NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_file_modes),
                SubrArity::new(2, Some(3)),
            ))
            .interactive(
                neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::Form(
                    neovm_eval::emacs_core::builtins::set_file_modes_interactive_spec,
                ),
            ),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-file-times",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_file_times),
            SubrArity::new(1, Some(3)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-process-inherit-coding-system-flag",
            NativeFn::ContextVec(
                crate::emacs_core::process::builtin_set_process_inherit_coding_system_flag,
            ),
            SubrArity::new(2, Some(2)),
        )),
}
