//! Pixel-size font metric probe, porting GNU's cairo freetype driver.
//!
//! `font-info` on a font ENTITY opens the font at the entity's size — for a
//! scalable entity, `font_open_entity` (GNU src/font.c) bumps pixel size 0
//! upward until `average_width > 0 && height > 0`, which lands at 1px for
//! ordinary scalable fonts. The metrics GNU reports come from
//! `ftcrfont_open` (src/ftcrfont.c): a cairo scaled font's per-glyph
//! `x_advance` rounded via `lround` (cairo hint-metrics rounds advances to
//! integer pixels over FreeType's hinted loads) and `lround`ed cairo font
//! extents for ascent/descent.
//!
//! This module reproduces that with FreeType directly:
//! - per-glyph width = hinted `FT_Load_Char(FT_LOAD_DEFAULT)` advance,
//!   rounded from 26.6 to integer pixels (cairo hint-metrics equivalent);
//! - ascent/descent = rounded FT size metrics (what cairo's font extents
//!   report for an FT backend with hint-metrics on).
//!
//! Byte-exactness is enforced by tests against captured GNU output for
//! concrete font files; if a future font/hinting configuration diverges,
//! the tests say so instead of the probe silently guessing.
//!
//! The FreeType-backed probes live in a `cfg_select!` block and compile only
//! with the Fontconfig adapter. macOS gets metrics from CoreText, while targets
//! without this implementation receive `None`/empty results so shared callers
//! compile unchanged.
//! The GSUB/GPOS `otf_capability` reader uses ttf-parser and is fully
//! cross-platform (all three).

use neomacs_display_protocol::font::{FontBackendKind, FontVariationCoord, ResolvedFontIdentity};

/// Metrics of one font file probed at an exact pixel size, shaped like the
/// `font-info` elements GNU fills in `ftcrfont_open` + `font_open_entity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontPxMetrics {
    /// The pixel size the font actually opened at (`font_open_entity` may
    /// bump the requested size upward until the font is "manageable").
    pub pixel_size: u32,
    pub height: i32,
    pub ascent: i32,
    pub descent: i32,
    pub max_width: i32,
    pub space_width: i32,
    pub average_width: i32,
}

/// Hinted horizontal advances measured in device pixels for ASCII printable
/// glyphs of one exact opened font instance.
///
/// GNU's Cairo display backend stores these integer advances on the opened
/// font and xdisp uses them directly for ordinary characters.  Keeping the
/// coordinate domain in the type prevents a HiDPI device width from being
/// mistaken for logical layout geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceAsciiAdvances {
    device_pixel_size: u32,
    widths: [i32; 128],
}

impl DeviceAsciiAdvances {
    #[must_use]
    pub const fn device_pixel_size(&self) -> u32 {
        self.device_pixel_size
    }

    /// Convert one printable ASCII advance to the logical-pixel domain used
    /// by the display matrix.
    #[must_use]
    pub fn logical_advance(
        &self,
        ch: char,
        device_scale: neomacs_display_protocol::geometry::DeviceScale,
    ) -> Option<f32> {
        let index = usize::try_from(u32::from(ch)).ok()?;
        let width = *self.widths.get(index)?;
        (width > 0).then(|| width as f32 / device_scale.get())
    }
}

/// Physical realization size, never a logical layout size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeviceFontPixelSize(std::num::NonZeroU32);

impl DeviceFontPixelSize {
    fn new(device_pixels: u32) -> Result<Self, DeviceFontProbeError> {
        std::num::NonZeroU32::new(device_pixels)
            .map(Self)
            .ok_or(DeviceFontProbeError::InvalidSize)
    }

    const fn get(self) -> u32 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug)]
struct FreeTypeFaceSelector(u32);

#[derive(Clone, Copy, Debug)]
enum DeviceFontWeight {
    Default,
    Explicit(f32),
    LegacyBestEffort(f32),
}

