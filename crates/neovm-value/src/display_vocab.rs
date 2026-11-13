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

/// Evaluator-owned correlation ID for one asynchronous xwidget script call.
///
/// The display host cannot allocate this ID: only the evaluator knows which
/// GC-rooted Lisp callback must receive the eventual result.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct XwidgetScriptRequestId(u64);

impl XwidgetScriptRequestId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

use crate::intern::{self, SymId, WellKnown, resolve_sym};
use crate::value::{Value, ValueKind, VecLikeType, eq_value};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use strum::{EnumCount, EnumString, IntoStaticStr, VariantArray};

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

/// GNU's frame fullscreen/windowed state, as a closed set.
///
/// GNU's symbols are nil/`fullboth`/`fullscreen`/`fullwidth`/`fullheight`/
/// `maximized`; `Windowed` is the explicit nil request, not a missing value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum FrameFullscreen {
    /// Lisp nil explicitly requests restoration; it is not a missing request.
    #[strum(serialize = "nil")]
    Windowed,
    Fullboth,
    Fullscreen,
    Fullwidth,
    Fullheight,
    Maximized,
}

impl FrameFullscreen {
    pub fn from_symbol_value(value: &Value) -> Option<Self> {
        if value.is_nil() {
            return Some(Self::Windowed);
        }
        value.as_bare_symbol_name()?.parse().ok()
    }

    pub fn name(self) -> &'static str {
        self.into()
    }
}

/// A frame's display output method, as GNU's `framep` vocabulary spells it.
///
/// This is the frame's INTERNAL output method: the backend installs it when
/// the frame is created, and GNU's `Fwindow_system` derives its answer from
/// it alone (src/frame.c:415-436). The Lisp `window-system` frame parameter
/// is not an input. `Neo` is this runtime's GUI backend identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum WindowSystemKind {
    X,
    W32,
    Pc,
    Ns,
    Pgtk,
    Haiku,
    Android,
    Neo,
}

impl WindowSystemKind {
    /// Parse a display symbol name (`x`, `w32`, `neo`, ...).
    pub fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    /// Parse a bare Lisp symbol value.
    pub fn from_symbol_value(value: &Value) -> Option<Self> {
        Self::from_symbol_name(value.as_bare_symbol_name()?)
    }

    /// The symbol name of this output method.
    pub fn name(self) -> &'static str {
        self.into()
    }

    /// Whether this runtime's GUI path serves the method.
    pub const fn is_neomacs_gui_compatible(self) -> bool {
        matches!(self, Self::Neo | Self::X)
    }

    /// GNU's selection-capable window systems.
    pub const fn supports_selections(self) -> bool {
        matches!(
            self,
            Self::X | Self::W32 | Self::Ns | Self::Pgtk | Self::Haiku | Self::Android | Self::Neo
        )
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

/// GNU-shaped frame parameter keys that both display and the evaluator name.

/// Fixed Core query keys outside GNU's indexed frame-parameter handler table.
#[repr(u8)]
#[derive(Clone, Copy, Debug, EnumCount, IntoPrimitive, IntoStaticStr, VariantArray)]
#[strum(serialize_all = "kebab-case")]
pub enum FrameQueryParam {
    Width,
    Height,
    WindowSystem,
    FontParameter,
    Fontsize,
    Tooltip,
    Display,
    MinWidth,
    MinHeight,
    #[strum(serialize = "neovm--frame-text-lines")]
    TextLines,
    #[strum(serialize = "neovm--frame-total-cols")]
    TotalCols,
    #[strum(serialize = "neovm--frame-total-lines")]
    TotalLines,
}

const _: [(); FrameQueryParam::COUNT] = [(); FrameQueryParam::VARIANTS.len()];

impl FrameQueryParam {
    /// The fixed DEFSYM identity this query key is spelled as.
    pub const fn well_known(self) -> WellKnown {
        match self {
            FrameQueryParam::Width => WellKnown::Width,
            FrameQueryParam::Height => WellKnown::Height,
            FrameQueryParam::WindowSystem => WellKnown::WindowSystem,
            FrameQueryParam::FontParameter => WellKnown::FontParameter,
            FrameQueryParam::Fontsize => WellKnown::Fontsize,
            FrameQueryParam::Tooltip => WellKnown::Tooltip,
            FrameQueryParam::Display => WellKnown::Display,
            FrameQueryParam::MinWidth => WellKnown::MinWidth,
            FrameQueryParam::MinHeight => WellKnown::MinHeight,
            FrameQueryParam::TextLines => WellKnown::NeovmFrameTextLines,
            FrameQueryParam::TotalCols => WellKnown::NeovmFrameTotalCols,
            FrameQueryParam::TotalLines => WellKnown::NeovmFrameTotalLines,
        }
    }
}

impl FrameQueryParam {
    /// Interned symbol this query key names in the frame parameter map.
    pub fn symbol_id(self) -> SymId {
        self.well_known().id()
    }

    /// Lisp-visible symbol value of this query key.
    pub fn symbol(self) -> Value {
        Value::from_sym_id(self.symbol_id())
    }
}

#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    EnumString,
    EnumCount,
    IntoStaticStr,
    IntoPrimitive,
    TryFromPrimitive,
)]
#[strum(serialize_all = "kebab-case")]
pub enum FrameParam {
    AutoRaise = 0,
    AutoLower,
    BackgroundColor,
    BorderColor,
    BorderWidth,
    CursorColor,
    CursorType,
    Font,
    ForegroundColor,
    IconName,
    IconType,
    ChildFrameBorderWidth,
    InternalBorderWidth,
    RightDividerWidth,
    BottomDividerWidth,
    MenuBarLines,
    MouseColor,
    Name,
    ScrollBarWidth,
    ScrollBarHeight,
    Title,
    Unsplittable,
    VerticalScrollBars,
    HorizontalScrollBars,
    Visibility,
    TabBarLines,
    ToolBarLines,
    ScrollBarForeground,
    ScrollBarBackground,
    ScreenGamma,
    LineSpacing,
    LeftFringe,
    RightFringe,
    WaitForWm,
    Fullscreen,
    FontBackend,
    Alpha,
    Sticky,
    ToolBarPosition,
    InhibitDoubleBuffering,
    Undecorated,
    ParentFrame,
    SkipTaskbar,
    NoFocusOnMap,
    NoAcceptFocus,
    ZGroup,
    OverrideRedirect,
    NoSpecialGlyphs,
    AlphaBackground,
    BordersRespectAlphaBackground,
    UseFrameSynchronization,
    Shaded,
    NsAppearance,
    NsTransparentTitlebar,
}

