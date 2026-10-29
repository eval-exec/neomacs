//! Native Lisp declarations for video sessions.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-video-p",
        NativeFn::ContextVec(predicate),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-video-load",
        NativeFn::ContextVec(load),
        SubrArity::new(1, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-video-play",
        NativeFn::ContextVec(play),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-video-pause",
        NativeFn::ContextVec(pause),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-video-stop",
        NativeFn::ContextVec(stop),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-video-set-loop",
        NativeFn::ContextVec(set_loop),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-video-destroy",
        NativeFn::ContextVec(destroy),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-video-diagnostics",
        NativeFn::ContextVec(diagnostics),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-video-begin-measurement-epoch",
        NativeFn::ContextVec(begin_measurement_epoch),
        SubrArity::new(0, Some(0)),
    )),
}
