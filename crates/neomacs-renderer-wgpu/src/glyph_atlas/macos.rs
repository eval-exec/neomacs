//! Native outline masks, replaying the exact font bytes already pinned by layout.
//!
//! Core Graphics font smoothing affects stroke weight even for grayscale text.
//! Swash's outline coverage is not a substitute for that macOS drawing policy.
//! Keep the native objects on the atlas owner and upload ordinary grayscale
//! masks, so WGPU composition and layout advances remain shared.

use super::{RasterContent, RasterGlyphImage};
use cosmic_text::{CacheKey, CacheKeyFlags, Font};
use objc2_core_foundation::{CFArray, CFData, CFRetained, CGPoint};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGImageByteOrderInfo,
};
use objc2_core_text::{
    CTFont, CTFontDescriptor, CTFontManagerCreateFontDescriptorsFromData, CTFontOrientation,
};
use rustc_hash::FxHashMap;
use std::ptr::{self, NonNull};
use swash::TableProvider;

#[derive(Default)]
pub(super) struct NativeRasterizer {
    // fontdb IDs are generation-local. The atlas clears this cache whenever
    // its source database or scale changes. Cache failures as well as fonts.
    sources: FxHashMap<fontdb::ID, Option<CFRetained<CTFont>>>,
    fonts: FxHashMap<(fontdb::ID, u32), CFRetained<CTFont>>,
}

impl NativeRasterizer {
    pub(super) fn clear(&mut self) {
        self.fonts.clear();
        self.sources.clear();
    }

    pub(super) fn rasterize(
        &mut self,
        source: &Font,
        postscript_name: &str,
        key: CacheKey,
    ) -> Option<RasterGlyphImage> {
        let face = source.as_swash();
        // Keep variable instances, synthetic styles and embedded strikes on
        // their existing paths until the native adapter can replay their full
        // contract. Color artwork has already been handled by glyph_image.
        if face.variations().next().is_some()
            || key.flags.contains(CacheKeyFlags::FAKE_ITALIC)
            || key.flags.contains(CacheKeyFlags::PIXEL_FONT)
            || [*b"sbix", *b"CBDT", *b"EBDT", *b"COLR", *b"SVG "]
                .iter()
                .any(|tag| face.table_by_tag(u32::from_be_bytes(*tag)).is_some())
        {
            return None;
        }
        let size = f32::from_bits(key.font_size_bits);
        if !size.is_finite() || size <= 0.0 {
            return None;
        }
        // Copy a collection's bytes only once, not once for every text scale.
        let base = self
            .sources
            .entry(key.font_id)
            .or_insert_with(|| open_exact_font(source.data(), postscript_name, 1.0))
            .as_ref()?;
        let font = self
            .fonts
            .entry((key.font_id, key.font_size_bits))
            .or_insert_with(|| {
                // SAFETY: null preserves the base matrix; no attributes are
                // overridden, so the copied font retains the same exact source.
                unsafe { base.copy_with_attributes(size.into(), ptr::null(), None) }
            });
        rasterize(font, key)
    }
}

fn open_exact_font(bytes: &[u8], postscript_name: &str, size: f32) -> Option<CFRetained<CTFont>> {
    let data = CFData::from_bytes(bytes);
    // SAFETY: CoreText returns an array of CTFontDescriptor objects, retained
    // here and by each typed get. Descriptors refer to these bytes, not to a
    // system family lookup (which could select another installed font).
    let descriptors = unsafe {
        CFRetained::cast_unchecked::<CFArray<CTFontDescriptor>>(
            CTFontManagerCreateFontDescriptorsFromData(&data),
        )
    };
    for index in 0..descriptors.len() {
        let descriptor = descriptors.get(index)?;
        // SAFETY: null requests the identity transform; the descriptor is live.
        let font = unsafe { CTFont::with_font_descriptor(&descriptor, size.into(), ptr::null()) };
        // Select the exact collection member. Never substitute a best-match
        // family when the selected face is missing from these bytes.
        if unsafe { font.post_script_name() }.to_string() == postscript_name {
            return Some(font);
        }
    }
    None
}

