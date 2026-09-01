//! Face system for text appearance attributes.
//!
//! A *face* defines how text is displayed: foreground/background colors,
//! font weight, slant, underline, etc.  Faces can inherit from each other
//! and are merged at display time.
//!
//! This module provides:
//! - `FaceAttribute` — individual attribute values
//! - `Face` — a collection of attributes (some may be unspecified)
//! - `FaceTable` — global registry mapping names to face definitions
//! - Face merging (overlay face on top of base face)

use crate::emacs_core::intern::{SymId, resolve_sym};
use crate::emacs_core::value::{Value, ValueKind};
use crate::gc_trace::GcTrace;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::collections::{HashMap, HashSet};
use strum::{EnumString, IntoStaticStr};

// X11 color table generated at compile time from etc/rgb.txt
include!(concat!(env!("OUT_DIR"), "/x11_colors.rs"));

/// Identity of a GNU Lisp face in a frame's lface table.
///
/// This is deliberately distinct from `neomacs_display_protocol::FaceId`,
/// which identifies a realized render face.  Display-table glyph codes carry
/// this Lisp identity; redisplay merges that lface over the surrounding
/// realized face and only then allocates a render-face id.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LispFaceId(i64);

impl LispFaceId {
    pub const fn new(id: i64) -> Option<Self> {
        if id >= 0 { Some(Self(id)) } else { None }
    }

    /// A display-vector face id of zero means "keep the saved face" in GNU.
    pub const fn glyph_override(id: i64) -> Option<Self> {
        if id > 0 { Some(Self(id)) } else { None }
    }

    pub const fn get(self) -> i64 {
        self.0
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    PartialEq,
    EnumString,
    IntoPrimitive,
    IntoStaticStr,
    TryFromPrimitive,
)]
#[repr(u8)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
pub enum LFaceAttr {
    Family = 1,
    Foundry,
    Width,
    Height,
    Weight,
    Slant,
    Underline,
    InverseVideo,
    Foreground,
    Background,
    Stipple,
    Overline,
    StrikeThrough,
    Box,
    Font,
    Inherit,
    Fontset,
    DistantForeground,
    Extend,
}

impl LFaceAttr {
    pub(crate) fn index(self) -> usize {
        usize::from(u8::from(self))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn from_index(index: usize) -> Option<Self> {
        let index = u8::try_from(index).ok()?;
        Self::try_from(index).ok()
    }

    pub(crate) fn keyword(self) -> &'static str {
        self.into()
    }

    pub(crate) const fn is_discrete_boolean(self) -> bool {
        matches!(
            self,
            LFaceAttr::Underline
                | LFaceAttr::Overline
                | LFaceAttr::StrikeThrough
                | LFaceAttr::InverseVideo
                | LFaceAttr::Extend
        )
    }

    pub(crate) fn from_keyword(name: &str) -> Option<Self> {
        name.strip_prefix(':')?.parse().ok()
    }
}

pub(crate) const LFACE_VECTOR_SIZE: usize = 20;

pub(crate) const LFACE_ATTRS: [LFaceAttr; LFACE_VECTOR_SIZE - 1] = [
    LFaceAttr::Family,
    LFaceAttr::Foundry,
    LFaceAttr::Width,
    LFaceAttr::Height,
    LFaceAttr::Weight,
    LFaceAttr::Slant,
    LFaceAttr::Underline,
    LFaceAttr::InverseVideo,
    LFaceAttr::Foreground,
    LFaceAttr::Background,
    LFaceAttr::Stipple,
    LFaceAttr::Overline,
    LFaceAttr::StrikeThrough,
    LFaceAttr::Box,
    LFaceAttr::Font,
    LFaceAttr::Inherit,
    LFaceAttr::Fontset,
    LFaceAttr::DistantForeground,
    LFaceAttr::Extend,
];

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

/// A REALIZED face color: RGBA in sRGB space (0-255 per channel), the
/// render-layer output of realizing a [`SpecifiedColor`] under a stated
/// frame-class policy (`FaceColorResolver::realize` in xfaces) — GNU's
/// realized-face pixel, as opposed to the lface-vector spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RealizedColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    /// What a TERMINAL frame's realized face carries for this colour: the
    /// INDEX `tty-color-desc` returned, which is the whole of GNU's
    /// `face->foreground` on a tty (`map_tty_color`, src/xfaces.c:6620-6694).
    ///
    /// It rides inside the colour rather than beside it because every merge,
    /// `:inherit` walk and face copy moves the colour as one value; a separate
    /// slot could be updated in one place and not the other, and the writer
    /// would then be back to guessing. `None` means "not realized for a
    /// terminal frame" -- GNU's `FACE_TTY_DEFAULT_COLOR`, which
    /// `face_tty_specified_color` (src/dispextern.h:1933-1936) rejects, so the
    /// writer emits no colour rather than one it invented.
    pub terminal: Option<neomacs_display_protocol::TerminalColor>,
}

/// Compatibility alias for [`RealizedColor`]: the pre-split name used
/// throughout the face-table path and external crates. New code on the
/// render side should say `RealizedColor`; spec-side code should carry
/// [`SpecifiedColor`] instead.
pub type Color = RealizedColor;

impl RealizedColor {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r,
            g,
            b,
            a: 255,
            terminal: None,
        }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r,
            g,
            b,
            a,
            terminal: None,
        }
    }

    /// The same colour, realized for a terminal frame: GNU `map_tty_color`
    /// storing `tty-color-desc`'s INDEX in the realized face's colour slot.
    #[must_use]
    pub const fn with_terminal(self, terminal: neomacs_display_protocol::TerminalColor) -> Self {
        Self {
            terminal: Some(terminal),
            ..self
        }
    }

    /// Pack as an sRGB pixel (`0x00RRGGBB`) — the form face colors take
    /// everywhere they cross into display code and image cache keys.
    #[must_use]
    pub const fn to_pixel(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    /// Parse a GNU/X11 hex color: `#` followed by 3, 6, 9, or 12 hex digits —
    /// i.e. 1..=4 hex digits PER CHANNEL (4/8/12/16 bits). Each channel is
    /// downscaled to 8 bits using its most-significant bits, so the wide forms
    /// parse correctly instead of being dropped. In particular `#RRRRGGGGBBBB`
    /// (16-bit channels) is what Emacs' `color-values`/blend math emits — e.g.
    /// indent-bars' computed bar colors like `#ffff33333333` — and dropping it
    /// left those faces with no foreground (rendered as the default black).
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.strip_prefix('#')?;
        if s.is_empty() || s.len() % 3 != 0 {
            return None;
        }
        let per = s.len() / 3;
        if per > 4 {
            return None;
        }
        let bits = 4 * per as u32;
        let channel = |index: usize| -> Option<u8> {
            let start = index * per;
            let raw = u16::from_str_radix(&s[start..start + per], 16).ok()?;
            Some(if bits >= 8 {
                // Take the most-significant 8 bits (8/12/16-bit channels).
                (raw >> (bits - 8)) as u8
            } else {
                // 4-bit `#RGB`: replicate the nibble so 0xf -> 0xff (== v*17).
                ((raw << 4) | raw) as u8
            })
        };
        Some(Color::rgb(channel(0)?, channel(1)?, channel(2)?))
    }

    /// Convert to "#RRGGBB" hex string.
    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Named color lookup (common X11/Emacs colors).
    pub fn from_name(name: &str) -> Option<Self> {
        x11_color_lookup(name).map(|(r, g, b)| Color::rgb(r, g, b))
    }

    /// Parse a color spec: hex string or named color.
    pub fn parse(spec: &str) -> Option<Self> {
        if spec.starts_with('#') {
            Self::from_hex(spec)
        } else {
            Self::from_name(spec)
        }
    }
}

/// Realize one colour SPEC string the way GNU's `tty-color-desc` does
/// (lisp/term/tty-colors.el:975-987), or context-free when there is no palette.
///
/// The name match comes first, exactly as `map_tty_color` takes it
/// (src/xfaces.c:6640-6648): it is the half no RGB search can reproduce,
/// because `tty-color-define` can put a name at an index its own RGB would
/// never approximate to.
fn realize_color_spec(
    spec: &str,
    palette: Option<&neomacs_display_protocol::TtyPalette>,
) -> Option<RealizedColor> {
    let Some(palette) = palette.filter(|palette| !palette.is_empty()) else {
        return RealizedColor::parse(spec);
    };
    if let Some((terminal, (r, g, b))) = palette.named(spec) {
        return Some(RealizedColor::rgb(r, g, b).with_terminal(terminal));
    }
    let parsed = RealizedColor::parse(spec)?;
    match palette.approximate(parsed.r, parsed.g, parsed.b) {
        Some((terminal, (r, g, b))) => Some(RealizedColor::rgb(r, g, b).with_terminal(terminal)),
        // Only an all-gray palette can answer nothing; GNU's callers treat that
        // as "not resolved" and keep no colour rather than substituting one.
        None => Some(parsed),
    }
}

// ---------------------------------------------------------------------------
// Specified vs realized colors (GNU xfaces staging)
// ---------------------------------------------------------------------------

/// A face color SPECIFICATION, the form colors take inside lface vectors
/// before realization — GNU's lface slots store strings/`unspecified`, and
/// only `realize_gui_face`/`realize_tty_face` produce pixels (xfaces.c).
///
/// Deliberately NO `From<SpecifiedColor>` for a realized color and no
/// name-to-pixel shortcut here: `FaceColorResolver::realize` (xfaces) is the
/// only bridge, so every realization states its frame-class policy.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SpecifiedColor {
    /// A color string, e.g. `"white"` or `"#ff0000"` — meaning depends on
    /// the frame class. This covers HEX strings too: GNU routes every lface
    /// color string through realization (`map_tty_color` ->
    /// `tty-color-desc`), which approximates hex through the terminal
    /// palette on ttys without 24-bit color, so hex is not
    /// frame-independent and the exact spec string must survive to
    /// realization (the tty palette is keyed by it).
    Named(String),
    /// Frame-independent RGB, for programmatically constructed colors that
    /// never had a spec string. `parse` never produces this.
    Rgb(u8, u8, u8),
    /// The `unspecified` token: attribute carries no color.
    Unspecified,
    /// `"unspecified-fg"` — the frame's default foreground.
    FrameForeground,
    /// `"unspecified-bg"` — the frame's default background.
    FrameBackground,
}

