//! Color composition keeps source attributes separate from realized paint.
//!
//! GNU merges lface attributes before load_face_colors/realize_tty_face.
//! In particular, neither distant-color substitution nor a terminal default
//! crossing channels can be inverted to recover those attributes.

use super::{NeoColor, NeoFace, ResolvedFace, TerminalColor, color_to_pixel, colors_close};
use neovm_core::window::FrameId;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum FaceVideo {
    #[default]
    Normal,
    Inverse,
}

/// Which GNU realization rule applies to a composed face.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaceRealization {
    /// Window-system frames: `load_face_colors` swaps unconditionally.
    Gui,
    /// Terminal frames: `realize_tty_face` plus `turn_on_face`.
    Terminal,
}

/// Seed faces are manually built frame fallbacks. Once realized, every merge
/// uses the retained attributes, never the public paint fields.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum FaceColorState {
    Seed,
    Realized(FaceColorAttributes),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FaceColorAttributes {
    foreground: TerminalFaceColor,
    background: TerminalFaceColor,
    distant_foreground: Option<TerminalFaceColor>,
    video: FaceVideo,
}

impl FaceColorAttributes {
    pub(super) fn from_base(base: &ResolvedFace) -> Self {
        match &base.color_state {
            FaceColorState::Realized(attributes) => *attributes,
            FaceColorState::Seed => Self {
                foreground: TerminalFaceColor::from_resolved_slot(
                    base.fg,
                    base.terminal_fg,
                    base.use_default_foreground,
                    FaceColorSlot::Foreground,
                ),
                background: TerminalFaceColor::from_resolved_slot(
                    base.bg,
                    base.terminal_bg,
                    base.use_default_background,
                    FaceColorSlot::Background,
                ),
                distant_foreground: None,
                video: FaceVideo::Normal,
            },
        }
    }

    pub(super) fn merge(mut self, face: &NeoFace) -> Self {
        if let Some(color) = &face.foreground {
            self.foreground = TerminalFaceColor::from_color(color);
        }
        if let Some(color) = &face.background {
            self.background = TerminalFaceColor::from_color(color);
        }
        if let Some(color) = &face.distant_foreground {
            self.distant_foreground = Some(TerminalFaceColor::from_color(color));
        }
        if let Some(inverse) = face.inverse_video {
            self.video = if inverse {
                FaceVideo::Inverse
            } else {
                FaceVideo::Normal
            };
        }
        self
    }

    /// Only the terminal callback boundary may default a failed explicit
    /// source channel. Pure composition and GUI rendering retain their own
    /// ordinary inheritance policy.
    pub(super) fn merge_terminal_realization(self, face: &NeoFace) -> Self {
        let mut attributes = self.merge(face);
        if face.foreground.is_none() && face.color_specifications.foreground.is_some() {
            attributes.foreground = TerminalFaceColor::TerminalDefault {
                slot: match face.color_specifications.foreground.as_deref() {
                    Some(neovm_core::face::SpecifiedColor::FrameBackground) => {
                        FaceColorSlot::Background
                    }
                    _ => FaceColorSlot::Foreground,
                },
                fallback_pixel: match face.color_specifications.foreground.as_deref() {
                    Some(neovm_core::face::SpecifiedColor::FrameBackground) => {
                        attributes.background.pixel()
                    }
                    _ => attributes.foreground.pixel(),
                },
            };
        }
        if face.background.is_none() && face.color_specifications.background.is_some() {
            attributes.background = TerminalFaceColor::TerminalDefault {
                slot: match face.color_specifications.background.as_deref() {
                    Some(neovm_core::face::SpecifiedColor::FrameForeground) => {
                        FaceColorSlot::Foreground
                    }
                    _ => FaceColorSlot::Background,
                },
                fallback_pixel: match face.color_specifications.background.as_deref() {
                    Some(neovm_core::face::SpecifiedColor::FrameForeground) => {
                        attributes.foreground.pixel()
                    }
                    _ => attributes.background.pixel(),
                },
            };
        }
        attributes
    }

    pub(super) fn realize(self) -> RealizedFaceColors {
        self.realize_for(FaceRealization::Gui)
    }

