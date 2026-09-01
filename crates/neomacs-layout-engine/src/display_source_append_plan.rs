use crate::display_item::{DisplayItem, DisplayItemKind};
use crate::display_row::append_context::DisplayRowAppendFrame;
use crate::display_row::builder::DisplayRowItemMeasurement;
use crate::display_row::render_policy::DisplayRowRenderPolicy;
use crate::display_text_run_measurement::DisplayTextRunMeasurementPlan;
use crate::font::metrics::FontMetricsService;
use neomacs_display_protocol::types::FaceId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NaturalDisplayRowAppendRenderPolicy;

impl DisplayRowRenderPolicy for NaturalDisplayRowAppendRenderPolicy {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolvedSourceAdvanceRenderPolicy {
    advance_px: f32,
}

impl ResolvedSourceAdvanceRenderPolicy {
    pub(crate) fn new(advance_px: f32) -> Self {
        Self { advance_px }
    }

    fn measurement_for_text(&self, text: &str) -> DisplayRowItemMeasurement {
        DisplayRowItemMeasurement::TextRun(
            DisplayTextRunMeasurementPlan::from_resolved_source_advance(text, self.advance_px),
        )
    }
}

impl DisplayRowRenderPolicy for ResolvedSourceAdvanceRenderPolicy {
    fn measurement_for(
        &mut self,
        item: &DisplayItem,
        _face_id: FaceId,
        _font_metrics: &mut Option<FontMetricsService>,
    ) -> DisplayRowItemMeasurement {
        match &item.kind {
            DisplayItemKind::TextRun(run) => self.measurement_for_text(&run.text),
            DisplayItemKind::SourceMappedText(text) => self.measurement_for_text(&text.text),
            _ => DisplayRowItemMeasurement::Default,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplaySourceAppendRenderPolicy {
    Natural(NaturalDisplayRowAppendRenderPolicy),
    Resolved(ResolvedSourceAdvanceRenderPolicy),
}

impl DisplaySourceAppendRenderPolicy {
    pub(crate) fn natural() -> Self {
        Self::Natural(NaturalDisplayRowAppendRenderPolicy)
    }

    pub(crate) fn resolved_advance(advance_px: f32) -> Self {
        Self::Resolved(ResolvedSourceAdvanceRenderPolicy::new(advance_px))
    }
}

impl DisplayRowRenderPolicy for DisplaySourceAppendRenderPolicy {
    fn measurement_for(
        &mut self,
        item: &DisplayItem,
        face_id: FaceId,
        font_metrics: &mut Option<FontMetricsService>,
    ) -> DisplayRowItemMeasurement {
        match self {
            Self::Natural(policy) => policy.measurement_for(item, face_id, font_metrics),
            Self::Resolved(policy) => policy.measurement_for(item, face_id, font_metrics),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DisplaySourceFallbackWidth {
    columns: usize,
}

impl DisplaySourceFallbackWidth {
    pub(crate) fn columns(columns: usize) -> Self {
        Self { columns }
    }

    #[cfg(test)]
    pub(crate) fn column_count(self) -> usize {
        self.columns
    }

    pub(crate) fn resolve_to_text_row(self, frame: &DisplayRowAppendFrame) -> f32 {
        frame.width_for_columns(self.columns)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplaySourceAppendMeasurementKind {
    NaturalRenderedSource,
    ResolvedComplexRun,
}

impl DisplaySourceAppendMeasurementKind {
    pub(crate) fn for_char(ch: char) -> Self {
        // A char shown via an ASCII escape substitute (`\NNN` octal, see
        // `is_escape_glyph_octal`) is NOT complex-shaped: the glyphs actually
        // drawn are the ASCII escape chars, which must be measured naturally.
        // GNU resolves an escape glyph's font/advance from `char_to_display`
        // (the substitute char) via FACE_FOR_CHAR (xdisp.c:8729), never from the
        // source char. Without this guard, a non-printable char that happens to
        // sit in a complex-script block -- e.g. the noncharacter U+FDD0, which
        // lives in Arabic Presentation Forms -- would take the ResolvedComplexRun
        // path and stamp its wide Arabic-shaped advance onto every `\NNN` digit.
        if crate::composition::needs_complex_shaping(ch)
            && !crate::display_source::is_escape_glyph_octal(ch)
        {
            Self::ResolvedComplexRun
        } else {
            Self::NaturalRenderedSource
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplaySourceAppendRenderPlan {
    advance_px: f32,
    policy: DisplaySourceAppendRenderPolicy,
}

impl DisplaySourceAppendRenderPlan {
    pub(crate) fn natural(advance_px: f32) -> Self {
        Self {
            advance_px,
            policy: DisplaySourceAppendRenderPolicy::natural(),
        }
    }

    pub(crate) fn resolved_advance(advance_px: f32) -> Self {
        Self {
            advance_px,
            policy: DisplaySourceAppendRenderPolicy::resolved_advance(advance_px),
        }
    }

    pub(crate) fn advance_px(self) -> f32 {
        self.advance_px
    }

    pub(crate) fn render_policy(self) -> DisplaySourceAppendRenderPolicy {
        self.policy
    }
}

#[cfg(test)]
mod measurement_kind_tests {
    use super::*;

    #[test]
    fn escape_octal_char_in_complex_block_measures_naturally_not_complex() {
        // U+FDD0 is a noncharacter shown as the ASCII octal escape `\176720`,
        // but it sits in the Arabic Presentation Forms block so it needs complex
        // shaping. The substitute is ASCII and must be measured naturally, else
        // every escape digit inherits FDD0's wide Arabic-shaped advance.
        assert!(crate::composition::needs_complex_shaping('\u{fdd0}'));
        assert!(crate::display_source::is_escape_glyph_octal('\u{fdd0}'));
        assert_eq!(
            DisplaySourceAppendMeasurementKind::for_char('\u{fdd0}'),
            DisplaySourceAppendMeasurementKind::NaturalRenderedSource
        );
        // A real Arabic letter (printable, drawn as itself) keeps complex shaping.
        assert_eq!(
            DisplaySourceAppendMeasurementKind::for_char('\u{0645}'), // MEEM
            DisplaySourceAppendMeasurementKind::ResolvedComplexRun
        );
        // Plain ASCII stays natural.
        assert_eq!(
            DisplaySourceAppendMeasurementKind::for_char('a'),
            DisplaySourceAppendMeasurementKind::NaturalRenderedSource
        );
    }
}
