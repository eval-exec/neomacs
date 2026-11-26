//! Linux outline masks with the selected font's Fontconfig rendering policy.
//!
//! GNU ftcrfont_open prepares a Fontconfig pattern and hands it to Cairo;
//! ftfont_fix_match preserves its hinting, hintstyle and LCD options. Swash's
//! fixed hinting target cannot reproduce FT_LOAD_TARGET_LIGHT. Keep native
//! rasterization on the atlas owner and retain the shared layout advances.

use super::{GlyphRenderMode, RasterContent, RasterGlyphImage};
use cosmic_text::{CacheKey, CacheKeyFlags, Font};
use freetype::{Face, Library, Matrix, RenderMode, Vector, bitmap::PixelMode};
use neomacs_font_materializer::fontconfig::{
    FontRenderMode, FontRenderOptions as RenderOptions, FontconfigContext, SurfaceFontOptions,
};
use rustc_hash::FxHashMap;
use std::{borrow::Borrow, sync::Arc};
use swash::TableProvider;

const MAX_PIXELS: usize = 4 * 1024 * 1024;

struct FontBacking(Arc<Font>);

impl Borrow<[u8]> for FontBacking {
    fn borrow(&self) -> &[u8] {
        self.0.data()
    }
}

struct OpenedFace {
    face: Face<FontBacking>,
    size_26_6: Option<isize>,
}

pub(super) struct NativeRasterizer {
    library: Option<Library>,
    // Face retains the exact cosmic Font, which owns its immutable backing.
    // No font-data copy or independent file lookup occurs when opening a face.
    faces: FxHashMap<fontdb::ID, Option<OpenedFace>>,
    options: FxHashMap<(fontdb::ID, u32), Option<RenderOptions>>,
    surface_options: SurfaceFontOptions,
}

impl Default for NativeRasterizer {
    fn default() -> Self {
        Self {
            library: Library::init().ok(),
            faces: FxHashMap::default(),
            options: FxHashMap::default(),
            surface_options: SurfaceFontOptions::default(),
        }
    }
}

impl NativeRasterizer {
    pub(super) fn surface_options(&self) -> SurfaceFontOptions {
        self.surface_options
    }

    /// Return whether cached masks/options must be invalidated by the owner.
    pub(super) fn set_surface_options(&mut self, options: SurfaceFontOptions) -> bool {
        if self.surface_options == options {
            return false;
        }
        self.surface_options = options;
        self.clear();
        true
    }

    pub(super) fn clear(&mut self) {
        self.faces.clear();
        self.options.clear();
    }

    pub(super) fn rasterize(
        &mut self,
        source: &Arc<Font>,
        info: &fontdb::FaceInfo,
        key: CacheKey,
        mode: GlyphRenderMode,
    ) -> Option<RasterGlyphImage> {
        let swash_face = source.as_swash();
        // These sources need their existing variation, synthesis or artwork
        // replay contracts. A monochrome outline must not replace that ink.
        if swash_face.variations().next().is_some()
            || key.flags.contains(CacheKeyFlags::FAKE_ITALIC)
            || key.flags.contains(CacheKeyFlags::PIXEL_FONT)
            || [*b"sbix", *b"CBDT", *b"EBDT", *b"COLR", *b"SVG "]
                .iter()
                .any(|tag| swash_face.table_by_tag(u32::from_be_bytes(*tag)).is_some())
        {
            return None;
        }
        let size = f32::from_bits(key.font_size_bits);
        let size_26_6 = (f64::from(size) * 64.0).round();
        if !size.is_finite() || size <= 0.0 || size > 8192.0 || size_26_6 < 1.0 {
            return None;
        }
        let options = *self
            .options
            .entry((key.font_id, key.font_size_bits))
            .or_insert_with(|| FontconfigContext::system()?.options_for_face(info, size))
            .as_ref()?;
        let options = options.for_surface(self.surface_options);
        let library = self.library.as_ref()?;
        let opened = self
            .faces
            .entry(key.font_id)
            .or_insert_with(|| {
                let face = library
                    .new_memory_face2(
                        FontBacking(Arc::clone(source)),
                        isize::try_from(info.index).ok()?,
                    )
                    .ok()?;
                (face.is_scalable() && !face.has_fixed_sizes() && !face.has_color()).then_some(
                    OpenedFace {
                        face,
                        size_26_6: None,
                    },
                )
            })
            .as_mut()?;
        // The cache key size already includes backing scale. 72 DPI makes
        // the 26.6 character size a physical pixel size without a second scale.
        let size_26_6 = size_26_6 as isize;
        if opened.size_26_6 != Some(size_26_6) {
            opened.face.set_char_size(0, size_26_6, 72, 72).ok()?;
            opened.size_26_6 = Some(size_26_6);
        }
        let face = &opened.face;
        let mut matrix = Matrix {
            xx: 65536,
            xy: 0,
            yx: 0,
            yy: 65536,
        };
        // CacheKey bins use the Swash/native-adapter convention: positive y
        // moves ink upwards, just as FreeType's outline coordinates do.
        let mut delta = Vector {
            x: (key.x_bin.as_float() * 64.0).round() as _,
            y: (key.y_bin.as_float() * 64.0).round() as _,
        };
        face.set_transform(&mut matrix, &mut delta);
        let requested = match mode {
            GlyphRenderMode::Alpha => FontRenderMode::Alpha,
            GlyphRenderMode::Subpixel => FontRenderMode::Subpixel,
        };
        let render_mode = options.render_mode(requested);
        if matches!(render_mode, RenderMode::Lcd) {
            library.set_lcd_filter(options.lcd_filter()).ok()?;
        }
        face.load_glyph(
            u32::from(key.glyph_id),
            options.load_flags(
                key.flags.contains(CacheKeyFlags::DISABLE_HINTING),
                requested,
            ),
        )
        .ok()?;
        face.glyph().render_glyph(render_mode).ok()?;
        let slot = face.glyph();
        let bitmap = slot.bitmap();
        let width = usize::try_from(bitmap.width()).ok()?;
        let height = usize::try_from(bitmap.rows()).ok()?;
        let pixel_mode = bitmap.pixel_mode().ok()?;
        let (physical_width, row_bytes) = match pixel_mode {
            PixelMode::Gray => (width, width),
            PixelMode::Mono => (width, width.checked_add(7)? / 8),
            PixelMode::Lcd if width % 3 == 0 => (width / 3, width),
            // Empty glyphs can have no pixel format and a null buffer.
            PixelMode::None if width == 0 || height == 0 => (width, 0),
            _ => return None,
        };
        if physical_width.checked_mul(height)? > MAX_PIXELS {
            return None;
        }
        // SAFETY: this live glyph slot owns the bitmap until the next load.
        // FreeType stores `buffer` at the lowest memory address of the
        // `height * |pitch|`-byte allocation, with negative `pitch` indicating
        // bottom-up row storage; `copy_rows` bounds every access to that span.
        let rows = unsafe { copy_rows(bitmap.raw().buffer, bitmap.pitch(), height, row_bytes) }?;
        let (data, content) = match pixel_mode {
            PixelMode::Gray => (
                normalize_gray(rows, u32::try_from(bitmap.raw().num_grays).ok()?)?,
                RasterContent::Mask,
            ),
            PixelMode::Mono => (unpack_mono(&rows, width, height)?, RasterContent::Mask),
            PixelMode::Lcd => (pack_lcd(&rows)?, RasterContent::SubpixelMask),
            PixelMode::None => (Vec::new(), RasterContent::Mask),
            _ => return None,
        };
        Some(RasterGlyphImage {
            left: slot.bitmap_left(),
            top: slot.bitmap_top(),
            width: u32::try_from(physical_width).ok()?,
            height: u32::try_from(height).ok()?,
            data,
            content,
        })
    }
}