impl SpecifiedColor {
    /// Classify a color spec string. Total: unknown names stay `Named` —
    /// realization is where lookup failure surfaces.
    pub fn parse(spec: &str) -> Self {
        match spec {
            "unspecified" => Self::Unspecified,
            "unspecified-fg" => Self::FrameForeground,
            "unspecified-bg" => Self::FrameBackground,
            _ => Self::Named(spec.to_owned()),
        }
    }

    /// The lface-vector string form of this spec, when it has one.
    pub fn spec_string(&self) -> Option<&str> {
        match self {
            Self::Named(s) => Some(s),
            Self::Rgb(..) => None,
            Self::Unspecified => Some("unspecified"),
            Self::FrameForeground => Some("unspecified-fg"),
            Self::FrameBackground => Some("unspecified-bg"),
        }
    }
}

// ---------------------------------------------------------------------------
// Underline style
// ---------------------------------------------------------------------------

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoPrimitive, IntoStaticStr, TryFromPrimitive,
)]
#[repr(u8)]
#[strum(serialize_all = "kebab-case")]
pub enum UnderlineStyle {
    Line = 1,
    DoubleLine = 2,
    Wave = 3,
    Dots = 4,
    Dashes = 5,
}

impl UnderlineStyle {
    pub fn from_symbol(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn symbol_name(self) -> &'static str {
        self.into()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }

    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }
}

/// Where an underline is placed vertically.
///
/// GNU distinguishes the font's recommended underline metric from the
/// `(:position POSITION)` face syntax.  Any non-nil POSITION selects the
/// glyph row's descent line; a non-negative integer additionally moves the
/// line that many pixels above it (`src/xfaces.c`, `src/xterm.c`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnderlinePosition {
    /// Use the concrete font's recommended offset below the baseline.
    #[default]
    FontMetric,
    /// Align to the bottom of the glyph row, optionally inset upward.
    DescentLine { pixels_above: u32 },
}

impl UnderlinePosition {
    pub fn from_lisp(value: &Value) -> Self {
        if value.is_nil() {
            return Self::FontMetric;
        }
        let pixels_above = value
            .as_fixnum()
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0);
        Self::DescentLine { pixels_above }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Underline {
    pub style: UnderlineStyle,
    pub color: Option<Color>,
    pub position: UnderlinePosition,
}

/// A decoration attribute in a partial Lisp face specification.
///
/// GNU face vectors distinguish `unspecified` (inherit the lower-priority
/// value) from nil (explicitly disable the decoration).  `Option<T>` cannot
/// represent both states, so decoration composition must retain this tri-state
/// until the face is fully realized.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum FaceDecoration<T> {
    #[default]
    Unspecified,
    Disabled,
    Enabled(T),
}

impl<T> FaceDecoration<T> {
    pub fn enabled(&self) -> Option<&T> {
        match self {
            Self::Enabled(value) => Some(value),
            Self::Unspecified | Self::Disabled => None,
        }
    }

    fn merge_over(&self, base: &Self) -> Self
    where
        T: Clone,
    {
        match self {
            Self::Unspecified => base.clone(),
            Self::Disabled => Self::Disabled,
            Self::Enabled(value) => Self::Enabled(value.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// Box border
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct BoxBorder {
    pub color: Option<Color>,
    pub width: i32,
    pub style: BoxStyle,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoPrimitive, IntoStaticStr, TryFromPrimitive,
)]
#[repr(u8)]
#[strum(serialize_all = "kebab-case")]
pub enum BoxStyle {
    #[strum(to_string = "flat-button")]
    Flat = 1,
    #[strum(to_string = "released-button")]
    Raised = 2,
    #[strum(to_string = "pressed-button")]
    Pressed = 3,
}

impl BoxStyle {
    pub fn from_symbol(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn symbol_name(self) -> &'static str {
        self.into()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }

    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }
}

// ---------------------------------------------------------------------------
// Font weight / slant / width
// ---------------------------------------------------------------------------

/// GNU Emacs Lisp face weight domain.
///
/// GNU keeps face weights as symbols in a fixed ordered table
/// (`src/font.c:weight_table`).  Display backends still need CSS-style
/// numeric weights, so use [`FontWeight::css_weight`] at renderer boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(ascii_case_insensitive)]
pub enum FontWeight {
    #[strum(to_string = "thin")]
    Thin,
    #[strum(to_string = "ultra-light")]
    UltraLight,
    #[strum(to_string = "ultralight")]
    Ultralight,
    #[strum(to_string = "extra-light")]
    ExtraLight,
    #[strum(to_string = "extralight")]
    Extralight,
    #[strum(to_string = "light")]
    Light,
    #[strum(to_string = "semi-light")]
    SemiLight,
    #[strum(to_string = "semilight")]
    Semilight,
    #[strum(to_string = "demilight")]
    Demilight,
    #[strum(to_string = "regular")]
    Regular,
    #[strum(to_string = "normal")]
    Normal,
    #[strum(to_string = "unspecified")]
    Unspecified,
    #[strum(to_string = "book")]
    Book,
    #[strum(to_string = "medium")]
    Medium,
    #[strum(to_string = "semi-bold")]
    SemiBold,
    #[strum(to_string = "semibold")]
    Semibold,
    #[strum(to_string = "demibold")]
    Demibold,
    #[strum(to_string = "demi-bold")]
    DemiBold,
    #[strum(to_string = "demi")]
    Demi,
    #[strum(to_string = "bold")]
    Bold,
    #[strum(to_string = "extra-bold")]
    ExtraBold,
    #[strum(to_string = "extrabold")]
    Extrabold,
    #[strum(to_string = "ultra-bold")]
    UltraBold,
    #[strum(to_string = "ultrabold")]
    Ultrabold,
    #[strum(to_string = "black")]
    Black,
    #[strum(to_string = "heavy")]
    Heavy,
    #[strum(to_string = "ultra-heavy")]
    UltraHeavy,
    #[strum(to_string = "ultraheavy")]
    Ultraheavy,
}

impl FontWeight {
    pub const THIN: Self = Self::Thin;
    pub const ULTRA_LIGHT: Self = Self::UltraLight;
    pub const EXTRA_LIGHT: Self = Self::ExtraLight;
    pub const LIGHT: Self = Self::Light;
    pub const SEMI_LIGHT: Self = Self::SemiLight;
    pub const NORMAL: Self = Self::Normal;
    pub const MEDIUM: Self = Self::Medium;
    pub const SEMI_BOLD: Self = Self::SemiBold;
    pub const BOLD: Self = Self::Bold;
    pub const EXTRA_BOLD: Self = Self::ExtraBold;
    pub const ULTRA_BOLD: Self = Self::UltraBold;
    pub const BLACK: Self = Self::Black;
    pub const ULTRA_HEAVY: Self = Self::UltraHeavy;

    pub fn from_symbol(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn from_css_weight(weight: u16) -> Self {
        match weight {
            0..=150 => Self::Thin,
            151..=250 => Self::ExtraLight,
            251..=325 => Self::Light,
            326..=375 => Self::SemiLight,
            376..=450 => Self::Normal,
            451..=550 => Self::Medium,
            551..=650 => Self::SemiBold,
            651..=750 => Self::Bold,
            751..=850 => Self::ExtraBold,
            851..=925 => Self::Black,
            _ => Self::UltraHeavy,
        }
    }

    pub fn from_gnu_style_code(code: i64) -> Option<Self> {
        let code = u16::try_from(code).ok()?;
        let numeric = code >> 8;
        let row = (code >> 4) & 0x0f;
        let alias = code & 0x0f;
        match (numeric, row, alias) {
            (0, 0, 0) => Some(Self::Thin),
            (40, 1, 0) => Some(Self::UltraLight),
            (40, 1, 1) => Some(Self::Ultralight),
            (40, 1, 2) => Some(Self::ExtraLight),
            (40, 1, 3) => Some(Self::Extralight),
            (50, 2, 0) => Some(Self::Light),
            (55, 3, 0) => Some(Self::SemiLight),
            (55, 3, 1) => Some(Self::Semilight),
            (55, 3, 2) => Some(Self::Demilight),
            (80, 4, 0) => Some(Self::Regular),
            (80, 4, 1) => Some(Self::Normal),
            (80, 4, 2) => Some(Self::Unspecified),
            (80, 4, 3) => Some(Self::Book),
            (100, 5, 0) => Some(Self::Medium),
            (180, 6, 0) => Some(Self::SemiBold),
            (180, 6, 1) => Some(Self::Semibold),
            (180, 6, 2) => Some(Self::Demibold),
            (180, 6, 3) => Some(Self::DemiBold),
            (180, 6, 4) => Some(Self::Demi),
            (200, 7, 0) => Some(Self::Bold),
            (205, 8, 0) => Some(Self::ExtraBold),
            (205, 8, 1) => Some(Self::Extrabold),
            (205, 8, 2) => Some(Self::UltraBold),
            (205, 8, 3) => Some(Self::Ultrabold),
            (210, 9, 0) => Some(Self::Black),
            (210, 9, 1) => Some(Self::Heavy),
            (250, 10, 0) => Some(Self::UltraHeavy),
            (250, 10, 1) => Some(Self::Ultraheavy),
            _ => None,
        }
    }

    pub fn from_dump_code(code: u16) -> Self {
        match code {
            100 => Self::Thin,
            101 => Self::UltraLight,
            102 => Self::Ultralight,
            200 => Self::ExtraLight,
            201 => Self::Extralight,
            300 => Self::Light,
            350 => Self::SemiLight,
            351 => Self::Semilight,
            352 => Self::Demilight,
            401 => Self::Regular,
            400 => Self::Normal,
            402 => Self::Unspecified,
            403 => Self::Book,
            500 => Self::Medium,
            600 => Self::SemiBold,
            601 => Self::Semibold,
            602 => Self::Demibold,
            603 => Self::DemiBold,
            604 => Self::Demi,
            700 => Self::Bold,
            800 => Self::ExtraBold,
            801 => Self::Extrabold,
            802 => Self::UltraBold,
            803 => Self::Ultrabold,
            900 => Self::Black,
            901 => Self::Heavy,
            950 => Self::UltraHeavy,
            951 => Self::Ultraheavy,
            other => Self::from_css_weight(other),
        }
    }

    pub fn dump_code(self) -> u16 {
        match self {
            Self::Thin => 100,
            Self::UltraLight => 101,
            Self::Ultralight => 102,
            Self::ExtraLight => 200,
            Self::Extralight => 201,
            Self::Light => 300,
            Self::SemiLight => 350,
            Self::Semilight => 351,
            Self::Demilight => 352,
            Self::Regular => 401,
            Self::Normal => 400,
            Self::Unspecified => 402,
            Self::Book => 403,
            Self::Medium => 500,
            Self::SemiBold => 600,
            Self::Semibold => 601,
            Self::Demibold => 602,
            Self::DemiBold => 603,
            Self::Demi => 604,
            Self::Bold => 700,
            Self::ExtraBold => 800,
            Self::Extrabold => 801,
            Self::UltraBold => 802,
            Self::Ultrabold => 803,
            Self::Black => 900,
            Self::Heavy => 901,
            Self::UltraHeavy => 950,
            Self::Ultraheavy => 951,
        }
    }

    pub fn gnu_numeric(self) -> u16 {
        match self {
            Self::Thin => 0,
            Self::UltraLight | Self::Ultralight | Self::ExtraLight | Self::Extralight => 40,
            Self::Light => 50,
            Self::SemiLight | Self::Semilight | Self::Demilight => 55,
            Self::Regular | Self::Normal | Self::Unspecified | Self::Book => 80,
            Self::Medium => 100,
            Self::SemiBold | Self::Semibold | Self::Demibold | Self::DemiBold | Self::Demi => 180,
            Self::Bold => 200,
            Self::ExtraBold | Self::Extrabold | Self::UltraBold | Self::Ultrabold => 205,
            Self::Black | Self::Heavy => 210,
            Self::UltraHeavy | Self::Ultraheavy => 250,
        }
    }

    pub fn css_weight(self) -> u16 {
        match self {
            Self::Thin => 100,
            Self::UltraLight | Self::Ultralight | Self::ExtraLight | Self::Extralight => 200,
            Self::Light => 300,
            Self::SemiLight | Self::Semilight | Self::Demilight => 350,
            Self::Regular | Self::Normal | Self::Unspecified | Self::Book => 400,
            Self::Medium => 500,
            Self::SemiBold | Self::Semibold | Self::Demibold | Self::DemiBold | Self::Demi => 600,
            Self::Bold => 700,
            Self::ExtraBold | Self::Extrabold | Self::UltraBold | Self::Ultrabold => 800,
            Self::Black | Self::Heavy => 900,
            Self::UltraHeavy | Self::Ultraheavy => 950,
        }
    }

    pub fn symbol_name(self) -> &'static str {
        self.into()
    }

    pub fn is_bold(self) -> bool {
        matches!(
            self,
            Self::SemiBold
                | Self::Semibold
                | Self::Demibold
                | Self::DemiBold
                | Self::Demi
                | Self::Bold
                | Self::ExtraBold
                | Self::Extrabold
                | Self::UltraBold
                | Self::Ultrabold
        )
    }
}

/// Font slant.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr, IntoPrimitive, TryFromPrimitive,
)]
#[repr(u16)]
#[strum(serialize_all = "kebab-case", ascii_case_insensitive)]
pub enum FontSlant {
    #[strum(to_string = "normal", serialize = "r", serialize = "unspecified")]
    Normal = 100,
    #[strum(to_string = "italic", serialize = "i", serialize = "ot")]
    Italic = 200,
    #[strum(to_string = "oblique", serialize = "o")]
    Oblique = 210,
    #[strum(to_string = "reverse-italic", serialize = "ri")]
    ReverseItalic = 10,
    #[strum(to_string = "reverse-oblique", serialize = "ro")]
    ReverseOblique = 0,
}

