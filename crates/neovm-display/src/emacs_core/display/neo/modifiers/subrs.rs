//! Native Lisp declarations for the NS modifier policy.

use super::set_modifier_policy;
use neovm_eval::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

neovm_eval::emacs_core::subr::define_subrs! {
    neovm_eval::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-set-modifier-policy",
        NativeFn::ContextVec(set_modifier_policy),
        SubrArity::new(0, Some(0)),
    )),
}
