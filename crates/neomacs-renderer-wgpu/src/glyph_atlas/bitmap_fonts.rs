//! Renderer-side replay of layout-opened fixed bitmap fonts.
//!
//! Semantic selection belongs to layout. This module only reopens the exact
//! identity and strike recorded in [`ResolvedFont::replay`], rasterizes one
//! glyph, and converts its pixels into the atlas contract.

use std::collections::HashMap;

use neomacs_display_protocol::font::{
    FontReplay, GlyphSampling, ResolvedFont, ResolvedFontIdentity, ResolvedGlyphId,
};
use neomacs_display_protocol::geometry::DeviceScale;
use neomacs_font_materializer::{
    FontMaterializationError, FontMaterializer, FontOpenRequest, OpenedFont, RasterPixels,
};

use super::{GlyphPixelKind, RasterizeResult};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct BitmapFontReplayKey {
    identity: ResolvedFontIdentity,
    replay: FontReplay,
}

/// Open fixed-strike faces keyed by their durable identity and replay plan.
///
/// The key deliberately excludes the frame-local `ResolvedFontId`: two
/// frames naming the same exact face/strike should share one FreeType face,
/// while an id reused for another identity cannot alias the cache.
pub(super) struct BitmapFontReplayCache {
    materializer: FontMaterializer,
    opened: HashMap<BitmapFontReplayKey, OpenedFont>,
}

impl BitmapFontReplayCache {
    pub(super) fn new() -> Result<Self, FontMaterializationError> {
        Ok(Self {
            materializer: FontMaterializer::new()?,
            opened: HashMap::new(),
        })
    }

    pub(super) fn rasterize_char(
        &mut self,
        font: &ResolvedFont,
        ch: char,
    ) -> Result<Option<RasterizeResult>, FontMaterializationError> {
        let sampling = replay_sampling(&font.replay)?;
        let opened = self.opened_font(font)?;
        let Some(glyph) = opened.glyph_for_char(ch) else {
            return Ok(None);
        };
        Self::rasterize_opened_glyph(opened, glyph, sampling).map(Some)
    }

    pub(super) fn rasterize_glyph(
        &mut self,
        font: &ResolvedFont,
        glyph: ResolvedGlyphId,
    ) -> Result<RasterizeResult, FontMaterializationError> {
        let sampling = replay_sampling(&font.replay)?;
        let opened = self.opened_font(font)?;
        Self::rasterize_opened_glyph(opened, glyph, sampling)
    }

    fn opened_font(
        &mut self,
        font: &ResolvedFont,
    ) -> Result<&OpenedFont, FontMaterializationError> {
        if !matches!(&font.replay, FontReplay::FreeTypeBitmap { .. }) {
            return Err(FontMaterializationError::ReplayMethodMismatch);
        }
        let key = BitmapFontReplayKey {
            identity: font.identity.clone(),
            replay: font.replay.clone(),
        };
        if !self.opened.contains_key(&key) {
            let FontReplay::FreeTypeBitmap { asset, spacing, .. } = &font.replay else {
                return Err(FontMaterializationError::ReplayMethodMismatch);
            };
            let opened = self.materializer.reopen(
                FontOpenRequest {
                    asset,
                    requested_layout_px: font.pixel_size,
                    // Replay names a physical strike. Reopening at scale one
                    // keeps glyph pixels and bearings in device-pixel space.
                    device_scale: DeviceScale::new(1.0)
                        .expect("one is always a valid device scale"),
                    selected_device_ppem_26_6: None,
                    line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
                    spacing: *spacing,
                },
                font.replay.clone(),
            )?;
            self.opened.insert(key.clone(), opened);
        }
        Ok(self
            .opened
            .get(&key)
            .expect("inserted exact bitmap font must be present"))
    }

    fn rasterize_opened_glyph(
        opened: &OpenedFont,
        glyph: ResolvedGlyphId,
        sampling: GlyphSampling,
    ) -> Result<RasterizeResult, FontMaterializationError> {
        let glyph = opened.rasterize(glyph)?;
        let sampling = bitmap_pixel_sampling(&glyph.pixels, sampling);
        let (pixel_data, pixel_kind, sampling) = match glyph.pixels {
            RasterPixels::Mask8(mask) => (mask, GlyphPixelKind::AlphaMask, sampling),
            RasterPixels::Bgra8(bgra) => {
                let mut rgba = bgra;
                for pixel in rgba.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                // Color bitmap glyphs are antialiased images, not pixel-art
                // masks. Keep the same linear sampling used by color outline
                // glyphs even when the containing face is fixed-size.
                (rgba, GlyphPixelKind::ColorRgba, sampling)
            }
        };

        Ok(RasterizeResult {
            width: glyph.width_px,
            height: glyph.height_px,
            pixel_data,
            bearing_x: glyph.left_px as f32,
            bearing_y: glyph.top_px as f32,
            pixel_kind,
            advance_width: glyph.advance_px,
            sampling,
        })
    }
}

pub(super) fn bitmap_pixel_sampling(
    pixels: &RasterPixels,
    mask_sampling: GlyphSampling,
) -> GlyphSampling {
    match pixels {
        RasterPixels::Mask8(_) => mask_sampling,
        RasterPixels::Bgra8(_) => GlyphSampling::Linear,
    }
}

fn replay_sampling(replay: &FontReplay) -> Result<GlyphSampling, FontMaterializationError> {
    match replay {
        FontReplay::FreeTypeBitmap { sampling, .. } => Ok(*sampling),
        FontReplay::Swash { .. } => Err(FontMaterializationError::ReplayMethodMismatch),
    }
}
