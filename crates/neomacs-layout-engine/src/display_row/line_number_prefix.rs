//! Display-line-number TEXT_AREA-prefix rendering, following GNU
//! `maybe_produce_line_number` (`xdisp.c`): emit a complete face-backed padded
//! field at the start of TEXT_AREA, then fit only its padding to the prefix's
//! authoritative pixel extent.

use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplayTextRun, RenderFaceRef, SourceSpan,
};
use crate::display_row::geometry::DisplayRowGeometryState;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::source_state::DisplayRowSourceState;
use crate::display_row::walk_state::{
    FaceScanCheckpoint, LineNumberFieldLayout, LineNumberRenderState,
};
use crate::display_source::{DisplayItemSource, DisplaySourceContext};
use crate::frame_face_arena::FrameFaceAttempt;
use crate::types::DisplayLineNumbersMode;
use neomacs_display_protocol::glyph_matrix::GlyphArea;
use neomacs_display_protocol::types::FaceId;

const LINE_NUMBER_MARGIN_SOURCE_ID: u64 = 0x6c6e_756d;

struct LineNumberTextPrefixItemSource {
    items: std::vec::IntoIter<DisplayItem>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferLineNumberTextPrefixRenderRequest {
    mode: DisplayLineNumbersMode,
    current_absolute: bool,
    offset: i64,
    major_tick: i32,
    field: LineNumberFieldLayout,
}

impl BufferLineNumberTextPrefixRenderRequest {
    pub(crate) fn new(
        mode: DisplayLineNumbersMode,
        current_absolute: bool,
        offset: i64,
        major_tick: i32,
        field: LineNumberFieldLayout,
    ) -> Self {
        Self {
            mode,
            current_absolute,
            offset,
            major_tick,
            field,
        }
    }

    pub(crate) fn render_pending_with_source_state(
        self,
        line_numbers: &mut LineNumberRenderState,
        source_render: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        row_geometry: &mut DisplayRowGeometryState,
        face_scan: &mut FaceScanCheckpoint,
    ) -> bool {
        let Some(line_number_request) = line_numbers.take_text_prefix(
            self.mode,
            self.current_absolute,
            self.offset,
            self.major_tick,
            self.field,
        ) else {
            return false;
        };

        let line_number_face =
            source_render.resolve_named_face(line_number_request.face().face_name());
        let line_number_face_id = crate::display_row::face_state::stable_face_id_for_resolved(
            face_ids,
            &line_number_face,
        );
        source_render.insert_resolved_face(line_number_face_id, &line_number_face);

        let text = line_number_request.padded_text();
        let mut source = LineNumberTextPrefixItemSource::new(&text, line_number_face_id);
        let mut source_state = DisplayRowSourceState::frame_local();
        let margin_cols = line_number_request.cols().max(1) as usize + 1;
        source_render.render_natural_fragment_from_row_geometry_columns(
            row_geometry,
            &mut source,
            &mut source_state,
            margin_cols,
            line_number_request.cell_width_px(),
            neomacs_display_protocol::frame_glyphs::GlyphRowRole::Text,
            line_number_face_id,
            &line_number_face,
            0,
            margin_cols,
            GlyphArea::Text,
            face_ids,
        );
        let extent = line_number_request.pixel_extent();
        source_render.fit_current_row_area_padding_to_extent(GlyphArea::Text, extent.get());
        source_render.include_current_row_visible_content_metrics(
            face_ids,
            DisplayRowFallbackMetrics::from_default_face_extents(
                line_number_request.cell_width_px(),
                row_geometry.height(),
                row_geometry.ascent(),
            ),
            row_geometry,
        );

        face_scan.invalidate();
        true
    }
}

fn line_number_prefix_text_item(text: &str, face_id: FaceId, start_offset: usize) -> DisplayItem {
    let end_offset = start_offset.saturating_add(text.chars().count());
    DisplayItem::new(
        SourceSpan::synthetic(LINE_NUMBER_MARGIN_SOURCE_ID, start_offset, end_offset),
        RenderFaceRef::FaceId(face_id),
        DisplayItemKind::TextRun(DisplayTextRun::new(text.to_owned())),
    )
}

impl LineNumberTextPrefixItemSource {
    fn new(text: &str, face_id: FaceId) -> Self {
        Self {
            items: vec![line_number_prefix_text_item(text, face_id, 0)].into_iter(),
        }
    }
}

impl DisplayItemSource for LineNumberTextPrefixItemSource {
    fn next_item(&mut self, _context: &mut DisplaySourceContext<'_>) -> Option<DisplayItem> {
        self.items.next()
    }
}
