//! Row finalization after source-specific acquisition has ended.
//!
//! [`DisplayRowLineEndFinalizer`] applies GNU line-end semantics while the row
//! is still in logical order. [`GlyphRowFinalizer`] performs presentation
//! commit work (bidi reorder, pointer runs, and cursor remapping) after the
//! completed row is installed. Keeping both phases here gives display sources
//! one narrow destination without mixing source acquisition into row policy.

use crate::display_cursor::cursor_window_matches_current;
use crate::display_item::{DisplayLineHeightPolicy, DisplayRowBreak};
use crate::display_row::face_state::{DisplayRowFace, DisplayRowMeasurementMode};
use crate::display_row::line_end::{
    LineEndContext, LineEndExtend, LineEndFillGeometry, NoNamedLineEndFaces, plan,
};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::glyph_row_writer::push_stretch_to_area;
use neomacs_display_protocol::face::BoxVerticalEdges;
use neomacs_display_protocol::frame_glyphs::PhysCursor;
use neomacs_display_protocol::glyph_matrix::{
    Glyph, GlyphArea, GlyphProvenance, GlyphRow, GlyphType, RedisplayGlyphProvenance,
};
use neomacs_display_protocol::types::{Color, FaceId, Rect};

/// Finalizes the semantic effects of a typed newline after every source has
/// converged into the shared display-item stream. This is deliberately below
/// buffer text, Lisp strings, display replacements, and overlay strings: GNU's
/// `line-height` and `:extend` behavior depends on the completed display row,
/// not on which source happened to produce the newline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowLineEndFinalizer {
    row_break: DisplayRowBreak,
    row_break_face_id: FaceId,
    remaining_width_px: f32,
    fallback_metrics: DisplayRowFallbackMetrics,
    base_background: Color,
    /// GNU `append_space_for_newline` (xdisp.c:24122) appends the terminal
    /// end-of-line glyph only for [`DisplayRowMeasurementMode::LogicalCells`]
    /// rows: on terminal frames the appended space KEEPS the newline's face
    /// (`it->face_id`), so a face spanning the newline -- e.g. a font-lock
    /// comment -- paints its foreground on the end-of-line cell.
    /// Window-system frames append with the DEFAULT face instead (invisible,
    /// cursor-hosting only), which our GUI path already covers without a
    /// glyph. The shared line-end seam encodes that rule.
    measurement_mode: DisplayRowMeasurementMode,
    box_vertical_edges: BoxVerticalEdges,
    box_run_membership: neomacs_display_protocol::face::BoxRunMembership,
}

impl DisplayRowLineEndFinalizer {
    pub(crate) fn new(
        row_break: DisplayRowBreak,
        row_break_face_id: FaceId,
        remaining_width_px: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
        base_background: Color,
        measurement_mode: DisplayRowMeasurementMode,
        box_vertical_edges: BoxVerticalEdges,
        box_run_membership: neomacs_display_protocol::face::BoxRunMembership,
    ) -> Self {
        Self {
            row_break,
            row_break_face_id,
            remaining_width_px,
            fallback_metrics,
            base_background,
            measurement_mode,
            box_vertical_edges,
            box_run_membership,
        }
    }