impl FontSlant {
    pub fn from_symbol(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn symbol_name(self) -> &'static str {
        self.into()
    }

    pub fn gnu_numeric(self) -> u16 {
        self.into()
    }

    pub fn from_gnu_numeric(value: u16) -> Option<Self> {
        Self::try_from(value).ok()
    }

    pub fn is_italic(&self) -> bool {
        matches!(self, Self::Italic | Self::Oblique)
    }
}

/// Font width (condensed, normal, expanded).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr, IntoPrimitive, TryFromPrimitive,
)]
#[repr(u16)]
#[strum(serialize_all = "kebab-case", ascii_case_insensitive)]
pub enum FontWidth {
    #[strum(to_string = "ultra-condensed", serialize = "ultracondensed")]
    UltraCondensed = 50,
    #[strum(to_string = "extra-condensed", serialize = "extracondensed")]
    ExtraCondensed = 63,
    #[strum(
        to_string = "condensed",
        serialize = "compressed",
        serialize = "narrow"
    )]
    Condensed = 75,
    #[strum(
        to_string = "semi-condensed",
        serialize = "semicondensed",
        serialize = "demicondensed"
    )]
    SemiCondensed = 87,
    #[strum(
        to_string = "normal",
        serialize = "medium",
        serialize = "regular",
        serialize = "unspecified"
    )]
    Normal = 100,
    #[strum(
        to_string = "semi-expanded",
        serialize = "semiexpanded",
        serialize = "demiexpanded"
    )]
    SemiExpanded = 113,
    Expanded = 125,
    #[strum(to_string = "extra-expanded", serialize = "extraexpanded")]
    ExtraExpanded = 150,
    #[strum(
        to_string = "ultra-expanded",
        serialize = "ultraexpanded",
        serialize = "wide"
    )]
    UltraExpanded = 200,
}

impl FontWidth {
    pub fn from_symbol(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn symbol_name(self) -> &'static str {
        self.into()
    }

    pub fn gnu_numeric(self) -> u16 {
        self.into()
    }

    pub fn from_gnu_numeric(value: u16) -> Option<Self> {
        Self::try_from(value).ok()
    }
}

// ---------------------------------------------------------------------------
// Face attribute value (for set_attribute)
// ---------------------------------------------------------------------------

/// A typed face attribute value for `FaceTable::set_attribute()`.
#[derive(Clone, Debug)]
pub enum FaceAttrValue {
    Color(Color),
    Weight(FontWeight),
    Slant(FontSlant),
    Height(FaceHeight),
    Width(FontWidth),
    Underline(Underline),
    Box(BoxBorder),
    Bool(bool),
    Text(Value),
    /// Raw `:inherit` face_ref (symbol/list/plist). `None` means nil /
    /// effectively unspecified. Matches GNU's `LFACE_INHERIT_INDEX` slot.
    Inherit(Option<Value>),
    /// Raw `:stipple` spec (a `(WIDTH HEIGHT DATA)` cons, a bitmap file name
    /// string, or a symbol). `None` means nil / unspecified. Matches GNU's
    /// `LFACE_STIPPLE_INDEX` slot, which holds the spec until it is realized to
    /// a pixmap at draw time.
    Stipple(Option<Value>),
    Unspecified,
}

// ---------------------------------------------------------------------------
// Face
// ---------------------------------------------------------------------------

/// A face definition. Fields are `Option` to support partial specification
/// (inheriting unset attributes from the default face).
///
/// The face name is owned by the surrounding registry key, matching GNU
/// Emacs's frame-local face hash table design.
#[derive(Clone, Debug, Default)]
pub struct Face {
    /// Foreground color.
    pub foreground: Option<Color>,
    /// Background color.
    pub background: Option<Color>,
    /// Font family name.
    pub family: Option<Value>,
    /// Font height in 1/10 pt (e.g. 120 = 12pt).
    /// Can also be a float relative to the default face (e.g. 1.5).
    pub height: Option<FaceHeight>,
    /// Font weight.
    pub weight: Option<FontWeight>,
    /// Font slant.
    pub slant: Option<FontSlant>,
    /// Underline specification, preserving explicit nil separately from
    /// `unspecified` until face composition is complete.
    pub underline: FaceDecoration<Underline>,
    /// Overline (true = draw overline).
    pub overline: Option<bool>,
    /// Overline color (None = use foreground).
    pub overline_color: Option<Color>,
    /// Strike-through.
    pub strike_through: Option<bool>,
    /// Strike-through color (None = use foreground).
    pub strike_through_color: Option<Color>,
    /// Box border, preserving explicit nil separately from `unspecified` so a
    /// higher-priority face can disable an inherited box.
    pub box_border: FaceDecoration<BoxBorder>,
    /// Inverse video.
    pub inverse_video: Option<bool>,
    /// Lisp stipple value, mirroring GNU face attribute ownership.
    pub stipple: Option<Value>,
    /// Whether to extend face background to end of line.
    pub extend: Option<bool>,
    /// `:inherit` face reference, stored raw matching GNU's
    /// `LFACE_INHERIT_INDEX` slot. `None` means unspecified. When set, the
    /// value is any valid face_ref: a symbol (named face), a list of
    /// face_refs (merged left-to-right by `merge_face_ref`), or a plist of
    /// face attributes. Resolution walks this recursively via
    /// `resolve_face_value_over`, mirroring GNU `xfaces.c:merge_face_ref`.
    pub inherit: Option<Value>,
    /// Whether bold is simulated via overstrike.
    pub overstrike: bool,
    /// Face documentation string or nil-equivalent absence.
    pub doc: Option<Value>,
    /// Distant foreground color (used when fg matches bg).
    pub distant_foreground: Option<Color>,
    /// Font foundry name.
    pub foundry: Option<Value>,
    /// Font width (condensed/expanded).
    pub width: Option<FontWidth>,
}

