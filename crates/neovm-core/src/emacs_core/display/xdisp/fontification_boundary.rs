//! Exact fontification boundaries under an existing raw-property proof.
//!
//! The selector is immutable once/process policy, safe for independent
//! mutators. Each admission borrows only the current Context/Buffer, and no
//! Lisp, GC, mutation or retained witness crosses the synchronous scan.

use crate::buffer::LispCharPos1;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
pub(super) use crate::emacs_core::textprop::{
    FontificationBoundaryAdmission, FontificationBoundaryMode, FontificationScanAdmission,
};
use std::ffi::OsStr;

#[cold]
#[inline(never)]
fn parse(value: Option<&OsStr>) -> FontificationBoundaryMode {
    match value {
        None => FontificationBoundaryMode::On,
        Some(value) => {
            if value.to_str().is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "on" | "1" | "true" | "yes"
                )
            }) {
                FontificationBoundaryMode::On
            } else {
                FontificationBoundaryMode::Off
            }
        }
    }
}

#[inline]
pub(super) fn mode() -> FontificationBoundaryMode {
    static MODE: std::sync::OnceLock<FontificationBoundaryMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| parse(std::env::var_os("NEOMACS_FONTIFICATION_RAW_BOUNDARY").as_deref()))
}

/// The core property module shares its existing resolved buffer tail on
/// proof refusal. This borrows one immutable query and never runs Lisp/GC.
#[inline]
pub(super) fn admit(
    ctx: &Context,
    position: i64,
    end: i64,
    mode: FontificationBoundaryMode,
) -> Result<FontificationBoundaryAdmission, Flow> {
    crate::emacs_core::textprop::next_fontified_property_boundary_in_state(
        ctx.variables(),
        ctx.buffer_manager(),
        LispCharPos1::new(position),
        LispCharPos1::new(end),
        mode,
    )
}

/// A synchronous first-nil query. Nothing returned retains a property-tree
/// borrow or a proof which could survive a fontification callback.
#[inline]
pub(super) fn first_unfontified(
    ctx: &Context,
    position: i64,
    end: i64,
    mode: FontificationBoundaryMode,
) -> FontificationScanAdmission {
    crate::emacs_core::textprop::first_unfontified_position_in_state(
        ctx.variables(),
        ctx.buffer_manager(),
        LispCharPos1::new(position),
        LispCharPos1::new(end),
        mode,
    )
}

#[cfg(test)]
#[path = "tests/fontification_boundary_test.rs"]
mod tests;