    pub(crate) fn finalize(self, row: &mut GlyphRow, faces: &[DisplayRowFace]) {
        if self.row_break.line_height == DisplayLineHeightPolicy::ContentOnly {
            let (height, ascent) = display_row_visible_content_metrics(
                row,
                self.fallback_metrics.row_height(),
                self.fallback_metrics.ascent(),
                |face_id| {
                    faces
                        .iter()
                        .find(|face| face.face_id == face_id)
                        .map(|face| (face.metrics.line_height_px(), face.metrics.ascent_px()))
                },
            );
            row.height_px = height;
            row.ascent_px = ascent;
        }

        // GNU order at a line end -- append_space_for_newline first, then
        // extend_face_to_end_of_line (xdisp.c:26530-26533) -- decided by the
        // shared line-end seam. The item renderer's context is the degenerate
        // one: no fill-column indicator, no trailing-whitespace highlight.
        // The trailing-whitespace step stays UNWIRED here on purpose: GNU's
        // highlight_trailing_whitespace re-faces only glyphs whose object is
        // the buffer, and the rows this finalizer completes are produced from
        // Lisp strings, whose glyphs can never satisfy that test — the step
        // could not fire. NoNamedLineEndFaces relies on both flags staying
        // off (its lookups are unreachable!()).
        let extend_face = faces
            .iter()
            .find(|face| face.face_id == self.row_break_face_id && face.extend);
        let ctx = LineEndContext {
            reason: self.row_break.reason,
            newline_face_id: self.row_break_face_id,
            measurement_mode: self.measurement_mode,
            pen_x: 0.0,
            pen_col: 0,
            right_edge_x: self.remaining_width_px,
            char_width: self.fallback_metrics.char_width().max(1.0),
            indicator: None,
            // Unfiltered on purpose -- see line_end::extend_fill_runs: the
            // frame-background skip is GNU's FRAME_WINDOW_P-guarded early
            // return (xdisp.c:24388) and must not reach a terminal row.
            extend: extend_face.map(|face| LineEndExtend {
                bg: face.background,
                face_id: face.face_id,
            }),
            frame_background: self.base_background,
            trailing_whitespace_enabled: false,
            box_vertical_edges: self.box_vertical_edges,
            box_run_membership: self.box_run_membership,
        };
        let height_px = row.height_px.max(1.0);
        let geometry = LineEndFillGeometry {
            content_x: 0.0,
            height_px,
            ascent_px: row.ascent_px.max(0.0).min(height_px),
            fill_char_width: extend_face
                .map(|face| {
                    face.metrics
                        .char_width_px(self.fallback_metrics.char_width())
                })
                .unwrap_or_else(|| self.fallback_metrics.char_width()),
        };
        plan(&ctx)
            .resolve(&ctx, geometry, &mut NoNamedLineEndFaces)
            .apply_to(row);
    }
}

/// Metrics contributed by visible glyphs, excluding the row's default
/// minimum. GNU keeps these as iterator `max_ascent`/`max_descent` values.
pub(crate) fn display_row_visible_content_metrics(
    row: &GlyphRow,
    fallback_height: f32,
    fallback_ascent: f32,
    mut face_metrics: impl FnMut(FaceId) -> Option<(f32, f32)>,
) -> (f32, f32) {
    let mut max_ascent = 0.0f32;
    let mut max_descent = 0.0f32;
    let mut saw_glyph = false;
    for glyph in row.glyphs.iter().flatten() {
        saw_glyph = true;
        let (height, ascent) = display_glyph_visible_metrics(
            glyph,
            fallback_height,
            fallback_ascent,
            &mut face_metrics,
        );
        let ascent = ascent.max(0.0).min(height);
        let descent = (height - ascent).max(0.0);
        max_ascent = max_ascent.max((ascent - glyph.vertical_offset_px).max(0.0));
        max_descent = max_descent.max((descent + glyph.vertical_offset_px).max(0.0));
    }
    if saw_glyph {
        let height = (max_ascent + max_descent).max(1.0);
        (height, max_ascent.min(height))
    } else {
        (1.0, 1.0)
    }
}

fn display_glyph_visible_metrics(
    glyph: &Glyph,
    fallback_height: f32,
    fallback_ascent: f32,
    face_metrics: &mut impl FnMut(FaceId) -> Option<(f32, f32)>,
) -> (f32, f32) {
    if glyph.pixel_height > 0.0 {
        return (glyph.pixel_height, glyph.pixel_ascent);
    }
    face_metrics(glyph.face_id).unwrap_or_else(|| {
        let height = fallback_height.max(1.0);
        (height, fallback_ascent.max(0.0).min(height))
    })
}

/// Result of attempting to append an exact trailing face fill.
///
/// Keeping "already present" distinct from "appended" prevents callers that
/// also advance row progress from advancing it twice when finalization is
/// retried.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RowTrailingFaceFillResult {
    Appended,
    AlreadyPresent,
    NotApplicable,
}

/// Exact trailing fill emitted by GNU `extend_face_to_end_of_line`.
///
/// This operation does not manufacture a text glyph.  GNU's empty-line face
/// anchor is a separate step (`append_space_for_newline`); keeping that policy
/// out of this primitive lets window chrome fill an empty row to exactly its
/// bounds without adding one extra character cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RowTrailingFaceFill {
    face_id: FaceId,
    width_px: f32,
    height_px: f32,
    ascent_px: f32,
    char_width: f32,
    box_vertical_edges: BoxVerticalEdges,
}