/// Height specification.
#[derive(Clone, Debug, PartialEq)]
pub enum FaceHeight {
    /// Absolute height in 1/10 pt.
    Absolute(i32),
    /// Relative to default face (multiplier).
    Relative(f64),
}

fn merge_face_height(
    overlay: Option<&FaceHeight>,
    base: Option<&FaceHeight>,
) -> Option<FaceHeight> {
    match overlay {
        None => base.cloned(),
        Some(FaceHeight::Absolute(height)) => Some(FaceHeight::Absolute(*height)),
        Some(FaceHeight::Relative(scale)) => match base {
            Some(FaceHeight::Absolute(height)) => {
                Some(FaceHeight::Absolute((*scale * *height as f64) as i32))
            }
            Some(FaceHeight::Relative(other_scale)) => {
                Some(FaceHeight::Relative(*scale * *other_scale))
            }
            None => Some(FaceHeight::Relative(*scale)),
        },
    }
}

fn face_symbol_value(name: &str) -> Value {
    Value::symbol(name)
}

fn normalized_face_name_value(value: &Value) -> Option<Value> {
    if let Some(name) = value.as_symbol_name() {
        Some(face_symbol_value(name))
    } else if value.is_string() {
        value
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
            .map(|name| face_symbol_value(&name))
    } else {
        None
    }
}

impl Face {
    pub fn family_runtime_string_owned(&self) -> Option<String> {
        self.family.and_then(|value| {
            value
                .as_lisp_string()
                .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        })
    }

    pub fn foundry_runtime_string_owned(&self) -> Option<String> {
        self.foundry.and_then(|value| {
            value
                .as_lisp_string()
                .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        })
    }

    /// Compatibility constructor for existing call sites. The name is owned
    /// by `FaceTable`, not by `Face` itself.
    pub fn new(_name: &str) -> Self {
        Self::default()
    }

    /// Apply one typed Lisp face attribute to this face.
    ///
    /// This is the shared mutation primitive for both the global runtime
    /// table and frame-local face realization. GNU keeps Lisp face vectors as
    /// the authoritative storage and realizes `struct face` values from those
    /// vectors; keeping this operation on `Face` lets callers derive runtime
    /// faces without routing through a separate table.
    pub fn set_attribute(&mut self, attr: LFaceAttr, value: FaceAttrValue) -> bool {
        macro_rules! set_option {
            ($field:expr, $variant:ident) => {
                match value {
                    FaceAttrValue::$variant(v) => $field = Some(v),
                    FaceAttrValue::Unspecified => $field = None,
                    _ => return false,
                }
            };
        }

        match attr {
            LFaceAttr::Foreground => set_option!(self.foreground, Color),
            LFaceAttr::Background => set_option!(self.background, Color),
            LFaceAttr::DistantForeground => set_option!(self.distant_foreground, Color),
            LFaceAttr::Weight => set_option!(self.weight, Weight),
            LFaceAttr::Slant => set_option!(self.slant, Slant),
            LFaceAttr::Width => set_option!(self.width, Width),
            LFaceAttr::Height => set_option!(self.height, Height),
            LFaceAttr::Family => match value {
                FaceAttrValue::Text(text) => self.family = Some(text),
                FaceAttrValue::Unspecified => self.family = None,
                _ => return false,
            },
            LFaceAttr::Foundry => match value {
                FaceAttrValue::Text(text) => self.foundry = Some(text),
                FaceAttrValue::Unspecified => self.foundry = None,
                _ => return false,
            },
            LFaceAttr::Underline => match value {
                FaceAttrValue::Underline(u) => self.underline = FaceDecoration::Enabled(u),
                FaceAttrValue::Bool(true) => {
                    self.underline = FaceDecoration::Enabled(Underline {
                        style: UnderlineStyle::Line,
                        color: None,
                        position: UnderlinePosition::FontMetric,
                    });
                }
                FaceAttrValue::Bool(false) => self.underline = FaceDecoration::Disabled,
                FaceAttrValue::Unspecified => self.underline = FaceDecoration::Unspecified,
                _ => return false,
            },
            LFaceAttr::Overline => match value {
                FaceAttrValue::Bool(b) => self.overline = Some(b),
                FaceAttrValue::Color(c) => {
                    self.overline = Some(true);
                    self.overline_color = Some(c);
                }
                FaceAttrValue::Unspecified => {
                    self.overline = None;
                    self.overline_color = None;
                }
                _ => return false,
            },
            LFaceAttr::StrikeThrough => match value {
                FaceAttrValue::Bool(b) => self.strike_through = Some(b),
                FaceAttrValue::Color(c) => {
                    self.strike_through = Some(true);
                    self.strike_through_color = Some(c);
                }
                FaceAttrValue::Unspecified => {
                    self.strike_through = None;
                    self.strike_through_color = None;
                }
                _ => return false,
            },
            LFaceAttr::Box => match value {
                FaceAttrValue::Box(border) => self.box_border = FaceDecoration::Enabled(border),
                FaceAttrValue::Bool(false) => self.box_border = FaceDecoration::Disabled,
                FaceAttrValue::Unspecified => self.box_border = FaceDecoration::Unspecified,
                _ => return false,
            },
            LFaceAttr::InverseVideo => set_option!(self.inverse_video, Bool),
            LFaceAttr::Extend => set_option!(self.extend, Bool),
            LFaceAttr::Inherit => match value {
                FaceAttrValue::Inherit(v) => self.inherit = v,
                FaceAttrValue::Unspecified => self.inherit = None,
                _ => return false,
            },
            LFaceAttr::Stipple => match value {
                FaceAttrValue::Stipple(v) => self.stipple = v,
                FaceAttrValue::Unspecified => self.stipple = None,
                _ => return false,
            },
            LFaceAttr::Font | LFaceAttr::Fontset => return false,
        }
        true
    }

    /// Merge `overlay` on top of `self`.  Non-None fields in `overlay`
    /// override those in `self`.
    pub fn merge(&self, overlay: &Face) -> Face {
        Face {
            foreground: overlay.foreground.or(self.foreground),
            background: overlay.background.or(self.background),
            family: overlay.family.or(self.family),
            height: merge_face_height(overlay.height.as_ref(), self.height.as_ref()),
            weight: overlay.weight.or(self.weight),
            slant: overlay.slant.or(self.slant),
            underline: overlay.underline.merge_over(&self.underline),
            overline: overlay.overline.or(self.overline),
            strike_through: overlay.strike_through.or(self.strike_through),
            box_border: overlay.box_border.merge_over(&self.box_border),
            inverse_video: overlay.inverse_video.or(self.inverse_video),
            stipple: overlay.stipple.or(self.stipple),
            extend: overlay.extend.or(self.extend),
            inherit: overlay.inherit.or(self.inherit),
            overstrike: overlay.overstrike || self.overstrike,
            doc: overlay.doc.or(self.doc),
            overline_color: overlay.overline_color.or(self.overline_color),
            strike_through_color: overlay.strike_through_color.or(self.strike_through_color),
            distant_foreground: overlay.distant_foreground.or(self.distant_foreground),
            foundry: overlay.foundry.or(self.foundry),
            width: overlay.width.or(self.width),
        }
    }

    /// Effective foreground, accounting for inverse video.
    pub fn effective_foreground(&self) -> Option<Color> {
        if self.inverse_video == Some(true) {
            self.background
        } else {
            self.foreground
        }
    }

    /// Effective background, accounting for inverse video.
    pub fn effective_background(&self) -> Option<Color> {
        if self.inverse_video == Some(true) {
            self.foreground
        } else {
            self.background
        }
    }

    /// Convert to a Lisp plist.
    pub fn to_plist(&self) -> Value {
        let mut items = Vec::new();

        if let Some(fg) = &self.foreground {
            items.push(Value::keyword("foreground-color"));
            items.push(Value::string(fg.to_hex()));
        }
        if let Some(bg) = &self.background {
            items.push(Value::keyword("background-color"));
            items.push(Value::string(bg.to_hex()));
        }
        if let Some(w) = &self.weight {
            items.push(Value::keyword("weight"));
            items.push(Value::symbol(w.symbol_name()));
        }
        if let Some(s) = &self.slant {
            items.push(Value::keyword("slant"));
            items.push(Value::symbol(s.symbol_name()));
        }
        if let Some(h) = &self.height {
            items.push(Value::keyword("height"));
            match h {
                FaceHeight::Absolute(n) => items.push(Value::fixnum(*n as i64)),
                FaceHeight::Relative(f) => items.push(Value::make_float(*f)),
            }
        }

        Value::list(items)
    }

    /// Parse face attributes from a Lisp plist.
    pub fn from_plist(name: &str, plist: &[Value]) -> Self {
        Self::from_plist_realized(name, plist, None)
    }

