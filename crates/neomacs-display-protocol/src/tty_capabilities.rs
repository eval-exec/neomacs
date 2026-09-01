//! What THIS terminal can render — GNU's `struct tty_display_info` capability
//! fields, resolved from terminfo once per terminal.
//!
//! GNU asks terminfo what the terminal can do (`term.c:init_tty`), stores the
//! answer on the terminal, and then consults it from exactly two places:
//! `turn_on_face`, which emits the capability's sequence (with documented
//! fallbacks — italics become dim, a styled underline becomes a plain one), and
//! `tty_capable_p`, which answers `display-supports-face-attributes-p`.
//!
//! neomacs had neither: the renderer hardcoded SGR sequences, so it emitted
//! `ESC [ 3 m` for `:slant italic` on a terminal whose terminfo has no `sitm`
//! (GNU emits its dim fallback there), and the Lisp predicate had no tty branch
//! at all, answering nil for bold and underline that GNU reports as supported.
//! Those are the same fact answered two different wrong ways, so this type is
//! the single answer both paths read.
//!
//! What each capability is SPELLED as is part of that same fact.  GNU's fields
//! are `const char *` and `turn_on_face`'s guard IS the pointer
//! (`OUTPUT1_IF (tty, tty->TS_enter_bold_mode)`, src/term.c:2061), so presence
//! and bytes are one answer.  Carrying a `bool` here and spelling the sequence
//! in the writer made them two, and the two disagree on the database ncurses
//! ships: of its 1,862 unique entries, 448 of the 1,303 that have `us` spell it
//! something other than `ESC [ 4 m`, 234 of 996 spell `md` something other than
//! `ESC [ 1 m`, and 281 of 616 spell `mh` something other than `ESC [ 2 m`
//! (ledger 186).  `so` was already carried as bytes, which is why inverse video
//! was the one attribute this port got right on `screen`, whose standout is
//! `ESC [ 3 m`.

use crate::face::UnderlineStyle;
use crate::terminal_color::TerminalColor;

/// Terminfo `ncv` (`NC`): attributes that CANNOT be combined with colors on this
/// terminal. Bit values are GNU's own `NC_*` enum (src/term.c), not ncurses'.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TtyNoColorVideo(pub u16);

impl TtyNoColorVideo {
    pub const NONE: Self = Self(0);
    pub const STANDOUT: Self = Self(1 << 0);
    pub const UNDERLINE: Self = Self(1 << 1);
    pub const REVERSE: Self = Self(1 << 2);
    pub const ITALIC: Self = Self(1 << 3);
    pub const DIM: Self = Self(1 << 4);
    pub const BOLD: Self = Self(1 << 5);
    pub const STRIKE_THROUGH: Self = Self(1 << 6);
    pub const PROTECT: Self = Self(1 << 7);

    pub const fn contains(self, bit: Self) -> bool {
        self.0 & bit.0 != 0
    }
}

/// One renderable attribute, as GNU's `TTY_CAP_*` flags name them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtyCapability {
    /// `so` — GNU pairs inverse video with the standout capability.
    Inverse,
    /// `us`
    Underline,
    /// `Smulx` — the parameterized styled underline (wave, dotted, …).
    UnderlineStyled,
    /// `md`
    Bold,
    /// `mh`
    Dim,
    /// `ZH`
    Italic,
    /// `smxx`
    StrikeThrough,
}

impl TtyCapability {
    /// The `ncv` bit that disables this attribute on a color terminal, per
    /// GNU's `tty_capable_p` pairings.
    const fn no_color_video_bit(self) -> TtyNoColorVideo {
        match self {
            Self::Inverse => TtyNoColorVideo::REVERSE,
            // GNU tests NC_UNDERLINE for both the plain and the styled form.
            Self::Underline | Self::UnderlineStyled => TtyNoColorVideo::UNDERLINE,
            Self::Bold => TtyNoColorVideo::BOLD,
            Self::Dim => TtyNoColorVideo::DIM,
            Self::Italic => TtyNoColorVideo::ITALIC,
            Self::StrikeThrough => TtyNoColorVideo::STRIKE_THROUGH,
        }
    }
}