impl FrameParam {
    /// The fixed DEFSYM identity GNU's `frame_parms[]` entry is spelled as.
    pub const fn well_known(self) -> WellKnown {
        match self {
            FrameParam::AutoRaise => WellKnown::AutoRaise,
            FrameParam::AutoLower => WellKnown::AutoLower,
            FrameParam::BackgroundColor => WellKnown::BackgroundColor,
            FrameParam::BorderColor => WellKnown::BorderColor,
            FrameParam::BorderWidth => WellKnown::BorderWidth,
            FrameParam::CursorColor => WellKnown::CursorColor,
            FrameParam::CursorType => WellKnown::CursorType,
            FrameParam::Font => WellKnown::Font,
            FrameParam::ForegroundColor => WellKnown::ForegroundColor,
            FrameParam::IconName => WellKnown::IconName,
            FrameParam::IconType => WellKnown::IconType,
            FrameParam::ChildFrameBorderWidth => WellKnown::ChildFrameBorderWidth,
            FrameParam::InternalBorderWidth => WellKnown::InternalBorderWidth,
            FrameParam::RightDividerWidth => WellKnown::RightDividerWidth,
            FrameParam::BottomDividerWidth => WellKnown::BottomDividerWidth,
            FrameParam::MenuBarLines => WellKnown::MenuBarLines,
            FrameParam::MouseColor => WellKnown::MouseColor,
            FrameParam::Name => WellKnown::Name,
            FrameParam::ScrollBarWidth => WellKnown::ScrollBarWidth,
            FrameParam::ScrollBarHeight => WellKnown::ScrollBarHeight,
            FrameParam::Title => WellKnown::Title,
            FrameParam::Unsplittable => WellKnown::Unsplittable,
            FrameParam::VerticalScrollBars => WellKnown::VerticalScrollBars,
            FrameParam::HorizontalScrollBars => WellKnown::HorizontalScrollBars,
            FrameParam::Visibility => WellKnown::Visibility,
            FrameParam::TabBarLines => WellKnown::TabBarLines,
            FrameParam::ToolBarLines => WellKnown::ToolBarLines,
            FrameParam::ScrollBarForeground => WellKnown::ScrollBarForeground,
            FrameParam::ScrollBarBackground => WellKnown::ScrollBarBackground,
            FrameParam::ScreenGamma => WellKnown::ScreenGamma,
            FrameParam::LineSpacing => WellKnown::LineSpacing,
            FrameParam::LeftFringe => WellKnown::LeftFringe,
            FrameParam::RightFringe => WellKnown::RightFringe,
            FrameParam::WaitForWm => WellKnown::WaitForWm,
            FrameParam::Fullscreen => WellKnown::Fullscreen,
            FrameParam::FontBackend => WellKnown::FontBackend,
            FrameParam::Alpha => WellKnown::Alpha,
            FrameParam::Sticky => WellKnown::Sticky,
            FrameParam::ToolBarPosition => WellKnown::ToolBarPosition,
            FrameParam::InhibitDoubleBuffering => WellKnown::InhibitDoubleBuffering,
            FrameParam::Undecorated => WellKnown::Undecorated,
            FrameParam::ParentFrame => WellKnown::ParentFrame,
            FrameParam::SkipTaskbar => WellKnown::SkipTaskbar,
            FrameParam::NoFocusOnMap => WellKnown::NoFocusOnMap,
            FrameParam::NoAcceptFocus => WellKnown::NoAcceptFocus,
            FrameParam::ZGroup => WellKnown::ZGroup,
            FrameParam::OverrideRedirect => WellKnown::OverrideRedirect,
            FrameParam::NoSpecialGlyphs => WellKnown::NoSpecialGlyphs,
            FrameParam::AlphaBackground => WellKnown::AlphaBackground,
            FrameParam::BordersRespectAlphaBackground => WellKnown::BordersRespectAlphaBackground,
            FrameParam::UseFrameSynchronization => WellKnown::UseFrameSynchronization,
            FrameParam::Shaded => WellKnown::Shaded,
            FrameParam::NsAppearance => WellKnown::NsAppearance,
            FrameParam::NsTransparentTitlebar => WellKnown::NsTransparentTitlebar,
        }
    }

