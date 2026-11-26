// W-REG R0 compile probe for the additive crate-private batch form: the
// catalog and registrar must stay crate-visible (no public SUBRS), while the
// owning crate still installs them through its own code.
use super::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    crate_private,
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "wreg-crate-private-probe",
        NativeFn::ContextVec(super::vector),
        SubrArity::new(0, Some(0)),
    )),
}