/// GNU `TF_set_underline_style` (`Smulx`), already expanded.
///
/// GNU expands at emit time — `tparam (tty->TF_set_underline_style, NULL, 0,
/// face->underline, 0, 0, 0)` (src/term.c:2083), and in a terminfo build
/// `tparam` IS ncurses' `tparm` (src/terminfo.c:43-55).  The parameter is an
/// `enum face_underline_type` and its domain is CLOSED: `turn_on_face` reaches
/// this call only when `face->underline != FACE_UNDERLINE_SINGLE`
/// (src/term.c:2076-2085), so exactly four values can arrive.  Expanding all
/// four when the terminal is resolved is therefore the same answer GNU
/// computes lazily, and it keeps the terminfo expander in the one crate that
/// links ncurses.
///
/// There is no field-wise constructor: [`TtyStyledUnderline::expand_all`] is
/// the only way to build one, so a half-filled set cannot exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtyStyledUnderline {
    double_line: Vec<u8>,
    wave: Vec<u8>,
    dots: Vec<u8>,
    dashes: Vec<u8>,
}

impl TtyStyledUnderline {
    /// Expand `Smulx` for every style that can reach it, through `expand` —
    /// which is GNU's `tparam` at the call site that has one.  `None` when any
    /// expansion fails, because a terminal that can spell three of the four is
    /// not a terminal GNU would emit the fourth on.
    pub fn expand_all(mut expand: impl FnMut(u8) -> Option<Vec<u8>>) -> Option<Self> {
        Some(Self {
            double_line: expand(UnderlineStyle::Double.gnu_code())?,
            wave: expand(UnderlineStyle::Wave.gnu_code())?,
            dots: expand(UnderlineStyle::Dotted.gnu_code())?,
            dashes: expand(UnderlineStyle::Dashed.gnu_code())?,
        })
    }

    /// The sequence for `style`, or `None` for the two styles that never reach
    /// `Smulx`: `None` emits nothing and `Line` takes the `smul` arm above.
    pub fn sequence(&self, style: UnderlineStyle) -> Option<&[u8]> {
        match style {
            UnderlineStyle::None | UnderlineStyle::Line => None,
            UnderlineStyle::Double => Some(&self.double_line),
            UnderlineStyle::Wave => Some(&self.wave),
            UnderlineStyle::Dotted => Some(&self.dots),
            UnderlineStyle::Dashed => Some(&self.dashes),
        }
    }
}

/// How a `:slant italic` face is rendered on this terminal, and with which
/// bytes.
///
/// GNU `turn_on_face` (src/term.c:2063-2072): the whole arm is gated on
/// `MAY_USE_WITH_COLORS_P (tty, NC_ITALIC)`, and INSIDE it the choice is the
/// pointer — `sitm` when the terminal has it, otherwise `dim`, "Italics mode is
/// unavailable on many terminals.  In that case, map slant to dimmed text; we
/// want italic text to appear different and dimming is not otherwise used."
/// The fallback is emitted with `OUTPUT1`, not `OUTPUT1_IF`, and no second
/// `MAY_USE_WITH_COLORS_P` — so a terminal whose `ncv` forbids DIM on a colour
/// frame still gets the dim fallback for an italic face.  78 of the entries
/// ncurses ships are in exactly that state (ledger 186).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtyItalicRendition<'a> {
    Italic(&'a [u8]),
    Dim(&'a [u8]),
    None,
}

/// GNU's `tparam` (src/terminfo.c:43-55), supplied by the crate that links the
/// terminfo library.
///
/// Nothing in this crate can expand a terminfo format string: the expander IS
/// ncurses' `tparm`, and only `neomacs-bin` links it.  A parameterized
/// capability string is inert without one, which is why the two travel
/// together and [`TtyColorCapabilities`] has no constructor that takes a
/// string alone.  Re-implementing terminfo's stack language on the display
/// side is the mistake ledger 186 declined to make for `Smulx`; a function
/// pointer is what keeps it declined for `setaf` too, where the parameter
/// domain is 16.7 million values wide and cannot be pre-expanded the way
/// [`TtyStyledUnderline`]'s four can.
#[derive(Clone, Copy)]
pub struct TerminfoExpander(fn(&[u8], TerminfoParameters) -> Option<Vec<u8>>);

/// Two capability records describe the same terminal when their capability
/// STRINGS agree; which pointer to `tparam` they hold is not part of that.
///
/// Deriving the comparison instead would compare function addresses, which
/// rustc warns are not unique across codegen units and may be merged -- an
/// answer that is neither true nor false, in a type whose whole purpose is to
/// stop a string and its expander from being separated.
impl PartialEq for TerminfoExpander {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for TerminfoExpander {}

impl TerminfoExpander {
    #[must_use]
    pub const fn new(expand: fn(&[u8], TerminfoParameters) -> Option<Vec<u8>>) -> Self {
        Self(expand)
    }

    #[must_use]
    pub fn expand(self, sequence: &[u8], parameters: TerminfoParameters) -> Option<Vec<u8>> {
        (self.0)(sequence, parameters)
    }
}

impl std::fmt::Debug for TerminfoExpander {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TerminfoExpander(..)")
    }
}

/// The two shapes of `tparam` call `turn_on_face` makes for a colour
/// (src/term.c:2098-2117):
///
/// ```c
///   if (tty->TF_rgb_separate)
///     p = tparam (ts, NULL, 0, fg >> 16, (fg >> 8) & 0xFF, fg & 0xFF, 0);
///   else
///     p = tparam (ts, NULL, 0, fg, 0, 0, 0);
/// ```
///
/// Which one is used is not a choice a caller makes -- it is
/// `tty->TF_rgb_separate` -- so the variant is built inside
/// [`TtyColorCapabilities::ground_sequence`] and never passed in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminfoParameters {
    /// `tparam (ts, NULL, 0, VALUE, 0, 0, 0)`.
    One(u32),
    /// GNU's `TF_rgb_separate` branch: the realized slot split into channels.
    Rgb { r: u8, g: u8, b: u8 },
}