    pub fn gnu_index(self) -> usize {
        usize::from(u8::from(self))
    }

    pub fn from_gnu_index(index: usize) -> Option<Self> {
        let index = u8::try_from(index).ok()?;
        Self::try_from(index).ok()
    }

    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn symbol(self) -> Value {
        Value::from_sym_id(self.well_known().id())
    }

    pub fn from_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn from_symbol_id(symbol: SymId) -> Option<Self> {
        Self::from_name(resolve_sym(symbol))
    }
}

/// Every Lisp variable whose effective value is read by window layout.
///
/// This is deliberately a closed set. In-flight layout validation snapshots
/// the effective values by variant, so adding a new display-variable read to
/// layout requires extending this enum and cannot silently leave the
/// validation boundary incomplete.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    strum::EnumString,
    strum::IntoStaticStr,
    strum::EnumCount,
    strum::VariantArray,
)]
#[repr(usize)]
#[strum(serialize_all = "kebab-case")]
pub enum WindowLayoutVariable {
    AutoCompositionFunction,
    AutoCompositionMode,
    BidiDisplayReordering,
    BidiParagraphDirection,
    BidiParagraphSeparateRe,
    BidiParagraphStartRe,
    BufferDisplayTable,
    BufferInvisibilitySpec,
    CharPropertyAliasAlist,
    CompactBarMode,
    CtlArrow,
    CursorInNonSelectedWindows,
    CursorType,
    DefaultTextProperties,
    DisplayFillColumnIndicator,
    DisplayFillColumnIndicatorCharacter,
    DisplayFillColumnIndicatorColumn,
    DisplayLineNumbers,
    DisplayLineNumbersCurrentAbsolute,
    DisplayLineNumbersMajorTick,
    DisplayLineNumbersMinorTick,
    DisplayLineNumbersOffset,
    DisplayLineNumbersWiden,
    DisplayLineNumbersWidth,
    FaceRemappingAlist,
    FillColumn,
    FringeCursorAlist,
    FringeIndicatorAlist,
    FringesOutsideMargins,
    GlyphlessCharDisplay,
    HeaderLineFormat,
    HeaderLineIndentWidth,
    HorizontalScrollBar,
    ImageScalingFactor,
    IndicateEmptyLines,
    IndicateBufferBoundaries,
    LeftFringeWidth,
    LeftMargin,
    LeftMarginWidth,
    LinePrefix,
    LineSpacing,
    MaxMiniWindowHeight,
    MaximumScrollMargin,
    MenuBarFinalItems,
    ModeLineFormat,
    NeomacsCursorEffect,
    NeomacsToolbarIconDirectory,
    NeomacsToolbarIconTheme,
    NeomacsVisualCursors,
    NobreakCharDisplay,
    OverlayArrowPosition,
    OverlayArrowString,
    OverlayArrowVariableList,
    RedisplayAdhocScrollInResizeMiniWindows,
    ResizeMiniWindows,
    RightFringeWidth,
    RightMarginWidth,
    ScrollBarHeight,
    ScrollBarWidth,
    ScrollConservatively,
    ScrollDownAggressively,
    ScrollMargin,
    ScrollMinibufferConservatively,
    ScrollStep,
    ScrollUpAggressively,
    SelectiveDisplay,
    SelectiveDisplayEllipses,
    ShowTrailingWhitespace,
    StandardDisplayTable,
    TabBarButtonMargin,
    TabBarButtonRelief,
    TabLineFormat,
    TabStopList,
    TabWidth,
    ToolBarMap,
    TruncateLines,
    TruncatePartialWidthWindows,
    VerticalScrollBar,
    WordWrap,
    WordWrapByCategory,
    WrapPrefix,
    XCursorForePixel,
    XStretchCursor,
}

