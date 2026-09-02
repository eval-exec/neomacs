//! Initial display-protocol frame shown before evaluator redisplay is attached.

use neomacs_display_protocol::font::{ResolvedCharGlyph, ResolvedGlyphId};
use neomacs_display_protocol::{
    BasicFaceId, Color, DisplayWindowId, FaceId, FrameGlyphBuffer, GeometrySize, GlyphRowRole,
    LogicalPixels,
};
use thiserror::Error;

use crate::font::metrics::FontMetricsService;

const FONT_FAMILY: &str = "monospace";
const FONT_SIZE: f32 = 16.0;
const FONT_WEIGHT: u16 = 400;
const TEXT_LINES: &[&str] = &[
    "Neomacs portable frontend",
    "GPU renderer and packaged fonts are ready.",
];

/// Failure to materialize the product's initial text frame.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapFrameError {
    #[error("the platform font catalog cannot render bootstrap character {0:?}")]
    MissingCharacter(char),
}

/// Reusable producer for the pre-evaluator frame.
///
/// Keeping the metrics service alive makes resize relayout reuse its font and
/// glyph-selection caches instead of rescanning the platform catalog.
pub struct PortableBootstrapFrameBuilder {
    metrics: FontMetricsService,
}

impl Default for PortableBootstrapFrameBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PortableBootstrapFrameBuilder {
    pub fn new() -> Self {
        Self {
            metrics: FontMetricsService::new(),
        }
    }

    pub fn with_font_backend(backend: Box<dyn crate::font_backend::FontBackend>) -> Self {
        Self {
            metrics: FontMetricsService::with_font_backend(backend),
        }
    }

    pub fn build(
        &mut self,
        size: GeometrySize<LogicalPixels>,
    ) -> Result<FrameGlyphBuffer, BootstrapFrameError> {
        build_with_metrics(size, &mut self.metrics)
    }
}

fn build_with_metrics(
    size: GeometrySize<LogicalPixels>,
    metrics: &mut FontMetricsService,
) -> Result<FrameGlyphBuffer, BootstrapFrameError> {
    let width = size.width();
    let height = size.height();
    let face_id: FaceId = BasicFaceId::Default.into();
    let window_id = DisplayWindowId::new(1);
    let background = Color::rgb(0.055, 0.067, 0.090);
    let foreground = Color::rgb(0.820, 0.855, 0.925);
    let accent = Color::rgb(0.180, 0.255, 0.420);

    let primary = metrics
        .select_font_for_char('N', FONT_FAMILY, FONT_WEIGHT, false, FONT_SIZE)
        .ok_or(BootstrapFrameError::MissingCharacter('N'))?;
    let line_height = primary.metrics.height.max(1) as f32;
    let ascent = primary.metrics.ascent.max(1) as f32;
    let default_font_id = primary.resolved.id;

    let mut frame = FrameGlyphBuffer::with_size(width, height);
    frame.background = background;
    frame.char_height = line_height;
    frame.font_pixel_size = FONT_SIZE;
    frame.font_catalog_generation = metrics.font_catalog_generation();
    frame.set_draw_context(window_id, GlyphRowRole::Text, None);
    frame.set_face_with_font(
        face_id,
        foreground,
        Some(background),
        FONT_FAMILY,
        FONT_WEIGHT,
        false,
        FONT_SIZE,
        0,
        None,
        0,
        None,
        0,
        None,
        false,
    );
    frame
        .faces
        .get_mut(&face_id)
        .expect("set_face_with_font installs the selected face")
        .default_resolved_font_id = Some(default_font_id);
    frame.fonts.insert(default_font_id, primary.resolved);

    frame.add_background(0.0, 0.0, width, 5.0, accent);
    let mut y = 32.0;
    for text in TEXT_LINES {
        let mut x = 24.0;
        for ch in text.chars() {
            let selected = metrics
                .select_font_for_char(ch, FONT_FAMILY, FONT_WEIGHT, false, FONT_SIZE)
                .ok_or(BootstrapFrameError::MissingCharacter(ch))?;
            let glyph_id = selected
                .glyph_code
                .map(ResolvedGlyphId::new)
                .ok_or(BootstrapFrameError::MissingCharacter(ch))?;
            let advance = metrics.char_width(ch, FONT_FAMILY, FONT_WEIGHT, false, FONT_SIZE);
            let resolved_font_id = selected.resolved.id;
            frame
                .fonts
                .entry(resolved_font_id)
                .or_insert(selected.resolved);
            frame.char_fonts.entry(face_id).or_default().insert(
                ch,
                ResolvedCharGlyph {
                    resolved_font_id,
                    glyph_id,
                    advance_px: advance,
                },
            );
            frame.add_char(ch, x, y, advance, line_height, ascent, false);
            x += advance;
        }
        y += line_height * 1.5;
    }

    Ok(frame)
}

#[cfg(test)]
mod tests {
    use neomacs_display_protocol::font::FontBackendKind;
    use neomacs_display_protocol::{FrameGlyph, GeometrySize, LogicalPixels};

    use super::*;
    use crate::font_backend::PackagedFontBackend;

    fn size() -> GeometrySize<LogicalPixels> {
        GeometrySize::<LogicalPixels>::from_px(640.0, 400.0).unwrap()
    }

    #[test]
    fn bootstrap_frame_publishes_exact_packaged_glyph_bindings() {
        let mut builder =
            PortableBootstrapFrameBuilder::with_font_backend(Box::new(PackagedFontBackend));
        let frame = builder.build(size()).expect("packaged bootstrap frame");

        assert_eq!(
            frame.default_resolved_font().unwrap().identity.backend,
            FontBackendKind::Packaged
        );
        for glyph in &frame.glyphs {
            let FrameGlyph::Char { char, face_id, .. } = glyph else {
                continue;
            };
            let binding = frame.char_fonts[face_id][char];
            let font = &frame.fonts[&binding.resolved_font_id];
            assert!(font.replay.outline_asset().unwrap().bytes().is_some());
        }
    }

    #[test]
    fn bootstrap_frame_uses_typed_requested_extent() {
        let mut builder =
            PortableBootstrapFrameBuilder::with_font_backend(Box::new(PackagedFontBackend));
        let frame = builder.build(size()).expect("packaged bootstrap frame");

        assert_eq!((frame.width, frame.height), (640.0, 400.0));
        assert!(
            frame
                .glyphs
                .iter()
                .any(|glyph| matches!(glyph, FrameGlyph::Char { .. }))
        );
    }
}