/// Which of GNU's two colour capabilities writes a colour.
///
/// GNU picks the field with `tty->standout_mode ? TS_set_background :
/// TS_set_foreground` (src/term.c:2098, :2109), i.e. reverse video is
/// implemented by swapping the two capabilities rather than by an SGR
/// parameter.  This port emits `so` for an inverse face instead, so the swap
/// has no counterpart here and the ground is exactly the face's own.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorGround {
    /// GNU `TS_set_foreground`, terminfo `setaf` (termcap `AF`), or `setf`.
    Foreground,
    /// GNU `TS_set_background`, terminfo `setab` (termcap `AB`), or `setb`.
    Background,
}

/// Which of GNU's five colour resolutions an entry took, and therefore what
/// `TN_max_colors` is (src/term.c:4616-4667).
///
/// The five are one `else if` chain, and each arm decides the COUNT and the
/// SETTER together -- `setf24`/`setrgbf` replace the setters and set
/// `TN_max_colors = 16777216`, `RGB` replaces nothing but the count, and
/// `Tc`/`COLORTERM` installs GNU's own literal and sets both.  Carrying the
/// arm rather than a bare number is what makes "resolved a `setrgbf` and then
/// answered `Co`" -- ledger 188's `xterm-kitty` row -- unspellable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtyColorDepth {
    /// `tty->TN_max_colors = tgetnum ("Co")` and no arm replaced it
    /// (src/term.c:4616).  GNU's `-1` for an absent `Co` is spelled `0` here,
    /// because `TN_max_colors > 0` is the only question anything asks of it.
    Indexed(u32),
    /// One of GNU's four 24-bit arms: `TN_max_colors = 16777216`.
    Direct(TtyDirectColorRoute),
}

impl TtyColorDepth {
    /// GNU `TN_max_colors`.
    #[must_use]
    pub fn max_colors(self) -> i64 {
        match self {
            Self::Indexed(cells) => i64::from(cells),
            Self::Direct(_) => 16_777_216,
        }
    }
}

/// Which of GNU's four 24-bit arms an entry took, in GNU's own order.
///
/// Recorded rather than collapsed to a bool because the arms differ in what
/// else they do, and because "which route did this terminal take" is the
/// question ledger 188's finding is about.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtyDirectColorRoute {
    /// GNU's own non-standard `setf24`/`setb24` (src/term.c:4625-4632): the
    /// setters become the entry's own strings.
    Setf24,
    /// The other non-standard pair, `setrgbf`/`setrgbb` (:4634-4643): the
    /// setters become the entry's own strings AND `TF_rgb_separate` is set.
    /// This is `xterm-kitty`'s route with COLORTERM unset.
    Setrgbf,
    /// The standard `RGB` boolean (:4645-4653).  The setters keep the entry's
    /// own spelling and receive the packed pixel -- the `*-direct` entries.
    RgbFlag,
    /// `Tc` (tmux's de-facto flag) or `COLORTERM` spelled exactly `truecolor`
    /// (:4655-4667).  GNU installs its own literal here rather than the
    /// entry's, and sets `TF_rgb_separate`.
    TcOrColorterm,
}