impl WindowLayoutVariable {
    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn sym_id(self) -> SymId {
        use std::sync::OnceLock;
        use strum::VariantArray;

        static SYMBOLS: OnceLock<[SymId; <WindowLayoutVariable as strum::EnumCount>::COUNT]> =
            OnceLock::new();
        SYMBOLS.get_or_init(|| {
            let mut symbols =
                [intern::NIL_SYM_ID; <WindowLayoutVariable as strum::EnumCount>::COUNT];
            for variable in WindowLayoutVariable::VARIANTS {
                symbols[*variable as usize] = intern::intern(variable.name());
            }
            symbols
        })[self as usize]
    }
}

/// Live-window history state that GNU Emacs stores directly on `struct window`.
#[derive(Clone, Debug)]
pub struct WindowHistoryState {
    pub prev_buffers: Value,
    pub next_buffers: Value,
    pub use_time: i64,
}

impl Default for WindowHistoryState {
    fn default() -> Self {
        Self {
            prev_buffers: Value::NIL,
            next_buffers: Value::NIL,
            use_time: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourcePreloadWindowRetirementRefusal {
    LiveFramesRemain,
}

/// Which frames a snapshot request covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotTarget {
    /// The selected frame only.
    Selected,
    /// Every visible frame in bottom-to-top z order — the full composited
    /// screen, including child frames (posframe/corfu popups, tooltips).
    All,
    /// One specific frame (core `FrameId.0`).
    Frame(u64),
}

/// Serialization format of a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFormat {
    /// Greppable logical text grid (`FrameDisplayState::render_text`).
    Text,
    /// Text plus per-row face runs with names and resolved hex colors.
    TextFaces,
    /// Full-fidelity JSON: serde on the real protocol structs.
    Json,
    /// Frame and window geometry without replay assets or glyph matrices.
    JsonGeometry,
}

/// One `neomacs--frame-snapshot` request, handed to the frontend hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotRequest {
    pub target: SnapshotTarget,
    pub format: SnapshotFormat,
}

// --- stage-4 W2/E5: display space geometry keys (TYPE-DOWN) ---

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr, strum::EnumCount,
)]
#[repr(usize)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
pub enum DisplaySpaceKey {
    Width,
    RelativeWidth,
    AlignTo,
    Height,
    RelativeHeight,
    Ascent,
}

impl DisplaySpaceKey {
    pub fn from_lisp_value(value: Value) -> Option<Self> {
        Self::from_keyword(value.as_bare_symbol_name()?)
    }

    pub fn from_keyword(name: &str) -> Option<Self> {
        name.strip_prefix(':')?.parse().ok()
    }

    pub fn keyword(self) -> &'static str {
        self.into()
    }

    pub fn value(self) -> Value {
        Value::keyword(self.keyword())
    }
}

/// The internal meaning of GNU's numeric `invisible-p` result.
///
/// Lisp compatibility still exposes nil/t/2, but native consumers match this
/// enum so adding or changing an invisibility class is compiler-checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invisibility {
    Visible,
    Hidden,
    HiddenWithEllipsis,
}

impl Invisibility {
    pub const fn hides_source(self) -> bool {
        !matches!(self, Self::Visible)
    }

    pub const fn shows_ellipsis(self) -> bool {
        matches!(self, Self::HiddenWithEllipsis)
    }