fn rasterize(font: &CTFont, key: CacheKey) -> Option<RasterGlyphImage> {
    let mut glyph = key.glyph_id;
    // SAFETY: one live glyph, no per-glyph output array, count is one.
    let bounds = unsafe {
        font.bounding_rects_for_glyphs(
            CTFontOrientation::Horizontal,
            NonNull::from(&mut glyph),
            ptr::null_mut(),
            1,
        )
    };
    let x_offset = f64::from(key.x_bin.as_float());
    // Atlas y bins increase downwards, Core Graphics coordinates upwards.
    let y_offset = -f64::from(key.y_bin.as_float());
    if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
        return Some(RasterGlyphImage {
            left: 0,
            top: 0,
            width: 0,
            height: 0,
            data: Vec::new(),
            content: RasterContent::Mask,
        });
    }
    // Leave room for native smoothing beyond the mathematical outline bounds.
    let left = (bounds.origin.x + x_offset).floor() - 2.0;
    let bottom = (bounds.origin.y + y_offset).floor() - 2.0;
    let right = (bounds.origin.x + bounds.size.width + x_offset).ceil() + 2.0;
    let top = (bounds.origin.y + bounds.size.height + y_offset).ceil() + 2.0;
    let width = right - left;
    let height = top - bottom;
    if ![left, bottom, right, top, width, height]
        .iter()
        .all(|v| v.is_finite())
        || width <= 0.0
        || height <= 0.0
        || width > 8192.0
        || height > 8192.0
        || left < i32::MIN as f64
        || left > i32::MAX as f64
        || top < i32::MIN as f64
        || top > i32::MAX as f64
    {
        return None;
    }
    let width = width as usize;
    let height = height as usize;
    let mut pixels = vec![0u8; width.checked_mul(height)?.checked_mul(4)?];
    let colorspace = CGColorSpace::new_device_rgb()?;
    // An opaque RGB bitmap enables native font smoothing. Alpha-only contexts
    // disable that treatment and recreate the thin strokes this adapter fixes.
    // SAFETY: pixels owns width*height*4 writable bytes and outlives the context.
    let context = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * 4,
            Some(&colorspace),
            CGImageByteOrderInfo::Order32Big.0 | CGImageAlphaInfo::NoneSkipLast.0,
        )
    }?;
    CGContext::set_should_antialias(Some(&context), true);
    CGContext::set_allows_font_smoothing(Some(&context), true);
    CGContext::set_should_smooth_fonts(Some(&context), true);
    CGContext::set_allows_font_subpixel_positioning(Some(&context), true);
    CGContext::set_should_subpixel_position_fonts(Some(&context), true);
    CGContext::set_should_subpixel_quantize_fonts(Some(&context), false);
    CGContext::set_rgb_fill_color(Some(&context), 1.0, 1.0, 1.0, 1.0);
    // Match GNU macfont's ordinary-outline drawing operation. The font size
    // already includes backing scale; do not scale the context a second time.
    let graphics_font = unsafe { font.graphics_font(ptr::null_mut()) };
    CGContext::set_font(Some(&context), Some(&graphics_font));
    CGContext::set_font_size(Some(&context), f32::from_bits(key.font_size_bits).into());
    let position = CGPoint {
        x: x_offset - left,
        y: y_offset - bottom,
    };
    // SAFETY: both pointers address one live element for this synchronous call.
    unsafe { CGContext::show_glyphs_at_positions(Some(&context), &glyph, &position, 1) };
    drop(context);
    // Bitmap rows are top-down. Collapse any OS-dependent channel differences
    // to grayscale instead of assuming a physical LCD component order.
    let data: Vec<u8> = pixels
        .chunks_exact(4)
        .map(|p| ((u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2]) + 1) / 3) as u8)
        .collect();
    // Published bitmap bounds are also used as ink bounds by clipping and
    // overlap diagnostics. Remove the transparent smoothing guard band while
    // retaining every nonzero antialiased pixel.
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    for (index, &coverage) in data.iter().enumerate() {
        if coverage != 0 {
            let x = index % width;
            let y = index / width;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x == width {
        return Some(RasterGlyphImage {
            left: 0,
            top: 0,
            width: 0,
            height: 0,
            data: Vec::new(),
            content: RasterContent::Mask,
        });
    }
    let ink_width = max_x - min_x + 1;
    let ink_height = max_y - min_y + 1;
    let mut ink = Vec::with_capacity(ink_width * ink_height);
    for y in min_y..=max_y {
        ink.extend_from_slice(&data[y * width + min_x..y * width + max_x + 1]);
    }
    Some(RasterGlyphImage {
        left: left as i32 + min_x as i32,
        top: top as i32 - min_y as i32,
        width: ink_width as u32,
        height: ink_height as u32,
        data: ink,
        content: RasterContent::Mask,
    })
}

#[cfg(test)]
#[path = "tests/macos_test.rs"]
mod tests;
