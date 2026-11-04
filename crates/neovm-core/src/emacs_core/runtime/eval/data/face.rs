//! Face vocabulary owned by the evaluator layer (stage-4 W1 E2).

use neovm_value::display_vocab::{FontSlant, FontWeight, FontWidth};
use neovm_value::value::Value;

/// GNU's forced basic face roles, in realization order. A role retains its
/// own result even when advised color resolution gives equal source attrs
/// different colors on successive calls (xfaces.c:5952-5983,6182-6219).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    enum_map::Enum,
    strum::EnumIter,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum BasicFace {
    Default,
    ModeLineActive,
    ModeLineInactive,
    ToolBar,
    Fringe,
    HeaderLineActive,
    HeaderLineInactive,
    ScrollBar,
    Border,
    Cursor,
    Mouse,
    Menu,
    VerticalBorder,
    WindowDivider,
    WindowDividerFirstPixel,
    WindowDividerLastPixel,
    InternalBorder,
    ChildFrameBorder,
    TabBar,
    TabLineActive,
    TabLineInactive,
    Margin,
}

impl BasicFace {
    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn from_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }
}

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
    ///
    /// The one implementation of that syntax lives in
    /// [`neomacs_display_protocol::x11_hex_color`], shared with the XPM decoder:
    /// a second copy here is how those two came to disagree (issue #545).
    pub fn from_hex(s: &str) -> Option<Self> {
        neomacs_display_protocol::x11_hex_color(s).map(|(r, g, b)| Color::rgb(r, g, b))
    }

    /// Convert to "#RRGGBB" hex string.
    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Named color lookup (the X11 `rgb.txt` database GNU resolves names from).
    ///
    /// [`neomacs_display_protocol::x11_color_lookup`] is the whole of the
    /// database, shared with the image decoders that resolve names (XPM `c`
    /// keys), so a face and an XPM carrying the same name cannot disagree.
    pub fn from_name(name: &str) -> Option<Self> {
        neomacs_display_protocol::x11_color_lookup(name).map(|(r, g, b)| Color::rgb(r, g, b))
    }

    /// Parse a color spec the way GNU's color hook does: a numeric form
    /// (`#`-hex, `rgb:`, `rgbi:`) or a database name.
    pub fn parse(spec: &str) -> Option<Self> {
        neomacs_display_protocol::x11_color_value(spec).map(|(r, g, b)| Color::rgb(r, g, b))
    }
}

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

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    strum::EnumString,
    num_enum::IntoPrimitive,
    strum::IntoStaticStr,
    num_enum::TryFromPrimitive,
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

#[derive(Clone, Debug, PartialEq)]
pub struct BoxBorder {
    pub color: Option<Color>,
    pub width: i32,
    pub style: BoxStyle,
}

/// Logical box styles, before choosing the face-relative default color.
///
/// GNU draws both a plain box and `flat-button` as FACE_SIMPLE_BOX, but
/// defaults their colors to foreground and background respectively. Keep
/// that distinction until realization instead of storing the drawing code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum BoxStyle {
    #[strum(to_string = "nil")]
    Simple,
    #[strum(to_string = "flat-button")]
    Flat,
    #[strum(to_string = "released-button")]
    Raised,
    #[strum(to_string = "pressed-button")]
    Pressed,
}

impl BoxStyle {
    pub fn from_symbol(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn symbol_name(self) -> &'static str {
        self.into()
    }

    pub fn gnu_code(self) -> u8 {
        match self {
            Self::Simple | Self::Flat => 1,
            Self::Raised => 2,
            Self::Pressed => 3,
        }
    }

    /// Decode a drawing kind, which cannot recover an explicit `flat-button`.
    /// Lisp parsing and dump storage retain the logical style independently.
    pub fn from_gnu_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Simple),
            2 => Some(Self::Raised),
            3 => Some(Self::Pressed),
            _ => None,
        }
    }
}

/// A typed face attribute value for `FaceTable::set_attribute()`.
#[derive(Clone, Debug)]
pub enum FaceAttrValue {
    Color(Color),
    /// Preserve an lface color string together with its current realization.
    /// Failed realization retains the specification for a later retry.
    SpecifiedColor {
        specification: SpecifiedColor,
        realized: Option<Color>,
    },
    /// An underline whose original color specification must survive realization.
    SpecifiedUnderline {
        underline: Underline,
        color: Option<SpecifiedColor>,
    },
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

/// Height specification.
#[derive(Clone, Debug, PartialEq)]
pub enum FaceHeight {
    /// Absolute height in 1/10 pt.
    Absolute(i32),
    /// Relative to default face (multiplier).
    Relative(f64),
}
