//! Linux outline masks with the selected font's Fontconfig rendering policy.
//!
//! GNU ftcrfont_open prepares a Fontconfig pattern and hands it to Cairo;
//! ftfont_fix_match preserves its hinting, hintstyle and LCD options. Swash's
//! fixed hinting target cannot reproduce FT_LOAD_TARGET_LIGHT. Keep native
//! rasterization on the atlas owner and retain the shared layout advances.

use super::{GlyphRenderMode, RasterContent, RasterGlyphImage};
use cosmic_text::{CacheKey, CacheKeyFlags, Font};
use fontconfig_sys as fc;
use freetype::{
    Face, LcdFilter, Library, Matrix, RenderMode, Vector, bitmap::PixelMode, face::LoadFlag,
};
use rustc_hash::FxHashMap;
use std::{
    borrow::Borrow,
    ffi::{CStr, CString},
    os::unix::ffi::OsStrExt,
    ptr::{self, NonNull},
    sync::Arc,
};
use swash::TableProvider;

const MAX_PIXELS: usize = 4 * 1024 * 1024;

// Fontconfig has provided this conversion since 2.11.91, but the raw binding
// crate does not expose it. Use the native table rather than a CSS approximation.
unsafe extern "C" {
    fn FcWeightFromOpenType(weight: i32) -> i32;
}

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
}

impl Default for NativeRasterizer {
    fn default() -> Self {
        Self {
            library: Library::init().ok(),
            faces: FxHashMap::default(),
            options: FxHashMap::default(),
        }
    }
}

