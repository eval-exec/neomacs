//! Canonical descriptors and Context adapters for edit-owned buffer primitives.

use crate::emacs_core::eval::Context;
use crate::emacs_core::subr::SubrSpec;
use neovm_value::error::EvalResult;

/// Production and unit registration share this fixed-zero-slot descriptor.
#[doc(hidden)]
pub static BOBP_SUBR: SubrSpec =
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed0("bobp", bobp_entry));

#[inline]
fn bobp_entry(ctx: &mut Context) -> EvalResult {
    neovm_edit::emacs_core::navigation::builtin_bobp_0(&ctx.edit.buffers)
}
