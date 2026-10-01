//! Numeric visual-row finalization shared with the visible writer's fill primitive.
use super::*;
use crate::display_row::face_state::DisplayRowExtendFace;

pub(super) fn extend_background(
    row: &mut GlyphRow,
    wrap_extend: Option<DisplayRowExtendFace>,
    position: DisplayRowPosition,
    geometry: &RowProgramGeometry,
    backend: DisplayGeometryBackend,
) {
    if let Some(extend) = wrap_extend {
        let mode = match backend {
            DisplayGeometryBackend::WindowSystemPixels => DisplayRowMeasurementMode::ConcreteFont,
            DisplayGeometryBackend::TerminalCells => DisplayRowMeasurementMode::LogicalCells,
        };
        if crate::display_row::line_end::extend_fill_runs(
            mode,
            Some(crate::display_row::line_end::LineEndExtend {
                bg: extend.background(),
                face_id: extend.face_id(),
            }),
            geometry.background,
        ) {
            let metrics = extend.metrics();
            let owns_right = row.glyphs[GlyphArea::Text.index()]
                .iter()
                .rev()
                .find(|glyph| !glyph.padding)
                .is_some_and(|glyph| glyph.box_vertical_edges.owns_right());
            crate::display_row::finalizer::RowExtendFill::new(
                extend.background(),
                extend.face_id(),
                geometry.width - position.x_px(),
                metrics.row_height(),
                metrics.ascent(),
                metrics.char_width(),
            )
            .with_box_vertical_edges(
                neomacs_display_protocol::face::BoxVerticalEdges::from_ownership(false, owns_right),
            )
            .apply_to(row);
        }
    }
}
