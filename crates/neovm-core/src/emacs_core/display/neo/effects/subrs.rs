//! Native Lisp declarations for renderer effects.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    SubrSpec::new(
        "neomacs-effect-set",
        NativeFn::ContextVec(set),
        SubrArity::new(1, None),
    ),
    SubrSpec::new(
        "neomacs-effect-get",
        NativeFn::ContextVec(get),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "neomacs-effect-reset",
        NativeFn::ContextVec(reset),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "neomacs-effects-apply",
        NativeFn::ContextVec(apply),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "neomacs-effect-names",
        NativeFn::ContextVec(names),
        SubrArity::new(0, Some(1)),
    ),
}
