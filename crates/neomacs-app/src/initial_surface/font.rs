//! Publish the exact opening font to Lisp before startup faces are initialized.

use super::InitialFrameFont;
use crate::font_queries::{core_opened_font_from_selection, font_otf_capability_for_file};
use neomacs_layout_engine::font::metrics::SelectedFontInfo;
use neomacs_layout_engine::font::sizing::FontSizing;
use neovm_core::emacs_core::{Value, eval::ResolvedFontMatch};
use neovm_core::face::{Face, FaceHeight, FontWeight};

impl InitialFrameFont {
    /// Seed Lisp from the same opened font that supplied frame geometry.
    /// A bare family selector has no realized size and must not replace this
    /// object: GNU startup derives the default face from `font-parameter`.
    pub fn opened(selected: SelectedFontInfo, sizing: FontSizing) -> Self {
        let weight = FontWeight::from_css_weight(selected.resolved.weight);
        let weight_name = match weight {
            FontWeight::Normal => "regular",
            _ => weight.symbol_name(),
        };
        let name = Value::string(format!(
            "-*-{}-{}-{}-*-*-{}-*-*-*-*-*-*-*",
            selected.resolved.family,
            weight_name,
            selected.slant.symbol_name(),
            selected.metrics.pixel_size,
        ));
        let mut face = Face::new("default");
        face.height = Some(FaceHeight::Absolute(
            sizing.face_height_tenths_for_layout_pixels(selected.metrics.pixel_size.max(1)),
        ));
        let matched = ResolvedFontMatch {
            glyph_code: None,
            font: core_opened_font_from_selection(selected, font_otf_capability_for_file),
        };
        Self::new(
            neovm_core::emacs_core::font::opened_font_from_resolved_match(&face, &matched),
            name,
        )
    }
}
