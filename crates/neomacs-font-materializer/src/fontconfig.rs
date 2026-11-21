//! GNU-compatible Fontconfig rendering policy shared by metrics and masks.
//!
//! Semantic font selection and font-byte ownership stay with the caller.
//! GNU ftcrfont.c:156-163 prepares a pattern and ftfont.c:1203-1255 preserves
//! its rendering-only properties before Cairo opens the selected face.

use fontconfig_sys as fc;
use freetype::{LcdFilter, RenderMode, face::LoadFlag};
use std::{
    ffi::{CStr, CString},
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr::{self, NonNull},
};

// Fontconfig exposes this since 2.11.91; the raw binding omits it.
unsafe extern "C" {
    fn FcWeightFromOpenType(weight: i32) -> i32;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontRenderMode {
    Alpha,
    Subpixel,
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
pub struct FontRenderOptions {
    hinting: Hinting,
    antialias: bool,
    autohint: bool,
    horizontal_lcd: bool,
    metrics_mode: FontRenderMode,
    lcd_filter: Filter,
}

/// Fontconfig authority borrowed for one policy query. No context is globalized.
#[derive(Clone, Copy)]
pub struct FontconfigContext<'a> {
    raw: *mut fc::FcConfig,
    owner: std::marker::PhantomData<&'a fc::FcConfig>,
}

impl FontconfigContext<'static> {
    /// Deliberately use the process configuration, as GNU's GUI backend does.
    pub fn system() -> Option<Self> {
        // SAFETY: Fontconfig owns the process configuration; never call FcFini.
        (unsafe { fc::FcInit() } != 0).then_some(Self {
            raw: ptr::null_mut(),
            owner: std::marker::PhantomData,
        })
    }
}

impl<'a> FontconfigContext<'a> {
    /// Borrow an explicit configuration without replacing process defaults.
    ///
    /// # Safety
    /// `config` must remain live and protected against concurrent mutation for
    /// `'a`, including every query performed through this context.
    pub unsafe fn from_raw(config: NonNull<fc::FcConfig>) -> Self {
        Self {
            raw: config.as_ptr(),
            owner: std::marker::PhantomData,
        }
    }

    pub fn options_for_face(
        &self,
        info: &fontdb::FaceInfo,
        size: f32,
    ) -> Option<FontRenderOptions> {
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
        self.prepare_options(pattern)
    }

    /// Read attributes of the exact selected file/selector; the returned match
    /// supplies policy only and never substitutes the bytes opened by callers.
    pub fn options_for_file(
        &self,
        file: &Path,
        selector: u32,
        size: f32,
    ) -> Option<FontRenderOptions> {
        self.options_for_file_with_weight(file, selector, size, None)
    }

    /// An explicit replayed weight can affect weight-conditioned rendering rules.
    pub fn options_for_file_with_weight(
        &self,
        file: &Path,
        selector: u32,
        size: f32,
        weight: Option<f32>,
    ) -> Option<FontRenderOptions> {
        let file = CString::new(file.as_os_str().as_bytes()).ok()?;
        let mut count = 0;
        // SAFETY: terminated filename and writable count; a successful query
        // returns a new pattern uniquely owned by its RAII guard.
        let pattern = Pattern(NonNull::new(unsafe {
            fc::FcFreeTypeQuery(
                file.as_ptr().cast(),
                i32::try_from(selector).ok()?,
                ptr::null_mut(),
                &mut count,
            )
        })?);
        pattern.double(c"pixelsize", size.into())?;
        if let Some(weight) = weight {
            if !weight.is_finite() || !(1.0..=1000.0).contains(&weight) {
                return None;
            }
            // SAFETY: the pattern is owned; weight conversion is pure.
            unsafe { fc::FcPatternDel(pattern.raw(), c"weight".as_ptr()) };
            pattern.integer(c"weight", unsafe {
                FcWeightFromOpenType(weight.round() as i32)
            })?;
        }

        self.prepare_options(pattern)
    }

    fn prepare_options(&self, pattern: Pattern) -> Option<FontRenderOptions> {
        // SAFETY: pattern is live and owned; null config means Fontconfig's
        // current configuration. Defaults/substitution do not open our font.
        unsafe {
            if fc::FcConfigSubstitute(self.raw, pattern.raw(), fc::FcMatchPattern) == 0 {
                return None;
            }
            fc::FcDefaultSubstitute(pattern.raw());
        }
        let mut result = fc::FcResultNoMatch;
        // SAFETY: a successful match returns a separately owned pattern. It
        // supplies rendering options only: font bytes/index above remain exact.
        let matched = Pattern(NonNull::new(unsafe {
            fc::FcFontMatch(self.raw, pattern.raw(), &mut result)
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
        let rgba = integer(c"rgba").unwrap_or(0);
        Some(FontRenderOptions {
            hinting,
            antialias: flag(c"antialias", true),
            // Unlike hinting/hintstyle, ftfont_fix_match does not override
            // the prepared font's autohint value with the query's value.
            autohint: matched.boolean(c"autohint").unwrap_or(false),
            horizontal_lcd: match rgba {
                0..=2 => true,
                3..=5 => false,
                _ => return None,
            },
            metrics_mode: if matches!(rgba, 1 | 2) {
                FontRenderMode::Subpixel
            } else {
                FontRenderMode::Alpha
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
}

impl FontRenderOptions {
    /// Preserve the configured native hinting target for device advances.
    pub fn metrics_load_flags(self) -> LoadFlag {
        self.load_flags(false, self.metrics_mode) & !LoadFlag::NO_BITMAP
    }

    pub fn lcd_filter(self) -> LcdFilter {
        self.lcd_filter.native()
    }

    pub fn load_flags(self, disable_hinting: bool, requested: FontRenderMode) -> LoadFlag {
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

    pub fn render_mode(self, requested: FontRenderMode) -> RenderMode {
        if !self.antialias {
            RenderMode::Mono
        } else if requested == FontRenderMode::Subpixel && self.horizontal_lcd {
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

#[cfg(test)]
#[path = "tests/fontconfig_test.rs"]
mod tests;