    /// GNU's terminal realization (`realize_tty_face` plus `turn_on_face`).
    ///
    /// `realize_tty_face` swaps the two mapped colors for every inverse-video
    /// face (src/xfaces.c:6800-6810), then `turn_on_face` toggles the
    /// terminal's standout mode whenever either swapped slot is a default
    /// marker -- `fg == FACE_TTY_DEFAULT_BG_COLOR || bg ==
    /// FACE_TTY_DEFAULT_FG_COLOR` (src/term.c:2049-2058) -- and, while
    /// standout is active, writes each color through the OPPOSITE SGR channel
    /// (src/term.c:2095-2115).  The swap and the channel crossing cancel: a
    /// face with at least one defaulted channel paints its OWN colors plus
    /// standalone standout.  Only a face whose two colors are both concrete
    /// keeps the swap and skips standout.  Emit that net pair directly: keep
    /// the slots and let the INVERSE attribute produce the standout sequence.
    /// A window-system face is realized by `load_face_colors`, which swaps
    /// unconditionally, so it always takes the plain swap below.
    pub(super) fn realize_terminal(self) -> RealizedFaceColors {
        self.realize_for(FaceRealization::Terminal)
    }

    fn realize_for(self, kind: FaceRealization) -> RealizedFaceColors {
        let (mut foreground, mut background, terminal_inverse) = match self.video {
            FaceVideo::Normal => (self.foreground, self.background, false),
            FaceVideo::Inverse
                if kind == FaceRealization::Terminal
                    && (self.foreground.is_terminal_default()
                        || self.background.is_terminal_default()) =>
            {
                (self.foreground, self.background, true)
            }
            FaceVideo::Inverse => (self.background, self.foreground, false),
        };
        if let Some(distant) = self.distant_foreground
            && colors_close(foreground.pixel(), background.pixel())
        {
            match self.video {
                FaceVideo::Normal => foreground = distant,
                FaceVideo::Inverse => background = distant,
            }
        }
        RealizedFaceColors {
            attributes: self,
            foreground,
            background,
            terminal_inverse,
        }
    }
}

/// Only realization constructs this value; only installation publishes paint.
/// A realized palette deliberately has no merge operation.
pub(super) struct RealizedFaceColors {
    attributes: FaceColorAttributes,
    foreground: TerminalFaceColor,
    background: TerminalFaceColor,
    terminal_inverse: bool,
}

impl RealizedFaceColors {
    pub(super) fn install(self, face: &mut ResolvedFace) {
        (face.fg, face.terminal_fg, face.use_default_foreground) =
            self.foreground.materialize_in(FaceColorSlot::Foreground);
        (face.bg, face.terminal_bg, face.use_default_background) =
            self.background.materialize_in(FaceColorSlot::Background);
        face.terminal_inverse_video = self.terminal_inverse;
        face.color_state = FaceColorState::Realized(self.attributes);
    }
}

/// The terminal channel a default-color sentinel belongs to.
///
/// ANSI has distinct "default foreground" and "default background" values;
/// a boolean attached to the destination slot cannot represent one after
/// inverse-video moves it to the other slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaceColorSlot {
    Foreground,
    Background,
}

/// A face color before it is assigned to its post-inverse destination slot.
///
/// It carries the realized terminal index next to the pixel because inverse
/// video MOVES a colour between slots: GNU `realize_tty_face` maps both source
/// colours through `map_tty_color` and then swaps the results
/// (src/xfaces.c:6800-6810), so whatever the writer emits for the foreground
/// must be exactly what was realized for the background.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalFaceColor {
    Concrete {
        pixel: u32,
        terminal: Option<TerminalColor>,
    },
    TerminalDefault {
        slot: FaceColorSlot,
        fallback_pixel: u32,
    },
}

impl TerminalFaceColor {
    fn from_resolved_slot(
        pixel: u32,
        terminal: Option<TerminalColor>,
        defaulted: bool,
        slot: FaceColorSlot,
    ) -> Self {
        if defaulted {
            Self::TerminalDefault {
                slot,
                fallback_pixel: pixel,
            }
        } else {
            Self::Concrete { pixel, terminal }
        }
    }

    fn materialize_in(self, destination: FaceColorSlot) -> (u32, Option<TerminalColor>, bool) {
        match self {
            Self::Concrete { pixel, terminal } => (pixel, terminal, false),
            Self::TerminalDefault {
                slot,
                fallback_pixel,
            } if slot == destination => (fallback_pixel, None, true),
            // ANSI cannot select the terminal's default background as a
            // foreground (or vice versa). GNU realizes the frame color first
            // and swaps that concrete color, so use the carried fallback.
            //
            // That fallback is a frame pixel, not a `tty-color-desc` answer, so
            // it carries no terminal colour: GNU's `FACE_TTY_DEFAULT_COLOR`,
            // which `turn_on_face` emits nothing for.
            Self::TerminalDefault { fallback_pixel, .. } => (fallback_pixel, None, false),
        }
    }

