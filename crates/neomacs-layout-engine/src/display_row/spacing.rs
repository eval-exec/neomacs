//! GNU line spacing is a nonnegative, integral logical-pixel extent.
//!
//! Resolve before committing a row: fractional spacing must not accumulate
//! differently in layout, retained rows, backgrounds, and hit testing.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ResolvedLineSpacing(u32);

impl ResolvedLineSpacing {
    pub(crate) const NONE: Self = Self(0);

    pub(crate) fn from_pixels(pixels: f32) -> Self {
        // GNU calc_line_height_property truncates the scaled height. Rust's
        // saturating float-to-integer conversion also bounds oversized input.
        if pixels.is_finite() {
            Self(pixels.max(0.0) as u32)
        } else {
            Self::NONE
        }
    }

    pub(crate) fn pixels(self) -> f32 {
        self.0 as f32
    }
}
