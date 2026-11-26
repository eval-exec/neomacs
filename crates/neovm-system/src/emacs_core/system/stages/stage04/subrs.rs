//! W-REG R1 stage 04: core manifest lines 1013-1078 (access-file .. directory-files).
//!
//! Crate-private batch: the stage table in `super` owns the public
//! dispatcher and the typed segment id.

use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    crate_private,
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "access-file",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_access_file),
            SubrArity::new(2, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "expand-file-name",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_expand_file_name),
            SubrArity::new(1, Some(2)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "delete-file-internal",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_delete_file_internal),
            SubrArity::new(1, Some(1)),
        )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
                "rename-file",
                NativeFn::ContextVec(crate::emacs_core::fileio::builtin_rename_file),
                SubrArity::new(2, Some(3)),
            ))
            .requires_eval_state()
            .interactive(
                neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(
                    "fRename file: \nGRename %s to file: \np",
                ),
            ),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
                "copy-file",
                NativeFn::ContextVec(crate::emacs_core::fileio::builtin_copy_file),
                SubrArity::new(2, Some(6)),
            ))
            .requires_eval_state()
            .interactive(
                neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(
                    "fCopy file: \nGCopy %s to file: \np\nP",
                ),
            ),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
                "add-name-to-file",
                NativeFn::ContextVec(crate::emacs_core::fileio::builtin_add_name_to_file),
                SubrArity::new(2, Some(3)),
            ))
            .interactive(
                neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(
                    "fAdd name to file: \nGName to add to %s: \np",
                ),
            ),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
                "make-symbolic-link",
                NativeFn::ContextVec(crate::emacs_core::fileio::builtin_make_symbolic_link),
                SubrArity::new(2, Some(3)),
            ))
            .interactive(
                neovm_eval::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(
                    "FMake symbolic link to file: \nGMake symbolic link to file %s: \np",
                ),
            ),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
            "directory-files",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_directory_files),
            SubrArity::new(1, Some(5)),
        )),
}
