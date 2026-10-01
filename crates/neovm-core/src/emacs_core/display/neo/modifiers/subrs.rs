//! Native Lisp declarations for the NS modifier policy.

use super::set_modifier_policy;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    SubrSpec::new(
        "neomacs-set-modifier-policy",
        NativeFn::ContextVec(set_modifier_policy),
        SubrArity::new(0, Some(0)),
    ),
}