/// Exact selected face and physical size for a native glyph measurement.
/// Construction refuses native backends and coordinates this probe cannot replay.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ExactDeviceFontRequest<'a> {
    file: &'a str,
    selector: FreeTypeFaceSelector,
    size: DeviceFontPixelSize,
    weight: DeviceFontWeight,
}

#[derive(Clone, Copy)]
enum DeviceVariationPolicy {
    ExactGlyph,
    ExistingAsciiBestEffort,
}

impl<'a> ExactDeviceFontRequest<'a> {
    pub(crate) fn new(
        identity: &'a ResolvedFontIdentity,
        size: crate::font_backend::FontSelectionSize,
    ) -> Result<Self, DeviceFontProbeError> {
        Self::from_identity(identity, size, DeviceVariationPolicy::ExactGlyph)
    }

    /// Preserve the existing ASCII probe's weight-only, best-effort replay.
    pub(crate) fn for_ascii(
        identity: &'a ResolvedFontIdentity,
        size: crate::font_backend::FontSelectionSize,
    ) -> Result<Self, DeviceFontProbeError> {
        Self::from_identity(
            identity,
            size,
            DeviceVariationPolicy::ExistingAsciiBestEffort,
        )
    }

    fn from_identity(
        identity: &'a ResolvedFontIdentity,
        size: crate::font_backend::FontSelectionSize,
        policy: DeviceVariationPolicy,
    ) -> Result<Self, DeviceFontProbeError> {
        let selector = identity
            .freetype_selector()
            .ok_or(DeviceFontProbeError::UnsupportedBackend(identity.backend))?;
        let file = identity
            .file_path
            .as_deref()
            .ok_or(DeviceFontProbeError::MissingFile)?;
        let mut weight = DeviceFontWeight::Default;
        for coordinate in identity.variation_coords.iter().copied() {
            if coordinate.tag() != u32::from_be_bytes(*b"wght") {
                match policy {
                    DeviceVariationPolicy::ExactGlyph => {
                        return Err(DeviceFontProbeError::UnsupportedVariation(coordinate));
                    }
                    DeviceVariationPolicy::ExistingAsciiBestEffort => continue,
                }
            }
            weight = match policy {
                DeviceVariationPolicy::ExactGlyph => DeviceFontWeight::Explicit(coordinate.value()),
                DeviceVariationPolicy::ExistingAsciiBestEffort => {
                    DeviceFontWeight::LegacyBestEffort(coordinate.value())
                }
            };
        }
        Ok(Self {
            file,
            selector: FreeTypeFaceSelector(selector),
            size: DeviceFontPixelSize::new(size.rounded_device_px())?,
            weight,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DeviceFontProbeError {
    #[error("device glyph probe does not support backend {0:?}")]
    UnsupportedBackend(FontBackendKind),
    #[error("device glyph probe cannot replay coordinate {0:?}")]
    UnsupportedVariation(FontVariationCoord),
    #[error("selected font has no local file")]
    MissingFile,
    #[error("native glyph advance is outside the supported nonnegative pixel range")]
    InvalidAdvance,
    #[error("device font pixel size must be positive")]
    InvalidSize,
    #[error("selected FreeType face selector cannot be represented on this target")]
    InvalidSelector,
    #[error("selected font has no usable weight variation axis")]
    UnavailableWeightAxis,
    #[cfg(not(target_os = "linux"))]
    #[error("this target has no Fontconfig device glyph probe")]
    UnsupportedPlatform,
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    FreeType(#[from] freetype::Error),
}

/// One selected glyph's native advance, in the physical domain of its opened font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DeviceGlyphAdvance {
    device_pixel_size: DeviceFontPixelSize,
    advance_px: i32,
}

impl DeviceGlyphAdvance {
    /// Zero is meaningful for combining marks and must not become a missing glyph.
    pub(crate) fn logical_advance(
        self,
        scale: neomacs_display_protocol::geometry::DeviceScale,
    ) -> f32 {
        self.advance_px as f32 / scale.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeviceGlyphProbe {
    Present(DeviceGlyphAdvance),
    MissingGlyph,
}

// Real FreeType implementations accompany the Fontconfig adapter. In
// particular, macOS must not reopen CoreText selections through FreeType.
std::cfg_select! {
    target_os = "linux" => {
        use freetype::Library;
        use freetype::face::LoadFlag;

        /// Probe `file`[`face_index`] like GNU `font_open_entity`: try
        /// `pixel_size`, bumping upward (at most 15 times) until average
        /// width and height are positive.
        pub fn probe_font_px_metrics(
            file: &str,
            face_index: u32,
            pixel_size: u32,
            wght: Option<f32>,
        ) -> Option<FontPxMetrics> {
            let library = Library::init().ok()?;
            let mut face = library.new_face(file, face_index as isize).ok()?;
            // Variable fonts: GNU probes the MATCHED named instance (the
            // cairo scaled font carries the fontconfig pattern's wght), not
            // the default instance — thin/bold advances differ from regular
            // at real sizes.
            if let Some(wght) = wght {
                apply_wght_axis(&library, &mut face, wght);
            }
            let start = pixel_size.max(1);
            for psize in start..=start + 15 {
                if let Some(metrics) = probe_at_exact_px(&face, psize)
                    && metrics.average_width > 0
                    && metrics.height > 0
                {
                    return Some(metrics);
                }
            }
            None
        }

        /// Open one exact Fontconfig/FreeType face at its device-pixel size
        /// and capture the per-glyph advances GNU's Cairo backend gives xdisp
        /// for ASCII printables.
        pub fn probe_device_ascii_advances(
            file: &str,
            face_selector: u32,
            device_pixel_size: u32,
            wght: Option<f32>,
        ) -> Option<DeviceAsciiAdvances> {
            let request = ExactDeviceFontRequest {
                file,
                selector: FreeTypeFaceSelector(face_selector),
                size: DeviceFontPixelSize::new(device_pixel_size.max(1)).ok()?,
                weight: match wght {
                    Some(weight) => DeviceFontWeight::LegacyBestEffort(weight),
                    None => DeviceFontWeight::Default,
                },
            };
            probe_device_ascii_advances_for_request(&request).ok()
        }

        pub(crate) fn probe_device_ascii_advances_for_request(
            request: &ExactDeviceFontRequest<'_>,
        ) -> Result<DeviceAsciiAdvances, DeviceFontProbeError> {
            with_device_sized_face(request, |face| {
                let mut widths = [0; 128];
                for byte in 32u8..127 {
                    let glyph = face.get_char_index(usize::from(byte)).unwrap_or(0);
                    // GNU ftcrfont_open deliberately probes glyph 0 for missing ASCII.
                    if let Ok(advance) = hinted_device_advance(face, glyph, request.size)
                        .or_else(|_| hinted_device_advance(face, 0, request.size))
                    {
                        widths[usize::from(byte)] = advance.advance_px;
                    }
                }
                Ok(DeviceAsciiAdvances {
                    device_pixel_size: request.size.get(),
                    widths,
                })
            })
        }

        /// Measure an ordinary selected character using GNU's native device grid.
        /// A missing Unicode character is distinct from the ASCII glyph-0 fallback.
        pub(crate) fn probe_device_glyph_advance(
            request: &ExactDeviceFontRequest<'_>,
            ch: char,
        ) -> Result<DeviceGlyphProbe, DeviceFontProbeError> {
            with_device_sized_face(request, |face| {
                let Some(glyph) = face.get_char_index(ch as usize) else {
                    return Ok(DeviceGlyphProbe::MissingGlyph);
                };
                hinted_device_advance(face, glyph, request.size).map(DeviceGlyphProbe::Present)
            })
        }

        fn with_device_sized_face<T>(
            request: &ExactDeviceFontRequest<'_>,
            measure: impl FnOnce(&freetype::Face) -> Result<T, DeviceFontProbeError>,
        ) -> Result<T, DeviceFontProbeError> {
            let library = Library::init()?;
            let selector = isize::try_from(request.selector.0)
                .map_err(|_| DeviceFontProbeError::InvalidSelector)?;
            let mut face = library.new_face(request.file, selector)?;
            match request.weight {
                DeviceFontWeight::Default => {}
                DeviceFontWeight::Explicit(weight) => apply_wght_axis_checked(&library, &mut face, weight)?,
                DeviceFontWeight::LegacyBestEffort(weight) => apply_wght_axis(&library, &mut face, weight),
            }
            face.set_pixel_sizes(request.size.get(), request.size.get())?;
            measure(&face)
        }

        fn hinted_device_advance(
            face: &freetype::Face,
            glyph: u32,
            size: DeviceFontPixelSize,
        ) -> Result<DeviceGlyphAdvance, DeviceFontProbeError> {
            face.load_glyph(glyph, LoadFlag::TARGET_LIGHT)?;
            // GNU ftcrfont_glyph_extents uses lround of the hinted advance.
            let rounded = face.glyph().advance().x.checked_add(32)
                .ok_or(DeviceFontProbeError::InvalidAdvance)? >> 6;
            let advance_px = i32::try_from(rounded)
                .ok().filter(|advance| *advance >= 0)
                .ok_or(DeviceFontProbeError::InvalidAdvance)?;
            Ok(DeviceGlyphAdvance { device_pixel_size: size, advance_px })
        }

        /// The `wght` design-axis values of a variable font's fvar NAMED
        /// INSTANCES (OT axis units == CSS weight), sorted ascending and
        /// deduped. Empty for a non-variable font or one whose instances omit
        /// a `wght` axis.
        ///
        /// GNU/fontconfig expose named instances as separate font entities,
        /// so weight resolution snaps a request to the nearest instance
        /// rather than synthesizing an arbitrary axis value
        /// (font_match::resolve_requested_weight).
        pub fn named_instance_wght_values(file: &str, face_index: u32) -> Vec<u16> {
            use freetype::freetype_sys as ft;
            let Ok(library) = Library::init() else {
                return Vec::new();
            };
            let Ok(face) = library.new_face(file, face_index as isize) else {
                return Vec::new();
            };
            let mut weights = Vec::new();
            unsafe {
                let raw = face.raw() as *const ft::FT_FaceRec as *mut ft::FT_FaceRec;
                let mut mm: *mut ft::FT_MM_Var = std::ptr::null_mut();
                if ft::FT_Get_MM_Var(raw, &mut mm) != 0 || mm.is_null() {
                    return Vec::new();
                }
                let axis_count = (*mm).num_axis as usize;
                let axes = std::slice::from_raw_parts((*mm).axis, axis_count);
                let wght_tag = u32::from_be_bytes(*b"wght") as ft::FT_ULong;
                if let Some(wght_index) = axes.iter().position(|axis| axis.tag == wght_tag) {
                    let n = (*mm).num_namedstyles as usize;
                    let styles = std::slice::from_raw_parts((*mm).namedstyle, n);
                    for style in styles {
                        let coords = std::slice::from_raw_parts(style.coords, axis_count);
                        // FT_Fixed 16.16 → integer CSS weight.
                        let value = (coords[wght_index] as f64 / 65536.0).round();
                        if (1.0..=1000.0).contains(&value) {
                            weights.push(value as u16);
                        }
                    }
                }
                ft::FT_Done_MM_Var(library.raw(), mm);
            }
            weights.sort_unstable();
            weights.dedup();
            weights
        }

        /// Decode the complete axis tuple selected by a Fontconfig/FreeType
        /// encoded face selector. FreeType numbers named instances from one in
        /// bits 16-30; the raw collection face remains in bits 0-15.
        pub fn named_instance_variation_coords(
            file: &str,
            face_selector: u32,
        ) -> Vec<FontVariationCoord> {
            use freetype::freetype_sys as ft;
            let named_instance = ((face_selector >> 16) & 0x7fff) as usize;
            if named_instance == 0 {
                return Vec::new();
            }
            let Ok(library) = Library::init() else {
                return Vec::new();
            };
            let Ok(face) = library.new_face(file, (face_selector & 0xffff) as isize) else {
                return Vec::new();
            };
            unsafe {
                let raw = face.raw() as *const ft::FT_FaceRec as *mut ft::FT_FaceRec;
                let mut mm: *mut ft::FT_MM_Var = std::ptr::null_mut();
                if ft::FT_Get_MM_Var(raw, &mut mm) != 0 || mm.is_null() {
                    return Vec::new();
                }
                let axis_count = (*mm).num_axis as usize;
                let style_count = (*mm).num_namedstyles as usize;
                if named_instance > style_count {
                    ft::FT_Done_MM_Var(library.raw(), mm);
                    return Vec::new();
                }
                let axes = std::slice::from_raw_parts((*mm).axis, axis_count);
                let styles = std::slice::from_raw_parts((*mm).namedstyle, style_count);
                let coords = std::slice::from_raw_parts(
                    styles[named_instance - 1].coords,
                    axis_count,
                );
                let result = axes
                    .iter()
                    .zip(coords)
                    .filter(|&(axis, value)| *value != axis.def).filter_map(|(axis, value)| FontVariationCoord::try_new(axis.tag as u32, *value as f32 / 65536.0))
                    .collect();
                ft::FT_Done_MM_Var(library.raw(), mm);
                result
            }
        }

        /// PostScript name of the exact FreeType face or named instance.
        ///
        /// `face_selector` intentionally keeps Fontconfig's high named-instance
        /// bits; opening that selector is what makes FreeType report names such
        /// as `NotoSans-Bold` instead of the variable file's default name.
        pub fn postscript_name(file: &str, face_selector: u32) -> Option<String> {
            let library = Library::init().ok()?;
            library
                .new_face(file, face_selector as isize)
                .ok()?
                .postscript_name()
        }

        /// Legacy aggregate probes preserve their previous no-op-on-refusal behavior.
        fn apply_wght_axis(library: &Library, face: &mut freetype::Face, wght: f32) {
            let _ = replay_wght_axis(library, face, wght, WeightAxisRequirement::LegacyOptional);
        }

        #[derive(Clone, Copy, Debug)]
        enum WeightAxisRequirement {
            Exact,
            LegacyOptional,
        }

        #[must_use]
        struct VariationDescriptor<'a> {
            library: &'a Library,
            descriptor: std::ptr::NonNull<freetype::freetype_sys::FT_MM_Var>,
        }

        static_assertions::assert_not_impl_any!(VariationDescriptor<'static>: Send, Sync);

        impl std::fmt::Debug for VariationDescriptor<'_> {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.debug_struct("VariationDescriptor")
                    .field("descriptor", &self.descriptor)
                    .finish_non_exhaustive()
            }
        }

        impl Drop for VariationDescriptor<'_> {
            fn drop(&mut self) {
                // SAFETY: FT_Get_MM_Var allocated this descriptor for this live library;
                // this unique guard releases it exactly once after the last axis read.
                unsafe {
                    freetype::freetype_sys::FT_Done_MM_Var(
                        self.library.raw(), self.descriptor.as_ptr(),
                    );
                }
            }
        }

        /// The one weight replay implementation shared by aggregate/device probes.
        /// Other coordinates are explicitly refused by ExactDeviceFontRequest.
        fn apply_wght_axis_checked(
            library: &Library,
            face: &mut freetype::Face,
            wght: f32,
        ) -> Result<(), DeviceFontProbeError> {
            replay_wght_axis(library, face, wght, WeightAxisRequirement::Exact)
        }

        fn replay_wght_axis(
            library: &Library,
            face: &mut freetype::Face,
            wght: f32,
            requirement: WeightAxisRequirement,
        ) -> Result<(), DeviceFontProbeError> {
            use freetype::freetype_sys as ft;
            let raw = face.raw_mut() as *mut ft::FT_FaceRec;
            let mut descriptor = std::ptr::null_mut();
            // SAFETY: raw points to the live uniquely borrowed face; descriptor is
            // a valid out pointer and is adopted immediately on success below.
            let status = unsafe { ft::FT_Get_MM_Var(raw, &mut descriptor) };
            if status != 0 {
                return Err(freetype::Error::from(status).into());
            }
            let descriptor = std::ptr::NonNull::new(descriptor)
                .ok_or(DeviceFontProbeError::UnavailableWeightAxis)?;
            let owner = VariationDescriptor { library, descriptor };
            // SAFETY: the owned FreeType descriptor remains live through this call.
            let descriptor = unsafe { owner.descriptor.as_ref() };
            let axes = if descriptor.num_axis == 0 {
                &[][..]
            } else {
                if descriptor.axis.is_null() {
                    return Err(DeviceFontProbeError::UnavailableWeightAxis);
                }
                // SAFETY: FreeType supplies num_axis initialized axis records owned
                // by the descriptor guard, which outlives this borrowed slice.
                unsafe { std::slice::from_raw_parts(descriptor.axis, descriptor.num_axis as usize) }
            };
            let mut coords: Vec<ft::FT_Fixed> = axes.iter().map(|axis| axis.def).collect();
            let wght_tag = u32::from_be_bytes(*b"wght") as ft::FT_ULong;
            match axes.iter().position(|axis| axis.tag == wght_tag) {
                Some(index) => coords[index] = (f64::from(wght) * 65536.0) as ft::FT_Fixed,
                None => match requirement {
                    WeightAxisRequirement::Exact => return Err(DeviceFontProbeError::UnavailableWeightAxis),
                    WeightAxisRequirement::LegacyOptional => {}
                },
            }
            // SAFETY: raw is still the live face and coords contains exactly the
            // descriptor's num_axis coordinates for the duration of the call.
            let status = unsafe {
                ft::FT_Set_Var_Design_Coordinates(raw, descriptor.num_axis, coords.as_mut_ptr())
            };
            if status != 0 {
                return Err(freetype::Error::from(status).into());
            }
            Ok(())
        }

        fn probe_at_exact_px(face: &freetype::Face, pixel_size: u32) -> Option<FontPxMetrics> {
            face.set_pixel_sizes(pixel_size, pixel_size).ok()?;

            // ASCII printables loop (ftcrfont.c ftcrfont_open): per-glyph
            // width is the hinted advance rounded to integer pixels (cairo
            // lround of x_advance with hint-metrics on). Glyphs a char is
            // missing fall back to glyph id 0, mirroring the cairo
            // text_to_glyphs failure path.
            let mut max_width = 0i32;
            let mut space_width = 0i32;
            let mut average_width = 0i64;
            let mut n = 0i64;
            // Cairo under fontconfig's default hintstyle=hintslight loads
            // with FT_LOAD_TARGET_LIGHT: vertical-only hinting, so horizontal
            // advances stay fractional and hint-metrics rounding (lround)
            // decides the pixel width. Full bytecode hinting (LOAD_DEFAULT)
            // would widen some glyphs (Noto '@'-class) to 2px where GNU
            // reports 1.
            let load_flags = LoadFlag::TARGET_LIGHT;
            for c in 32u8..127 {
                if face.load_char(c as usize, load_flags).is_err()
                    && face.load_glyph(0, load_flags).is_err()
                {
                    continue;
                }
                // 26.6 fixed-point hinted advance → integer pixels, round
                // half up (lround semantics for the non-negative advances
                // fonts produce).
                let advance = face.glyph().advance().x;
                let this_width = ((advance + 32) >> 6) as i32;
                if this_width > 0 {
                    if this_width > max_width {
                        max_width = this_width;
                    }
                    if c == 32 {
                        space_width = this_width;
                    }
                    average_width += this_width as i64;
                    n += 1;
                }
            }
            if n > 0 {
                average_width /= n;
            }

            // Font extents (cairo_scaled_font_extents → lround): for an FT
            // backend with hint-metrics on these are the grid-fitted size
            // metrics.
            let size_metrics = face.size_metrics()?;
            let ascent = ((size_metrics.ascender + 32) >> 6) as i32;
            let descent = ((-size_metrics.descender + 32) >> 6) as i32;

            Some(FontPxMetrics {
                pixel_size,
                height: ascent + descent,
                ascent,
                descent,
                max_width,
                space_width,
                average_width: average_width as i32,
            })
        }

        // The probe tests exercise the FreeType functions against real fonts.
        #[cfg(test)]
        #[path = "tests/probe_test.rs"]
        mod tests;
    }
    _ => {
        /// Target without the Fontconfig/FreeType probe.
        pub fn probe_font_px_metrics(
            _file: &str,
            _face_index: u32,
            _pixel_size: u32,
            _wght: Option<f32>,
        ) -> Option<FontPxMetrics> {
            None
        }

        pub fn probe_device_ascii_advances(
            _file: &str,
            _face_selector: u32,
            _device_pixel_size: u32,
            _wght: Option<f32>,
        ) -> Option<DeviceAsciiAdvances> {
            None
        }

        pub(crate) fn probe_device_ascii_advances_for_request(
            _request: &ExactDeviceFontRequest<'_>,
        ) -> Result<DeviceAsciiAdvances, DeviceFontProbeError> {
            Err(DeviceFontProbeError::UnsupportedPlatform)
        }

        pub(crate) fn probe_device_glyph_advance(
            _request: &ExactDeviceFontRequest<'_>,
            _ch: char,
        ) -> Result<DeviceGlyphProbe, DeviceFontProbeError> {
            Err(DeviceFontProbeError::UnsupportedPlatform)
        }

        /// Target without the FreeType probe: no named-instance enumeration.
        pub fn named_instance_wght_values(_file: &str, _face_index: u32) -> Vec<u16> {
            Vec::new()
        }

        pub fn named_instance_variation_coords(
            _file: &str,
            _face_selector: u32,
        ) -> Vec<FontVariationCoord> {
            Vec::new()
        }

        pub fn postscript_name(_file: &str, _face_selector: u32) -> Option<String> {
            None
        }
    }
}

/// One langsys of an OpenType script: `None` tag = the default langsys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtfLangSys {
    pub tag: Option<String>,
    pub features: Vec<String>,
}

/// One script of a GSUB/GPOS table with its langsyses (default first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtfScript {
    pub tag: String,
    pub lang_syses: Vec<OtfLangSys>,
}

/// GSUB/GPOS capability of a font file, shaped like GNU's
/// `hbfont_otf_capability` (src/hbfont.c): per table, scripts in table
/// order; per script the default langsys first (tag `None`) then named
/// langsyses in table order, langsyses without features skipped; features
/// are the langsys's feature indices mapped to tags in index order. Tags
/// keep their trailing spaces ("MKD ").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OtfCapability {
    pub gsub: Vec<OtfScript>,
    pub gpos: Vec<OtfScript>,
}

