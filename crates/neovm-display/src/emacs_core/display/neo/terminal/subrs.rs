//! Native Lisp declarations for compositor-owned terminals.

use super::{create, destroy, get_text, resize, set_float, write};
use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-create",
        NativeFn::ContextVec(create),
        SubrArity::new(3, Some(4)),
    )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-write",
        NativeFn::ContextVec(write),
        SubrArity::new(2, Some(2)),
    )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-resize",
        NativeFn::ContextVec(resize),
        SubrArity::new(3, Some(3)),
    )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-destroy",
        NativeFn::ContextVec(destroy),
        SubrArity::new(1, Some(1)),
    )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-set-float",
        NativeFn::ContextVec(set_float),
        SubrArity::new(4, Some(4)),
    )),
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-get-text",
        NativeFn::ContextVec(get_text),
        SubrArity::new(1, Some(1)),
    )),
}
