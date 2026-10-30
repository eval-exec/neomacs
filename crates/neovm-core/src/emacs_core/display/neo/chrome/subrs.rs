//! Native Lisp declarations for renderer chrome controls.

use super::{set_extra_spacing, set_scroll_indicators};
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-set-scroll-indicators",
        NativeFn::ContextVec(set_scroll_indicators),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-set-extra-spacing",
        NativeFn::ContextVec(set_extra_spacing),
        SubrArity::new(2, Some(2)),
    )),
}
