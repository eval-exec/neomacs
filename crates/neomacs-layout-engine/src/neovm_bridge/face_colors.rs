//! Color composition keeps source attributes separate from realized paint.
//!
//! GNU merges lface attributes before load_face_colors/realize_tty_face.
//! In particular, neither distant-color substitution nor a terminal default
//! crossing channels can be inverted to recover those attributes.

use super::{NeoColor, NeoFace, ResolvedFace, TerminalColor, color_to_pixel, colors_close};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum FaceVideo {
    #[default]
    Normal,
    Inverse,
}

/// Seed faces are manually built frame fallbacks. Once realized, every merge
/// uses the retained attributes, never the public paint fields.
#[derive(Clone, Debug, PartialEq)]
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

    pub(super) fn realize(self) -> RealizedFaceColors {
        let (mut foreground, mut background, terminal_inverse) = match self.video {
            FaceVideo::Normal => (self.foreground, self.background, false),
            FaceVideo::Inverse
                if self.foreground.is_terminal_default()
                    && self.background.is_terminal_default() =>
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
