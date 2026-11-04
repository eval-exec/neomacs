//! Display vocabulary shared below the evaluator (stage-4 W1 E2).
//!
//! These are pure ids/constants that both the display layer and the evaluator
//! name. They live in the lowest crate that can name them so that
//! `neovm-eval`'s Context fields and signatures need no display-crate type.

/// Opaque frame identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameId(pub u64);

/// Keep frame and window numeric domains disjoint while both are represented
/// as Lisp integers.
pub const FRAME_ID_BASE: u64 = 1 << 32;

use crate::value::Value;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use strum::{EnumString, IntoStaticStr};

/// GNU's two frame-argument domains, as a closed set.
///
/// The frame decoders differ only in whether a dead frame is admitted, and --
/// like the window ones -- each fixes the predicate its rejection reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
pub enum FrameDomain {
    /// `decode_live_frame` / `CHECK_LIVE_FRAME`.
    #[strum(serialize = "frame-live-p")]
    Live,
    /// `decode_any_frame` / `CHECK_FRAME`: any frame object.
    #[strum(serialize = "framep")]
    Any,
}

impl FrameDomain {
    /// The symbol a rejection reports, derived from the variant.
    pub fn predicate(self) -> &'static str {
        self.into()
    }
}

/// GNU-compatible lifecycle state for a live frame.
///
/// GNU stores `visible` and `iconified` as separate bits, but only these three
/// public states are valid.  Keeping the sum type here prevents callers from
/// manufacturing the impossible `visible && iconified` combination and makes
/// every projection choose its intended semantics explicitly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameVisibility {
    #[default]
    Visible,
    Iconified,
    Invisible,
}

impl FrameVisibility {
    pub const fn is_visible(self) -> bool {
        matches!(self, Self::Visible)
    }

    pub const fn is_visible_or_iconified(self) -> bool {
        matches!(self, Self::Visible | Self::Iconified)
    }

    pub const fn is_invisible(self) -> bool {
        matches!(self, Self::Invisible)
    }

    pub fn from_lisp_value(value: Value) -> Self {
        if value.is_nil() {
            Self::Invisible
        } else if value == Value::symbol("icon") {
            Self::Iconified
        } else {
            Self::Visible
        }
    }

    pub fn lisp_value(self) -> Value {
        match self {
            Self::Visible => Value::T,
            Self::Iconified => Value::symbol("icon"),
            Self::Invisible => Value::NIL,
        }
    }
}

/// Whether hidden frames participate in a frame-tree query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderFrameVisibility {
    VisibleOnly,
    IncludeInvisible,
}

impl RenderFrameVisibility {
    pub const fn includes_invisible(self) -> bool {
        matches!(self, Self::IncludeInvisible)
    }
}

/// A redisplay epoch stamp.
///
/// `NonZeroU32` makes the reserved value unrepresentable, so
/// `Option<ChangeStamp>` IS GNU's "0 or a stamp" in one word and the guard
/// cannot be dropped by accident.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChangeStamp(std::num::NonZeroU32);

impl ChangeStamp {
    /// The first epoch.
    pub const FIRST: Self = Self(std::num::NonZeroU32::new(1).unwrap());

