//! Canonical descriptors and Context adapters for edit-owned buffer primitives.

use crate::emacs_core::eval::Context;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};
use neovm_value::error::EvalResult;
use neovm_value::value::Value;

/// Production and unit registration share this fixed-zero-slot descriptor.
#[doc(hidden)]
pub static BOBP_SUBR: SubrSpec =
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed0("bobp", bobp_entry));

#[inline]
fn bobp_entry(ctx: &mut Context) -> EvalResult {
    neovm_edit::emacs_core::navigation::builtin_bobp_0(&ctx.edit.buffers)
}

/// Production and unit registration share the optional-buffer descriptor.
#[doc(hidden)]
pub static BUFFER_LINE_STATISTICS_SUBR: SubrSpec =
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-line-statistics",
        NativeFn::ContextVec(buffer_line_statistics_entry),
        SubrArity::new(0, Some(1)),
    ));

#[inline]
fn buffer_line_statistics_entry(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    neovm_edit::emacs_core::buffer::builtin_buffer_line_statistics(&mut ctx.edit.buffers, args)
}