/// Metadata belongs to the native FontMetricsService and is retired with its
/// caches. Store only parsed capabilities, never whole font allocations.
#[derive(Default)]
pub(crate) struct OtfCapabilityCache {
    entries: std::collections::HashMap<(String, u32), (OtfFileIdentity, Option<OtfCapability>)>,
}

const OTF_CAPABILITY_CACHE_CAP: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
struct OtfFileIdentity {
    len: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    changed: (i64, i64),
}

impl OtfFileIdentity {
    fn for_file(file: &str) -> Option<Self> {
        // On Unix, inode/device distinguish replacement, and ctime catches
        // in-place changes even if mtime is restored. Other platforms retain
        // the uncached probe until an equally strong native identity is used.
        #[cfg(not(unix))]
        {
            let _ = file;
            None
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = std::fs::metadata(file).ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some(Self {
                len: metadata.len(),
                modified: metadata.modified().ok()?,
                device: metadata.dev(),
                inode: metadata.ino(),
                changed: (metadata.ctime(), metadata.ctime_nsec()),
            })
        }
    }
}

impl OtfCapabilityCache {
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn get(&mut self, file: &str, face_index: u32) -> Option<OtfCapability> {
        self.get_with(file, face_index, |path| std::fs::read(path))
    }

    fn get_with(
        &mut self,
        file: &str,
        face_index: u32,
        read: impl FnOnce(&str) -> std::io::Result<Vec<u8>>,
    ) -> Option<OtfCapability> {
        let key = (file.to_owned(), face_index);
        let identity = OtfFileIdentity::for_file(file);
        if let Some(identity) = &identity
            && let Some((cached_identity, capability)) = self.entries.get(&key)
            && identity == cached_identity
        {
            return capability.clone();
        }
        // Never return an earlier successful observation after a missing,
        // unreadable or changed file. Read errors remain immediately retryable.
        self.entries.remove(&key);
        let data = read(file).ok()?;
        let capability = otf_capability_from_bytes(&data, face_index);
        // A malformed but unchanged readable asset may cache None; a changing
        // asset is not cached. This is a metadata observation, not an atomic
        // defense against adversarial concurrent file replacement.
        if let Some(identity) = identity
            && Some(&identity) == OtfFileIdentity::for_file(file).as_ref()
        {
            if self.entries.len() >= OTF_CAPABILITY_CACHE_CAP {
                self.clear();
            }
            self.entries.insert(key, (identity, capability.clone()));
        }
        capability
    }
}

