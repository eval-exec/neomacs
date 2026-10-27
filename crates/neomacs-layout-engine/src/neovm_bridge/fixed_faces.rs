//! Fixed face-name producers for bridge parameter construction.
//! Only native epoch/ID cells are shared. Each handle resolves lazily at the
//! original lookup branch; no face value, Lisp heap object or membership proof
//! is cached. Existing caller order and skipped branches remain authoritative.

use super::{FaceTable, NeoFace};
use neovm_core::emacs_core::intern::WellKnown;

#[derive(Clone, Copy, Debug, strum::IntoStaticStr, strum::VariantArray)]
#[strum(serialize_all = "kebab-case")]
pub(super) enum FixedFace {
    Default,
    Cursor,
    VerticalBorder,
    WindowDivider,
    WindowDividerFirstPixel,
    WindowDividerLastPixel,
    TrailingWhitespace,
    FillColumnIndicator,
    EscapeGlyph,
    NobreakSpace,
    GlyphlessChar,
}

static DEFAULT: WellKnown = WellKnown::Default;
static CURSOR: WellKnown = WellKnown::Cursor;
static VERTICAL_BORDER: WellKnown = WellKnown::VerticalBorder;
static WINDOW_DIVIDER: WellKnown = WellKnown::WindowDivider;
static WINDOW_DIVIDER_FIRST_PIXEL: WellKnown = WellKnown::WindowDividerFirstPixel;
static WINDOW_DIVIDER_LAST_PIXEL: WellKnown = WellKnown::WindowDividerLastPixel;
static TRAILING_WHITESPACE: WellKnown = WellKnown::TrailingWhitespace;
static FILL_COLUMN_INDICATOR: WellKnown = WellKnown::FillColumnIndicator;
static ESCAPE_GLYPH: WellKnown = WellKnown::EscapeGlyph;
static NOBREAK_SPACE: WellKnown = WellKnown::NobreakSpace;
static GLYPHLESS_CHAR: WellKnown = WellKnown::GlyphlessChar;

impl FixedFace {
    #[cfg(test)]
    pub(super) fn name(self) -> &'static str {
        self.into()
    }

    fn handle(self) -> &'static WellKnown {
        match self {
            Self::Default => &DEFAULT,
            Self::Cursor => &CURSOR,
            Self::VerticalBorder => &VERTICAL_BORDER,
            Self::WindowDivider => &WINDOW_DIVIDER,
            Self::WindowDividerFirstPixel => &WINDOW_DIVIDER_FIRST_PIXEL,
            Self::WindowDividerLastPixel => &WINDOW_DIVIDER_LAST_PIXEL,
            Self::TrailingWhitespace => &TRAILING_WHITESPACE,
            Self::FillColumnIndicator => &FILL_COLUMN_INDICATOR,
            Self::EscapeGlyph => &ESCAPE_GLYPH,
            Self::NobreakSpace => &NOBREAK_SPACE,
            Self::GlyphlessChar => &GLYPHLESS_CHAR,
        }
    }

    pub(super) fn resolve(self, table: &FaceTable) -> NeoFace {
        // Value::symbol(handle) performs the sole old constructor charge and
        // validates the current epoch at this exact original lookup boundary.
        table.resolve_fixed_symbol(self.handle())
    }
}
