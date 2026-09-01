//! Native Lisp declarations owned by GNU `src/data.c`'s mirror.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    SubrSpec::new(
        "default-boundp",
        NativeFn::ContextVec(default_boundp),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "default-value",
        NativeFn::ContextVec(default_value),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "set-default",
        NativeFn::ContextVec(set_default),
        SubrArity::new(2, Some(2)),
    ),
}