impl RowTrailingFaceFill {
    pub(crate) fn new(
        face_id: FaceId,
        width_px: f32,
        height_px: f32,
        ascent_px: f32,
        char_width: f32,
    ) -> Self {
        Self {
            face_id,
            width_px,
            height_px,
            ascent_px,
            char_width,
            // A trailing `:extend` fill normally continues the source box run
            // beyond this visual row. The source owner upgrades this to Right
            // only when GNU's iterator proves the box run ends here.
            box_vertical_edges: BoxVerticalEdges::Neither,
        }
    }

    pub(crate) fn with_box_vertical_edges(mut self, edges: BoxVerticalEdges) -> Self {
        self.box_vertical_edges = edges;
        self
    }

    pub(crate) fn apply_to(self, row: &mut GlyphRow) -> RowTrailingFaceFillResult {
        debug_assert!(
            !row.reversed_p,
            "line-end fill must precede bidi finalization"
        );
        if self.width_px <= 0.0 {
            return RowTrailingFaceFillResult::NotApplicable;
        }
        let text_index = GlyphArea::Text.index();
        if row.glyphs[text_index]
            .last()
            .is_some_and(|glyph| self.matches_existing_fill(glyph))
        {
            return RowTrailingFaceFillResult::AlreadyPresent;
        }
        push_stretch_to_area(
            row,
            text_index,
            self.width_cols(),
            self.face_id,
            self.width_px,
            self.height_px,
            self.ascent_px,
            // Redisplay's own fill stands for no buffer character, so the
            // blank-line cursor never latches to it.
            GlyphProvenance::line_end(),
        );
        row.glyphs[text_index]
            .last_mut()
            .expect("the trailing fill was just appended")
            .box_vertical_edges = self.box_vertical_edges;
        RowTrailingFaceFillResult::Appended
    }

    pub(crate) fn width_px(self) -> f32 {
        self.width_px
    }

    pub(crate) fn width_cols(self) -> u16 {
        let char_width = self.char_width.max(1.0);
        ((self.width_px / char_width).ceil() as i64).clamp(1, u16::MAX as i64) as u16
    }

    fn matches_existing_fill(self, glyph: &Glyph) -> bool {
        const PIXEL_TOLERANCE: f32 = 0.01;
        matches!(
            glyph.provenance,
            GlyphProvenance::Redisplay(RedisplayGlyphProvenance::LineEnd)
        ) && glyph.face_id == self.face_id
            && matches!(
                glyph.glyph_type,
                GlyphType::Stretch { width_cols } if width_cols == self.width_cols()
            )
            && (glyph.pixel_width - self.width_px).abs() <= PIXEL_TOLERANCE
            && (glyph.pixel_height - self.height_px).abs() <= PIXEL_TOLERANCE
            && (glyph.pixel_ascent - self.ascent_px).abs() <= PIXEL_TOLERANCE
            && glyph.box_vertical_edges == self.box_vertical_edges
    }
}

/// Newline face-extension policy around the exact trailing fill primitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RowExtendFill {
    bg: Color,
    trailing: RowTrailingFaceFill,
}

impl RowExtendFill {
    pub(crate) fn new(
        bg: Color,
        face_id: FaceId,
        width_px: f32,
        height_px: f32,
        ascent_px: f32,
        char_width: f32,
    ) -> Self {
        Self {
            bg,
            trailing: RowTrailingFaceFill::new(face_id, width_px, height_px, ascent_px, char_width),
        }
    }

    pub(crate) fn with_box_vertical_edges(mut self, edges: BoxVerticalEdges) -> Self {
        self.trailing = self.trailing.with_box_vertical_edges(edges);
        self
    }