    pub fn into_lisp(self) -> Value {
        match self {
            Self::Visible => Value::NIL,
            Self::Hidden => Value::T,
            Self::HiddenWithEllipsis => Value::fixnum(2),
        }
    }
}

fn invisible_prop_member(propval: Value, list: Value) -> Invisibility {
    let mut tail = list;
    while tail.is_cons() {
        let element = tail.cons_car();
        if eq_value(&propval, &element) {
            return Invisibility::Hidden;
        }
        if element.is_cons() && eq_value(&propval, &element.cons_car()) {
            return if element.cons_cdr().is_nil() {
                Invisibility::Hidden
            } else {
                Invisibility::HiddenWithEllipsis
            };
        }
        tail = tail.cons_cdr();
    }
    Invisibility::Visible
}

fn invisible_prop(propval: Value, list: Value) -> Invisibility {
    let direct = invisible_prop_member(propval, list);
    if direct.hides_source() {
        return direct;
    }

    let mut proptail = propval;
    while proptail.is_cons() {
        let result = invisible_prop_member(proptail.cons_car(), list);
        if result.hides_source() {
            return result;
        }
        proptail = proptail.cons_cdr();
    }
    Invisibility::Visible
}

/// GNU `TEXT_PROP_MEANS_INVISIBLE`: classify a resolved `invisible` property
/// against the current buffer's `buffer-invisibility-spec`.
pub fn text_prop_means_invisible(prop: Value, invisibility_spec: Value) -> Invisibility {
    if invisibility_spec == Value::T {
        if prop.is_truthy() {
            Invisibility::Hidden
        } else {
            Invisibility::Visible
        }
    } else {
        invisible_prop(prop, invisibility_spec)
    }
}

/// Font-vector tag spellings (GNU font-spec/font-entity/font-object).
pub const FONT_SPEC_TAG: &str = "font-spec";
pub const FONT_ENTITY_TAG: &str = "font-entity";
pub const FONT_OBJECT_TAG: &str = "font-object";

/// Whether `val` is a vector whose first element is `tag` (with GNU's
/// optional leading-colon spelling).
pub fn is_tagged_font_vector(val: &Value, tag: &str) -> bool {
    match val.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let elems = val.as_vector_data().unwrap().clone();
            elems
                .first()
                .and_then(|v| v.as_bare_symbol_name())
                .is_some_and(|name| name.trim_start_matches(':') == tag)
        }
        _ => false,
    }
}

/// GNU `cl-type-of`'s font-vector classification.
pub fn font_value_type_symbol(val: &Value) -> Option<&'static str> {
    if is_tagged_font_vector(val, FONT_SPEC_TAG) {
        Some(FONT_SPEC_TAG)
    } else if is_tagged_font_vector(val, FONT_ENTITY_TAG) {
        Some(FONT_ENTITY_TAG)
    } else if val.is_font_object() {
        Some(FONT_OBJECT_TAG)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Image type vocabulary (stage-4 K2 display boot: values shared with the
// evaluator bootstrap, which must not name the display image module).
// ---------------------------------------------------------------------------
/// GNU image types available in this build.
///
/// GNU keeps these in `image_types[]` in `src/image.c`; optional entries are
/// present only when their backing decoder is built.  Neomacs currently ships
/// the portable decoder set below and deliberately leaves optional GNU entries
/// such as `postscript`, `imagemagick`, and `native-image` unavailable until
/// their loaders are implemented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum ImageType {
    Svg,
    Webp,
    Png,
    Gif,
    Tiff,
    Jpeg,
    Xpm,
    Xbm,
    Pbm,
}

impl ImageType {
    const AVAILABLE_IN_GNU_LIST_ORDER: [Self; 9] = [
        Self::Svg,
        Self::Webp,
        Self::Png,
        Self::Gif,
        Self::Tiff,
        Self::Jpeg,
        Self::Xpm,
        Self::Xbm,
        Self::Pbm,
    ];

    pub fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn from_file_extension(extension: &str) -> Option<Self> {
        ImageFilenameType::from_file_extension(extension)?.available_type()
    }

    pub fn from_file_name(path: &str) -> Option<Self> {
        ImageFilenameType::from_file_name(path)?.available_type()
    }

    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn value(self) -> Value {
        Value::symbol(self.name())
    }
}

/// Image types recognized by GNU filename inference.
///
/// GNU `lisp/image.el:image-type-file-name-regexps` has a wider domain than
/// `image-type-available-p`: unavailable loaders such as `bmp`, `postscript`,
/// and `heic` can still be inferred from file names for diagnostics and later
/// availability checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum ImageFilenameType {
    Png,
    Gif,
    Jpeg,
    Webp,
    Bmp,
    Xpm,
    Pbm,
    Xbm,
    Postscript,
    Tiff,
    Svg,
    Heic,
}