    fn is_terminal_default(self) -> bool {
        matches!(self, Self::TerminalDefault { .. })
    }
}

impl TerminalFaceColor {
    fn from_color(color: &NeoColor) -> Self {
        Self::Concrete {
            pixel: color_to_pixel(color),
            terminal: color.terminal,
        }
    }

    fn pixel(self) -> u32 {
        match self {
            Self::Concrete { pixel, .. } => pixel,
            Self::TerminalDefault { fallback_pixel, .. } => fallback_pixel,
        }
    }
}

/// One realized terminal paint, snapshotted after installation.
///
/// GNU's frame face cache returns the SAME realized face for a repeated
/// merged-lface lookup: only the first `lookup_face` runs
/// `load_face_colors`/`realize_tty_face` (src/xfaces.c:5064-5093). This is
/// the paint half of that reuse — installing a snapshot reproduces the exact
/// field writes `install_realized_tty_colors` performed.
#[derive(Clone, Copy, Debug)]
pub(super) struct TtyPaint {
    fg: u32,
    terminal_fg: Option<TerminalColor>,
    use_default_foreground: bool,
    bg: u32,
    terminal_bg: Option<TerminalColor>,
    use_default_background: bool,
    terminal_inverse_video: bool,
    terminal_underline_color: Option<TerminalColor>,
    underline_color: u32,
    color_state: FaceColorState,
}

impl TtyPaint {
    pub(super) fn snapshot(face: &ResolvedFace) -> Self {
        Self {
            fg: face.fg,
            terminal_fg: face.terminal_fg,
            use_default_foreground: face.use_default_foreground,
            bg: face.bg,
            terminal_bg: face.terminal_bg,
            use_default_background: face.use_default_background,
            terminal_inverse_video: face.terminal_inverse_video,
            terminal_underline_color: face.terminal_underline_color,
            underline_color: face.underline_color,
            color_state: face.color_state.clone(),
        }
    }

    pub(super) fn install(self, face: &mut ResolvedFace) {
        face.fg = self.fg;
        face.terminal_fg = self.terminal_fg;
        face.use_default_foreground = self.use_default_foreground;
        face.bg = self.bg;
        face.terminal_bg = self.terminal_bg;
        face.use_default_background = self.use_default_background;
        face.terminal_inverse_video = self.terminal_inverse_video;
        face.terminal_underline_color = self.terminal_underline_color;
        face.underline_color = self.underline_color;
        face.color_state = self.color_state;
    }
}

/// Memo key for one terminal realization.
///
/// The realization consumed the composed source (its palette callbacks and
/// underline decoration) plus the face's entry color state — the merged
/// attributes the merge/realize pipeline read — and the entry underline
/// slot, which the install keeps when the realized underline is disabled.
/// The source enters by allocation identity: the memo holds the allocation,
/// so an equal key is the same rooted source by construction. Keys are
/// compared, never hashed — this is a probe-side equality on a handful of
/// copies, not a bucket index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TtyPaintKey {
    attributes: FaceColorAttributes,
    underline_color: u32,
}

impl TtyPaintKey {
    pub(super) fn of(face: &ResolvedFace) -> Self {
        Self {
            attributes: FaceColorAttributes::from_base(face),
            underline_color: face.underline_color,
        }
    }

    /// Whether `face` is in the same realization state this key recorded.
    pub(super) fn matches(self, face: &ResolvedFace) -> bool {
        self.attributes == FaceColorAttributes::from_base(face)
            && self.underline_color == face.underline_color
    }
}

/// One realized terminal paint retained beside the composed source it came
/// from. The frame and binding-generation stamp validity, exactly GNU's
/// `face_change` retirement of frame face-cache entries (src/xfaces.c:349,
/// src/xfaces.c:3098-3105); the source allocation is the memo's lifetime.
#[derive(Clone, Copy, Debug)]
pub(super) struct TtyPaintMemo {
    pub(super) frame: FrameId,
    pub(super) face_change_count: u64,
    pub(super) key: TtyPaintKey,
    pub(super) paint: TtyPaint,
}