/// GNU's colour half of `struct tty_display_info`: `TS_set_foreground`,
/// `TS_set_background`, `TS_orig_pair`, `TF_rgb_separate` and
/// `TN_max_colors`.
///
/// `init_tty` reads all four inside ONE gate -- `tty->TS_orig_pair = tgetstr
/// ("op"); if (tty->TS_orig_pair) { ... }` (src/term.c:4604-4674), with the
/// comment "If `op' isn't available, don't support color because we can't
/// switch back to the default foreground and background."  So a terminal
/// either has the whole set or has no colour at all, and that is why this is
/// one `Option<TtyColorCapabilities>` on the record rather than four
/// independently-absent fields.
///
/// There is no field-wise constructor: [`TtyColorCapabilities::new`] takes the
/// expander with the strings, so a string that cannot be expanded cannot exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtyColorCapabilities {
    set_foreground: Option<Vec<u8>>,
    set_background: Option<Vec<u8>>,
    orig_pair: Vec<u8>,
    rgb_separate: bool,
    depth: TtyColorDepth,
    expand: TerminfoExpander,
}

impl TtyColorCapabilities {
    /// GNU's `init_tty` colour block, already resolved: `op`, the two setters,
    /// `TF_rgb_separate` and `TN_max_colors`.
    #[must_use]
    /// `op` is not optional and the setters are, which is GNU's own shape:
    /// `TS_orig_pair` gates the whole block, while `TS_set_foreground` and
    /// `TS_set_background` are each tested again at the emission site
    /// (`if (face_tty_specified_color (fg) && ts)`, src/term.c:2099).  Three
    /// reachable entries have `op` and neither setter -- `foot+base`,
    /// `kitty+common`, `linux-m` -- and GNU still emits `op` for them.
    ///
    /// The `depth` travels with the setters for the same reason the expander
    /// does: GNU decides both in one `else if` chain, so a record that
    /// resolved a `setrgbf` and answers `Co` is not a record this constructor
    /// can build (ledger 193).
    pub fn new(
        orig_pair: Vec<u8>,
        set_foreground: Option<Vec<u8>>,
        set_background: Option<Vec<u8>>,
        rgb_separate: bool,
        depth: TtyColorDepth,
        expand: TerminfoExpander,
    ) -> Self {
        Self {
            set_foreground,
            set_background,
            orig_pair,
            rgb_separate,
            depth,
            expand,
        }
    }

    /// Which arm of GNU's chain this entry took, and therefore
    /// `TN_max_colors`.
    #[must_use]
    pub fn depth(&self) -> TtyColorDepth {
        self.depth
    }

    /// GNU `TS_orig_pair` (`op`), which `turn_off_face` emits to put the
    /// colours back (src/term.c:2159-2165).
    #[must_use]
    pub fn orig_pair(&self) -> &[u8] {
        &self.orig_pair
    }

    /// `turn_on_face`'s colour emission for one ground: the entry's own
    /// `setaf`/`setab` run through GNU's `tparam` (src/term.c:2096-2117).
    ///
    /// `None` when the expansion fails, which for ncurses means the string is
    /// not a well-formed terminfo format -- GNU's `OUTPUT (tty, p)` would then
    /// have been handed a null pointer, so emitting nothing is its behaviour
    /// too.
    #[must_use]
    pub fn ground_sequence(&self, ground: ColorGround, color: TerminalColor) -> Option<Vec<u8>> {
        let sequence = match ground {
            ColorGround::Foreground => self.set_foreground.as_deref()?,
            ColorGround::Background => self.set_background.as_deref()?,
        };
        let parameters = match (self.rgb_separate, color) {
            (true, TerminalColor::Direct { r, g, b }) => TerminfoParameters::Rgb { r, g, b },
            // GNU splits the realized slot unconditionally under
            // `TF_rgb_separate`; an indexed colour cannot reach a terminal that
            // has it, because `TF_rgb_separate` is only ever set alongside
            // `TN_max_colors = 16777216` (src/term.c:4636-4667) and Lisp then
            // answers packed pixels.  Splitting it anyway is what GNU's code
            // does with the value it has.
            (true, TerminalColor::Indexed(index)) => TerminfoParameters::Rgb {
                r: (index >> 8) as u8,
                g: index as u8,
                b: 0,
            },
            (false, color) => TerminfoParameters::One(color.realized_pixel()),
        };
        self.expand.expand(sequence, parameters)
    }
}