    /// Parse an anonymous attribute plist, realizing its colours against a
    /// terminal palette.
    ///
    /// GNU has no separate path for an anonymous plist: `merge_face_ref` folds
    /// it into the same lface vector, and the one realization that follows puts
    /// every colour string through `map_tty_color` -> `tty-color-desc`
    /// (src/xfaces.c:6620-6694).  Passing `Some(palette)` reproduces that for
    /// the callers that realize a plist outside the evaluator; `None` is a GUI
    /// frame, where the realized colour IS the pixel.
    pub fn from_plist_realized(
        name: &str,
        plist: &[Value],
        palette: Option<&neomacs_display_protocol::TtyPalette>,
    ) -> Self {
        let mut face = Face::new(name);
        let mut i = 0;

        while i + 1 < plist.len() {
            let key = match plist[i].kind() {
                ValueKind::Symbol(id) => resolve_sym(id),
                _ => {
                    i += 2;
                    continue;
                }
            };
            let key = key.trim_start_matches(':');
            let val = &plist[i + 1];

            match key {
                "foreground" | "foreground-color" => {
                    if let Some(s) = val.as_utf8_str() {
                        face.foreground = realize_color_spec(s, palette);
                    }
                }
                "background" | "background-color" => {
                    if let Some(s) = val.as_utf8_str() {
                        face.background = realize_color_spec(s, palette);
                    }
                }
                "weight" => {
                    if let Some(s) = val.as_symbol_name() {
                        face.weight = FontWeight::from_symbol(s);
                    }
                }
                "slant" => {
                    if let Some(s) = val.as_symbol_name() {
                        face.slant = FontSlant::from_symbol(s);
                    }
                }
                "height" => match val.kind() {
                    ValueKind::Fixnum(n) => face.height = Some(FaceHeight::Absolute(n as i32)),
                    ValueKind::Float => face.height = Some(FaceHeight::Relative(val.xfloat())),
                    _ => {}
                },
                "family" => {
                    if val.is_string() {
                        face.family = Some(*val);
                    }
                }
                "underline" => face.underline = parse_underline_value(val, palette),
                "overline" => {
                    if let Some(s) = val.as_utf8_str() {
                        face.overline = Some(true);
                        face.overline_color = Color::parse(s);
                    } else {
                        face.overline = Some(val.is_truthy());
                    }
                }
                "strike-through" => {
                    if let Some(s) = val.as_utf8_str() {
                        face.strike_through = Some(true);
                        face.strike_through_color = Color::parse(s);
                    } else {
                        face.strike_through = Some(val.is_truthy());
                    }
                }
                "inverse-video" => {
                    face.inverse_video = Some(val.is_truthy());
                }
                "extend" => {
                    face.extend = Some(val.is_truthy());
                }
                "inherit" => {
                    // Store the raw face_ref. Matches GNU's
                    // `merge_face_ref` (xfaces.c:2960-2980) which accepts
                    // any face_ref — symbol, list, or plist — and defers
                    // type dispatch to the recursive resolver.
                    face.inherit = if val.is_nil() || val.is_symbol_named("nil") {
                        None
                    } else {
                        Some(*val)
                    };
                }
                "box" => {
                    face.box_border = parse_box_value(val);
                }
                "distant-foreground" => {
                    if let Some(s) = val.as_utf8_str() {
                        face.distant_foreground = realize_color_spec(s, palette);
                    }
                }
                "foundry" => {
                    if val.is_string() {
                        face.foundry = Some(*val);
                    }
                }
                "width" => {
                    if let Some(s) = val.as_symbol_name() {
                        face.width = FontWidth::from_symbol(s);
                    }
                }
                "stipple" => {
                    // Store the raw stipple spec — a `(WIDTH HEIGHT DATA)`
                    // cons, a bitmap file name, or a symbol. Realized to a
                    // `StipplePattern` in the layout bridge, mirroring GNU's
                    // `LFACE_STIPPLE_INDEX`.
                    face.stipple = if val.is_nil() || val.is_symbol_named("nil") {
                        None
                    } else {
                        Some(*val)
                    };
                }
                _ => {}
            }

            i += 2;
        }

        face
    }
}

/// Parse one `:underline` value from an anonymous attribute plist.
///
/// The colour is realized through the terminal palette, exactly as this
/// function's caller realizes `:foreground` and `:background`.  GNU realizes
/// all three through the same `map_tty_color` (src/xfaces.c:6748, :6777 for
/// the underline), and the writer emits the underline colour through
/// `TF_set_underline_color` (src/term.c:2119-2126); a `Color::parse` here
/// would produce a pixel with no terminal index, and the underline would
/// silently lose its colour on a terminal frame while keeping it in the GUI.
fn parse_underline_value(
    value: &Value,
    palette: Option<&neomacs_display_protocol::TtyPalette>,
) -> FaceDecoration<Underline> {
    match value.kind() {
        ValueKind::T => FaceDecoration::Enabled(Underline {
            style: UnderlineStyle::Line,
            color: None,
            position: UnderlinePosition::FontMetric,
        }),
        ValueKind::Nil => FaceDecoration::Disabled,
        _ if value.is_string() => {
            let Some(text) = face_runtime_string(value) else {
                return FaceDecoration::Unspecified;
            };
            FaceDecoration::Enabled(Underline {
                style: UnderlineStyle::Line,
                color: realize_color_spec(&text, palette),
                position: UnderlinePosition::FontMetric,
            })
        }
        ValueKind::Cons => {
            let Some(items) = crate::emacs_core::value::list_to_vec(value) else {
                return FaceDecoration::Unspecified;
            };
            let mut style = UnderlineStyle::Line;
            let mut color = None;
            let mut position = UnderlinePosition::FontMetric;
            let mut i = 0;
            while i + 1 < items.len() {
                let key = items[i]
                    .as_symbol_name()
                    .unwrap_or("")
                    .trim_start_matches(':');
                let item = &items[i + 1];
                match key {
                    "color" => {
                        color = face_runtime_string(item)
                            .as_deref()
                            .and_then(|spec| realize_color_spec(spec, palette));
                    }
                    "style" => {
                        if let Some(name) = item.as_symbol_name() {
                            style =
                                UnderlineStyle::from_symbol(name).unwrap_or(UnderlineStyle::Line);
                        }
                    }
                    "position" => {
                        position = UnderlinePosition::from_lisp(item);
                    }
                    _ => {}
                }
                i += 2;
            }
            FaceDecoration::Enabled(Underline {
                style,
                color,
                position,
            })
        }
        _ if value.is_truthy() => FaceDecoration::Enabled(Underline {
            style: UnderlineStyle::Line,
            color: None,
            position: UnderlinePosition::FontMetric,
        }),
        _ => FaceDecoration::Unspecified,
    }
}

fn parse_box_value(value: &Value) -> FaceDecoration<BoxBorder> {
    match value.kind() {
        ValueKind::T => FaceDecoration::Enabled(BoxBorder {
            color: None,
            width: 1,
            style: BoxStyle::Flat,
        }),
        ValueKind::Nil => FaceDecoration::Disabled,
        ValueKind::Fixnum(n) => FaceDecoration::Enabled(BoxBorder {
            color: None,
            width: n as i32,
            style: BoxStyle::Flat,
        }),
        _ if value.is_string() => {
            let Some(text) = face_runtime_string(value) else {
                return FaceDecoration::Unspecified;
            };
            FaceDecoration::Enabled(BoxBorder {
                color: Color::parse(&text),
                width: 1,
                style: BoxStyle::Flat,
            })
        }
        ValueKind::Cons => {
            let Some(items) = crate::emacs_core::value::list_to_vec(value) else {
                return FaceDecoration::Unspecified;
            };
            let mut color = None;
            let mut width = 1i32;
            let mut style = BoxStyle::Flat;
            let mut i = 0;
            while i + 1 < items.len() {
                let key = items[i]
                    .as_symbol_name()
                    .unwrap_or("")
                    .trim_start_matches(':');
                let item = &items[i + 1];
                match key {
                    "line-width" => match item.kind() {
                        ValueKind::Fixnum(n) => width = n as i32,
                        ValueKind::Cons => {
                            let pair_car = item.cons_car();
                            let _pair_cdr = item.cons_cdr();
                            if let Some(n) = pair_car.as_fixnum() {
                                width = n as i32;
                            }
                        }
                        _ => {}
                    },
                    "color" => {
                        color = parse_color_value(item);
                    }
                    "style" => {
                        if let Some(name) = item.as_symbol_name() {
                            style = BoxStyle::from_symbol(name).unwrap_or(BoxStyle::Flat);
                        }
                    }
                    _ => {}
                }
                i += 2;
            }
            FaceDecoration::Enabled(BoxBorder {
                color,
                width,
                style,
            })
        }
        _ if value.is_symbol_named("unspecified") => FaceDecoration::Unspecified,
        _ => FaceDecoration::Unspecified,
    }
}