    /// Apply the fill to `row`. The operation is idempotent so the unified
    /// item renderer can coexist with an older source caller while that caller
    /// is migrated to the shared row-finalization seam.
    pub(crate) fn apply_to(self, row: &mut GlyphRow) -> bool {
        debug_assert!(
            !row.reversed_p,
            "line-end fill must precede bidi finalization"
        );
        if self.trailing.width_px <= 0.0 {
            return false;
        }
        let text_index = GlyphArea::Text.index();
        if row.glyphs[text_index].is_empty() {
            row.glyphs[text_index].push(
                Glyph::char_with_provenance(
                    ' ',
                    self.trailing.face_id,
                    GlyphProvenance::line_end(),
                )
                .with_pixel_width(self.trailing.char_width.max(1.0)),
            );
            row.displays_text = true;
        }
        !matches!(
            self.trailing.apply_to(row),
            RowTrailingFaceFillResult::NotApplicable
        )
    }

    #[cfg(test)]
    pub(crate) fn width_cols(self) -> u16 {
        self.trailing.width_cols()
    }
}

#[cfg(test)]
thread_local! {
    static POINTER_RUN_GLYPH_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_pointer_run_glyph_visits() {
    POINTER_RUN_GLYPH_VISITS.with(|visits| visits.set(0));
}

#[cfg(test)]
pub(crate) fn pointer_run_glyph_visits() -> usize {
    POINTER_RUN_GLYPH_VISITS.with(std::cell::Cell::get)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlyphRowFinalizationContext {
    pub(crate) window_id: u64,
    pub(crate) row: usize,
    pub(crate) window_pixel_bounds: Rect,
}

impl GlyphRowFinalizationContext {
    pub(crate) fn new(window_id: u64, row: usize, window_pixel_bounds: Rect) -> Self {
        Self {
            window_id,
            row,
            window_pixel_bounds,
        }
    }

    pub(crate) fn finalize_row(
        self,
        row: &mut GlyphRow,
        matrix_ncols: usize,
        phys_cursor: Option<&mut PhysCursor>,
    ) {
        GlyphRowFinalizer::new(self, matrix_ncols, phys_cursor).finalize(row);
    }

    fn cursor_matches(self, cursor: &PhysCursor) -> bool {
        cursor_window_matches_current(cursor.window_id.get(), self.window_id)
            && cursor.row == self.row
    }
}

pub(crate) struct GlyphRowFinalizer<'cursor> {
    context: GlyphRowFinalizationContext,
    matrix_ncols: usize,
    phys_cursor: Option<&'cursor mut PhysCursor>,
}

impl<'cursor> GlyphRowFinalizer<'cursor> {
    pub(crate) fn new(
        context: GlyphRowFinalizationContext,
        matrix_ncols: usize,
        phys_cursor: Option<&'cursor mut PhysCursor>,
    ) -> Self {
        Self {
            context,
            matrix_ncols,
            phys_cursor,
        }
    }

    pub(crate) fn finalize(&mut self, row: &mut GlyphRow) {
        let phys_cursor_col = self
            .phys_cursor
            .as_deref()
            .filter(|cursor| self.context.cursor_matches(cursor))
            .map(|cursor| cursor.col);

        let remapped_cursor_col = crate::glyph_row_writer::reorder_row_bidi(row, phys_cursor_col);
        let char_width = if self.matrix_ncols > 0 {
            self.context.window_pixel_bounds.width / self.matrix_ncols as f32
        } else {
            1.0
        };
        row.rebuild_pointer_runs(char_width, self.context.window_pixel_bounds.width);
        #[cfg(test)]
        POINTER_RUN_GLYPH_VISITS.with(|visits| {
            visits.set(visits.get().saturating_add(row.total_glyphs()));
        });
        self.apply_phys_cursor_remap(remapped_cursor_col);
    }

    fn apply_phys_cursor_remap(&mut self, remapped_cursor_col: Option<u16>) {
        let Some(col) = remapped_cursor_col else {
            return;
        };
        let Some(cursor) = self.phys_cursor.as_deref_mut() else {
            return;
        };
        if !self.context.cursor_matches(cursor) {
            return;
        }

        cursor.col = col;
        cursor.slot_id.col = col;
        if self.matrix_ncols > 0 {
            let char_w = self.context.window_pixel_bounds.width / self.matrix_ncols as f32;
            cursor.x = self.context.window_pixel_bounds.x + col as f32 * char_w;
        }
    }
}

#[cfg(test)]
#[path = "finalizer_test.rs"]
mod tests;

#[cfg(test)]
#[path = "extend_fill_test.rs"]
mod extend_fill_tests;