/// Copy logical top-to-bottom rows, discarding pitch padding.
///
/// # Safety
/// `buffer` must point to the lowest memory address of a live FreeType bitmap
/// covering `height * |pitch|` readable bytes (`FT_Bitmap.buffer`). When
/// `pitch >= 0` row 0 starts at `buffer`; when `pitch < 0` (up-flow) row 0
/// starts at `buffer + (height - 1) * |pitch|` and row `height - 1` starts at
/// `buffer`.
unsafe fn copy_rows(
    buffer: *const u8,
    pitch: i32,
    height: usize,
    row_bytes: usize,
) -> Option<Vec<u8>> {
    let length = height.checked_mul(row_bytes)?;
    if length == 0 {
        return Some(Vec::new());
    }
    let stride = usize::try_from(pitch.unsigned_abs()).ok()?;
    if buffer.is_null() || stride < row_bytes {
        return None;
    }
    if length > MAX_PIXELS * 4 {
        return None;
    }
    let total_bytes = height.checked_mul(stride)?;
    if total_bytes > isize::MAX as usize {
        return None;
    }
    let mut data = Vec::with_capacity(length);
    for row in 0..height {
        let physical_row = if pitch < 0 { height - 1 - row } else { row };
        let offset = physical_row.checked_mul(stride)?;
        // SAFETY: `physical_row < height` and `row_bytes <= stride`, so
        // `offset + row_bytes <= total_bytes <= isize::MAX` stays inside the
        // `height * |pitch|`-byte FreeType bitmap buffer rooted at `buffer`.
        let bytes = unsafe { std::slice::from_raw_parts(buffer.add(offset), row_bytes) };
        data.extend_from_slice(bytes);
    }
    Some(data)
}

fn normalize_gray(mut bytes: Vec<u8>, levels: u32) -> Option<Vec<u8>> {
    if levels < 2 || levels > 256 {
        return None;
    }
    if levels != 256 {
        let maximum = levels - 1;
        for byte in &mut bytes {
            if u32::from(*byte) > maximum {
                return None;
            }
            *byte = ((u32::from(*byte) * 255 + maximum / 2) / maximum) as u8;
        }
    }
    Some(bytes)
}

fn unpack_mono(rows: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    let stride = width.checked_add(7)? / 8;
    if rows.len() != stride.checked_mul(height)? {
        return None;
    }
    let mut mask = Vec::with_capacity(width.checked_mul(height)?);
    for row in rows.chunks_exact(stride.max(1)) {
        mask.extend((0..width).map(|x| {
            if row[x / 8] & (0x80 >> (x % 8)) != 0 {
                255
            } else {
                0
            }
        }));
    }
    Some(mask)
}

fn pack_lcd(rgb: &[u8]) -> Option<Vec<u8>> {
    if rgb.len() % 3 != 0 {
        return None;
    }
    let mut rgba = Vec::with_capacity(rgb.len().checked_div(3)?.checked_mul(4)?);
    for pixel in rgb.chunks_exact(3) {
        // Fontconfig's RGB/BGR order is applied once by the atlas after this
        // adapter returns natural FreeType R,G,B samples.
        rgba.extend_from_slice(&[
            pixel[0],
            pixel[1],
            pixel[2],
            pixel[0].max(pixel[1]).max(pixel[2]),
        ]);
    }
    Some(rgba)
}

#[cfg(test)]
#[path = "tests/linux_test.rs"]
mod tests;
