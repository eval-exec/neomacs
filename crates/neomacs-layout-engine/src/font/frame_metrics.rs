//! Frame-level font geometry states.
//!
//! GNU redisplay never infers a terminal from small pixel metrics.  A graphic
//! frame publishes geometry only after opening a measurable font, while a
//! terminal frame has an explicit 1x1 logical cell.  These types preserve that
//! distinction at the layout boundary.

/// Last-resort graphic font size used only when neither the requested face nor
/// the retained frame carries a usable size.
const DEFAULT_GRAPHIC_FONT_SIZE_PX: f32 = 13.0;

/// A finite, strictly positive graphic-font pixel size.
///
/// Keeping the field private makes it impossible for the font measurement and
/// frame-publication paths to accidentally treat the transient `0.0` "not
/// realized yet" value as an opened font size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphicFontSizePx(f32);

impl GraphicFontSizePx {
    pub fn new(pixels: f32) -> Option<Self> {
        (pixels.is_finite() && pixels > 0.0).then_some(Self(pixels))
    }

    pub(crate) fn resolve(requested: f32, retained: f32) -> Self {
        Self::new(requested)
            .or_else(|| Self::new(retained))
            .unwrap_or_default()
    }

    pub fn get(self) -> f32 {
        self.0
    }
}

impl Default for GraphicFontSizePx {
    fn default() -> Self {
        Self(DEFAULT_GRAPHIC_FONT_SIZE_PX)
    }
}

/// Font-geometry domain of a frame.
///
/// `TerminalCell` is deliberately not represented as a one-pixel graphic
/// font.  That prevents a numeric floor from conflating GNU's two redisplay
/// paths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FrameFontDomain {
    Graphic { retained_size: GraphicFontSizePx },
    TerminalCell,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FaceSizeCandidate {
    Absolute(f32),
    Relative(f32),
}

impl FaceSizeCandidate {
    fn pixels(self) -> f32 {
        match self {
            Self::Absolute(pixels) | Self::Relative(pixels) => pixels,
        }
    }
}

impl FrameFontDomain {
    pub(crate) fn for_frame(has_window_system: bool, retained_size: f32) -> Self {
        if has_window_system {
            Self::Graphic {
                retained_size: GraphicFontSizePx::resolve(retained_size, retained_size),
            }
        } else {
            Self::TerminalCell
        }
    }

    /// Resolve a face size without allowing a transient unrealized value to
    /// replace the last coherent graphic-frame size.
    pub(crate) fn resolve_face_size(self, candidate: FaceSizeCandidate, inherited: f32) -> f32 {
        let requested = candidate.pixels();
        match self {
            Self::Graphic { retained_size } => GraphicFontSizePx::new(requested)
                .or_else(|| GraphicFontSizePx::new(inherited))
                .unwrap_or(retained_size)
                .get(),
            // Terminal face sizes are selector metadata, not frame geometry.
            // Preserve the pre-existing GNU-facing semantics: an absolute
            // zero remains zero, while a relative scale has a one-unit floor.
            Self::TerminalCell => match candidate {
                FaceSizeCandidate::Absolute(pixels) => pixels,
                FaceSizeCandidate::Relative(pixels) => pixels.max(1.0),
            },
        }
    }

    pub(crate) fn graphic_size(self, requested: f32) -> Option<GraphicFontSizePx> {
        match self {
            Self::Graphic { retained_size } => {
                Some(GraphicFontSizePx::new(requested).unwrap_or(retained_size))
            }
            Self::TerminalCell => None,
        }
    }

    /// Advance a graphic domain from requested/retained size to the size of
    /// the font that was actually opened. Terminal domains cannot make this
    /// transition.
    pub(crate) fn retain_opened_graphic_size(&mut self, opened_size: GraphicFontSizePx) {
        if let Self::Graphic { retained_size } = self {
            *retained_size = opened_size;
        } else {
            debug_assert!(false, "a terminal cell cannot retain a graphic font size");
        }
    }
}
