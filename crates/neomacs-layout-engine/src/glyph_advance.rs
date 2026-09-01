#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GlyphAdvanceQuantization {
    PreserveLogicalPixels,
    SnapToIntegerPixels,
}

impl GlyphAdvanceQuantization {
    pub(crate) fn resolve(self, measured: Option<f32>, fallback: f32, minimum: f32) -> f32 {
        let advance = measured
            .filter(|advance| advance.is_finite() && *advance > 0.0)
            .unwrap_or(fallback);
        self.quantize(advance, minimum)
    }

    pub(crate) fn quantize(self, advance: f32, minimum: f32) -> f32 {
        let minimum = if minimum.is_finite() && minimum > 0.0 {
            minimum
        } else {
            1.0
        };
        let advance = if advance.is_finite() && advance > 0.0 {
            advance
        } else {
            minimum
        };

        match self {
            Self::PreserveLogicalPixels => advance.max(minimum).max(1.0),
            Self::SnapToIntegerPixels => advance.round().max(minimum.round().max(1.0)),
        }
    }
}
