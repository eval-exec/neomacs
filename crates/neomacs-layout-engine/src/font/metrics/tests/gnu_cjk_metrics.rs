//! Test-only GNU31.1 Cairo metric contract for the recorded pinned SC Bold face.
//! Source: ftcrfont.c:163-185, 232-255, 94-110; the pinned named instance gives different advances with FreeType 2.13.2 and 2.14.3.
//! No catalog matching, process-global Fontconfig state, or Neo metric probes.
use crate::font_backend::FontSelectionSize;
use neomacs_display_protocol::{font::ResolvedFontIdentity, geometry::DeviceScale};
use std::{
    ffi::{CString, c_char, c_double, c_int, c_long, c_ulong},
    num::NonZeroU32,
    ptr::{self, NonNull},
};

const GNU_SELECTOR: c_int = 0x0006_0002;
#[derive(Debug, thiserror::Error)]
pub(super) enum OracleError {
    #[error("the pinned font path is not a native C string")]
    FixturePath,
    #[error("cannot allocate Cairo reference resource {0:?}")]
    Allocation(Resource),
    #[error("cannot construct the exact Fontconfig reference pattern")]
    FontconfigPattern,
    #[error("Cairo reference {0:?} failed with {1:?}")]
    Cairo(Stage, CairoStatus),
    #[error("Cairo opened a different face selector: {0}")]
    OpenedSelector(c_long),
    #[error("the reference accepts printable ASCII, received {0:?}")]
    UnsupportedAscii(char),
    #[error("Cairo mapped one ASCII character to {0} glyphs")]
    GlyphCount(c_int),
    #[error("Cairo returned an invalid advance for the pinned printable glyph")]
    InvalidAdvance,
    #[error("Cairo could not expose the opened FreeType face")]
    MissingLockedFace,
}
#[derive(Debug, Clone, Copy)]
pub(super) enum Resource {
    Pattern,
    Face,
    Options,
    ScaledFont,
    Glyphs,
}
#[derive(Debug, Clone, Copy)]
pub(super) enum Stage {
    Face,
    ScaledFont,
    Mapping,
    Extents,
}
#[derive(Debug, Clone, Copy)]
pub(super) struct CairoStatus(c_int);
impl CairoStatus {
    fn check(self, stage: Stage) -> Result<(), OracleError> {
        match self.0 {
            0 => Ok(()),
            _ => Err(OracleError::Cairo(stage, self)),
        }
    }
}
#[repr(C)]
struct CairoFace {
    _private: [u8; 0],
}
#[repr(C)]
struct CairoOptions {
    _private: [u8; 0],
}
#[repr(C)]
struct CairoScaledFont {
    _private: [u8; 0],
}
#[repr(C)]
struct Matrix {
    xx: f64,
    yx: f64,
    xy: f64,
    yy: f64,
    x0: f64,
    y0: f64,
}
impl Matrix {
    fn for_font(size: FontSelectionSize) -> Self {
        let pixels = f64::from(size.rounded_device_px());
        Self {
            xx: pixels,
            yx: 0.,
            xy: 0.,
            yy: pixels,
            x0: 0.,
            y0: 0.,
        }
    }
    fn identity() -> Self {
        Self {
            xx: 1.,
            yx: 0.,
            xy: 0.,
            yy: 1.,
            x0: 0.,
            y0: 0.,
        }
    }
}
#[repr(C)]
struct Glyph {
    index: c_ulong,
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Default)]
struct Extents {
    x_bearing: f64,
    y_bearing: f64,
    width: f64,
    height: f64,
    x_advance: f64,
    y_advance: f64,
}
// SAFETY: these declarations mirror Cairo's public C ABI; every call below
// validates native status and keeps allocations behind their typed owner.
#[link(name = "cairo")]
unsafe extern "C" {
    fn cairo_ft_font_face_create_for_pattern(p: *mut fontconfig_sys::FcPattern) -> *mut CairoFace;
    fn cairo_font_face_status(p: *mut CairoFace) -> c_int;
    fn cairo_font_face_destroy(p: *mut CairoFace);
    fn cairo_font_options_create() -> *mut CairoOptions;
    fn cairo_font_options_destroy(p: *mut CairoOptions);
    fn cairo_scaled_font_create(
        f: *mut CairoFace,
        m: *const Matrix,
        ctm: *const Matrix,
        o: *const CairoOptions,
    ) -> *mut CairoScaledFont;
    fn cairo_scaled_font_status(p: *mut CairoScaledFont) -> c_int;
    fn cairo_scaled_font_destroy(p: *mut CairoScaledFont);
    fn cairo_ft_scaled_font_lock_face(p: *mut CairoScaledFont) -> freetype::freetype_sys::FT_Face;
    fn cairo_ft_scaled_font_unlock_face(p: *mut CairoScaledFont);
    fn cairo_scaled_font_text_to_glyphs(
        p: *mut CairoScaledFont,
        x: c_double,
        y: c_double,
        text: *const c_char,
        len: c_int,
        glyphs: *mut *mut Glyph,
        count: *mut c_int,
        clusters: *mut *mut std::ffi::c_void,
        cluster_count: *mut c_int,
        flags: *mut c_int,
    ) -> c_int;
    fn cairo_scaled_font_glyph_extents(
        p: *mut CairoScaledFont,
        glyphs: *const Glyph,
        count: c_int,
        out: *mut Extents,
    );
    fn cairo_glyph_free(p: *mut Glyph);
}
// SAFETY: C lround takes a double and returns the platform's signed long.
#[link(name = "m")]
unsafe extern "C" {
    fn lround(value: c_double) -> c_long;
}
macro_rules! owner {
    ($name:ident, $raw:ty, $drop:path) => {
        struct $name(NonNull<$raw>);
        impl Drop for $name {
            fn drop(&mut self) {
                // SAFETY: this unique owner holds exactly the native allocation accepted by its matching destructor.
                unsafe { $drop(self.0.as_ptr()) };
            }
        }
    };
}
owner!(
    Pattern,
    fontconfig_sys::FcPattern,
    fontconfig_sys::FcPatternDestroy
);
owner!(Face, CairoFace, cairo_font_face_destroy);
owner!(Options, CairoOptions, cairo_font_options_destroy);
owner!(ScaledFont, CairoScaledFont, cairo_scaled_font_destroy);
owner!(Glyphs, Glyph, cairo_glyph_free);
struct LockedFace<'a> {
    font: &'a ScaledFont,
    face: NonNull<freetype::freetype_sys::FT_FaceRec>,
}
impl<'a> LockedFace<'a> {
    fn new(font: &'a ScaledFont) -> Result<Self, OracleError> {
        // SAFETY: font owns a live, status-checked Cairo FT scaled font; the guard borrows it through unlock.
        let face = NonNull::new(unsafe { cairo_ft_scaled_font_lock_face(font.0.as_ptr()) })
            .ok_or(OracleError::MissingLockedFace)?;
        Ok(Self { font, face })
    }
}
impl Drop for LockedFace<'_> {
    fn drop(&mut self) {
        // SAFETY: this guard represents one successful lock and its borrowed font remains alive.
        unsafe { cairo_ft_scaled_font_unlock_face(self.font.0.as_ptr()) };
    }
}
pub(super) struct GnuCjkBoldOracle {
    identity: ResolvedFontIdentity,
    font: ScaledFont,
}
static_assertions::assert_not_impl_any!(GnuCjkBoldOracle: Send, Sync);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GnuDeviceAdvance(NonZeroU32);
static_assertions::assert_eq_size!(GnuDeviceAdvance, u32);
impl GnuDeviceAdvance {
    pub(super) fn into_logical(self, scale: DeviceScale) -> f32 {
        self.0.get() as f32 / scale.get()
    }
}
impl GnuCjkBoldOracle {
    pub(super) fn open(size: FontSelectionSize) -> Result<Self, OracleError> {
        let file = neomacs_test_fonts::noto_sans_cjk_variable(); // Verified SHA d3d8256c…bac32, independent of Neo selection.
        let path = file.to_str().ok_or(OracleError::FixturePath)?;
        let c_path = CString::new(path).map_err(|_| OracleError::FixturePath)?;
        let identity = ResolvedFontIdentity::from_file(
            path,
            GNU_SELECTOR as u32,
            Some("NotoSansCJKsc-Bold".into()),
        );
        // SAFETY: allocates an independent empty pattern; it is never installed as a process-global config.
        let pattern = Pattern(
            NonNull::new(unsafe { fontconfig_sys::FcPatternCreate() })
                .ok_or(OracleError::Allocation(Resource::Pattern))?,
        );
        // SAFETY: live exclusive pattern; Fontconfig copies this NUL-terminated verified path and primitive values.
        let added = unsafe {
            let p = pattern.0.as_ptr();
            fontconfig_sys::FcPatternAddString(p, c"file".as_ptr(), c_path.as_ptr().cast()) != 0
                && fontconfig_sys::FcPatternAddInteger(p, c"index".as_ptr(), GNU_SELECTOR) != 0
                && fontconfig_sys::FcPatternAddDouble(
                    p,
                    c"pixelsize".as_ptr(),
                    f64::from(size.rounded_device_px()),
                ) != 0
        };
        if !added {
            return Err(OracleError::FontconfigPattern);
        }
        // SAFETY: pattern is already fully selected by the recorded exact file/selector; Cairo retains what it needs.
        let face = Face(
            NonNull::new(unsafe { cairo_ft_font_face_create_for_pattern(pattern.0.as_ptr()) })
                .ok_or(OracleError::Allocation(Resource::Face))?,
        );
        // SAFETY: face owns the returned native face, including Cairo error objects.
        CairoStatus(unsafe { cairo_font_face_status(face.0.as_ptr()) }).check(Stage::Face)?;
        // SAFETY: allocates independent default font options, matching GNU's X11 ftcrfont_open.
        let options = Options(
            NonNull::new(unsafe { cairo_font_options_create() })
                .ok_or(OracleError::Allocation(Resource::Options))?,
        );
        let matrix = Matrix::for_font(size);
        let ctm = Matrix::identity();
        // SAFETY: all arguments are live and correctly laid out; Cairo retains the face/options data it requires.
        let font = ScaledFont(
            NonNull::new(unsafe {
                cairo_scaled_font_create(face.0.as_ptr(), &matrix, &ctm, options.0.as_ptr())
            })
            .ok_or(OracleError::Allocation(Resource::ScaledFont))?,
        );
        // SAFETY: font owns the native scaled font, including possible Cairo error objects.
        CairoStatus(unsafe { cairo_scaled_font_status(font.0.as_ptr()) })
            .check(Stage::ScaledFont)?;
        {
            let locked = LockedFace::new(&font)?;
            // SAFETY: Cairo's successful lock exposes this live FreeType face until the guard unlocks it.
            let actual = unsafe { locked.face.as_ref().face_index };
            if actual != c_long::from(GNU_SELECTOR) {
                return Err(OracleError::OpenedSelector(actual));
            }
        }
        Ok(Self { identity, font })
    }
    pub(super) fn identity(&self) -> &ResolvedFontIdentity {
        &self.identity
    }
    pub(super) fn measure_ascii(&self, ch: char) -> Result<GnuDeviceAdvance, OracleError> {
        if !(' '..='~').contains(&ch) {
            return Err(OracleError::UnsupportedAscii(ch));
        }
        let text = [u8::try_from(u32::from(ch)).map_err(|_| OracleError::UnsupportedAscii(ch))?];
        let mut raw = ptr::null_mut();
        let mut count = 0;
        // SAFETY: one validated ASCII byte and writable outputs; null cluster outputs request no clusters.
        let status = unsafe {
            cairo_scaled_font_text_to_glyphs(
                self.font.0.as_ptr(),
                0.,
                0.,
                text.as_ptr().cast(),
                1,
                &mut raw,
                &mut count,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        // Optional native allocation is owned before checking status, so an error cannot leak returned storage.
        let glyphs = NonNull::new(raw).map(Glyphs);
        CairoStatus(status).check(Stage::Mapping)?;
        if count != 1 {
            return Err(OracleError::GlyphCount(count));
        }
        let glyphs = glyphs.ok_or(OracleError::Allocation(Resource::Glyphs))?;
        let mut extents = Extents::default();
        // SAFETY: successful mapping returned exactly one owned glyph; scaled font and output remain live.
        unsafe {
            cairo_scaled_font_glyph_extents(
                self.font.0.as_ptr(),
                glyphs.0.as_ptr(),
                1,
                &mut extents,
            )
        };
        // SAFETY: self owns the live font whose status reports any extents failure.
        CairoStatus(unsafe { cairo_scaled_font_status(self.font.0.as_ptr()) })
            .check(Stage::Extents)?;
        let maximum = f64::from(i32::MAX);
        if !extents.x_advance.is_finite() || !(0. ..=maximum).contains(&extents.x_advance) {
            return Err(OracleError::InvalidAdvance);
        }
        // SAFETY: finite nonnegative input is bounded by i32::MAX, which fits every supported Linux c_long; GNU uses this exact lround conversion.
        let rounded = unsafe { lround(extents.x_advance) };
        let advance = u32::try_from(rounded)
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or(OracleError::InvalidAdvance)?;
        Ok(GnuDeviceAdvance(advance))
    }
}