#[cfg(all(test, unix))]
#[path = "tests/otf_cache_test.rs"]
mod otf_cache_tests;

pub fn otf_capability(file: &str, face_index: u32) -> Option<OtfCapability> {
    let data = std::fs::read(file).ok()?;
    otf_capability_from_bytes(&data, face_index)
}

pub fn otf_capability_from_bytes(data: &[u8], face_index: u32) -> Option<OtfCapability> {
    let face = ttf_parser::Face::parse(data, face_index).ok()?;
    let tables = face.tables();
    Some(OtfCapability {
        gsub: tables.gsub.map(otf_table_scripts).unwrap_or_default(),
        gpos: tables.gpos.map(otf_table_scripts).unwrap_or_default(),
    })
}

fn otf_table_scripts(table: ttf_parser::opentype_layout::LayoutTable) -> Vec<OtfScript> {
    // GNU only reports a table with at least one feature tag.
    if table.features.is_empty() {
        return Vec::new();
    }
    let feature_tag = |index: u16| -> Option<String> {
        table
            .features
            .get(index)
            .map(|feature| tag_string(feature.tag))
    };
    let mut scripts = Vec::new();
    for script in table.scripts {
        let mut lang_syses = Vec::new();
        let mut push_langsys =
            |tag: Option<String>, lang: ttf_parser::opentype_layout::LanguageSystem| {
                let features: Vec<String> = lang
                    .feature_indices
                    .into_iter()
                    .filter_map(feature_tag)
                    .collect();
                if !features.is_empty() {
                    lang_syses.push(OtfLangSys { tag, features });
                }
            };
        if let Some(default) = script.default_language {
            push_langsys(None, default);
        }
        for lang in script.languages {
            push_langsys(Some(tag_string(lang.tag)), lang);
        }
        scripts.push(OtfScript {
            tag: tag_string(script.tag),
            lang_syses,
        });
    }
    scripts
}

fn tag_string(tag: ttf_parser::Tag) -> String {
    String::from_utf8_lossy(&tag.to_bytes()).into_owned()
}

#[cfg(test)]
#[path = "tests/device_glyph_probe_test.rs"]
mod device_glyph_probe_tests;
