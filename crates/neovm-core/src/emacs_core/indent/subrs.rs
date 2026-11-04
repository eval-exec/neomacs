//! Native Lisp declarations owned by GNU `src/indent.c`'s mirror.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-indentation",
        NativeFn::ContextVec(current_indentation),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "indent-to",
        NativeFn::ContextVec(indent_to),
        SubrArity::new(1, Some(2)),
    ))
    .interactive(
        crate::emacs_core::interactive::BuiltinInteractiveSpec::String("NIndent to column: "),
    ),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-column",
        NativeFn::ContextVec(current_column),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "move-to-column",
        NativeFn::ContextVec(move_to_column),
        SubrArity::new(1, Some(2)),
    ))
    .interactive(
        crate::emacs_core::interactive::BuiltinInteractiveSpec::String("NMove to column: "),
    ),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "line-number-display-width",
        NativeFn::ContextSlice(line_number_display_width),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "vertical-motion",
        NativeFn::ContextSlice(vertical_motion),
        SubrArity::new(1, Some(3)),
    ))
    .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "compute-motion",
        NativeFn::ContextVec(compute_motion),
        SubrArity::new(7, Some(7)),
    )),
}
