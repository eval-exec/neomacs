//! Native Lisp declarations for shader surfaces.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    SubrSpec::new(
        "neomacs-surface-create",
        NativeFn::ContextVec(create),
        SubrArity::new(0, None),
    ),
    SubrSpec::new(
        "neomacs-surface-set-uniform",
        NativeFn::ContextVec(set_uniform),
        SubrArity::new(3, Some(3)),
    ),
    SubrSpec::new(
        "neomacs-surface-destroy",
        NativeFn::ContextVec(destroy),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "neomacs-surface-available-p",
        NativeFn::ContextVec(available),
        SubrArity::new(0, Some(0)),
    ),
    SubrSpec::new(
        "neomacs-frame-shader",
        NativeFn::ContextVec(set_frame_shader),
        SubrArity::new(1, Some(3)),
    ),
    SubrSpec::new(
        "neomacs-frame-shader-set-uniform",
        NativeFn::ContextVec(set_frame_shader_uniform),
        SubrArity::new(2, Some(2)),
    ),
}