/// Where the writer's colour bytes come from.
///
/// GNU has only the first two states: a terminal has the colour block or it
/// does not, and a terminal with no terminfo entry does not exist, because
/// `init_tty` reaches `maybe_fatal` before anything can render
/// (src/term.c:4880-4890).  This port keeps running there, so it has a third --
/// and the three must be a type rather than an `Option`, because
/// [`Absent`](Self::Absent) and [`NoDatabase`](Self::NoDatabase) demand
/// OPPOSITE behaviour from the writer and an `Option` spells them the same.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TtyColorSource {
    /// The entry's own `setaf`/`setab`/`op`, read behind GNU's `op` gate.
    Entry(TtyColorCapabilities),
    /// GNU's "if `op' isn't available, don't support color because we can't
    /// switch back to the default foreground and background"
    /// (src/term.c:4600-4604), which leaves `TN_max_colors` at zero.  The
    /// writer emits no colour at all -- three of the 927 reachable terminfo
    /// entries are in this state (`amiga-vnc`, `djgpp204`, `vwmterm`).
    Absent,
    /// No terminfo entry could be read, so there is no `setaf` to spell with.
    /// The writer falls back to a fixed ANSI rule, which is neomacs' own
    /// choice and not a port of anything: a missing terminfo database should
    /// not silently strip highlighting.
    ///
    /// It carries its own `max_colors` because there is no entry to take one
    /// from, and because this is the ONLY state in which the count is not
    /// GNU's answer -- GNU exits with "terminal type not defined" here
    /// (src/term.c:4880-4890).  Keeping the number inside the variant is what
    /// stops it from becoming a second, independently-settable answer beside
    /// the resolved one (ledger 193).
    NoDatabase { max_colors: i64 },
}

impl TtyColorSource {
    /// The entry's colour capabilities, or `None` for either colourless state.
    #[must_use]
    pub fn entry(&self) -> Option<&TtyColorCapabilities> {
        match self {
            Self::Entry(colors) => Some(colors),
            Self::Absent | Self::NoDatabase { .. } => None,
        }
    }

    /// GNU `TN_max_colors`, which is a property of WHICH of these three states
    /// the terminal is in and of nothing else.
    ///
    /// [`Absent`](Self::Absent) is zero because GNU never executes
    /// `tty->TN_max_colors = tgetnum ("Co")` for a terminal with no `op` --
    /// the assignment is inside the gate (src/term.c:4604-4616), so the field
    /// keeps the zero `create_tty_output` left it with.
    #[must_use]
    pub fn max_colors(&self) -> i64 {
        match self {
            Self::Entry(colors) => colors.depth().max_colors(),
            Self::Absent => 0,
            Self::NoDatabase { max_colors } => *max_colors,
        }
    }

    /// Whether the writer may spell a colour with its own fixed ANSI rule --
    /// true ONLY for [`NoDatabase`](Self::NoDatabase).  This is the whole
    /// reason the type exists: an entry GNU renders monochrome for want of
    /// `op` must not be painted by a fallback.
    #[must_use]
    pub fn allows_ansi_fallback(&self) -> bool {
        matches!(self, Self::NoDatabase { .. })
    }
}