    /// The next epoch, wrapping past zero exactly as GNU's guard does.
    #[must_use]
    pub fn next(self) -> Self {
        Self(std::num::NonZeroU32::new(self.0.get().wrapping_add(1)).unwrap_or(Self::FIRST.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal, // side by side
    Vertical,   // stacked
}

/// Host-local reference to an exact selected font entity. Handles are never
/// reused during a host's lifetime; the host validates every lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontEntityHandle(std::num::NonZeroU32);

impl FontEntityHandle {
    pub fn new(value: u32) -> Option<Self> {
        std::num::NonZeroU32::new(value).map(Self)
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// The cache key for shaped gstrings: the header's font family, pixel size
/// (bit-exact), and the run's characters. Two gstrings with the same key
/// shape identically, so one cache entry serves both.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GstringShapeCacheKey {
    pub family: String,
    pub pixel_size_bits: u64,
    pub chars: Vec<i64>,
}

// ---------------------------------------------------------------------------
// Face attribute vocabulary (stage-4 W1 E2)
// ---------------------------------------------------------------------------

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
    pub fn index(self) -> usize {
        usize::from(u8::from(self))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub fn from_index(index: usize) -> Option<Self> {
        let index = u8::try_from(index).ok()?;
        Self::try_from(index).ok()
    }

    pub fn keyword(self) -> &'static str {
        self.into()
    }

    pub const fn is_discrete_boolean(self) -> bool {
        matches!(
            self,
            LFaceAttr::Underline
                | LFaceAttr::Overline
                | LFaceAttr::StrikeThrough
                | LFaceAttr::InverseVideo
                | LFaceAttr::Extend
        )
    }

    pub fn from_keyword(name: &str) -> Option<Self> {
        name.strip_prefix(':')?.parse().ok()
    }
}

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
        // Preserve the legacy CSS-weight fallback for unrecognized alias codes.
        match FontWeightDumpCode::try_from(code) {
            Ok(code) => code.into(),
            Err(_) => Self::from_css_weight(code),
        }
    }

    pub fn dump_code(self) -> u16 {
        u16::from(FontWeightDumpCode::from(self))
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

// Dump-code vocabulary for in-memory face weights (canonical image codes).

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoPrimitive, TryFromPrimitive, strum::EnumIter)]
#[repr(u16)]
pub enum FontWeightDumpCode {
    Thin = 100,
    UltraLight = 101,
    Ultralight = 102,
    ExtraLight = 200,
    Extralight = 201,
    Light = 300,
    SemiLight = 350,
    Semilight = 351,
    Demilight = 352,
    Regular = 401,
    Normal = 400,
    Unspecified = 402,
    Book = 403,
    Medium = 500,
    SemiBold = 600,
    Semibold = 601,
    Demibold = 602,
    DemiBold = 603,
    Demi = 604,
    Bold = 700,
    ExtraBold = 800,
    Extrabold = 801,
    UltraBold = 802,
    Ultrabold = 803,
    Black = 900,
    Heavy = 901,
    UltraHeavy = 950,
    Ultraheavy = 951,
}

impl From<FontWeightDumpCode> for FontWeight {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn from(code: FontWeightDumpCode) -> Self {
        match code {
            FontWeightDumpCode::Thin => Self::Thin,
            FontWeightDumpCode::UltraLight => Self::UltraLight,
            FontWeightDumpCode::Ultralight => Self::Ultralight,
            FontWeightDumpCode::ExtraLight => Self::ExtraLight,
            FontWeightDumpCode::Extralight => Self::Extralight,
            FontWeightDumpCode::Light => Self::Light,
            FontWeightDumpCode::SemiLight => Self::SemiLight,
            FontWeightDumpCode::Semilight => Self::Semilight,
            FontWeightDumpCode::Demilight => Self::Demilight,
            FontWeightDumpCode::Regular => Self::Regular,
            FontWeightDumpCode::Normal => Self::Normal,
            FontWeightDumpCode::Unspecified => Self::Unspecified,
            FontWeightDumpCode::Book => Self::Book,
            FontWeightDumpCode::Medium => Self::Medium,
            FontWeightDumpCode::SemiBold => Self::SemiBold,
            FontWeightDumpCode::Semibold => Self::Semibold,
            FontWeightDumpCode::Demibold => Self::Demibold,
            FontWeightDumpCode::DemiBold => Self::DemiBold,
            FontWeightDumpCode::Demi => Self::Demi,
            FontWeightDumpCode::Bold => Self::Bold,
            FontWeightDumpCode::ExtraBold => Self::ExtraBold,
            FontWeightDumpCode::Extrabold => Self::Extrabold,
            FontWeightDumpCode::UltraBold => Self::UltraBold,
            FontWeightDumpCode::Ultrabold => Self::Ultrabold,
            FontWeightDumpCode::Black => Self::Black,
            FontWeightDumpCode::Heavy => Self::Heavy,
            FontWeightDumpCode::UltraHeavy => Self::UltraHeavy,
            FontWeightDumpCode::Ultraheavy => Self::Ultraheavy,
        }
    }
}

impl From<FontWeight> for FontWeightDumpCode {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn from(weight: FontWeight) -> Self {
        match weight {
            FontWeight::Thin => Self::Thin,
            FontWeight::UltraLight => Self::UltraLight,
            FontWeight::Ultralight => Self::Ultralight,
            FontWeight::ExtraLight => Self::ExtraLight,
            FontWeight::Extralight => Self::Extralight,
            FontWeight::Light => Self::Light,
            FontWeight::SemiLight => Self::SemiLight,
            FontWeight::Semilight => Self::Semilight,
            FontWeight::Demilight => Self::Demilight,
            FontWeight::Regular => Self::Regular,
            FontWeight::Normal => Self::Normal,
            FontWeight::Unspecified => Self::Unspecified,
            FontWeight::Book => Self::Book,
            FontWeight::Medium => Self::Medium,
            FontWeight::SemiBold => Self::SemiBold,
            FontWeight::Semibold => Self::Semibold,
            FontWeight::Demibold => Self::Demibold,
            FontWeight::DemiBold => Self::DemiBold,
            FontWeight::Demi => Self::Demi,
            FontWeight::Bold => Self::Bold,
            FontWeight::ExtraBold => Self::ExtraBold,
            FontWeight::Extrabold => Self::Extrabold,
            FontWeight::UltraBold => Self::UltraBold,
            FontWeight::Ultrabold => Self::Ultrabold,
            FontWeight::Black => Self::Black,
            FontWeight::Heavy => Self::Heavy,
            FontWeight::UltraHeavy => Self::UltraHeavy,
            FontWeight::Ultraheavy => Self::Ultraheavy,
        }
    }
}