impl NativeRasterizer {
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
            .or_insert_with(|| RenderOptions::for_face(info, size))
            .as_ref()?;
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
        let render_mode = options.render_mode(mode);
        if matches!(render_mode, RenderMode::Lcd) {
            library.set_lcd_filter(options.lcd_filter.native()).ok()?;
        }
        face.load_glyph(
            u32::from(key.glyph_id),
            options.load_flags(key.flags.contains(CacheKeyFlags::DISABLE_HINTING), mode),
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
        // FreeType defines pitch as the signed offset to the next logical row;
        // copy_rows validates row sizes before reading and returns owned bytes.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hinting {
    None,
    Slight,
    Medium,
    Full,
}

#[derive(Clone, Copy, Debug)]
enum Filter {
    None,
    Default,
    Light,
    Legacy,
}

impl Filter {
    fn native(self) -> LcdFilter {
        match self {
            Self::None => LcdFilter::LcdFilterNone,
            Self::Default => LcdFilter::LcdFilterDefault,
            Self::Light => LcdFilter::LcdFilterLight,
            Self::Legacy => LcdFilter::LcdFilterLegacy,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RenderOptions {
    hinting: Hinting,
    antialias: bool,
    autohint: bool,
    horizontal_lcd: bool,
    lcd_filter: Filter,
}

impl RenderOptions {
    fn for_face(info: &fontdb::FaceInfo, size: f32) -> Option<Self> {
        // SAFETY: Fontconfig initializes its process-global configuration;
        // no ownership is taken and this adapter never calls FcFini.
        if unsafe { fc::FcInit() } == 0 {
            return None;
        }
        let pattern = Pattern::new()?;
        for (family, _) in &info.families {
            pattern.string(c"family", &CString::new(family.as_str()).ok()?)?;
        }
        if !info.post_script_name.is_empty() {
            pattern.string(
                c"postscriptname",
                &CString::new(info.post_script_name.as_str()).ok()?,
            )?;
        }
        match &info.source {
            fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => {
                pattern.string(c"file", &CString::new(path.as_os_str().as_bytes()).ok()?)?;
            }
            fontdb::Source::Binary(_) => {}
        }
        pattern.integer(c"index", i32::try_from(info.index).ok()?)?;
        pattern.double(c"pixelsize", size.into())?;
        // SAFETY: conversion is a pure Fontconfig numeric operation.
        let weight = unsafe { FcWeightFromOpenType(i32::from(info.weight.0)) };
        pattern.integer(c"weight", weight)?;
        pattern.integer(
            c"width",
            match info.stretch.to_number() {
                1 => 50,
                2 => 63,
                3 => 75,
                4 => 87,
                5 => 100,
                6 => 113,
                7 => 125,
                8 => 150,
                9 => 200,
                _ => return None,
            },
        )?;
        pattern.integer(
            c"slant",
            match info.style {
                fontdb::Style::Normal => 0,
                fontdb::Style::Italic => 100,
                fontdb::Style::Oblique => 110,
            },
        )?;
        // SAFETY: pattern is live and owned; null config means Fontconfig's
        // current configuration. Defaults/substitution do not open our font.
        unsafe {
            if fc::FcConfigSubstitute(ptr::null_mut(), pattern.raw(), fc::FcMatchPattern) == 0 {
                return None;
            }
            fc::FcDefaultSubstitute(pattern.raw());
        }
        let mut result = fc::FcResultNoMatch;
        // SAFETY: a successful match returns a separately owned pattern. It
        // supplies rendering options only: font bytes/index above remain exact.
        let matched = Pattern(NonNull::new(unsafe {
            fc::FcFontMatch(ptr::null_mut(), pattern.raw(), &mut result)
        })?);
        if result != fc::FcResultMatch {
            return None;
        }
        // Reproduce ftfont_fix_match's rendering-only overrides. In particular,
        // the query's false hinting/antialias and explicit hintstyle survive a
        // matched font whose intrinsic properties have different defaults.
        let flag = |name, default| {
            if pattern.boolean(name) == Some(false) {
                false
            } else {
                matched.boolean(name).unwrap_or(default)
            }
        };
        let integer = |name| {
            pattern
                .get_integer(name)
                .or_else(|| matched.get_integer(name))
        };
        // Matrix synthesis and emboldening require another replay contract.
        if pattern.boolean(c"embolden") == Some(true)
            || matched.boolean(c"embolden") == Some(true)
            || pattern.has_transform()
            || matched.has_transform()
        {
            return None;
        }
        let hinting = if !flag(c"hinting", true) {
            Hinting::None
        } else {
            match integer(c"hintstyle").unwrap_or(3) {
                0 => Hinting::None,
                1 => Hinting::Slight,
                2 => Hinting::Medium,
                3 => Hinting::Full,
                _ => return None,
            }
        };
        Some(Self {
            hinting,
            antialias: flag(c"antialias", true),
            // Unlike hinting/hintstyle, ftfont_fix_match does not override
            // the prepared font's autohint value with the query's value.
            autohint: matched.boolean(c"autohint").unwrap_or(false),
            horizontal_lcd: match integer(c"rgba").unwrap_or(0) {
                0..=2 => true,
                3..=5 => false,
                _ => return None,
            },
            lcd_filter: match integer(c"lcdfilter").unwrap_or(1) {
                0 => Filter::None,
                1 => Filter::Default,
                2 => Filter::Light,
                3 => Filter::Legacy,
                _ => return None,
            },
        })
    }

    fn load_flags(self, disable_hinting: bool, requested: GlyphRenderMode) -> LoadFlag {
        let mut flags = LoadFlag::NO_BITMAP;
        if disable_hinting || self.hinting == Hinting::None {
            flags |= LoadFlag::NO_HINTING;
        } else {
            flags |= if !self.antialias {
                LoadFlag::TARGET_MONO
            } else if self.hinting == Hinting::Slight {
                LoadFlag::TARGET_LIGHT
            } else if self.hinting == Hinting::Full
                && matches!(self.render_mode(requested), RenderMode::Lcd)
            {
                // Cairo 1.18.4 _cairo_ft_options_merge distinguishes medium
                // (NORMAL) from full hinting for horizontal LCD masks.
                LoadFlag::TARGET_LCD
            } else {
                LoadFlag::TARGET_NORMAL
            };
            if self.autohint {
                flags |= LoadFlag::FORCE_AUTOHINT;
            }
        }
        flags
    }

    fn render_mode(self, requested: GlyphRenderMode) -> RenderMode {
        if !self.antialias {
            RenderMode::Mono
        } else if requested == GlyphRenderMode::Subpixel && self.horizontal_lcd {
            RenderMode::Lcd
        } else {
            RenderMode::Normal
        }
    }
}

struct Pattern(NonNull<fc::FcPattern>);

impl Pattern {
    fn new() -> Option<Self> {
        // SAFETY: the returned allocation is owned by this RAII wrapper.
        NonNull::new(unsafe { fc::FcPatternCreate() }).map(Self)
    }

    fn raw(&self) -> *mut fc::FcPattern {
        self.0.as_ptr()
    }

    fn string(&self, name: &CStr, value: &CStr) -> Option<()> {
        // SAFETY: all pointers are live; Fontconfig copies the string value.
        (unsafe { fc::FcPatternAddString(self.raw(), name.as_ptr(), value.as_ptr().cast()) } != 0)
            .then_some(())
    }

    fn integer(&self, name: &CStr, value: i32) -> Option<()> {
        // SAFETY: live pattern and terminated property name, copied value.
        (unsafe { fc::FcPatternAddInteger(self.raw(), name.as_ptr(), value) } != 0).then_some(())
    }

    fn double(&self, name: &CStr, value: f64) -> Option<()> {
        // SAFETY: live pattern and terminated property name, copied value.
        (unsafe { fc::FcPatternAddDouble(self.raw(), name.as_ptr(), value) } != 0).then_some(())
    }

    fn get_integer(&self, name: &CStr) -> Option<i32> {
        let mut value = 0;
        // SAFETY: live pattern and property name; output is writable.
        (unsafe { fc::FcPatternGetInteger(self.raw(), name.as_ptr(), 0, &mut value) }
            == fc::FcResultMatch)
            .then_some(value)
    }

    fn boolean(&self, name: &CStr) -> Option<bool> {
        let mut value = 0;
        // SAFETY: live pattern and property name; output is writable.
        if unsafe { fc::FcPatternGetBool(self.raw(), name.as_ptr(), 0, &mut value) }
            != fc::FcResultMatch
        {
            return None;
        }
        match value {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn has_transform(&self) -> bool {
        let mut matrix = ptr::null_mut();
        // SAFETY: the returned matrix is borrowed from this still-live pattern.
        if unsafe { fc::FcPatternGetMatrix(self.raw(), c"matrix".as_ptr(), 0, &mut matrix) }
            != fc::FcResultMatch
        {
            return false;
        }
        let Some(matrix) = NonNull::new(matrix) else {
            return true;
        };
        // SAFETY: Fontconfig returned a non-null matrix owned by self.
        let matrix = unsafe { matrix.as_ref() };
        matrix.xx != 1.0 || matrix.yy != 1.0 || matrix.xy != 0.0 || matrix.yx != 0.0
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        // SAFETY: exactly one wrapper owns this pattern; no borrow survives it.
        unsafe { fc::FcPatternDestroy(self.raw()) };
    }
}

/// Copy logical top-to-bottom rows, discarding pitch padding.
///
/// # Safety
/// `buffer` must name the first logical row of a live FreeType bitmap, and
/// every row at the signed `pitch` offset must contain `row_bytes` valid bytes.
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
    if buffer.is_null() || usize::try_from(pitch.unsigned_abs()).ok()? < row_bytes {
        return None;
    }
    if length > MAX_PIXELS * 4 {
        return None;
    }
    let mut data = Vec::with_capacity(length);
    for row in 0..height {
        let offset = isize::try_from(row).ok()?.checked_mul(pitch as isize)?;
        // SAFETY: the caller guarantees each row at this signed offset. Read
        // only row_bytes, not abs(pitch), so padding is neither read nor copied.
        let bytes = unsafe { std::slice::from_raw_parts(buffer.offset(offset), row_bytes) };
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