/// What `turn_off_face` emits for the face it is turning off (src/term.c:2136-2157).
///
/// GNU's structure is an `if (tty->TS_exit_attribute_mode) ... else ...`, not
/// two independent emissions: a terminal that has `me` never emits `ue`, and a
/// terminal without `me` can only undo the one appearance that has its own
/// exit sequence.  Naming the branches makes that exclusivity a compile-time
/// fact rather than a comment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtyAttributeExit<'a> {
    /// `OUTPUT1_IF (tty, tty->TS_exit_attribute_mode)` — the `me` branch, taken
    /// when the face had any of bold, italic, reverse, underline or
    /// strike-through.
    ExitAttributeMode(&'a [u8]),
    /// GNU's else branch: `if (face->underline) OUTPUT_IF (tty,
    /// tty->TS_exit_underline_mode)`.
    ExitUnderlineMode(&'a [u8]),
    /// The face had nothing on, or the terminal has no exit string for what it
    /// did have.
    Nothing,
}

/// What `turn_off_face` asks about the face it is turning off.
///
/// The three questions are GNU's own disjunctions, kept apart because GNU
/// answers them with different strings: the first chooses `me`, the second is
/// the only thing the no-`me` branch can undo, and the third chooses `op`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TtyFaceAppearance {
    /// `face->tty_bold_p || face->tty_italic_p || face->tty_reverse_p
    /// || face->underline || face->tty_strike_through_p` (src/term.c:2140-2144).
    pub any_appearance: bool,
    /// `face->underline` (src/term.c:2155).
    pub underline: bool,
    /// GNU's colour disjunct (src/term.c:2160-2164): a foreground or a
    /// background that is not the terminal's default.
    pub non_default_color: bool,
}

/// The capabilities of one terminal.
///
/// Fields mirror the terminfo capabilities GNU reads in `init_tty`: `so`, `us`,
/// `Smulx`, `md`, `mh`, `ZH`, `smxx`, `me`, `ue`, `op`, `AF`, `AB`, `Co` and
/// `NC`.  Each string capability is carried as its own bytes, terminfo padding
/// removed, because that is what GNU emits and because presence is not
/// separable from spelling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtyAttributeCapabilities {
    /// `so` — GNU `TS_standout_mode`.
    pub standout_sequence: Option<Vec<u8>>,
    /// `us` — GNU `TS_enter_underline_mode`.
    pub underline_sequence: Option<Vec<u8>>,
    /// `md` — GNU `TS_enter_bold_mode`.
    pub bold_sequence: Option<Vec<u8>>,
    /// `mh` — GNU `TS_enter_dim_mode`.
    pub dim_sequence: Option<Vec<u8>>,
    /// `ZH` (`sitm`) — GNU `TS_enter_italic_mode`.
    pub italic_sequence: Option<Vec<u8>>,
    /// `smxx` — GNU `TS_enter_strike_through_mode`.
    pub strike_through_sequence: Option<Vec<u8>>,
    /// `Smulx` (or GNU's `Su` fallback literal) — GNU
    /// `TF_set_underline_style`, expanded.
    pub styled_underline: Option<TtyStyledUnderline>,
    /// `me` — GNU `TS_exit_attribute_mode` (src/term.c:4585), the string
    /// `turn_off_face` emits to take every appearance back off.
    pub exit_attribute_mode: Option<Vec<u8>>,
    /// `ue` — GNU `TS_exit_underline_mode` (src/term.c:4578), the ONLY thing
    /// `turn_off_face`'s no-`me` branch can undo.
    pub exit_underline_mode: Option<Vec<u8>>,
    /// `op` + `AF`/`AB` + `TF_rgb_separate` — GNU's colour block, which is one
    /// answer because `init_tty` reads all of it behind the `op` gate, plus the
    /// third state GNU cannot be in.  See [`TtyColorSource`].
    pub colors: TtyColorSource,
    /// `NC` — GNU `TN_no_color_video`.
    pub no_color_video: TtyNoColorVideo,
}

impl TtyAttributeCapabilities {
    /// Every attribute available, no `ncv` restrictions, 24-bit color.
    ///
    /// This is the assumption neomacs shipped with before capabilities existed,
    /// so it stays the default for a terminal whose terminfo entry cannot be
    /// read: a missing entry should not silently strip highlighting.  The
    /// spellings are the xterm-family ones, and the styled underline is the
    /// literal GNU installs itself when a terminal claims `Su` without `Smulx`
    /// (src/term.c:4703).
    ///
    /// `colors` is `None` here, and that is not "no colour": it is "no entry to
    /// take a `setaf` from".  GNU has no such state -- it exits with "terminal
    /// type not defined" (src/term.c:4883) rather than run without a database --
    /// so the writer's fallback for it is neomacs' own and is documented at the
    /// emission site.
    pub fn full() -> Self {
        Self::full_with_color_cells(16_777_216)
    }

