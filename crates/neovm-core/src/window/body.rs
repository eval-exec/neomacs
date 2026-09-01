//! Unit semantics for Lisp queries over a window's text body.
//!
//! GNU Emacs has three states here, not a boolean `pixelwise` flag.  Keeping
//! those states as a closed Rust enum prevents the special `remap` unit from
//! silently collapsing into the generic non-nil/pixel case.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowBodyUnit {
    CanonicalChars,
    RemappedChars,
    Pixels,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowBodyAxis {
    Width,
    Height,
}

/// Positive pixel metrics for one character cell.
///
/// Construction normalizes invalid backend/frame metrics at the boundary, so
/// body measurement never divides by zero, a negative value, or NaN.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowBodyCellSize {
    width: f32,
    height: f32,
}

impl WindowBodyCellSize {
    pub(crate) fn new(width: f32, height: f32) -> Self {
        Self {
            width: positive_or_one(width),
            height: positive_or_one(height),
        }
    }

    fn along(self, axis: WindowBodyAxis) -> f32 {
        match axis {
            WindowBodyAxis::Width => self.width,
            WindowBodyAxis::Height => self.height,
        }
    }
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

fn positive_or_one(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}
