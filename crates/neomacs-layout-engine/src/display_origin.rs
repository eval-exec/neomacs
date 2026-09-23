use crate::display_face_policy::BaseFacePolicy;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neovm_core::buffer::CharPos0;
use neovm_core::emacs_core::Value;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum OverlayStringKind {
    Before,
    After,
}

/// Which property carrier a `display` replacement string came from.
///
/// GNU distinguishes these — `handle_display_prop` reads the spec through
/// `get_char_property_and_overlay` and keeps the overlay it came from
/// (xdisp.c) — and this crate already threads the answer from
/// `DisplayReplacementStringSourceItem::display_property_string` down onto the
/// origin. What is missing is upstream: `DisplayPropertyClassification` does
/// not yet tell a text-property `display` from an overlay one, so the single
/// production caller states `TextProperty` unconditionally. The `Overlay`
/// variant that stood here was never constructed by anything, tests included;
/// it comes back as one line at that call site on the day the classification
/// can answer the question.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayPropertySource {
    TextProperty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayOrigin {
    BufferText {
        charpos: CharPos0,
    },
    OverlayString {
        overlay_id: Value,
        anchor_charpos: CharPos0,
        kind: OverlayStringKind,
    },
    DisplayPropertyString {
        anchor_charpos: CharPos0,
        source: DisplayPropertySource,
    },
    LinePrefix {
        anchor_charpos: CharPos0,
    },
    WrapPrefix {
        anchor_charpos: CharPos0,
    },
    ModeLine {
        selected: bool,
    },
    HeaderLine {
        selected: bool,
    },
    TabLine,
    TabBar,
}

impl DisplayOrigin {
    pub(crate) fn default_base_face_policy(self) -> BaseFacePolicy {
        BaseFacePolicy::from(self)
    }

    pub(crate) fn glyph_row_role(self) -> Option<GlyphRowRole> {
        match self {
            Self::ModeLine { .. } => Some(GlyphRowRole::ModeLine),
            Self::HeaderLine { .. } => Some(GlyphRowRole::HeaderLine),
            Self::TabLine => Some(GlyphRowRole::TabLine),
            Self::TabBar => Some(GlyphRowRole::TabBar),
            Self::BufferText { .. }
            | Self::OverlayString { .. }
            | Self::DisplayPropertyString { .. }
            | Self::LinePrefix { .. }
            | Self::WrapPrefix { .. } => None,
        }
    }
}

#[cfg(test)]
#[path = "display_origin/tests/display_origin_test.rs"]
mod tests;
