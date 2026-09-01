//! Text shaper trait (render-boundary design §8).
//!
//! Hides the shaping engine behind one seam so the cosmic-text machinery is
//! an implementation detail, not the architectural contract: a
//! HarfBuzz/rustybuzz implementation over exact font bytes can replace
//! [`CosmicTextShaper`] without touching the resolver or the display
//! protocol. Consumers receive [`ShapedGlyph`]s (glyph ids + clusters +
//! advances); the conversion to durable resolved-font identities happens in
//! `FontMetricsService::resolved_glyphs_for_cluster`.

use crate::font::metrics::ShapedGlyph;
use cosmic_text::{Attrs, Buffer, FontSystem};

pub trait TextShaper: Send {
    /// Shape `text` as one unbounded run with the given attributes and
    /// return its glyphs in visual order.
    fn shape_run(
        &mut self,
        font_system: &mut FontSystem,
        text: &str,
        attrs: &Attrs<'static>,
        font_size: f32,
        line_height: f32,
    ) -> Vec<ShapedGlyph>;
}

/// Shaper backed by cosmic-text's `Shaping::Advanced` (HarfBuzz-class).
pub struct CosmicTextShaper;

impl TextShaper for CosmicTextShaper {
    fn shape_run(
        &mut self,
        font_system: &mut FontSystem,
        text: &str,
        attrs: &Attrs<'static>,
        font_size: f32,
        line_height: f32,
    ) -> Vec<ShapedGlyph> {
        let metrics = cosmic_text::Metrics::new(font_size.max(1.0), line_height.max(1.0));
        let mut buffer = Buffer::new(font_system, metrics);
        // No width bound: lay the whole run out on a single line so shaping
        // spans the entire run instead of wrapping mid-word.
        buffer.set_size(font_system, None, None);
        buffer.set_text(
            font_system,
            text,
            attrs,
            cosmic_text::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(font_system, false);

        let mut glyphs = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let phys = glyph.physical((0.0, 0.0), 1.0);
                glyphs.push(ShapedGlyph {
                    font_id: phys.cache_key.font_id,
                    glyph_id: phys.cache_key.glyph_id,
                    x: phys.x as f32,
                    y: phys.y as f32,
                    x_advance: glyph.w,
                    cluster_start: glyph.start,
                    cluster_end: glyph.end,
                });
            }
        }
        glyphs
    }
}

/// The default shaper.
pub fn default_text_shaper() -> Box<dyn TextShaper> {
    Box::new(CosmicTextShaper)
}
