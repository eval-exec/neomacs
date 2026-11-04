//! Unit semantics for Lisp queries over a window's text body.
//!
//! GNU Emacs has three states here, not a boolean `pixelwise` flag.  Keeping
//! those states as a closed Rust enum prevents the special `remap` unit from
//! silently collapsing into the generic non-nil/pixel case.

pub(crate) use neovm_edit::emacs_core::display_metrics::{WindowBodyAxis, WindowBodyCellSize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowBodyUnit {
    CanonicalChars,
    RemappedChars,
    Pixels,
}

impl WindowBodyUnit {
    /// Convert a pixel extent into this unit with GNU's floor semantics.
    ///
    /// GNU deliberately falls back to canonical cells when no face remapping
    /// is active.  Neomacs also takes that safe fallback when its display host
    /// cannot realize a remapped font, instead of accidentally returning a
    /// pixel count for the symbolic `remap` request.
    pub(crate) fn measure(
        self,
        axis: WindowBodyAxis,
        pixels: i64,
        canonical: WindowBodyCellSize,
        remapped: Option<WindowBodyCellSize>,
    ) -> i64 {
        let pixels = pixels.max(0);
        let divisor = match self {
            Self::Pixels => return pixels,
            Self::CanonicalChars => canonical.along(axis),
            Self::RemappedChars => remapped.unwrap_or(canonical).along(axis),
        };
        (pixels as f32 / divisor).floor() as i64
    }
}
