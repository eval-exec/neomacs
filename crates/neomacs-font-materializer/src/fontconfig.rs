//! GNU-compatible Fontconfig rendering policy shared by metrics and masks.
//!
//! Semantic font selection and font-byte ownership stay with the caller.
//! GNU ftcrfont.c:156-163 prepares a pattern and ftfont.c:1203-1255 preserves
//! its rendering-only properties before Cairo opens the selected face.

use fontconfig_sys as fc;
use freetype::{LcdFilter, RenderMode, face::LoadFlag};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr::{self, NonNull},
};

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum FontCatalogError {
    #[error("native Fontconfig is unavailable")]
    Unavailable,
    #[error("native Fontconfig catalog enumeration failed")]
    Query,
}

// Fontconfig exposes this since 2.11.91; the raw binding omits it.
unsafe extern "C" {
    fn FcWeightFromOpenType(weight: i32) -> i32;
    fn strtol(
        value: *const std::ffi::c_char,
        end: *mut *mut std::ffi::c_char,
        base: std::ffi::c_int,
    ) -> std::ffi::c_long;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filter {
    None,
    Default,
    Light,
    Legacy,
}

/// Borrowed XGetDefault("Xft", ...) values for one Xlib screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct XftResources<'a> {
    pub antialias: Option<&'a str>,
    pub hinting: Option<&'a str>,
    pub hintstyle: Option<&'a str>,
    pub rgba: Option<&'a str>,
    pub lcdfilter: Option<&'a str>,
}

/// One borrowed GSettings result. A missing key or a value of the wrong type
/// leaves GNU PGTK's current option untouched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GSettingsValue<'a> {
    #[default]
    Missing,
    Invalid,
    String(&'a str),
}

/// Font rendering keys read by GNU PGTK's xsettings.c. This value performs no
/// settings access and borrows strings only for the duration of conversion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PgtkFontSettings<'a> {
    pub antialias: GSettingsValue<'a>,
    pub hinting: GSettingsValue<'a>,
    pub rgba: GSettingsValue<'a>,
}

/// Physical color-component order reported by XRender or Xft.rgba.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SurfaceSubpixelOrder {
    #[default]
    Unknown,
    Rgb,
    Bgr,
    VRgb,
    VBgr,
    None,
}

impl SurfaceSubpixelOrder {
    pub fn from_xrender(value: i32) -> Self {
        match value {
            1 => Self::Rgb,
            2 => Self::Bgr,
            3 => Self::VRgb,
            4 => Self::VBgr,
            5 => Self::None,
            _ => Self::Unknown,
        }
    }

