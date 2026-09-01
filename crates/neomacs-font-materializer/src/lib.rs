//! Exact-font materialization shared by layout and rendering.
//!
//! The interface is intentionally capability-driven: callers provide the
//! durable font identity selected by the platform and receive an opaque
//! opened font. File-container detection, FreeType handles, fixed-strike
//! selection, and pixel-format normalization remain implementation details.

mod fontdb;

pub use fontdb::{
    FontDbLoadOutcome, FontDbSourceError, FontFileCache, LegacyBitmapFormat, PinnedFontFace,
};

use neomacs_display_protocol::font::ResolvedGlyphId;
use neomacs_display_protocol::geometry::DeviceScale;

pub use neomacs_display_protocol::font::{
    BitmapStrikeKey, FixedFontSpacing, FontFileAsset, FontReplay, GlyphSampling,
};

#[cfg(any(unix, windows))]
use freetype::Library;
#[cfg(any(unix, windows))]
use freetype::face::LoadFlag;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontCapability {
    FreeTypeBitmap { strikes: Vec<BitmapStrikeKey> },
}

/// GNU's `:minspace` decision for fixed bitmap line metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitmapLineHeightPolicy {
    /// GNU's default: use the occupied ascent + descent and discard internal
    /// leading reported by the driver.
    GnuDefault,
    /// GNU `:minspace nil`: retain the driver's native size height.
    NativeMetrics,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontOpenRequest<'a> {
    /// The exact file face selected by platform policy. This is the only
    /// source FreeType may open; semantic identity is diagnostic metadata and
    /// deliberately does not enter the materializer API.
    pub asset: &'a FontFileAsset,
    pub requested_layout_px: f32,
    pub device_scale: DeviceScale,
    /// Native selector's already-chosen fixed entity size. When present the
    /// materializer maps that size to an exact strike; it never re-scores it.
    pub selected_device_ppem_26_6: Option<u32>,
    pub line_height: BitmapLineHeightPolicy,
    pub spacing: FixedFontSpacing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RasterPixels {
    Mask8(Vec<u8>),
    Bgra8(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RasterizedGlyph {
    pub width_px: u32,
    pub height_px: u32,
    pub left_px: i32,
    pub top_px: i32,
    pub advance_px: f32,
    pub pixels: RasterPixels,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpenedFontMetrics {
    pub requested_layout_px: f32,
    /// Actual selected strike ppem converted back into logical pixels.
    pub effective_layout_px: f32,
    pub ascent_px: f32,
    pub descent_px: f32,
    pub height_px: f32,
    pub max_advance_px: f32,
    pub space_advance_px: f32,
    pub average_advance_px: f32,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum FontMaterializationError {
    #[error("requested font size must be positive and finite")]
    InvalidRequestedSize,
    #[error("FreeType is unavailable on this target")]
    BackendUnavailable,
    #[error("FreeType initialization failed: {0}")]
    BackendInitialization(String),
    #[error("failed opening exact font face {path}: {reason}")]
    OpenFace { path: String, reason: String },
    #[error("exact font face is not a fixed-size bitmap face")]
    NotBitmapFace,
    #[error("FreeType rejected requested device pixel size {device_px}: {reason}")]
    SelectSize { device_px: u32, reason: String },
    #[error("opened bitmap face has no active size metrics")]
    MissingSizeMetrics,
    #[error("FreeType selected a bitmap size absent from the face's strike table")]
    MissingSelectedStrike,
    #[error("FreeType rejected recorded bitmap strike {index}: {reason}")]
    SelectStrike { index: u32, reason: String },
    #[error("recorded bitmap strike no longer matches the exact font identity")]
    ReplayStrikeMismatch,
    #[error("recorded bitmap asset differs from the requested exact file face")]
    ReplayAssetMismatch,
    #[error("font has no glyph for the requested character")]
    MissingGlyph,
    #[error("glyph rasterization is not implemented")]
    RasterizationUnsupported,
    #[error("FreeType failed loading glyph {glyph_id}: {reason}")]
    LoadGlyph { glyph_id: u32, reason: String },
    #[error("FreeType returned unsupported bitmap pixel mode {0}")]
    UnsupportedPixelMode(String),
    #[error("FreeType returned a malformed bitmap row: expected {expected} bytes, got {actual}")]
    MalformedBitmapBuffer { expected: usize, actual: usize },
    #[error("font replay method does not belong to this materializer")]
    ReplayMethodMismatch,
}

pub struct OpenedFont {
    replay: FontReplay,
    metrics: OpenedFontMetrics,
    device_scale: f32,
    #[cfg(any(unix, windows))]
    _face: freetype::Face,
}

impl Clone for OpenedFont {
    fn clone(&self) -> Self {
        Self {
            replay: self.replay.clone(),
            metrics: self.metrics,
            device_scale: self.device_scale,
            #[cfg(any(unix, windows))]
            _face: self._face.clone(),
        }
    }
}

impl std::fmt::Debug for OpenedFont {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenedFont")
            .field("replay", &self.replay)
            .field("metrics", &self.metrics)
            .finish_non_exhaustive()
    }
}

impl OpenedFont {
    pub fn replay(&self) -> FontReplay {
        self.replay.clone()
    }

    pub fn metrics(&self) -> OpenedFontMetrics {
        self.metrics
    }

    pub fn glyph_for_char(&self, ch: char) -> Option<ResolvedGlyphId> {
        #[cfg(any(unix, windows))]
        {
            self._face
                .get_char_index(ch as usize)
                .map(ResolvedGlyphId::new)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = ch;
            None
        }
    }

    pub fn rasterize(
        &self,
        glyph: ResolvedGlyphId,
    ) -> Result<RasterizedGlyph, FontMaterializationError> {
        #[cfg(any(unix, windows))]
        {
            self._face
                .load_glyph(glyph.get(), LoadFlag::RENDER | LoadFlag::COLOR)
                .map_err(|error| FontMaterializationError::LoadGlyph {
                    glyph_id: glyph.get(),
                    reason: error.to_string(),
                })?;
            let slot = self._face.glyph();
            let bitmap = slot.bitmap();
            let mode = bitmap.pixel_mode().map_err(|error| {
                FontMaterializationError::UnsupportedPixelMode(error.to_string())
            })?;
            let pixels = match mode {
                freetype::bitmap::PixelMode::Mono => RasterPixels::Mask8(normalize_mono_bitmap(
                    &bitmap,
                    bitmap.width() as u32,
                    bitmap.rows() as u32,
                )?),
                freetype::bitmap::PixelMode::Gray => RasterPixels::Mask8(normalize_gray_bitmap(
                    &bitmap,
                    bitmap.width() as u32,
                    bitmap.rows() as u32,
                )?),
                freetype::bitmap::PixelMode::Gray2 => {
                    RasterPixels::Mask8(normalize_packed_gray_bitmap(
                        &bitmap,
                        bitmap.width() as u32,
                        bitmap.rows() as u32,
                        2,
                    )?)
                }
                freetype::bitmap::PixelMode::Gray4 => {
                    RasterPixels::Mask8(normalize_packed_gray_bitmap(
                        &bitmap,
                        bitmap.width() as u32,
                        bitmap.rows() as u32,
                        4,
                    )?)
                }
                freetype::bitmap::PixelMode::Bgra => RasterPixels::Bgra8(normalize_bgra_bitmap(
                    &bitmap,
                    bitmap.width() as u32,
                    bitmap.rows() as u32,
                )?),
                other => {
                    return Err(FontMaterializationError::UnsupportedPixelMode(format!(
                        "{other:?}"
                    )));
                }
            };
            Ok(RasterizedGlyph {
                width_px: bitmap.width() as u32,
                height_px: bitmap.rows() as u32,
                left_px: slot.bitmap_left(),
                top_px: slot.bitmap_top(),
                advance_px: (slot.metrics().horiAdvance >> 6) as f32 / self.device_scale,
                pixels,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = glyph;
            Err(FontMaterializationError::BackendUnavailable)
        }
    }

    /// Return the glyph's logical horizontal advance without rendering it.
    /// Layout uses this path so measurement and rendering share one opened
    /// face while avoiding a bitmap allocation for every width probe.
    pub fn glyph_advance_px(
        &self,
        glyph: ResolvedGlyphId,
    ) -> Result<f32, FontMaterializationError> {
        #[cfg(any(unix, windows))]
        {
            self._face
                .load_glyph(glyph.get(), LoadFlag::DEFAULT)
                .map_err(|error| FontMaterializationError::LoadGlyph {
                    glyph_id: glyph.get(),
                    reason: error.to_string(),
                })?;
            Ok((self._face.glyph().metrics().horiAdvance >> 6) as f32 / self.device_scale)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = glyph;
            Err(FontMaterializationError::BackendUnavailable)
        }
    }
}

pub struct FontMaterializer {
    #[cfg(any(unix, windows))]
    library: Library,
}

impl std::fmt::Debug for FontMaterializer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("FontMaterializer").finish()
    }
}

impl FontMaterializer {
    pub fn new() -> Result<Self, FontMaterializationError> {
        #[cfg(any(unix, windows))]
        {
            let library = Library::init().map_err(|error| {
                FontMaterializationError::BackendInitialization(error.to_string())
            })?;
            Ok(Self { library })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(FontMaterializationError::BackendUnavailable)
        }
    }

    pub fn open(
        &self,
        request: FontOpenRequest<'_>,
    ) -> Result<OpenedFont, FontMaterializationError> {
        #[cfg(any(unix, windows))]
        {
            self.open_freetype_bitmap(request)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = request;
            Err(FontMaterializationError::BackendUnavailable)
        }
    }

    /// Inspect whether the exact file asset has a native realization this
    /// module owns. Inspection does not select a size or mutate caller state.
    pub fn inspect(
        &self,
        asset: &FontFileAsset,
    ) -> Result<FontCapability, FontMaterializationError> {
        #[cfg(any(unix, windows))]
        {
            let path = asset.path();
            let face = self
                .library
                .new_face(path, asset.face_index() as isize)
                .map_err(|error| FontMaterializationError::OpenFace {
                    path: path.to_owned(),
                    reason: error.to_string(),
                })?;
            if face.is_scalable() || !face.has_fixed_sizes() || face.raw().num_fixed_sizes <= 0 {
                return Err(FontMaterializationError::NotBitmapFace);
            }
            Ok(FontCapability::FreeTypeBitmap {
                strikes: (0..face.raw().num_fixed_sizes as u32)
                    .filter_map(|index| strike_at(&face, index).map(|strike| (index, strike)))
                    .map(|(index, strike)| BitmapStrikeKey {
                        index,
                        x_ppem_26_6: ft_pos_i64(strike.x_ppem),
                        y_ppem_26_6: ft_pos_i64(strike.y_ppem),
                    })
                    .collect(),
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = asset;
            Err(FontMaterializationError::BackendUnavailable)
        }
    }

    pub fn reopen(
        &self,
        request: FontOpenRequest<'_>,
        replay: FontReplay,
    ) -> Result<OpenedFont, FontMaterializationError> {
        #[cfg(any(unix, windows))]
        {
            let FontReplay::FreeTypeBitmap {
                asset,
                strike,
                spacing,
                ..
            } = &replay
            else {
                return Err(FontMaterializationError::ReplayMethodMismatch);
            };
            if asset != request.asset {
                return Err(FontMaterializationError::ReplayAssetMismatch);
            }
            let strike = *strike;
            let request = FontOpenRequest {
                spacing: *spacing,
                ..request
            };
            let face = self.open_bitmap_face(request)?;
            let recorded = strike_at(&face, strike.index)
                .ok_or(FontMaterializationError::ReplayStrikeMismatch)?;
            if ft_pos_i64(recorded.x_ppem) != strike.x_ppem_26_6
                || ft_pos_i64(recorded.y_ppem) != strike.y_ppem_26_6
            {
                return Err(FontMaterializationError::ReplayStrikeMismatch);
            }
            face.select_size(strike.index as i32).map_err(|error| {
                FontMaterializationError::SelectStrike {
                    index: strike.index,
                    reason: error.to_string(),
                }
            })?;
            let size = face
                .size_metrics()
                .ok_or(FontMaterializationError::MissingSizeMetrics)?;
            if i64::from(size.x_ppem) << 6 != strike.x_ppem_26_6
                || i64::from(size.y_ppem) << 6 != strike.y_ppem_26_6
            {
                return Err(FontMaterializationError::ReplayStrikeMismatch);
            }
            Ok(opened_bitmap_font(request, face, replay, size))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (request, replay);
            Err(FontMaterializationError::BackendUnavailable)
        }
    }

    #[cfg(any(unix, windows))]
    fn open_freetype_bitmap(
        &self,
        request: FontOpenRequest<'_>,
    ) -> Result<OpenedFont, FontMaterializationError> {
        let face = self.open_bitmap_face(request)?;
        let scale = request.device_scale.get();
        let device_px = (request.requested_layout_px * scale).round().max(1.0) as u32;
        let strike_index = if let Some(selected_ppem) = request.selected_device_ppem_26_6 {
            let selected_ppem = i64::from(selected_ppem);
            let index = exact_strike_index(&face, selected_ppem)
                .ok_or(FontMaterializationError::MissingSelectedStrike)?;
            face.select_size(index as i32).map_err(|error| {
                FontMaterializationError::SelectStrike {
                    index,
                    reason: error.to_string(),
                }
            })?;
            index
        } else {
            match face.set_pixel_sizes(device_px, device_px) {
                Ok(()) => {
                    let size = face
                        .size_metrics()
                        .ok_or(FontMaterializationError::MissingSizeMetrics)?;
                    selected_strike_index(&face, size.x_ppem, size.y_ppem)
                        .ok_or(FontMaterializationError::MissingSelectedStrike)?
                }
                Err(size_error) => {
                    // GNU's font entity normally carries the concrete size chosen
                    // by Fontconfig before `ftfont_open` calls
                    // `FT_Set_Pixel_Sizes`. The replay boundary receives only the
                    // requested layout size, so fixed-only SFNT wrappers (OTB in
                    // particular) can reject that intermediate request. Select
                    // the nearest advertised strike explicitly, then record it;
                    // renderer replay never repeats this policy decision.
                    let target_ppem_26_6 = i64::from(device_px) << 6;
                    let Some(index) = closest_strike_index(&face, target_ppem_26_6) else {
                        return Err(FontMaterializationError::SelectSize {
                            device_px,
                            reason: size_error.to_string(),
                        });
                    };
                    face.select_size(index as i32).map_err(|error| {
                        FontMaterializationError::SelectStrike {
                            index,
                            reason: error.to_string(),
                        }
                    })?;
                    index
                }
            }
        };
        let size = face
            .size_metrics()
            .ok_or(FontMaterializationError::MissingSizeMetrics)?;
        let replay = FontReplay::FreeTypeBitmap {
            asset: request.asset.clone(),
            strike: BitmapStrikeKey {
                index: strike_index,
                x_ppem_26_6: i64::from(size.x_ppem) << 6,
                y_ppem_26_6: i64::from(size.y_ppem) << 6,
            },
            sampling: GlyphSampling::Nearest,
            spacing: request.spacing,
        };
        Ok(opened_bitmap_font(request, face, replay, size))
    }

    #[cfg(any(unix, windows))]
    fn open_bitmap_face(
        &self,
        request: FontOpenRequest<'_>,
    ) -> Result<freetype::Face, FontMaterializationError> {
        if !request.requested_layout_px.is_finite() || request.requested_layout_px <= 0.0 {
            return Err(FontMaterializationError::InvalidRequestedSize);
        }
        let path = request.asset.path();
        let face = self
            .library
            .new_face(path, request.asset.face_index() as isize)
            .map_err(|error| FontMaterializationError::OpenFace {
                path: path.to_owned(),
                reason: error.to_string(),
            })?;
        if face.is_scalable() || !face.has_fixed_sizes() {
            return Err(FontMaterializationError::NotBitmapFace);
        }
        Ok(face)
    }
}

#[cfg(any(unix, windows))]
fn opened_bitmap_font(
    request: FontOpenRequest<'_>,
    face: freetype::Face,
    replay: FontReplay,
    size: freetype::ffi::FT_Size_Metrics,
) -> OpenedFont {
    let mut average_advance = 0i64;
    let mut glyph_count = 0i64;
    let mut space_advance = 0i64;
    for ch in 32usize..127 {
        if face.load_char(ch, LoadFlag::DEFAULT).is_err() {
            continue;
        }
        let advance = ft_pos_i64(face.glyph().metrics().horiAdvance) >> 6;
        if ch == 32 {
            space_advance = advance;
        }
        average_advance += advance;
        glyph_count += 1;
    }
    if glyph_count != 0 {
        average_advance /= glyph_count;
    }

    let scale = request.device_scale.get();
    let to_layout_px = |device_value: i64| device_value as f32 / scale;
    let ascent_px = to_layout_px(ft_pos_i64(size.ascender) >> 6);
    let descent_px = to_layout_px((-ft_pos_i64(size.descender)) >> 6);
    let native_height_px = to_layout_px(ft_pos_i64(size.height) >> 6);
    let max_advance_px = to_layout_px(ft_pos_i64(size.max_advance) >> 6);
    let (space_advance_px, average_advance_px) = fixed_font_horizontal_metrics(
        request.spacing,
        max_advance_px,
        to_layout_px(space_advance),
        to_layout_px(average_advance),
    );
    OpenedFont {
        replay,
        metrics: OpenedFontMetrics {
            requested_layout_px: request.requested_layout_px,
            effective_layout_px: f32::from(size.y_ppem) / scale,
            ascent_px,
            descent_px,
            height_px: bitmap_line_height(
                ascent_px,
                descent_px,
                native_height_px,
                request.line_height,
            ),
            max_advance_px,
            space_advance_px,
            average_advance_px,
        },
        device_scale: scale,
        _face: face,
    }
}

fn fixed_font_horizontal_metrics(
    spacing: FixedFontSpacing,
    max_advance_px: f32,
    measured_space_px: f32,
    measured_average_px: f32,
) -> (f32, f32) {
    match spacing {
        FixedFontSpacing::ProportionalOrDual => (measured_space_px, measured_average_px),
        FixedFontSpacing::MonospaceOrCharacterCell => (max_advance_px, max_advance_px),
    }
}

fn bitmap_line_height(
    ascent_px: f32,
    descent_px: f32,
    native_height_px: f32,
    policy: BitmapLineHeightPolicy,
) -> f32 {
    match policy {
        BitmapLineHeightPolicy::GnuDefault => ascent_px + descent_px,
        BitmapLineHeightPolicy::NativeMetrics => native_height_px,
    }
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;

#[cfg(any(unix, windows))]
fn normalize_mono_bitmap(
    bitmap: &freetype::Bitmap,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, FontMaterializationError> {
    let mut mask = vec![0; width as usize * height as usize];
    let required_row_len = width.div_ceil(8) as usize;
    for y in 0..height as usize {
        let row = bitmap_row(bitmap, y, required_row_len)?;
        let output = &mut mask[y * width as usize..(y + 1) * width as usize];
        for (x, alpha) in output.iter_mut().enumerate() {
            let bit = 0x80 >> (x & 7);
            *alpha = if row[x >> 3] & bit != 0 { 255 } else { 0 };
        }
    }
    Ok(mask)
}

#[cfg(any(unix, windows))]
fn normalize_gray_bitmap(
    bitmap: &freetype::Bitmap,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, FontMaterializationError> {
    let mut mask = vec![0; width as usize * height as usize];
    let max_gray = bitmap.raw().num_grays.saturating_sub(1).max(1) as u32;
    for y in 0..height as usize {
        let row = bitmap_row(bitmap, y, width as usize)?;
        let output = &mut mask[y * width as usize..(y + 1) * width as usize];
        for (alpha, value) in output.iter_mut().zip(row.iter().copied()) {
            *alpha = (u32::from(value) * 255 / max_gray) as u8;
        }
    }
    Ok(mask)
}

#[cfg(any(unix, windows))]
fn normalize_packed_gray_bitmap(
    bitmap: &freetype::Bitmap,
    width: u32,
    height: u32,
    bits_per_pixel: u8,
) -> Result<Vec<u8>, FontMaterializationError> {
    let mut mask = vec![0; width as usize * height as usize];
    let pixels_per_byte = 8 / usize::from(bits_per_pixel);
    let required_row_len = (width as usize).div_ceil(pixels_per_byte);
    let value_mask = (1u8 << bits_per_pixel) - 1;
    for y in 0..height as usize {
        let row = bitmap_row(bitmap, y, required_row_len)?;
        let output = &mut mask[y * width as usize..(y + 1) * width as usize];
        for (x, alpha) in output.iter_mut().enumerate() {
            let shift = 8 - usize::from(bits_per_pixel) * (x % pixels_per_byte + 1);
            let value = (row[x / pixels_per_byte] >> shift) & value_mask;
            *alpha = (u16::from(value) * 255 / u16::from(value_mask)) as u8;
        }
    }
    Ok(mask)
}

#[cfg(any(unix, windows))]
fn normalize_bgra_bitmap(
    bitmap: &freetype::Bitmap,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, FontMaterializationError> {
    let row_len = width as usize * 4;
    let mut pixels = vec![0; row_len * height as usize];
    for y in 0..height as usize {
        pixels[y * row_len..(y + 1) * row_len].copy_from_slice(bitmap_row(bitmap, y, row_len)?);
    }
    Ok(pixels)
}

#[cfg(any(unix, windows))]
fn bitmap_row(
    bitmap: &freetype::Bitmap,
    y: usize,
    required_len: usize,
) -> Result<&[u8], FontMaterializationError> {
    let pitch = bitmap.pitch();
    let row_bytes = pitch.unsigned_abs() as usize;
    let buffer = bitmap.raw().buffer;
    if buffer.is_null() || row_bytes < required_len {
        return Err(FontMaterializationError::MalformedBitmapBuffer {
            expected: required_len,
            actual: row_bytes,
        });
    }
    // FreeType defines `pitch` as the signed offset from one logical row to
    // the next. The buffer remains valid until the next face operation.
    Ok(unsafe {
        std::slice::from_raw_parts(buffer.offset(y as isize * pitch as isize), required_len)
    })
}

#[cfg(any(unix, windows))]
fn selected_strike_index(face: &freetype::Face, x_ppem: u16, y_ppem: u16) -> Option<u32> {
    let raw = face.raw();
    if raw.num_fixed_sizes <= 0 || raw.available_sizes.is_null() {
        return None;
    }
    let strikes =
        unsafe { std::slice::from_raw_parts(raw.available_sizes, raw.num_fixed_sizes as usize) };
    strikes
        .iter()
        .position(|strike| {
            ft_pos_i64(strike.x_ppem) == i64::from(x_ppem) << 6
                && ft_pos_i64(strike.y_ppem) == i64::from(y_ppem) << 6
        })
        .map(|index| index as u32)
}

#[cfg(any(unix, windows))]
fn closest_strike_index(face: &freetype::Face, target_ppem_26_6: i64) -> Option<u32> {
    let raw = face.raw();
    if raw.num_fixed_sizes <= 0 || raw.available_sizes.is_null() {
        return None;
    }
    let strikes =
        unsafe { std::slice::from_raw_parts(raw.available_sizes, raw.num_fixed_sizes as usize) };
    strikes
        .iter()
        .enumerate()
        .min_by_key(|(index, strike)| {
            (
                ft_pos_i64(strike.x_ppem).abs_diff(target_ppem_26_6)
                    + ft_pos_i64(strike.y_ppem).abs_diff(target_ppem_26_6),
                *index,
            )
        })
        .map(|(index, _)| index as u32)
}

#[cfg(any(unix, windows))]
fn exact_strike_index(face: &freetype::Face, target_ppem_26_6: i64) -> Option<u32> {
    let raw = face.raw();
    if raw.num_fixed_sizes <= 0 || raw.available_sizes.is_null() {
        return None;
    }
    let strikes =
        unsafe { std::slice::from_raw_parts(raw.available_sizes, raw.num_fixed_sizes as usize) };
    strikes
        .iter()
        .position(|strike| ft_pos_i64(strike.y_ppem) == target_ppem_26_6)
        .map(|index| index as u32)
}

#[cfg(any(unix, windows))]
fn ft_pos_i64(value: freetype::ffi::FT_Pos) -> i64 {
    std::cfg_select! {
        any(windows, target_pointer_width = "32") => i64::from(value),
        _ => value,
    }
}

#[cfg(any(unix, windows))]
fn strike_at(face: &freetype::Face, index: u32) -> Option<freetype::ffi::FT_Bitmap_Size> {
    let raw = face.raw();
    if raw.num_fixed_sizes <= 0 || raw.available_sizes.is_null() {
        return None;
    }
    let strikes =
        unsafe { std::slice::from_raw_parts(raw.available_sizes, raw.num_fixed_sizes as usize) };
    strikes.get(index as usize).copied()
}