impl ImageFilenameType {
    pub fn from_file_extension(extension: &str) -> Option<Self> {
        let extension = extension
            .strip_prefix('.')
            .unwrap_or(extension)
            .to_ascii_lowercase();
        match extension.as_str() {
            "jpg" => Some(Self::Jpeg),
            "tif" => Some(Self::Tiff),
            "svgz" => Some(Self::Svg),
            "ps" => Some(Self::Postscript),
            "heics" | "heif" | "heifs" => Some(Self::Heic),
            name => name.parse().ok(),
        }
    }

    pub fn from_file_name(path: &str) -> Option<Self> {
        Self::from_file_extension(path.rsplit('.').next()?)
    }

    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn available_type(self) -> Option<ImageType> {
        ImageType::from_symbol_name(self.name())
    }
}

/// GNU `syms_of_image` initial value of `image-types` for the decoders this
/// build ships, in GNU list order.
pub fn supported_image_types_value() -> Value {
    Value::list(
        ImageType::AVAILABLE_IN_GNU_LIST_ORDER
            .iter()
            .copied()
            .map(|image_type| Value::symbol(image_type.name()))
            .collect(),
    )
}

/// GNU `decode_env_path (NULL, defalt, false)` restricted to the
/// no-environment-variable case `syms_of_image` uses.
/// GNU's `decode_env_path (NULL, defalt, false)` (`src/emacs.c:3262-3300`),
/// restricted to the no-environment-variable case `syms_of_image` uses.
///
/// With `evarname == 0` nothing is read from the environment, so the whole
/// function is "split `defalt` on `SEPCHAR` and turn each element into a
/// string, substituting `.` for an empty one" -- the `empty` argument is false
/// at this call site, which is what selects `.` rather than nil.
pub fn decode_env_path_default(defalt: &str) -> Value {
    Value::list(
        defalt
            .split(':')
            .map(|element| Value::string(if element.is_empty() { "." } else { element }))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Font style tables (GNU `syms_of_font` C defaults; the evaluator seeds
// them without naming the display font module).
// ---------------------------------------------------------------------------
pub const FONT_WEIGHT_STYLE_TABLE: &[(i64, &[&str])] = &[
    (0, &["thin"]),
    (
        40,
        &["ultra-light", "ultralight", "extra-light", "extralight"],
    ),
    (50, &["light"]),
    (55, &["semi-light", "semilight", "demilight"]),
    (80, &["regular", "normal", "unspecified", "book"]),
    (100, &["medium"]),
    (
        180,
        &["semi-bold", "semibold", "demibold", "demi-bold", "demi"],
    ),
    (200, &["bold"]),
    (205, &["extra-bold", "extrabold", "ultra-bold", "ultrabold"]),
    (210, &["black", "heavy"]),
    (250, &["ultra-heavy", "ultraheavy"]),
];

pub const FONT_SLANT_STYLE_TABLE: &[(i64, &[&str])] = &[
    (0, &["reverse-oblique", "ro"]),
    (10, &["reverse-italic", "ri"]),
    (100, &["normal", "r", "unspecified"]),
    (200, &["italic", "i", "ot"]),
    (210, &["oblique", "o"]),
];

pub const FONT_WIDTH_STYLE_TABLE: &[(i64, &[&str])] = &[
    (50, &["ultra-condensed", "ultracondensed"]),
    (63, &["extra-condensed", "extracondensed"]),
    (75, &["condensed", "compressed", "narrow"]),
    (87, &["semi-condensed", "semicondensed", "demicondensed"]),
    (100, &["normal", "medium", "regular", "unspecified"]),
    (113, &["semi-expanded", "semiexpanded", "demiexpanded"]),
    (125, &["expanded"]),
    (150, &["extra-expanded", "extraexpanded"]),
    (200, &["ultra-expanded", "ultraexpanded", "wide"]),
];

pub fn font_style_table(entries: &[(i64, &[&str])]) -> Value {
    Value::vector(
        entries
            .iter()
            .map(|(numeric, names)| {
                let mut row = Vec::with_capacity(names.len() + 1);
                row.push(Value::fixnum(*numeric));
                row.extend(names.iter().map(|name| Value::symbol(*name)));
                Value::vector(row)
            })
            .collect(),
    )
}