    /// [`Self::full`] with the colour count this port assumes for a terminal
    /// whose terminfo entry could not be read.
    ///
    /// The count is a parameter of THIS constructor and of no other, which is
    /// the whole shape of ledger 193's fix: `TN_max_colors` is decided by the
    /// resolution for every terminal GNU can start on, and is a free choice
    /// only in the state GNU cannot be in.
    pub fn full_with_color_cells(max_colors: i64) -> Self {
        Self {
            standout_sequence: Some(b"\x1b[7m".to_vec()),
            underline_sequence: Some(b"\x1b[4m".to_vec()),
            bold_sequence: Some(b"\x1b[1m".to_vec()),
            dim_sequence: Some(b"\x1b[2m".to_vec()),
            italic_sequence: Some(b"\x1b[3m".to_vec()),
            strike_through_sequence: Some(b"\x1b[9m".to_vec()),
            styled_underline: TtyStyledUnderline::expand_all(|style| {
                Some(format!("\x1b[4:{style}m").into_bytes())
            }),
            exit_attribute_mode: Some(b"\x1b[0m".to_vec()),
            exit_underline_mode: Some(b"\x1b[24m".to_vec()),
            colors: TtyColorSource::NoDatabase { max_colors },
            no_color_video: TtyNoColorVideo::NONE,
        }
    }

    /// A terminal that can render no attributes at all (a `dumb`-style entry).
    pub fn none() -> Self {
        Self {
            standout_sequence: None,
            underline_sequence: None,
            bold_sequence: None,
            dim_sequence: None,
            italic_sequence: None,
            strike_through_sequence: None,
            styled_underline: None,
            exit_attribute_mode: None,
            exit_underline_mode: None,
            colors: TtyColorSource::Absent,
            no_color_video: TtyNoColorVideo::NONE,
        }
    }