fn face_runtime_string(value: &Value) -> Option<String> {
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

fn parse_color_value(value: &Value) -> Option<Color> {
    face_runtime_string(value).as_deref().and_then(Color::parse)
}

// ---------------------------------------------------------------------------
// Face remapping (face-remapping-alist support)
// ---------------------------------------------------------------------------

/// A single entry in a remapping specification.
///
/// Corresponds to the CDR of an entry in `face-remapping-alist`:
/// - `(FACE . other-face)`        -> `[RemapFace("other-face")]`
/// - `(FACE . (:attr val ...))`   -> `[RemapAttrs(face)]`
/// - `(FACE . (a b (:k v) ...))`  -> mixed list of face names & attr plists
#[derive(Clone, Debug)]
pub enum FaceRemapEntry {
    /// Remap to another named face.
    RemapFace(Value),
    /// Inline attribute plist parsed into a `Face`.
    RemapAttrs(Face),
}

/// Parsed form of the buffer-local `face-remapping-alist`.
///
/// Maps original face name -> ordered list of remapping entries.
/// When resolving face `X`, if `X` is in this map the entries replace the
/// original face definition.
#[derive(Clone, Debug, Default)]
pub struct FaceRemapping {
    map: HashMap<Value, Vec<FaceRemapEntry>>,
}

impl FaceRemapping {
    /// Create an empty (no remapping) instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether there are any remappings.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Insert a remapping for the given face name.
    pub fn insert(&mut self, face_name: Value, entries: Vec<FaceRemapEntry>) {
        self.map.insert(face_name, entries);
    }

    /// Look up the remapping entries for a face name.
    pub fn get(&self, face_name: &str) -> Option<&[FaceRemapEntry]> {
        self.map
            .get(&face_symbol_value(face_name))
            .map(|v| v.as_slice())
    }

    /// Parse `face-remapping-alist` from its Lisp value.
    ///
    /// The alist has the form `((FACE . SPEC) ...)` where SPEC can be:
    /// - A symbol (face name)
    /// - A plist `(:attr val ...)`
    /// - A list of specs `(face1 face2 (:attr val ...) ...)`
    pub fn from_lisp(value: &Value) -> Self {
        use crate::emacs_core::value::list_to_vec;

        let mut remapping = Self::new();

        let Some(alist) = list_to_vec(value) else {
            return remapping;
        };

        for entry in &alist {
            // Each entry is (FACE . SPEC) — a cons cell
            if !entry.is_cons() {
                continue;
            };
            let cell_car = entry.cons_car();
            let cell_cdr = entry.cons_cdr();
            let Some(face_name) = normalized_face_name_value(&cell_car) else {
                continue;
            };
            if face_name.is_symbol_named("nil") {
                continue;
            }

            let entries = Self::parse_remap_spec(&cell_cdr);
            if !entries.is_empty() {
                remapping.insert(face_name, entries);
            }
        }

        remapping
    }

    /// Parse a single remapping spec (the CDR of an alist entry).
    fn parse_remap_spec(spec: &Value) -> Vec<FaceRemapEntry> {
        use crate::emacs_core::value::list_to_vec;

        match spec.kind() {
            // Simple symbol remap: (FACE . other-face)
            ValueKind::Symbol(_) | ValueKind::T | ValueKind::String => {
                if let Some(name) = normalized_face_name_value(spec)
                    && !name.is_symbol_named("nil")
                {
                    return vec![FaceRemapEntry::RemapFace(name)];
                }
                Vec::new()
            }
            ValueKind::Nil => Vec::new(),
            // List form: could be a plist or a list of specs
            ValueKind::Cons => {
                let Some(items) = list_to_vec(spec) else {
                    return Vec::new();
                };
                if items.is_empty() {
                    return Vec::new();
                }

                // Check if it's a plist (starts with keyword)
                if items[0].as_keyword_id().is_some() {
                    let face = Face::from_plist("--remap--", &items);
                    return vec![FaceRemapEntry::RemapAttrs(face)];
                }

                // Otherwise it's a list of specs: (face1 face2 (:k v ...) ...)
                let mut entries = Vec::new();
                for item in &items {
                    match item.kind() {
                        ValueKind::Symbol(_) | ValueKind::T | ValueKind::String => {
                            if let Some(name) = normalized_face_name_value(item)
                                && !name.is_symbol_named("nil")
                            {
                                entries.push(FaceRemapEntry::RemapFace(name));
                            }
                        }
                        ValueKind::Cons => {
                            if let Some(sub_items) = list_to_vec(item)
                                && sub_items.first().is_some_and(|v| v.is_keyword())
                            {
                                let face = Face::from_plist("--remap--", &sub_items);
                                entries.push(FaceRemapEntry::RemapAttrs(face));
                            }
                        }
                        _ => {}
                    }
                }
                entries
            }
            _ => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// FaceTable
// ---------------------------------------------------------------------------

/// Global face registry.
#[derive(Clone)]
pub struct FaceTable {
    /// Shared copy-on-write storage.
    ///
    /// `FaceResolver::new_with_font_sizing` clones the table ONCE PER
    /// REDISPLAY (twice, in fact -- the engine and the window-params paths
    /// each build a resolver), and with plain ownership that was a deep copy
    /// of every `Face`: 7.7% of a real typing session went to `Face::clone`,
    /// plus the memmove traffic to carry it. Behind `Rc` the same clone is a
    /// refcount bump, and the rare mutation paths (face materialization)
    /// pay a one-time copy via `Rc::make_mut`.
    ///
    /// GC safety is unchanged: the keys are `Value`s, rooted through
    /// `collect_gc_roots` on the CONTEXT's handle, and a resolver's shared
    /// handle aliases the same allocation for the duration of one redisplay
    /// -- exactly the lifetime the old bitwise deep copy had.
    faces: std::rc::Rc<HashMap<Value, Face>>,
    /// Frame lface identity to the symbol naming that face.
    ///
    /// GNU display vectors carry the numeric lface index, not a name.  Keep
    /// that identity beside the derived runtime face table so redisplay never
    /// needs a global id -> String reverse cache or a mirror-module lookup.
    lisp_face_refs: std::rc::Rc<HashMap<LispFaceId, Value>>,
    /// The terminal's `tty-color-alist` at the moment this table was realized,
    /// empty on a GUI frame.
    ///
    /// It travels WITH the table because it is the thing the table was realized
    /// against: every consumer that clones the table to realize one more face
    /// -- an anonymous attribute plist from a text property, an overlay, or
    /// `face-remapping-alist` -- must realize it against the same palette the
    /// named faces used, and GNU guarantees that by realizing all of them
    /// through one `map_tty_color` (src/xfaces.c:6620-6694).
    tty_palette: std::rc::Rc<neomacs_display_protocol::TtyPalette>,
}

impl FaceTable {
    pub fn new() -> Self {
        let mut table = Self {
            faces: std::rc::Rc::new(HashMap::new()),
            lisp_face_refs: std::rc::Rc::new(HashMap::new()),
            tty_palette: std::rc::Rc::new(neomacs_display_protocol::TtyPalette::default()),
        };
        table.register_standard_faces();
        table
    }

    /// The palette this table's faces were realized against, or `None` on a
    /// GUI frame -- where a realized colour IS the pixel and no palette exists.
    #[must_use]
    pub fn tty_palette(&self) -> Option<&neomacs_display_protocol::TtyPalette> {
        (!self.tty_palette.is_empty()).then(|| &*self.tty_palette)
    }

    /// Record the terminal palette this table was realized against.
    pub fn set_tty_palette(&mut self, palette: neomacs_display_protocol::TtyPalette) {
        self.tty_palette = std::rc::Rc::new(palette);
    }

    /// Register the standard Emacs faces.
    fn register_standard_faces(&mut self) {
        // default face
        let mut default = Face::new("default");
        // GNU realizes the TTY default face with FACE_TTY_DEFAULT_FG_COLOR /
        // FACE_TTY_DEFAULT_BG_COLOR, exposed to Lisp as "unspecified-fg" /
        // "unspecified-bg".  Keep these colors unset here so the display
        // realization layer can preserve the terminal-default sentinel.
        default.weight = Some(FontWeight::NORMAL);
        default.slant = Some(FontSlant::Normal);
        self.define("default", default);

        // bold
        let mut bold = Face::new("bold");
        bold.weight = Some(FontWeight::BOLD);
        bold.inherit = Some(face_symbol_value("default"));
        self.define("bold", bold);

        // italic
        let mut italic = Face::new("italic");
        italic.slant = Some(FontSlant::Italic);
        italic.inherit = Some(face_symbol_value("default"));
        self.define("italic", italic);

        // bold-italic
        let mut bold_italic = Face::new("bold-italic");
        bold_italic.weight = Some(FontWeight::BOLD);
        bold_italic.slant = Some(FontSlant::Italic);
        bold_italic.inherit = Some(face_symbol_value("default"));
        self.define("bold-italic", bold_italic);

        // underline
        let mut underline = Face::new("underline");
        underline.underline = FaceDecoration::Enabled(Underline {
            style: UnderlineStyle::Line,
            color: None,
            position: UnderlinePosition::FontMetric,
        });
        underline.inherit = Some(face_symbol_value("default"));
        self.define("underline", underline);

        // fixed-pitch
        let mut fixed_pitch = Face::new("fixed-pitch");
        fixed_pitch.inherit = Some(face_symbol_value("default"));
        self.define("fixed-pitch", fixed_pitch);

        // variable-pitch
        let mut variable_pitch = Face::new("variable-pitch");
        variable_pitch.inherit = Some(face_symbol_value("default"));
        self.define("variable-pitch", variable_pitch);

        // mode-line
        let mut mode_line = Face::new("mode-line");
        mode_line.foreground = Some(Color::rgb(0, 0, 0));
        mode_line.background = Some(Color::rgb(192, 192, 192));
        mode_line.weight = Some(FontWeight::NORMAL);
        mode_line.box_border = FaceDecoration::Enabled(BoxBorder {
            color: None,
            width: 1,
            style: BoxStyle::Raised,
        });
        self.define("mode-line", mode_line);

        // mode-line-active
        let mut mode_line_active = Face::new("mode-line-active");
        mode_line_active.inherit = Some(face_symbol_value("mode-line"));
        self.define("mode-line-active", mode_line_active);

        // mode-line-inactive
        let mut mode_line_inactive = Face::new("mode-line-inactive");
        mode_line_inactive.foreground = Some(Color::rgb(64, 64, 64));
        mode_line_inactive.background = Some(Color::rgb(224, 224, 224));
        mode_line_inactive.weight = Some(FontWeight::NORMAL);
        self.define("mode-line-inactive", mode_line_inactive);

        // mode-line-highlight
        let mut mode_line_highlight = Face::new("mode-line-highlight");
        mode_line_highlight.box_border = FaceDecoration::Enabled(BoxBorder {
            color: Some(Color::rgb(64, 64, 64)),
            width: 2,
            style: BoxStyle::Raised,
        });
        mode_line_highlight.inherit = Some(face_symbol_value("highlight"));
        self.define("mode-line-highlight", mode_line_highlight);

        // mode-line-emphasis
        let mut mode_line_emphasis = Face::new("mode-line-emphasis");
        mode_line_emphasis.weight = Some(FontWeight::BOLD);
        self.define("mode-line-emphasis", mode_line_emphasis);

        // mode-line-buffer-id
        let mut mode_line_buffer_id = Face::new("mode-line-buffer-id");
        mode_line_buffer_id.weight = Some(FontWeight::BOLD);
        self.define("mode-line-buffer-id", mode_line_buffer_id);

        // header-line
        let mut header = Face::new("header-line");
        header.inherit = Some(face_symbol_value("mode-line"));
        self.define("header-line", header);

        // header-line-highlight
        let mut header_line_highlight = Face::new("header-line-highlight");
        header_line_highlight.inherit = Some(face_symbol_value("mode-line-highlight"));
        self.define("header-line-highlight", header_line_highlight);

        // header-line-active
        let mut header_line_active = Face::new("header-line-active");
        header_line_active.inherit = Some(face_symbol_value("header-line"));
        self.define("header-line-active", header_line_active);

        // header-line-inactive
        let mut header_line_inactive = Face::new("header-line-inactive");
        header_line_inactive.inherit = Some(face_symbol_value("header-line"));
        self.define("header-line-inactive", header_line_inactive);

        // highlight
        let mut highlight = Face::new("highlight");
        highlight.background = Some(Color::rgb(180, 210, 240));
        self.define("highlight", highlight);

        // region
        let mut region = Face::new("region");
        region.background = Some(Color::rgb(100, 149, 237));
        region.extend = Some(true);
        self.define("region", region);

        // minibuffer-prompt
        let mut prompt = Face::new("minibuffer-prompt");
        prompt.foreground = Some(Color::rgb(0, 0, 128));
        prompt.weight = Some(FontWeight::BOLD);
        self.define("minibuffer-prompt", prompt);

        // cursor
        let mut cursor = Face::new("cursor");
        cursor.background = Some(Color::rgb(0, 0, 0));
        self.define("cursor", cursor);

        // fringe
        let mut fringe = Face::new("fringe");
        fringe.background = Some(Color::rgb(240, 240, 240));
        self.define("fringe", fringe);

        // vertical-border
        let mut vertical_border = Face::new("vertical-border");
        vertical_border.inherit = Some(face_symbol_value("mode-line-inactive"));
        self.define("vertical-border", vertical_border);

        // scroll-bar
        self.define("scroll-bar", Face::new("scroll-bar"));

        // border
        self.define("border", Face::new("border"));

        // internal-border
        self.define("internal-border", Face::new("internal-border"));

        // child-frame-border
        self.define("child-frame-border", Face::new("child-frame-border"));

        // line-number
        let mut line_num = Face::new("line-number");
        line_num.foreground = Some(Color::rgb(160, 160, 160));
        line_num.inherit = Some(face_symbol_value("default"));
        self.define("line-number", line_num);

        // line-number-current-line
        let mut line_num_cur = Face::new("line-number-current-line");
        line_num_cur.foreground = Some(Color::rgb(0, 0, 0));
        line_num_cur.weight = Some(FontWeight::BOLD);
        line_num_cur.inherit = Some(face_symbol_value("line-number"));
        self.define("line-number-current-line", line_num_cur);

        // shadow
        let mut shadow = Face::new("shadow");
        shadow.foreground = Some(Color::rgb(128, 128, 128));
        self.define("shadow", shadow);

        // mouse
        self.define("mouse", Face::new("mouse"));

        // tool-bar
        let mut tool_bar = Face::new("tool-bar");
        tool_bar.foreground = Some(Color::rgb(0, 0, 0));
        tool_bar.background = Some(Color::rgb(191, 191, 191));
        tool_bar.box_border = FaceDecoration::Enabled(BoxBorder {
            color: None,
            width: 1,
            style: BoxStyle::Raised,
        });
        self.define("tool-bar", tool_bar);

        // tab-bar
        let mut tab_bar = Face::new("tab-bar");
        tab_bar.foreground = Some(Color::rgb(0, 0, 0));
        tab_bar.background = Some(Color::rgb(217, 217, 217));
        tab_bar.inherit = Some(face_symbol_value("variable-pitch"));
        self.define("tab-bar", tab_bar);

        // tab-line
        let mut tab_line = Face::new("tab-line");
        tab_line.foreground = Some(Color::rgb(0, 0, 0));
        tab_line.background = Some(Color::rgb(217, 217, 217));
        tab_line.inherit = Some(face_symbol_value("variable-pitch"));
        self.define("tab-line", tab_line);

        // error
        let mut error = Face::new("error");
        error.foreground = Some(Color::rgb(255, 0, 0));
        error.weight = Some(FontWeight::BOLD);
        self.define("error", error);

        // warning
        let mut warning = Face::new("warning");
        warning.foreground = Some(Color::rgb(255, 165, 0));
        warning.weight = Some(FontWeight::BOLD);
        self.define("warning", warning);

        // success
        let mut success = Face::new("success");
        success.foreground = Some(Color::rgb(0, 128, 0));
        success.weight = Some(FontWeight::BOLD);
        self.define("success", success);

        // font-lock faces
        self.define_font_lock(
            "font-lock-comment-face",
            Color::rgb(128, 128, 128),
            Some(FontSlant::Italic),
        );
        self.define_font_lock("font-lock-string-face", Color::rgb(0, 128, 0), None);
        self.define_font_lock("font-lock-keyword-face", Color::rgb(128, 0, 128), None);
        self.define_font_lock("font-lock-function-name-face", Color::rgb(0, 0, 255), None);
        self.define_font_lock(
            "font-lock-variable-name-face",
            Color::rgb(139, 69, 19),
            None,
        );
        self.define_font_lock("font-lock-type-face", Color::rgb(0, 128, 0), None);
        self.define_font_lock("font-lock-constant-face", Color::rgb(0, 128, 128), None);
        self.define_font_lock("font-lock-builtin-face", Color::rgb(128, 0, 128), None);
        self.define_font_lock("font-lock-preprocessor-face", Color::rgb(128, 128, 0), None);
        self.define_font_lock("font-lock-negation-char-face", Color::rgb(255, 0, 0), None);
        self.define_font_lock("font-lock-warning-face", Color::rgb(255, 165, 0), None);
        self.define_font_lock(
            "font-lock-doc-face",
            Color::rgb(128, 128, 0),
            Some(FontSlant::Italic),
        );

        // isearch
        let mut isearch = Face::new("isearch");
        isearch.foreground = Some(Color::rgb(255, 255, 255));
        isearch.background = Some(Color::rgb(205, 92, 92));
        self.define("isearch", isearch);

        // lazy-highlight
        let mut lazy = Face::new("lazy-highlight");
        lazy.background = Some(Color::rgb(175, 238, 238));
        self.define("lazy-highlight", lazy);

        // trailing-whitespace
        let mut tw = Face::new("trailing-whitespace");
        tw.background = Some(Color::rgb(255, 0, 0));
        self.define("trailing-whitespace", tw);

        // region (active selection)
        let mut region = Face::new("region");
        region.background = Some(Color::rgb(60, 100, 180));
        region.foreground = Some(Color::rgb(255, 255, 255));
        self.define("region", region);

        // isearch (current search match)
        let mut isearch = Face::new("isearch");
        isearch.background = Some(Color::rgb(255, 200, 50));
        isearch.foreground = Some(Color::rgb(0, 0, 0));
        self.define("isearch", isearch);

        // lazy-highlight (other search matches)
        let mut lazy = Face::new("lazy-highlight");
        lazy.background = Some(Color::rgb(150, 180, 220));
        self.define("lazy-highlight", lazy);

        // show-paren-match
        let mut spm = Face::new("show-paren-match");
        spm.background = Some(Color::rgb(180, 210, 255));
        spm.weight = Some(FontWeight::BOLD);
        self.define("show-paren-match", spm);

        // show-paren-mismatch
        let mut spmm = Face::new("show-paren-mismatch");
        spmm.foreground = Some(Color::rgb(255, 255, 255));
        spmm.background = Some(Color::rgb(160, 0, 0));
        self.define("show-paren-mismatch", spmm);

        // link
        let mut link = Face::new("link");
        link.foreground = Some(Color::rgb(0, 0, 238));
        link.underline = FaceDecoration::Enabled(Underline {
            style: UnderlineStyle::Line,
            color: None,
            position: UnderlinePosition::FontMetric,
        });
        self.define("link", link);
    }

    fn define_font_lock(&mut self, name: &str, fg: Color, slant: Option<FontSlant>) {
        let mut face = Face::new(name);
        face.foreground = Some(fg);
        if let Some(s) = slant {
            face.slant = Some(s);
        }
        face.inherit = Some(face_symbol_value("default"));
        self.define(name, face);
    }

    /// Define or update a face.
    pub fn define(&mut self, name: &str, face: Face) {
        std::rc::Rc::make_mut(&mut self.faces).insert(face_symbol_value(name), face);
    }

    /// Define a runtime face together with its frame-local GNU Lisp face id.
    pub fn define_lisp_face(&mut self, id: LispFaceId, name: &str, face: Face) {
        let face_ref = face_symbol_value(name);
        std::rc::Rc::make_mut(&mut self.faces).insert(face_ref, face);
        std::rc::Rc::make_mut(&mut self.lisp_face_refs).insert(id, face_ref);
    }

    /// Typed face reference for a frame-local GNU Lisp face id.
    pub fn lisp_face_ref(&self, id: LispFaceId) -> Option<Value> {
        self.lisp_face_refs.get(&id).copied()
    }

    /// Ensure a face exists (create empty if not present).
    pub fn ensure_face(&mut self, name: &str) {
        let key = face_symbol_value(name);
        std::rc::Rc::make_mut(&mut self.faces)
            .entry(key)
            .or_insert_with(|| Face::new(name));
    }

    /// Update a single attribute on a face.
    /// Creates the face if it doesn't exist.
    /// Returns true if the face was actually modified.
    pub fn set_attribute(&mut self, name: &str, attr: LFaceAttr, value: FaceAttrValue) -> bool {
        self.ensure_face(name);
        let key = face_symbol_value(name);
        let face = std::rc::Rc::make_mut(&mut self.faces)
            .get_mut(&key)
            .unwrap();
        face.set_attribute(attr, value)
    }

    /// Look up a face by name.
    pub fn get(&self, name: &str) -> Option<&Face> {
        self.faces.get(&face_symbol_value(name))
    }

    /// Resolve a face name, merging inherited faces.
    /// Returns a fully-specified face with all inherited attributes filled in.
    pub fn resolve(&self, name: &str) -> Face {
        self.resolve_depth(name, 0)
    }

    /// Foreground and background of the `default` face as sRGB pixels.
    ///
    /// GNU's `lookup_image` maps a negative face id to `DEFAULT_FACE_ID` and
    /// reads `face->foreground`/`face->background` (image.c), so this is the
    /// answer both the image cache key and the display-side default face need.
    /// It lived only in the layout engine, and the image builtins substituted
    /// zeros — giving one image spec two different cache keys, hence two
    /// decodes of the same file.
    ///
    /// The unset case keeps GNU's black-on-white default. Callers that must
    /// preserve the TTY `unspecified-fg`/`unspecified-bg` sentinel look at the
    /// `Face` itself; this returns the realized pixels only.
    #[must_use]
    pub fn default_face_colors(&self) -> (u32, u32) {
        let default = self.resolve("default");
        (
            default.foreground.map_or(0x000000, Color::to_pixel),
            default.background.map_or(0x00FF_FFFF, Color::to_pixel),
        )
    }

    fn resolve_depth(&self, name: &str, depth: usize) -> Face {
        if depth > 10 {
            return Face::new(name);
        }

        let key = face_symbol_value(name);
        let Some(face) = self.faces.get(&key) else {
            return Face::new(name);
        };

        let mut result = face.clone();

        // Apply inheritance. Mirrors GNU `merge_face_vectors` (xfaces.c:2310)
        // which calls `merge_face_ref(from[LFACE_INHERIT_INDEX], to, ...)` —
        // the raw face_ref value is resolved recursively by shape
        // (symbol / list of face_refs / plist of attributes).
        if let Some(inherit_ref) = face.inherit {
            let parent = self.resolve_face_ref(inherit_ref, depth + 1);
            // Parent provides defaults — face overrides.
            result = parent.merge(&result);
        }

        result
    }

    /// Recursively resolve a face_ref value into a `Face` by inheritance.
    ///
    /// Dispatches on the value shape, mirroring GNU `merge_face_ref`
    /// (xfaces.c:2700-3025):
    /// - `nil` / unset → empty face
    /// - symbol → named face lookup (`resolve_depth`)
    /// - list with keyword head → attribute plist (`Face::from_plist`),
    ///   plus recursive resolution of its own `:inherit`
    /// - list with non-keyword head → list of face_refs, merged left-to-right
    ///   (first takes precedence, matching xfaces.c:3005-3014)
    fn resolve_face_ref(&self, face_ref: Value, depth: usize) -> Face {
        if depth > 40 {
            return Face::default();
        }
        if face_ref.is_nil() || face_ref.is_symbol_named("nil") {
            return Face::default();
        }
        if let Some(name) = face_ref.as_symbol_name() {
            return self.resolve_depth(name, depth);
        }
        let Some(items) = crate::emacs_core::value::list_to_vec(&face_ref) else {
            return Face::default();
        };
        if items.is_empty() {
            return Face::default();
        }
        let first_is_keyword = items[0]
            .as_symbol_name()
            .is_some_and(|s| s.starts_with(':'));
        if first_is_keyword {
            // Attribute plist. Parse own attributes, then recursively
            // merge its :inherit chain as parent (parent provides
            // defaults; own attributes already take precedence).
            let own = Face::from_plist("--inline--", &items);
            let parent = match own.inherit {
                Some(inherit_ref) => self.resolve_face_ref(inherit_ref, depth + 1),
                None => Face::default(),
            };
            parent.merge(&own)
        } else {
            // List of face_refs: merge right-to-left so the head
            // (left-most entry) takes precedence — matches GNU
            // xfaces.c:3005-3014 which merges XCDR first, then XCAR.
            let mut result = Face::default();
            for item in items.iter().rev() {
                let next = self.resolve_face_ref(*item, depth + 1);
                result = result.merge(&next);
            }
            result
        }
    }

    /// Resolve face for text: merge a list of face names in order.
    /// Uses raw (non-resolved) faces for overlaying, so only explicitly
    /// set attributes override — inherited attributes don't clobber.
    pub fn merge_faces(&self, face_names: &[&str]) -> Face {
        let default = self.resolve("default");
        let mut result = default;

        for name in face_names {
            // Use the raw face definition (not resolved), so inherited
            // attributes from the parent don't override prior merges.
            if let Some(face) = self.faces.get(&face_symbol_value(name)) {
                result = result.merge(face);
            }
        }

        result
    }

    /// Resolve a face name, consulting `face-remapping-alist`.
    ///
    /// If `name` appears in `remapping`, the remapping entries are merged
    /// together (in order) and returned instead of the original face.
    /// Cycle detection prevents infinite loops when a remapping refers
    /// back to the same face (or to another remapped face).
    pub fn resolve_with_remapping(&self, name: &str, remapping: &FaceRemapping) -> Face {
        let mut seen = HashSet::new();
        self.resolve_remapped(name, remapping, &mut seen, 0)
    }

    fn resolve_remapped(
        &self,
        name: &str,
        remapping: &FaceRemapping,
        seen: &mut HashSet<Value>,
        depth: usize,
    ) -> Face {
        if depth > 20 {
            return Face::new(name);
        }

        let key = face_symbol_value(name);
        // Check face-remapping-alist — but only if we haven't already
        // visited this face (cycle detection, matching GNU's
        // push_named_merge_point).
        if !seen.contains(&key)
            && let Some(entries) = remapping.get(name)
        {
            seen.insert(key);
            let base = self.resolve("default");
            let mut result = base;
            for entry in entries {
                match entry {
                    FaceRemapEntry::RemapFace(target) => {
                        if let Some(target_name) = target.as_symbol_name() {
                            let resolved =
                                self.resolve_remapped(target_name, remapping, seen, depth + 1);
                            result = result.merge(&resolved);
                        }
                    }
                    FaceRemapEntry::RemapAttrs(attrs) => {
                        result = result.merge(attrs);
                    }
                }
            }
            return result;
        }

        // No remapping — fall back to normal resolution.
        self.resolve_depth(name, 0)
    }

    /// Merge a list of face names, consulting `face-remapping-alist`.
    pub fn merge_faces_with_remapping(
        &self,
        face_names: &[&str],
        remapping: &FaceRemapping,
    ) -> Face {
        let default = self.resolve_with_remapping("default", remapping);
        let mut result = default;

        for name in face_names {
            let mut seen = HashSet::new();
            let resolved = self.resolve_remapped_raw(name, remapping, &mut seen, 0);
            result = result.merge(&resolved);
        }

        result
    }

    /// Like `resolve_remapped` but uses raw (non-inherited) face definitions
    /// when no remapping applies, matching `merge_faces` semantics.
    fn resolve_remapped_raw(
        &self,
        name: &str,
        remapping: &FaceRemapping,
        seen: &mut HashSet<Value>,
        depth: usize,
    ) -> Face {
        if depth > 20 {
            return Face::new(name);
        }

        let key = face_symbol_value(name);
        if !seen.contains(&key)
            && let Some(entries) = remapping.get(name)
        {
            seen.insert(key);
            let mut result = Face::new(name);
            for entry in entries {
                match entry {
                    FaceRemapEntry::RemapFace(target) => {
                        if let Some(target_name) = target.as_symbol_name() {
                            let resolved =
                                self.resolve_remapped_raw(target_name, remapping, seen, depth + 1);
                            result = result.merge(&resolved);
                        }
                    }
                    FaceRemapEntry::RemapAttrs(attrs) => {
                        result = result.merge(attrs);
                    }
                }
            }
            return result;
        }

        // No remapping — use the raw face definition (not resolved).
        self.faces
            .get(&face_symbol_value(name))
            .cloned()
            .unwrap_or_else(|| Face::new(name))
    }

    /// List all defined face names.
    pub fn face_list(&self) -> Vec<String> {
        self.faces
            .keys()
            .filter_map(|value| value.as_symbol_name().map(str::to_string))
            .collect()
    }

    /// Number of defined faces.
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    // pdump accessors
    pub(crate) fn dump_faces_by_sym_id(&self) -> Vec<(SymId, Face)> {
        self.faces
            .iter()
            .filter_map(|(name, face)| name.as_symbol_id().map(|id| (id, face.clone())))
            .collect()
    }

    pub(crate) fn from_dump(faces: HashMap<String, Face>) -> Self {
        Self {
            // A dump is written in batch, with no terminal and so no palette;
            // the first TTY frame's face sync installs one.
            tty_palette: std::rc::Rc::new(neomacs_display_protocol::TtyPalette::default()),
            lisp_face_refs: std::rc::Rc::new(HashMap::new()),
            faces: std::rc::Rc::new(
                faces
                    .into_iter()
                    .map(|(name, face)| (face_symbol_value(&name), face))
                    .collect(),
            ),
        }
    }

    pub(crate) fn from_dump_sym_ids(faces: Vec<(SymId, Face)>) -> Self {
        Self {
            tty_palette: std::rc::Rc::new(neomacs_display_protocol::TtyPalette::default()),
            lisp_face_refs: std::rc::Rc::new(HashMap::new()),
            faces: std::rc::Rc::new(
                faces
                    .into_iter()
                    .map(|(name, face)| (Value::from_sym_id(name), face))
                    .collect(),
            ),
        }
    }
}

impl Default for FaceTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Root all cells of a face_ref value so nested conses survive GC.
fn trace_face_ref_roots(value: Value, roots: &mut Vec<Value>) {
    roots.push(value);
    if let Some(items) = crate::emacs_core::value::list_to_vec(&value) {
        for item in items {
            if item.is_cons() {
                trace_face_ref_roots(item, roots);
            } else {
                roots.push(item);
            }
        }
    }
}

impl GcTrace for FaceTable {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        roots.extend(self.faces.keys().copied());
        for face in self.faces.values() {
            if let Some(family) = face.family {
                roots.push(family);
            }
            if let Some(foundry) = face.foundry {
                roots.push(foundry);
            }
            if let Some(stipple) = face.stipple {
                roots.push(stipple);
            }
            if let Some(doc) = face.doc {
                roots.push(doc);
            }
            // `:inherit` can be an arbitrary face_ref — walk it so any
            // cons cells in list/plist forms stay rooted across GC.
            if let Some(inherit) = face.inherit {
                trace_face_ref_roots(inherit, roots);
            }
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "face_test.rs"]
mod tests;
