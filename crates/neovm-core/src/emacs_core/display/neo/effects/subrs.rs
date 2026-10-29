//! Native Lisp declarations for renderer effects.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs--effect-set",
        NativeFn::ContextVec(set),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-effect-get",
        NativeFn::ContextVec(get),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs--effects-apply",
        NativeFn::ContextVec(apply),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-effect-schema",
        NativeFn::ContextVec(schema),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-effect-names",
        NativeFn::ContextVec(names),
        SubrArity::new(0, Some(1)),
    )),
}