    /// GNU `turn_off_face`'s appearance half (src/term.c:2136-2157).
    ///
    /// The `me` branch is taken whenever the terminal HAS `me`, and inside it
    /// the emission is conditional on the face having had an appearance --
    /// which is why a face carrying only a colour turns off with `op` alone,
    /// measured on a pty against GNU 31.0.90 on TERM=linux (ledger 188):
    ///
    /// ```text
    ///   ESC[31m PW188RED     ESC[39;49m            <- colour only, no `me`
    ///   ESC[1m ESC[31m PW188BOLDRED ESC[m ^O ESC[39;49m   <- bold, so `me`
    /// ```
    pub fn attribute_exit(&self, appearance: TtyFaceAppearance) -> TtyAttributeExit<'_> {
        match self.exit_attribute_mode.as_deref() {
            Some(exit) if appearance.any_appearance => TtyAttributeExit::ExitAttributeMode(exit),
            Some(_) => TtyAttributeExit::Nothing,
            None => match self.exit_underline_mode.as_deref() {
                Some(exit) if appearance.underline => TtyAttributeExit::ExitUnderlineMode(exit),
                _ => TtyAttributeExit::Nothing,
            },
        }
    }

    /// GNU `turn_off_face`'s colour half: `OUTPUT1_IF (tty, tty->TS_orig_pair)`
    /// under `TN_max_colors > 0` and a non-default foreground or background
    /// (src/term.c:2159-2165).
    pub fn orig_pair(&self, appearance: TtyFaceAppearance) -> Option<&[u8]> {
        if !self.supports_color() || !appearance.non_default_color {
            return None;
        }
        self.colors.entry().map(TtyColorCapabilities::orig_pair)
    }

    /// GNU's presence question — `if (tty->TS_enter_bold_mode)` and its six
    /// neighbours.  Exhaustive on purpose: a capability added to
    /// [`TtyCapability`] without a field to answer from is a compile error.
    fn has_capability_string(&self, capability: TtyCapability) -> bool {
        match capability {
            TtyCapability::Inverse => self.standout_sequence.is_some(),
            TtyCapability::Underline => self.underline_sequence.is_some(),
            TtyCapability::UnderlineStyled => self.styled_underline.is_some(),
            TtyCapability::Bold => self.bold_sequence.is_some(),
            TtyCapability::Dim => self.dim_sequence.is_some(),
            TtyCapability::Italic => self.italic_sequence.is_some(),
            TtyCapability::StrikeThrough => self.strike_through_sequence.is_some(),
        }
    }

    /// GNU `tty_capable_p`: the capability's terminfo string must exist, and on a
    /// terminal that has colors its `ncv` bit must be clear
    /// (`MAY_USE_WITH_COLORS_P`). A monochrome terminal ignores `ncv` entirely.
    pub fn supports(&self, capability: TtyCapability) -> bool {
        self.has_capability_string(capability)
            && self.may_use_with_colors(capability.no_color_video_bit())
    }

    /// GNU `MAY_USE_WITH_COLORS_P` (term.c).
    fn may_use_with_colors(&self, bit: TtyNoColorVideo) -> bool {
        !self.supports_color() || !self.no_color_video.contains(bit)
    }

    /// GNU `TN_max_colors` (src/termchar.h:157), the number
    /// `Ftty_display_color_cells` returns and therefore the number
    /// `tty-color-alist` and every `((class color) (min-colors N) ...)` face
    /// spec are decided by.
    ///
    /// GNU computes it ONCE, inside `init_tty`'s `op` gate, and the same
    /// `else if` chain that picks `TS_set_foreground` picks it
    /// (src/term.c:4602-4674) -- so it is a question about
    /// [`Self::colors`] and not a field of its own.
    #[must_use]
    pub fn color_cells(&self) -> i64 {
        self.colors.max_colors()
    }

    /// Whether the terminal has colors at all — GNU `TN_max_colors > 0`.
    pub fn supports_color(&self) -> bool {
        self.color_cells() > 0
    }

    // `turn_on_face` names each field literally rather than through a lookup,
    // and so does this: one accessor per GNU field, each carrying that field's
    // own `MAY_USE_WITH_COLORS_P` term, so an emission site cannot pick up the
    // bytes of a capability whose guard it did not check.

    /// GNU `TS_standout_mode` under `MAY_USE_WITH_COLORS_P (tty, NC_REVERSE)`.
    pub fn standout(&self) -> Option<&[u8]> {
        self.supports(TtyCapability::Inverse)
            .then(|| self.standout_sequence.as_deref())
            .flatten()
    }

    /// GNU `TS_enter_underline_mode` under `NC_UNDERLINE`.
    pub fn underline(&self) -> Option<&[u8]> {
        self.supports(TtyCapability::Underline)
            .then(|| self.underline_sequence.as_deref())
            .flatten()
    }

    /// GNU `TS_enter_bold_mode` under `NC_BOLD`.
    pub fn bold(&self) -> Option<&[u8]> {
        self.supports(TtyCapability::Bold)
            .then(|| self.bold_sequence.as_deref())
            .flatten()
    }

    /// GNU `TS_enter_strike_through_mode` under `NC_STRIKE_THROUGH`.
    pub fn strike_through(&self) -> Option<&[u8]> {
        self.supports(TtyCapability::StrikeThrough)
            .then(|| self.strike_through_sequence.as_deref())
            .flatten()
    }

    /// GNU `turn_on_face`'s slant decision, resolved once.  See
    /// [`TtyItalicRendition`] for why the dim fallback carries no `ncv` term.
    pub fn italic_rendition(&self) -> TtyItalicRendition<'_> {
        if !self.may_use_with_colors(TtyNoColorVideo::ITALIC) {
            return TtyItalicRendition::None;
        }
        match (
            self.italic_sequence.as_deref(),
            self.dim_sequence.as_deref(),
        ) {
            (Some(italic), _) => TtyItalicRendition::Italic(italic),
            (None, Some(dim)) => TtyItalicRendition::Dim(dim),
            (None, None) => TtyItalicRendition::None,
        }
    }

    /// The styled-underline sequence for `style`, or `None` when GNU takes the
    /// plain `smul` arm instead: no `Smulx`, a `Line` style, or an `ncv` that
    /// forbids underline on this colour terminal.
    pub fn styled_underline_sequence(&self, style: UnderlineStyle) -> Option<&[u8]> {
        self.supports(TtyCapability::UnderlineStyled)
            .then(|| {
                self.styled_underline
                    .as_ref()
                    .and_then(|styled| styled.sequence(style))
            })
            .flatten()
    }
}

impl Default for TtyAttributeCapabilities {
    fn default() -> Self {
        Self::full()
    }
}