    fn from_fontconfig(value: i32) -> Self {
        // Fontconfig and XRender use the same order constants.
        Self::from_xrender(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SurfaceAntialias {
    #[default]
    Default,
    None,
    Gray,
    Subpixel,
}

/// Cairo font options supplied by a display backend. Xlib merges these only
/// for masks; PGTK also supplies them when opening its metric-bearing font.
/// Default leaves the font pattern in charge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceFontOptions {
    antialias: SurfaceAntialias,
    // GNU retains the order independently while AA is gray or disabled.
    subpixel_order: SurfaceSubpixelOrder,
    hinting: Option<Hinting>,
    // None is Cairo DEFAULT/inherit, distinct from explicit FC_LCD_DEFAULT.
    lcd_filter: Option<Filter>,
}

impl SurfaceFontOptions {
    /// Mirror cairo-xlib-screen.c's screen defaults and Xft precedence.
    pub fn from_xlib_resources(
        resources: XftResources<'_>,
        server_order: SurfaceSubpixelOrder,
    ) -> Self {
        let hinting = if !resources.hinting.and_then(xft_boolean).unwrap_or(true) {
            Some(Hinting::None)
        } else {
            match resources.hintstyle.and_then(xft_integer).unwrap_or(3) {
                0 => Some(Hinting::None),
                1 => Some(Hinting::Slight),
                2 => Some(Hinting::Medium),
                3 => Some(Hinting::Full),
                _ => None,
            }
        };
        let order = resources
            .rgba
            .and_then(xft_integer)
            .map(SurfaceSubpixelOrder::from_fontconfig)
            .unwrap_or(server_order);
        let antialias = if !resources.antialias.and_then(xft_boolean).unwrap_or(true) {
            SurfaceAntialias::None
        } else {
            match order {
                SurfaceSubpixelOrder::Unknown | SurfaceSubpixelOrder::None => {
                    SurfaceAntialias::Gray
                }
                _ => SurfaceAntialias::Subpixel,
            }
        };
        let lcd_filter = match resources.lcdfilter.and_then(xft_integer) {
            Some(0) => Some(Filter::None),
            Some(1) => Some(Filter::Default),
            Some(2) => Some(Filter::Light),
            Some(3) => Some(Filter::Legacy),
            _ => None,
        };
        Self {
            antialias,
            subpixel_order: order,
            hinting,
            lcd_filter,
        }
    }

    /// Start with Cairo DEFAULT values, then apply the exact PGTK strings.
    pub fn from_pgtk_settings(settings: PgtkFontSettings<'_>) -> Self {
        Self::default().with_pgtk_settings(settings)
    }

    /// Mirror GNU xsettings.c's independent setters. Unknown strings, missing
    /// keys and non-string values retain the last valid option, including its
    /// initial Cairo DEFAULT. GSettings has no LCD-filter key here.
    pub fn with_pgtk_settings(mut self, settings: PgtkFontSettings<'_>) -> Self {
        if let GSettingsValue::String(value) = settings.antialias {
            self.antialias = match value {
                "none" => SurfaceAntialias::None,
                "grayscale" => SurfaceAntialias::Gray,
                "rgba" => SurfaceAntialias::Subpixel,
                _ => self.antialias,
            };
        }
        if let GSettingsValue::String(value) = settings.hinting {
            self.hinting = match value {
                "none" => Some(Hinting::None),
                "slight" => Some(Hinting::Slight),
                "medium" => Some(Hinting::Medium),
                "full" => Some(Hinting::Full),
                _ => self.hinting,
            };
        }
        if let GSettingsValue::String(value) = settings.rgba {
            self.subpixel_order = match value {
                "rgb" => SurfaceSubpixelOrder::Rgb,
                "bgr" => SurfaceSubpixelOrder::Bgr,
                "vrgb" => SurfaceSubpixelOrder::VRgb,
                "vbgr" => SurfaceSubpixelOrder::VBgr,
                _ => self.subpixel_order,
            };
        }
        self
    }

    /// Restore the owned connection snapshot without interpreting DEFAULT as
    /// an explicit disabled option. PGTK does not configure the LCD filter.
    pub fn from_desktop_options(
        options: neomacs_display_protocol::DesktopFontRenderOptions,
    ) -> Self {
        use neomacs_display_protocol::{
            DesktopFontAntialias as Aa, DesktopFontHinting as Hint,
            DesktopFontSubpixelOrder as Order,
        };
        Self {
            antialias: match options.antialias {
                Aa::Default => SurfaceAntialias::Default,
                Aa::None => SurfaceAntialias::None,
                Aa::Grayscale => SurfaceAntialias::Gray,
                Aa::Subpixel => SurfaceAntialias::Subpixel,
            },
            hinting: match options.hinting {
                Hint::Default => None,
                Hint::None => Some(Hinting::None),
                Hint::Slight => Some(Hinting::Slight),
                Hint::Medium => Some(Hinting::Medium),
                Hint::Full => Some(Hinting::Full),
            },
            subpixel_order: match options.subpixel_order {
                Order::Default => SurfaceSubpixelOrder::Unknown,
                Order::Rgb => SurfaceSubpixelOrder::Rgb,
                Order::Bgr => SurfaceSubpixelOrder::Bgr,
                Order::VRgb => SurfaceSubpixelOrder::VRgb,
                Order::VBgr => SurfaceSubpixelOrder::VBgr,
            },
            lcd_filter: None,
        }
    }

    /// Publish PGTK's three independent options, retaining inactive/default
    /// values for subsequent edits. Xlib-only LCD filtering is not transported.
    pub fn desktop_options(self) -> neomacs_display_protocol::DesktopFontRenderOptions {
        use neomacs_display_protocol::{
            DesktopFontAntialias as Aa, DesktopFontHinting as Hint, DesktopFontRenderOptions,
            DesktopFontSubpixelOrder as Order,
        };
        DesktopFontRenderOptions {
            antialias: match self.antialias {
                SurfaceAntialias::Default => Aa::Default,
                SurfaceAntialias::None => Aa::None,
                SurfaceAntialias::Gray => Aa::Grayscale,
                SurfaceAntialias::Subpixel => Aa::Subpixel,
            },
            hinting: match self.hinting {
                None => Hint::Default,
                Some(Hinting::None) => Hint::None,
                Some(Hinting::Slight) => Hint::Slight,
                Some(Hinting::Medium) => Hint::Medium,
                Some(Hinting::Full) => Hint::Full,
            },
            subpixel_order: match self.subpixel_order {
                SurfaceSubpixelOrder::Unknown | SurfaceSubpixelOrder::None => Order::Default,
                SurfaceSubpixelOrder::Rgb => Order::Rgb,
                SurfaceSubpixelOrder::Bgr => Order::Bgr,
                SurfaceSubpixelOrder::VRgb => Order::VRgb,
                SurfaceSubpixelOrder::VBgr => Order::VBgr,
            },
        }
    }

    /// None inherits the atlas's pattern order. Explicit gray/mono surfaces
    /// disable LCD even when the font pattern requests RGB antialiasing.
    pub fn raster_subpixel_order(self) -> Option<SurfaceSubpixelOrder> {
        match self.antialias {
            SurfaceAntialias::Default => None,
            SurfaceAntialias::None | SurfaceAntialias::Gray => Some(SurfaceSubpixelOrder::None),
            // Cairo's explicit SUBPIXEL + DEFAULT order selects horizontal RGB;
            // it does not inherit the pattern's BGR/vertical order.
            SurfaceAntialias::Subpixel => Some(match self.subpixel_order {
                SurfaceSubpixelOrder::Unknown => SurfaceSubpixelOrder::Rgb,
                order => order,
            }),
        }
    }
}

fn xft_boolean(value: &str) -> Option<bool> {
    match value.as_bytes() {
        [b't' | b'T' | b'y' | b'Y' | b'1', ..] => Some(true),
        [b'f' | b'F' | b'n' | b'N' | b'0', ..] => Some(false),
        [b'o', b'n' | b'N', ..] => Some(true),
        [b'o', b'f' | b'F', ..] => Some(false),
        _ => None,
    }
}

fn xft_integer(value: &str) -> Option<i32> {
    let value = CString::new(value).ok()?;
    let mut constant = 0;
    // SAFETY: terminated input remains live and Fontconfig writes one integer;
    // FcNameConstant borrows its input and does not retain or mutate it.
    if unsafe { fc::FcNameConstant(value.as_ptr().cast_mut().cast(), &mut constant) } != 0 {
        return Some(constant);
    }
    let mut end = ptr::null_mut();
    // SAFETY: terminated input and writable end pointer. Base zero and accepting
    // any consumed prefix match Cairo's get_integer_default, including hex/octal.
    let parsed = unsafe { strtol(value.as_ptr(), &mut end, 0) };
    (end != value.as_ptr().cast_mut()).then_some(parsed as i32)
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
    metrics_mode: MetricPolicy,
    lcd_filter: Filter,
}

#[derive(Clone, Copy, Debug)]
enum FontMetricMode {
    Alpha,
    HorizontalSubpixel,
    VerticalSubpixel,
}

#[derive(Clone, Copy, Debug)]
enum MetricPolicy {
    Pattern(FontMetricMode),
    Pgtk(LoadFlag),
}

/// Merged raster-only policy. It deliberately exposes no metric probe API.
#[derive(Clone, Copy, Debug)]
pub struct RasterFontOptions(FontRenderOptions);

impl RasterFontOptions {
    pub fn load_flags(self, disable_hinting: bool, requested: FontRenderMode) -> LoadFlag {
        let flags = self.0.load_flags(disable_hinting, requested);
        if self.0.antialias {
            flags
        } else {
            flags | LoadFlag::MONOCHROME
        }
    }

    pub fn render_mode(self, requested: FontRenderMode) -> RenderMode {
        self.0.render_mode(requested)
    }

    pub fn lcd_filter(self) -> LcdFilter {
        self.0.lcd_filter()
    }
}

/// The process configuration's LCD component order, from the match for the
/// generic "Monospace" family (`FC_RGBA`). It is read once per catalog
/// generation: the layout engine clears it with its own catalog caches
/// ([`invalidate_default_subpixel_order`]), and the renderer re-reads it when
/// it rebuilds its atlas.
pub fn default_subpixel_order() -> crate::subpixel::FontconfigSubpixelOrder {
    if let Some(order) = *default_subpixel_order_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
    {
        return order;
    }
    let order = query_default_subpixel_order();
    *default_subpixel_order_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(order);
    order
}

/// Drop the cached [`default_subpixel_order`], with the rest of the answers
/// derived from the previous native catalog generation.
pub fn invalidate_default_subpixel_order() {
    *default_subpixel_order_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

fn default_subpixel_order_cache()
-> &'static std::sync::Mutex<Option<crate::subpixel::FontconfigSubpixelOrder>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<Option<crate::subpixel::FontconfigSubpixelOrder>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

fn query_default_subpixel_order() -> crate::subpixel::FontconfigSubpixelOrder {
    use crate::subpixel::FontconfigSubpixelOrder;
    if FontconfigContext::system().is_none() {
        return FontconfigSubpixelOrder::Unknown;
    }
    let rgba = (|| {
        let pattern = Pattern::new()?;
        pattern.string(fc::constants::FC_FAMILY, c"Monospace")?;
        // SAFETY: the process configuration is initialized and the owned
        // pattern is live; the returned match has its own owner.
        let matched = unsafe {
            if fc::FcConfigSubstitute(ptr::null_mut(), pattern.raw(), fc::FcMatchPattern) == 0 {
                return None;
            }
            fc::FcDefaultSubstitute(pattern.raw());
            let mut result = fc::FcResultNoMatch;
            Pattern(NonNull::new(fc::FcFontMatch(
                ptr::null_mut(),
                pattern.raw(),
                &mut result,
            ))?)
        };
        let mut rgba = 0;
        // SAFETY: the matched pattern is live and owned; Fontconfig writes
        // one integer.
        (unsafe {
            fc::FcPatternGetInteger(matched.raw(), fc::constants::FC_RGBA.as_ptr(), 0, &mut rgba)
        } == fc::FcResultMatch)
            .then_some(rgba)
    })();
    match rgba {
        Some(fc::constants::FC_RGBA_NONE) => FontconfigSubpixelOrder::None,
        Some(fc::constants::FC_RGBA_RGB) => FontconfigSubpixelOrder::Rgb,
        Some(fc::constants::FC_RGBA_BGR) => FontconfigSubpixelOrder::Bgr,
        Some(fc::constants::FC_RGBA_VRGB) => FontconfigSubpixelOrder::VRgb,
        Some(fc::constants::FC_RGBA_VBGR) => FontconfigSubpixelOrder::VBgr,
        _ => FontconfigSubpixelOrder::Unknown,
    }
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
    /// Materialize only sources admitted by the native configuration. A
    /// successful empty catalog remains empty; it never triggers discovery.
    pub(crate) fn font_database(&self) -> Result<fontdb::Database, FontCatalogError> {
        self.font_database_with_count()
            .map(|(database, _)| database)
    }

    fn font_database_with_count(&self) -> Result<(fontdb::Database, usize), FontCatalogError> {
        let catalog = self.font_catalog()?;
        Ok(crate::fontdb::database_from_native_catalog(catalog))
    }

    fn font_catalog(&self) -> Result<crate::fontdb::NativeFontCatalog, FontCatalogError> {
        let pattern = Pattern::new().ok_or(FontCatalogError::Query)?;
        // SAFETY: create an independently owned query projection.
        let objects = CatalogObjects(
            NonNull::new(unsafe { fc::FcObjectSetCreate() }).ok_or(FontCatalogError::Query)?,
        );
        for property in [c"file", c"index"] {
            // SAFETY: owned object set and terminated property; Fontconfig copies it.
            if unsafe { fc::FcObjectSetAdd(objects.0.as_ptr(), property.as_ptr()) } == 0 {
                return Err(FontCatalogError::Query);
            }
        }
        // SAFETY: this borrowed configuration is live and immutable, both
        // query objects remain owned, and the returned font set is independent.
        let font_set = CatalogFonts(
            NonNull::new(unsafe { fc::FcFontList(self.raw, pattern.raw(), objects.0.as_ptr()) })
                .ok_or(FontCatalogError::Query)?,
        );
        // SAFETY: Fontconfig returned this non-null owned font set. Its owner
        // remains live until every filename/selector has been copied below.
        let fonts = unsafe { font_set.0.as_ref() };
        if fonts.nfont < 0 || (fonts.nfont > 0 && fonts.fonts.is_null()) {
            return Err(FontCatalogError::Query);
        }
        let mut files: BTreeMap<PathBuf, BTreeSet<u32>> = BTreeMap::new();
        for i in 0..fonts.nfont as usize {
            // SAFETY: the font set owns nfont entries, all live through this loop.
            let pattern = unsafe { *fonts.fonts.add(i) };
            // SAFETY: the font set owns this pattern and remains live and
            // immutable until the projection has copied its filename/index.
            if let Some((file, index)) = unsafe { catalog_file_face(pattern) } {
                files.entry(file).or_default().insert(index);
            }
        }
        let mut generics = Vec::new();
        if !files.is_empty() {
            use crate::fontdb::GenericFamily::*;
            for family in [Serif, SansSerif, Monospace, Cursive, Fantasy] {
                if let Some(face) = self.generic_catalog_face(family.name()) {
                    generics.push((family, face));
                }
            }
        }
        Ok(crate::fontdb::NativeFontCatalog { files, generics })
    }

    fn generic_catalog_face(&self, family: &str) -> Option<(PathBuf, u32)> {
        let pattern = Pattern::new()?;
        pattern.string(c"family", &CString::new(family).ok()?)?;
        // SAFETY: borrowed configuration and owned pattern are live. This
        // queries aliases in the same authority that supplied the font list.
        unsafe {
            if fc::FcConfigSubstitute(self.raw, pattern.raw(), fc::FcMatchPattern) == 0 {
                return None;
            }
            fc::FcDefaultSubstitute(pattern.raw());
        }
        let mut result = fc::FcResultNoMatch;
        // SAFETY: live borrowed configuration and owned substituted pattern;
        // the returned match has its own owner, retained through projection.
        let matched = Pattern(NonNull::new(unsafe {
            fc::FcFontMatch(self.raw, pattern.raw(), &mut result)
        })?);
        (result == fc::FcResultMatch)
            .then(|| {
                // SAFETY: matched owns the live, immutable pattern through
                // the projection, which copies rather than borrows its data.
                unsafe { catalog_file_face(matched.raw()) }
            })
            .flatten()
    }

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
            // FcFreeTypeQuery describes the file's default/named instance,
            // while the caller replays this explicit weight. Keeping Regular's
            // names or namedinstance=false can force a Regular match and invent
            // synthetic bold for a real variable-font weight. GNU's fresh
            // ftfont_entity_pattern (ftfont.c:1310-1358) carries the numeric
            // selection and file/index, not these descriptive constraints.
            for property in [
                c"style",
                c"stylelang",
                c"fullname",
                c"fullnamelang",
                c"postscriptname",
                c"namedinstance",
            ] {
                // SAFETY: the pattern is uniquely owned and property is a
                // terminated name. Rendering extras and file/index stay intact.
                unsafe { fc::FcPatternDel(pattern.raw(), property.as_ptr()) };
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
            metrics_mode: MetricPolicy::Pattern(match rgba {
                1 | 2 => FontMetricMode::HorizontalSubpixel,
                3 | 4 => FontMetricMode::VerticalSubpixel,
                _ => FontMetricMode::Alpha,
            }),
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

struct CatalogObjects(NonNull<fc::FcObjectSet>);
impl Drop for CatalogObjects {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the object set.
        unsafe { fc::FcObjectSetDestroy(self.0.as_ptr()) };
    }
}

struct CatalogFonts(NonNull<fc::FcFontSet>);
impl Drop for CatalogFonts {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the enumeration and its patterns.
        unsafe { fc::FcFontSetDestroy(self.0.as_ptr()) };
    }
}

/// Copy one native pattern's file and collection selector.
///
/// # Safety
/// A non-null `pattern` must point to a live Fontconfig pattern protected
/// against concurrent mutation for this call. Its owning pattern or font set
/// must retain the filename storage until this function finishes copying it.
unsafe fn catalog_file_face(pattern: *mut fc::FcPattern) -> Option<(PathBuf, u32)> {
    if pattern.is_null() {
        return None;
    }
    let mut file = ptr::null_mut();
    let mut index = 0;
    // SAFETY: callers retain the owning pattern/font set; output storage is
    // writable. Copy the filename bytes before the owner is dropped.
    unsafe {
        if fc::FcPatternGetString(pattern, c"file".as_ptr(), 0, &mut file) != fc::FcResultMatch
            || file.is_null()
            || fc::FcPatternGetInteger(pattern, c"index".as_ptr(), 0, &mut index)
                != fc::FcResultMatch
        {
            return None;
        }
        Some((
            PathBuf::from(std::ffi::OsStr::from_bytes(
                CStr::from_ptr(file.cast()).to_bytes(),
            )),
            u32::try_from(index).ok()?,
        ))
    }
}

impl FontRenderOptions {
    /// Cairo's FT merge gives explicit surface settings precedence, except
    /// that the font pattern's NONE values disable hinting, AA or filtering.
    /// This copy affects masks only; this font's metric policy stays intact.
    pub fn for_surface(self, surface: SurfaceFontOptions) -> RasterFontOptions {
        let mut raster = self;
        // With antialias=false Cairo never loads the pattern's hintstyle or
        // LCD filter; those remain DEFAULT until the surface merge.
        let pattern_hinting = self.antialias.then_some(self.hinting);
        raster.hinting = if pattern_hinting == Some(Hinting::None) {
            Hinting::None
        } else {
            // Cairo ignores pattern hintstyle when pattern AA is disabled;
            // unresolved DEFAULT then loads the hinted monochrome target.
            surface.hinting.or(pattern_hinting).unwrap_or(Hinting::Full)
        };
        raster.antialias = self.antialias && surface.antialias != SurfaceAntialias::None;
        raster.horizontal_lcd = match surface.antialias {
            SurfaceAntialias::Default => self.horizontal_lcd,
            SurfaceAntialias::Subpixel => matches!(
                surface.subpixel_order,
                SurfaceSubpixelOrder::Unknown
                    | SurfaceSubpixelOrder::Rgb
                    | SurfaceSubpixelOrder::Bgr
            ),
            _ => false,
        };
        raster.lcd_filter = if self.antialias && self.lcd_filter == Filter::None {
            Filter::None
        } else {
            surface.lcd_filter.unwrap_or(self.lcd_filter)
        };
        RasterFontOptions(raster)
    }

    /// PGTK supplies GSettings options at scaled-font creation, so its merged
    /// hinting/AA also affects glyph advances and opened-font metrics. Xlib's
    /// raster-only `for_surface` deliberately does not take this path.
    pub fn for_pgtk(self, surface: SurfaceFontOptions) -> Self {
        let pattern_mode = match self.metrics_mode {
            MetricPolicy::Pattern(mode) => mode,
            MetricPolicy::Pgtk(flags) => match flags.bits() & 0xf0000 {
                value if value == LoadFlag::TARGET_LCD.bits() => FontMetricMode::HorizontalSubpixel,
                value if value == LoadFlag::TARGET_LCD_V.bits() => FontMetricMode::VerticalSubpixel,
                _ => FontMetricMode::Alpha,
            },
        };
        // Cairo copies both AA and order from a SUBPIXEL pattern when surface
        // AA is DEFAULT. A retained order-only GSettings edit does not override
        // that branch until AA is explicitly set to "rgba".
        let mode = match surface.antialias {
            SurfaceAntialias::Default => pattern_mode,
            SurfaceAntialias::None | SurfaceAntialias::Gray => FontMetricMode::Alpha,
            SurfaceAntialias::Subpixel => match surface.subpixel_order {
                SurfaceSubpixelOrder::VRgb | SurfaceSubpixelOrder::VBgr => {
                    FontMetricMode::VerticalSubpixel
                }
                SurfaceSubpixelOrder::Unknown
                | SurfaceSubpixelOrder::Rgb
                | SurfaceSubpixelOrder::Bgr => FontMetricMode::HorizontalSubpixel,
                SurfaceSubpixelOrder::None => FontMetricMode::Alpha,
            },
        };
        let mut merged = self.for_surface(surface).0;
        let requested = match mode {
            FontMetricMode::HorizontalSubpixel => FontRenderMode::Subpixel,
            FontMetricMode::Alpha | FontMetricMode::VerticalSubpixel => FontRenderMode::Alpha,
        };
        let mut flags = merged.load_flags(false, requested) & !LoadFlag::NO_BITMAP;
        if !merged.antialias {
            flags |= LoadFlag::MONOCHROME;
        } else if merged.hinting == Hinting::Full
            && matches!(mode, FontMetricMode::VerticalSubpixel)
        {
            // Alpha above chose NORMAL (zero target bits), leaving the
            // vertical LCD target distinct from the horizontal renderer mode.
            flags |= LoadFlag::TARGET_LCD_V;
        }
        merged.metrics_mode = MetricPolicy::Pgtk(flags);
        merged
    }

    /// Preserve the configured native hinting target for device advances.
    pub fn metrics_load_flags(self) -> LoadFlag {
        match self.metrics_mode {
            MetricPolicy::Pattern(mode) => {
                // Preserve the pre-PGTK X11 metric policy: only horizontal
                // Fontconfig LCD used the LCD target here.
                let requested = match mode {
                    FontMetricMode::HorizontalSubpixel => FontRenderMode::Subpixel,
                    FontMetricMode::Alpha | FontMetricMode::VerticalSubpixel => {
                        FontRenderMode::Alpha
                    }
                };
                self.load_flags(false, requested) & !LoadFlag::NO_BITMAP
            }
            MetricPolicy::Pgtk(flags) => flags,
        }
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

#[cfg(test)]
#[path = "tests/surface_font_options_test.rs"]
mod surface_tests;

#[cfg(test)]
#[path = "tests/font_catalog_test.rs"]
mod catalog_tests;
