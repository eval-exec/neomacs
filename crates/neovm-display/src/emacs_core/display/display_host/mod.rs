//! Host-side display capabilities and validated boundary values.
//!
//! This is Neomacs integration machinery, not a port of GNU Emacs
//! src/eval.c, so it stays outside the GNU mirror module.

pub use neomacs_display_protocol::{GraphicalFaceAttribute, SelectionOwner};
pub use neovm_eval::emacs_core::eval::protocol::{
    FontOpeningSize, FrameFontSize, PositiveFontScalar, TerminalCreateRequest,
    TerminalDisplayTarget, TerminalFloatPlacement, TerminalGridSize, TerminalId,
};

pub use neovm_eval::emacs_core::eval::protocol::{
    AvailableFontFamilyName, DisplayHost, FontCoverage, FontCoverageRequest, FontCoverageTarget,
    FontResolveRequest, FrameFontRequest, GuiResourceQuery, InertRowsHostAuthority,
    InitialGuiFrameDefaults, SystemFontName, SystemFontRole, SystemFonts,
};

/// Parse native/Fontconfig/XLFD names through the shared Lisp font parser.
pub fn frame_font_request_from_name(name: &str) -> Option<FrameFontRequest> {
    crate::emacs_core::font::frame_font_request_from_named_font_string(name)
}
pub use neovm_value::display_vocab::XwidgetScriptRequestId;

/// Host-local reference to an exact selected font entity. Handles are never
/// reused during a host's lifetime; the host validates every lookup.
pub use neovm_value::display_vocab::FontEntityHandle;

#[cfg(test)]
#[path = "tests/display_host_test.rs"]
mod tests;
