//! W-REG R1 stage 06: core manifest lines 1223-1313 (current-time .. set-binary-mode).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "current-time",
            NativeFn::ContextVec(crate::emacs_core::timefns::builtin_current_time_in_context),
            SubrArity::new(0, Some(0)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "float-time",
            NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_float_time(args)),
            SubrArity::new(0, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-name-directory",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_directory),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-name-nondirectory",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_nondirectory),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-name-as-directory",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_as_directory),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "directory-file-name",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_directory_file_name),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-name-concat",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::fileio::builtin_file_name_concat(args)
            }),
            SubrArity::new(1, None),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "file-name-absolute-p",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::fileio::builtin_file_name_absolute_p(args)
            }),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "directory-name-p",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::fileio::builtin_directory_name_p(args)
            }),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "substitute-in-file-name",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_substitute_in_file_name),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-file-acl",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_file_acl),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-file-selinux-context",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_file_selinux_context),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "visited-file-modtime",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_visited_file_modtime),
            SubrArity::new(0, Some(0)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "make-temp-name",
            NativeFn::ContextVec(|ctx, args| {
                crate::emacs_core::fileio::builtin_make_temp_name(ctx, args)
            }),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "next-read-file-uses-dialog-p",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::fileio::builtin_next_read_file_uses_dialog_p(args)
            }),
            SubrArity::new(0, Some(0)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "unhandled-file-name-directory",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_unhandled_file_name_directory_eval),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-binary-mode",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::process::builtin_set_binary_mode(args)
            }),
            SubrArity::new(2, Some(2)),
        )),
}
