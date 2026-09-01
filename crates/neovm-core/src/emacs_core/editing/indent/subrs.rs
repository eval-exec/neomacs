//! Native Lisp declarations owned by GNU `src/indent.c`'s mirror.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    SubrSpec::new(
        "current-indentation",
        NativeFn::ContextVec(current_indentation),
        SubrArity::new(0, Some(0)),
    ),
    SubrSpec::new(
        "indent-to",
        NativeFn::ContextVec(indent_to),
        SubrArity::new(1, Some(2)),
    )
    .interactive(
        crate::emacs_core::interactive::BuiltinInteractiveSpec::String("NIndent to column: "),
    ),
    SubrSpec::new(
        "current-column",
        NativeFn::ContextVec(current_column),
        SubrArity::new(0, Some(0)),
    ),
    SubrSpec::new(
        "move-to-column",
        NativeFn::ContextVec(move_to_column),
        SubrArity::new(1, Some(2)),
    )
    .interactive(
        crate::emacs_core::interactive::BuiltinInteractiveSpec::String("NMove to column: "),
    ),
    SubrSpec::new(
        "line-number-display-width",
        NativeFn::ContextVec(line_number_display_width),
        SubrArity::new(0, Some(1)),
    ),
    SubrSpec::new(
        "vertical-motion",
        NativeFn::ContextVec(vertical_motion),
        SubrArity::new(1, Some(3)),
    )
    .requires_eval_state(),
    SubrSpec::new(
        "compute-motion",
        NativeFn::ContextVec(compute_motion),
        SubrArity::new(7, Some(7)),
    ),
}
