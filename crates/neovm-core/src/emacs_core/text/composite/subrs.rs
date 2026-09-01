//! Native Lisp declarations owned by GNU `src/composite.c`'s mirror.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    SubrSpec::new(
        "compose-region-internal",
        NativeFn::ContextVec(compose_region_internal),
        SubrArity::new(2, Some(4)),
    ),
    SubrSpec::new(
        "compose-string-internal",
        NativeFn::ContextVec(compose_string),
        SubrArity::new(3, Some(5)),
    ),
    SubrSpec::new(
        "find-composition-internal",
        NativeFn::ContextVec(find_composition_internal),
        SubrArity::new(4, Some(4)),
    ),
    SubrSpec::new(
        "composition-get-gstring",
        NativeFn::ContextVec(composition_get_gstring),
        SubrArity::new(4, Some(4)),
    ),
    SubrSpec::new(
        "clear-composition-cache",
        NativeFn::ContextVec(clear_cache),
        SubrArity::new(0, Some(0)),
    ),
    SubrSpec::new(
        "composition-sort-rules",
        NativeFn::ContextVec(sort_rules),
        SubrArity::new(1, Some(1)),
    ),
}
