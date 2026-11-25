//! Window and display vocabulary shared by the evaluator and every frontend:
//! native window state, monitors, desktop font preferences, resource queries
//! and frame font sizes. Values that carry an invariant decode through it.

use std::num::NonZeroU32;

/// WM size hints for a GUI frame's native window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GuiFrameGeometryHints {
    pub base_width: u32,
    pub base_height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub width_inc: u32,
    pub height_inc: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WindowFullscreenMode {
    None,
    Fullboth,
    Fullscreen,
    Fullwidth,
    Fullheight,
    Maximized,
}

/// Native-window projection of GNU's visible/iconified/invisible frame state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WindowVisibility {
    Visible,
    Iconified,
    Invisible,
}

impl WindowVisibility {
    #[must_use]
    pub const fn is_visible_or_iconified(self) -> bool {
        matches!(self, Self::Visible | Self::Iconified)
    }
}

/// Monitor information transported from the frontend to the evaluator.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MonitorInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    pub width_mm: i32,
    pub height_mm: i32,
    pub name: Option<String>,
}

/// Desktop applications distinguish document fixed-width and normal UI fonts.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, strum::EnumIter, serde::Serialize, serde::Deserialize,
)]
pub enum SystemFontRole {
    Monospace,
    Application,
}

/// An owned native font description. Its size/style syntax is interpreted by
/// the same GNU-compatible parser used for explicit Lisp font requests.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SystemFontName(String);

/// A desktop font name that is blank or contains NUL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidSystemFontName;

impl std::fmt::Display for InvalidSystemFontName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a system font name must be non-blank and contain no NUL")
    }
}

impl TryFrom<String> for SystemFontName {
    type Error = InvalidSystemFontName;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::new(name).ok_or(InvalidSystemFontName)
    }
}

impl From<SystemFontName> for String {
    fn from(name: SystemFontName) -> Self {
        name.0
    }
}

impl SystemFontName {
    #[must_use]
    pub fn new(name: String) -> Option<Self> {
        (!name.trim().is_empty() && !name.contains('\0')).then_some(Self(name))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One coherent discovery result, safe to transfer between native startup and
/// the evaluator. Absent preferences preserve the existing platform fallback.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SystemFonts {
    monospace: Option<SystemFontName>,
    application: Option<SystemFontName>,
}

impl SystemFonts {
    #[must_use]
    pub fn new(monospace: Option<SystemFontName>, application: Option<SystemFontName>) -> Self {
        Self {
            monospace,
            application,
        }
    }

    #[must_use]
    pub fn get(&self, role: SystemFontRole) -> Option<&SystemFontName> {
        match role {
            SystemFontRole::Monospace => self.monospace.as_ref(),
            SystemFontRole::Application => self.application.as_ref(),
        }
    }
}

/// Fully qualified GNU resource keys. Native backends own their database
/// precedence, including whether explicit resources bypass inhibition.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GuiResourceQuery {
    pub name: String,
    pub class: String,
    pub inhibit_native: bool,
}

impl GuiResourceQuery {
    #[must_use]
    pub fn normalize_instance_name(name: &str) -> String {
        let valid = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_';
        if name.bytes().all(valid) {
            return name.into();
        }
        if name.bytes().filter(|byte| valid(*byte)).count() < 2 {
            return "emacs".into();
        }
        name.bytes()
            .map(|byte| if valid(byte) { char::from(byte) } else { '_' })
            .collect()
    }
}

/// A finite, positive scalar used by point-size and relative-size requests.
///
/// Lisp numbers are validated once when they cross the display-host seam, so
/// native font adapters never need to recover from NaN, infinity, zero, or a
/// negative size.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct PositiveFontScalar(f64);

/// A font scalar that is not finite and positive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonPositiveFontScalar;

impl std::fmt::Display for NonPositiveFontScalar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a font scalar must be finite and positive")
    }
}

impl TryFrom<f64> for PositiveFontScalar {
    type Error = NonPositiveFontScalar;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(NonPositiveFontScalar)
    }
}

impl From<PositiveFontScalar> for f64 {
    fn from(value: PositiveFontScalar) -> Self {
        value.0
    }
}

impl PositiveFontScalar {
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A requested point size: finite and positive, and representable in GNU's
/// tenths-of-a-point face heights (`value * 10` within `1..=i32::MAX`).
/// Decoding checks the same range as [`FontPoints::new`].
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct FontPoints(PositiveFontScalar);

/// A point size outside the range [`FontPoints`] admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontPointsOutOfRange;

impl std::fmt::Display for FontPointsOutOfRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a point size must be finite with tenths of a point in 1..=i32::MAX")
    }
}

impl FontPoints {
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        let value = PositiveFontScalar::new(value)?;
        ((1.0..=f64::from(i32::MAX)).contains(&(value.get() * 10.0))).then_some(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0.get()
    }
}

impl TryFrom<f64> for FontPoints {
    type Error = FontPointsOutOfRange;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(FontPointsOutOfRange)
    }
}

impl From<FontPoints> for f64 {
    fn from(value: FontPoints) -> Self {
        value.get()
    }
}

/// The semantic size attached to a frame-font request.
///
/// GNU keeps integer font-spec sizes in pixels until font realization, while
/// floating sizes are points. Keeping those units in the type prevents a
/// frame-independent conversion from silently assuming one platform DPI.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FrameFontSize {
    Default,
    Pixels(NonZeroU32),
    Points(FontPoints),
    Relative(PositiveFontScalar),
}

impl FrameFontSize {
    #[must_use]
    pub fn pixels(value: i64) -> Option<Self> {
        u32::try_from(value)
            .ok()
            .and_then(NonZeroU32::new)
            .map(Self::Pixels)
    }

    #[must_use]
    pub fn points(value: f64) -> Option<Self> {
        FontPoints::new(value).map(Self::Points)
    }

    #[must_use]
    pub fn relative(value: f64) -> Option<Self> {
        PositiveFontScalar::new(value).map(Self::Relative)
    }
}
