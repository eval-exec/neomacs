//! Browser HTTP declarations; available on native only as unsupported operations.
use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    SubrSpec::new("neomacs-http-start", NativeFn::NoContextVec(start), SubrArity::new(4, Some(4))),
    SubrSpec::new("neomacs-http-take", NativeFn::NoContextVec(take), SubrArity::new(1, Some(1))),
    SubrSpec::new("neomacs-http-cancel", NativeFn::NoContextVec(cancel), SubrArity::new(1, Some(1))),
}
