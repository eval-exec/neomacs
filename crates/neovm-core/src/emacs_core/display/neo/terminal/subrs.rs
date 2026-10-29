//! Native Lisp declarations for compositor-owned terminals.

use super::{create, destroy, get_text, resize, set_float, write};
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-create",
        NativeFn::ContextVec(create),
        SubrArity::new(3, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-write",
        NativeFn::ContextVec(write),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-resize",
        NativeFn::ContextVec(resize),
        SubrArity::new(3, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-destroy",
        NativeFn::ContextVec(destroy),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-set-float",
        NativeFn::ContextVec(set_float),
        SubrArity::new(4, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-terminal-get-text",
        NativeFn::ContextVec(get_text),
        SubrArity::new(1, Some(1)),
    )),
}
