//! Owned row and hit-position accumulation, independent of window publication.
//!
//! This is the same recorder used by synchronous output. It contains neither
//! evaluator access nor rooted chrome strings; finished rows acquire freshness
//! and publication authority only in the enclosing window emitter.

use super::snapshot_rows::PreparedWindowRows;
use super::{DisplayRowTerminator, RowMetricsSnapshot};
use crate::display_status_line::DisplayRowOutputProgress;
use neovm_core::buffer::LispCharPos1;
use neovm_core::window::{
    DisplayPointRow, DisplayPointRows, DisplayPointRowsMode, DisplayPointSnapshot,
    DisplayRowEndSource, DisplayRowSnapshot, display_point_rows_mode,
};

#[derive(Clone, Copy, Debug)]
struct CurrentRowProgress {
    display_row_index: Option<usize>,
    row: i64,
    y: i64,
    col: i64,
    x: i64,
    start_col: i64,
    start_x: i64,
}

/// Emitter lookup precedes publication's canonical sort. A fresh closed row
/// keeps its walk order; retained points arrive as one canonically sorted group.
#[derive(Clone, Copy, Debug)]
enum PointLookupGroup {
    FreshRow { index: usize },
    ReusedRows { start: usize, end: usize },
}

pub(super) struct WindowRowGeometry {
    pub(super) text_row_base: i64,
    pub(super) text_x: f32,
    pub(super) window_top: f32,
    /// Enabled only while collecting restart certificates for pixel queries.
    query_translation_exact: Option<bool>,
    points: Vec<DisplayPointSnapshot>,
    point_rows: Option<DisplayPointRows>,
    point_lookup_groups: Vec<PointLookupGroup>,
    rows: Vec<DisplayRowSnapshot>,
    row_metrics: Vec<RowMetricsSnapshot>,
    current_row_first_display_pos: Option<LispCharPos1>,
    current_row_last_display_pos: Option<LispCharPos1>,
    current_row_end_source: DisplayRowEndSource,
    truncated_end_buffer_pos: Option<LispCharPos1>,
    /// The end this row was closed at, when it is a position that draws no
    /// glyph of its own. Recorded rather than published on the spot so that
    /// closing the row is the only thing that can publish it.
    current_row_terminator: Option<DisplayRowTerminator>,
    current_row_progress: Option<CurrentRowProgress>,
}

impl WindowRowGeometry {
    pub(super) fn new(text_row_base: usize, text_x: f32, window_top: f32) -> Self {
        Self {
            text_row_base: text_row_base as i64,
            text_x,
            window_top,
            query_translation_exact: None,
            points: Vec::new(),
            point_rows: (display_point_rows_mode() != DisplayPointRowsMode::Off)
                .then(|| DisplayPointRows { rows: Vec::new() }),
            point_lookup_groups: Vec::new(),
            rows: Vec::new(),
            row_metrics: Vec::new(),
            current_row_first_display_pos: None,
            current_row_last_display_pos: None,
            current_row_end_source: DisplayRowEndSource::Buffer,
            truncated_end_buffer_pos: None,
            current_row_terminator: None,
            current_row_progress: None,
        }
    }

    // Integers in this conservative range remain exact through the nominal
    // grid (at most 256 rows of at most 4096 pixels), its signed correction,
    // and frame/window origin subtraction. Rounded snapshot coordinates alone
    // cannot prove this: subtracting a rounded fractional origin changes phase.
    const QUERY_COORDINATE_BOUND: f32 = 1_048_576.0;
    const QUERY_METRIC_BOUND: f32 = 4096.0;

    fn exact_query_value(value: f32, bound: f32) -> bool {
        value.is_finite() && value.abs() <= bound && value.fract() == 0.0
    }

    pub(super) fn set_query_translation_tracking(
        &mut self,
        enabled: bool,
        text_y: f32,
        default_height: f32,
        default_ascent: f32,
    ) {
        self.query_translation_exact = enabled.then(|| {
            Self::exact_query_value(self.window_top, Self::QUERY_COORDINATE_BOUND)
                && Self::exact_query_value(text_y, Self::QUERY_COORDINATE_BOUND)
                && Self::exact_query_value(default_height, Self::QUERY_METRIC_BOUND)
                && Self::exact_query_value(default_ascent, Self::QUERY_METRIC_BOUND)
        });
    }

    pub(super) fn query_translation_is_exact(&self) -> bool {
        self.query_translation_exact == Some(true)
    }

    pub(super) fn note_query_y(&mut self, y: f32) {
        if self.query_translation_exact == Some(true)
            && !Self::exact_query_value(y, Self::QUERY_COORDINATE_BOUND)
        {
            self.query_translation_exact = Some(false);
        }
    }

    pub(super) fn note_query_metrics(&mut self, height: f32, ascent: f32) {
        if self.query_translation_exact == Some(true)
            && !(Self::exact_query_value(height, Self::QUERY_METRIC_BOUND)
                && Self::exact_query_value(ascent, Self::QUERY_METRIC_BOUND))
        {
            self.query_translation_exact = Some(false);
        }
    }

    pub(super) fn finish(self, body_origin_y: i64) -> PreparedWindowRows {
        PreparedWindowRows::with_point_rows(
            self.points,
            self.point_rows,
            self.rows,
            self.text_row_base,
            body_origin_y,
        )
    }

    /// Seed the body half of this emitter from a prior clean pass (Phase 1
    /// cursor-only replay), in place of walking the buffer. `rows` are the
    /// retained body [`DisplayRowSnapshot`]s and `points` the retained per-span
    /// display points — both are point-INDEPENDENT (they describe where glyphs
    /// render, not where the cursor is), so they replay verbatim. Chrome rows
    /// are appended afterward by the normal chrome path, and the cursor is set
    /// separately for the moved point.
    pub(super) fn seed_cursor_only_body(
        &mut self,
        rows: Vec<DisplayRowSnapshot>,
        points: Vec<DisplayPointSnapshot>,
        point_rows: Option<DisplayPointRows>,
    ) {
        self.rows = rows;
        self.points = points;
        if let Some(point_rows) = point_rows {
            self.point_rows = Some(point_rows);
        }
        self.point_lookup_groups.clear();
        if let Some(frozen) = &mut self.point_rows {
            if !self.points.is_empty() {
                frozen
                    .rows
                    .extend(DisplayPointRows::from_points(std::mem::take(&mut self.points)).rows);
            }
            if !frozen.rows.is_empty() {
                self.point_lookup_groups.push(PointLookupGroup::ReusedRows {
                    start: 0,
                    end: frozen.rows.len(),
                });
            }
        }
    }

    /// Append reused (Phase 2 scroll) body rows + points to the emitter, on top
    /// of the newly-exposed rows the partial walk produced. `finish_snapshot`
    /// sorts rows by index and points by buffer position, so insertion order does
    /// not matter. No `row_metrics` are added (reused grid rows are installed
    /// already-finalized, so the exposed-row finalize pass must not touch them).
    pub(super) fn push_reused_body(
        &mut self,
        rows: Vec<DisplayRowSnapshot>,
        points: Vec<DisplayPointSnapshot>,
        point_rows: Option<DisplayPointRows>,
    ) {
        self.rows.extend(rows);
        let lookup_start = self.point_rows.as_ref().map_or(0, |rows| rows.rows.len());
        if let Some(reused) = point_rows {
            self.point_rows
                .get_or_insert_with(DisplayPointRows::default)
                .rows
                .extend(reused.rows);
        }
        if let Some(frozen) = &mut self.point_rows {
            if !points.is_empty() {
                frozen
                    .rows
                    .extend(DisplayPointRows::from_points(points).rows);
            }
            if frozen.rows.len() > lookup_start {
                self.point_lookup_groups.push(PointLookupGroup::ReusedRows {
                    start: lookup_start,
                    end: frozen.rows.len(),
                });
            }
        } else {
            self.points.extend(points);
        }
    }

    /// Normalize the body rows' snapshot columns to the full walk's convention.
    ///
    /// Every display row starts emitting at the left edge of the text area —
    /// GNU's `display_line` opens each glyph row at `it->first_visible_x` —
    /// so `start_col` is a property of the row itself and not of the row above
    /// it, and `end_col` for a row whose pen never moved (an empty line, whose
    /// only content is its own newline) is that same column.
    ///
    /// This used to re-derive a CHAIN instead: `start_col` = the column where
    /// the PREVIOUS row broke. That was faithful to what the walk published,
    /// because the row transitions opened each output row at the pen of the
    /// row that had just ended, and it was invisible on any row that draws a
    /// glyph — the first glyph moves the output cursor and overwrites it. The
    /// transitions now open a row at the column the walk itself uses
    /// (`DisplayRowLineBreakTransitionPlan::row_start_col`), so the chain is
    /// gone from both sides and reused rows need only agree with the row they
    /// are, not with the row above them.
    pub(super) fn normalize_body_start_cols(&mut self) {
        for row in self.rows.iter_mut() {
            if row.start_buffer_pos.is_none() {
                continue;
            }
            row.start_col = 0;
            if row.end_x == row.start_x {
                row.end_col = row.start_col;
            }
        }
    }

    pub(super) fn display_point_len(&self) -> usize {
        self.points.len()
    }

    pub(super) fn truncate_display_points(&mut self, len: usize) {
        self.points.truncate(len);
    }

    pub(super) fn rows(&self) -> &[DisplayRowSnapshot] {
        &self.rows
    }

    pub(super) fn point_for_buffer_pos(&self, pos: LispCharPos1) -> Option<DisplayPointSnapshot> {
        if let Some(frozen) = &self.point_rows {
            for group in &self.point_lookup_groups {
                let point = match *group {
                    PointLookupGroup::FreshRow { index } => frozen.rows[index]
                        .points_emission_order()
                        .find(|point| point.buffer_pos == pos),
                    PointLookupGroup::ReusedRows { start, end } => frozen.rows[start..end]
                        .iter()
                        .flat_map(DisplayPointRow::points)
                        .filter(|point| point.buffer_pos == pos)
                        .min_by_key(|point| (point.buffer_pos, point.row, point.col, point.x)),
                };
                if point.is_some() {
                    return point;
                }
            }
        }
        self.points
            .iter()
            .find(|point| point.buffer_pos == pos)
            .cloned()
    }

    pub(super) fn point_for_lisp_buffer_pos(
        &self,
        pos: LispCharPos1,
    ) -> Option<DisplayPointSnapshot> {
        self.point_for_buffer_pos(pos)
    }

    pub(super) fn row_metrics(&self) -> &[RowMetricsSnapshot] {
        &self.row_metrics
    }

    pub(super) fn current_row_display_positions(
        &self,
    ) -> (Option<LispCharPos1>, Option<LispCharPos1>) {
        (
            self.current_row_first_display_pos,
            self.current_row_last_display_pos,
        )
    }

    pub(super) fn restore_current_row_display_positions(
        &mut self,
        first: Option<LispCharPos1>,
        last: Option<LispCharPos1>,
    ) {
        self.current_row_first_display_pos = first;
        self.current_row_last_display_pos = last;
        // A restore rewinds the row to a checkpoint taken while it was still
        // being filled, so by construction the row had not ended yet.
        self.current_row_terminator = None;
    }

    pub(super) fn restore_current_row_pen(&mut self, x: f32, col: usize) {
        if let Some(progress) = &mut self.current_row_progress {
            progress.x = (x - self.text_x).round() as i64;
            progress.col = col as i64;
        }
    }

    pub(super) fn current_row_has_output(&self) -> bool {
        self.current_row_progress.as_ref().is_some_and(|progress| {
            progress.x != progress.start_x
                || progress.col != progress.start_col
                || self.current_row_first_display_pos.is_some()
                || self.current_row_last_display_pos.is_some()
        })
    }

    pub(super) fn begin_current_row_progress(
        &mut self,
        display_row_index: Option<usize>,
        row: i64,
        col: i64,
        y: i64,
        x: i64,
    ) {
        self.current_row_progress = Some(CurrentRowProgress {
            display_row_index,
            row,
            y,
            col,
            x,
            start_col: col,
            start_x: x,
        });
    }

    pub(super) fn update_current_row_progress(&mut self, row: i64, col: i64, y: i64, x: i64) {
        match self.current_row_progress.as_mut() {
            Some(progress) if progress.row == row => {
                progress.y = y;
                progress.col = col;
                progress.x = x;
            }
            _ => self.begin_current_row_progress(None, row, col, y, x),
        }
    }

    pub(super) fn note_display_buffer_pos(&mut self, buffer_pos: LispCharPos1) {
        if self.current_row_first_display_pos.is_none() {
            self.current_row_first_display_pos = Some(buffer_pos);
        }
        self.current_row_last_display_pos = Some(buffer_pos);
    }

    pub(super) fn note_display_string_row_end(&mut self, buffer_pos: LispCharPos1) {
        self.note_display_buffer_pos(buffer_pos);
        self.current_row_end_source = DisplayRowEndSource::DisplayStringNewline;
    }

    pub(super) fn note_display_string_wrap(&mut self, buffer_pos: LispCharPos1) {
        self.note_display_buffer_pos(buffer_pos);
        self.current_row_end_source = DisplayRowEndSource::DisplayStringWrap;
    }

    pub(super) fn note_overlay_string_row_end(
        &mut self,
        buffer_pos: LispCharPos1,
        kind: crate::display_origin::OverlayStringKind,
    ) {
        self.note_display_buffer_pos(buffer_pos);
        self.current_row_end_source = match kind {
            crate::display_origin::OverlayStringKind::Before => {
                DisplayRowEndSource::OverlayBeforeString
            }
            crate::display_origin::OverlayStringKind::After => {
                DisplayRowEndSource::OverlayAfterString
            }
        };
    }

    /// Record where this row's WALK began, for a row whose first drawn glyph is
    /// not its first position.
    ///
    /// GNU takes a row's start before it does anything about the hscroll:
    /// `row->start = it->start` (src/xdisp.c:25857), and only then
    /// `move_it_in_display_line_to (it, ZV, it->first_visible_x, MOVE_TO_POS |
    /// MOVE_TO_X)` skips the columns scrolled off the left (:25878-25890).
    /// `it->start` is the previous row's end (`it->start = row->end`,
    /// src/xdisp.c:26855), so a truncating row hscrolled by any amount still
    /// starts at its LINE start -- measured, GNU Emacs 31.0.90: `vertical-motion
    /// 0` answers 202 for a line starting at 202 at hscroll 0, 5, 20 and 100
    /// alike (`scripts/l212-marker-column-probe.el`).
    ///
    /// Deliberately narrower than [`Self::note_display_buffer_pos`]: the skipped
    /// characters are not displayed, so they are the row's START and never its
    /// END.
    pub(super) fn note_row_walk_start(&mut self, buffer_pos: LispCharPos1) {
        if self.current_row_first_display_pos.is_none() {
            self.current_row_first_display_pos = Some(buffer_pos);
        }
    }

    /// Publish the screen column a truncation `$` or continuation `\` covers,
    /// standing in for the buffer position the walk had reached there.
    ///
    /// See [`neovm_core::window::DisplayPointRole`] for the GNU model this
    /// mirrors. Two things this deliberately does NOT do, both because the
    /// marker owns no position of its own:
    ///
    /// * it does not touch the row's first/last display positions, so a marker
    ///   changes neither `start_buffer_pos` (which is the walk's start, above)
    ///   nor `end_buffer_pos`;
    /// * it publishes with [`DisplayPointRole::OverlaidMarker`], so
    ///   `point_for_buffer_pos` prefers a drawn glyph for the same position and
    ///   reaches the marker only when nothing drew one.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_overlaid_marker_point(
        &mut self,
        buffer_pos: LispCharPos1,
        glyph_x: f32,
        glyph_y: f32,
        width: f32,
        height: f32,
        row: i64,
        col: usize,
    ) {
        self.note_query_y(glyph_y);
        self.note_query_metrics(height, 0.0);
        self.points.push(DisplayPointSnapshot {
            role: neovm_core::window::DisplayPointRole::OverlaidMarker,
            buffer_pos,
            x: (glyph_x - self.text_x).round() as i64,
            y: (glyph_y - self.window_top).round() as i64,
            width: width.max(0.0).round() as i64,
            height: height.max(1.0).round() as i64,
            row,
            col: col as i64,
        });
    }

    /// Record that this row ends at a buffer position which draws no glyph of
    /// its own -- GNU's `it->eol_pos`.
    ///
    /// This is the row's end in both senses at once: it is the row's last
    /// display position (so `end_buffer_pos`, and through it `window-end` and
    /// the screen-line motion goal stops, are unchanged) AND the position that
    /// owns every screen column past the row's last glyph. Only
    /// [`Self::push_text_row`] turns it into a display point, so a row cannot
    /// be closed having recorded a terminator and published no slot for it.
    pub(super) fn note_row_terminator(&mut self, terminator: DisplayRowTerminator) {
        self.note_display_buffer_pos(terminator.pos);
        self.current_row_terminator = Some(terminator);
    }

    /// Publish the slot of a recorded terminator, unless the row already draws
    /// a glyph at that position.
    ///
    /// The guard is not an optimisation: a row whose terminator coincides with
    /// a drawn glyph -- the accessible end of the buffer, where
    /// `push_text_insertion_boundary` has already published one -- must keep
    /// exactly one point per position, because `point_for_buffer_pos` binary
    /// searches `points` and `point_at_coords` takes the last point at or
    /// before a column.
    fn publish_row_terminator_slot(&mut self, progress: &CurrentRowProgress, row_height: f32) {
        let Some(terminator) = self.current_row_terminator.take() else {
            return;
        };
        if self
            .points
            .iter()
            .any(|point| point.row == progress.row && point.buffer_pos == terminator.pos)
        {
            return;
        }
        self.points.push(DisplayPointSnapshot {
            role: if neovm_core::window::posn_object_extent_mode().enabled() {
                neovm_core::window::DisplayPointRole::InsertionBoundary
            } else {
                neovm_core::window::DisplayPointRole::Glyph
            },
            buffer_pos: terminator.pos,
            x: progress.x,
            y: progress.y,
            width: terminator.cell.width.max(0.0).round() as i64,
            height: row_height.max(terminator.cell.height).max(1.0).round() as i64,
            row: progress.row,
            col: progress.col,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_display_point(
        &mut self,
        buffer_pos: LispCharPos1,
        glyph_x: f32,
        glyph_y: f32,
        width: f32,
        height: f32,
        row: i64,
        col: usize,
    ) {
        self.note_display_buffer_pos(buffer_pos);
        self.note_query_y(glyph_y);
        self.note_query_metrics(height, 0.0);
        self.points.push(DisplayPointSnapshot {
            role: neovm_core::window::DisplayPointRole::Glyph,
            buffer_pos,
            x: (glyph_x - self.text_x).round() as i64,
            y: (glyph_y - self.window_top).round() as i64,
            width: width.max(0.0).round() as i64,
            height: height.max(1.0).round() as i64,
            row,
            col: col as i64,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_text_display_point(
        &mut self,
        buffer_pos: LispCharPos1,
        glyph_x: f32,
        glyph_y: f32,
        width: f32,
        height: f32,
        row: usize,
        col: usize,
    ) {
        self.push_display_point(
            buffer_pos,
            glyph_x,
            glyph_y,
            width,
            height,
            self.text_row_base + row as i64,
            col,
        );
    }

    /// [`Self::push_overlaid_marker_point`] for a BODY row, whose row index is
    /// relative to the window's first text row.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_text_overlaid_marker_point(
        &mut self,
        buffer_pos: LispCharPos1,
        glyph_x: f32,
        glyph_y: f32,
        width: f32,
        height: f32,
        row: usize,
        col: usize,
    ) {
        self.push_overlaid_marker_point(
            buffer_pos,
            glyph_x,
            glyph_y,
            width,
            height,
            self.text_row_base + row as i64,
            col,
        );
    }

    /// Publish a visible insertion boundary that has row geometry but no
    /// source glyph of its own, such as end-of-buffer.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_text_insertion_boundary(
        &mut self,
        buffer_pos: LispCharPos1,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        row: usize,
        col: usize,
    ) {
        self.push_text_display_point(buffer_pos, x, y, width, height, row, col);
        if neovm_core::window::posn_object_extent_mode().enabled()
            && let Some(point) = self.points.last_mut()
        {
            point.role = neovm_core::window::DisplayPointRole::InsertionBoundary;
        }
    }

    pub(super) fn current_display_text_row_index(&self) -> usize {
        self.current_row_progress
            .and_then(|progress| progress.display_row_index)
            .expect("text row must have display row progress before finishing")
    }

    pub(super) fn current_output_row(&self) -> Option<i64> {
        self.current_row_progress.map(|progress| progress.row)
    }

    pub(super) fn note_truncated_end(&mut self, end: LispCharPos1) {
        self.truncated_end_buffer_pos = Some(end);
    }

    pub(super) fn push_text_row(&mut self, row_y_start: f32, row_height: f32, row_ascent: f32) {
        self.note_query_y(row_y_start);
        self.note_query_metrics(row_height, row_ascent);
        if self.rows.len() >= 256 {
            self.query_translation_exact = self.query_translation_exact.map(|_| false);
        }
        let row_progress = self
            .current_row_progress
            .take()
            .expect("text row must have live output progress before finishing");
        // GNU's `display_line` gives the row its own end before the row is
        // handed on (`it->eol_pos`, then `find_row_edges`); doing it here means
        // the slot is part of closing a row rather than a step a caller can
        // forget. The push must precede the `take()`s below, which clear the
        // row's first/last display positions.
        self.publish_row_terminator_slot(&row_progress, row_height);
        if let Some(point_rows) = &mut self.point_rows {
            if !self.points.is_empty() {
                let index = point_rows.rows.len();
                point_rows
                    .rows
                    .push(DisplayPointRow::from_points(std::mem::take(
                        &mut self.points,
                    )));
                self.point_lookup_groups
                    .push(PointLookupGroup::FreshRow { index });
            }
        }
        self.rows.push(DisplayRowSnapshot {
            row: row_progress.row,
            y: row_progress.y,
            height: row_height.max(1.0).round() as i64,
            start_x: row_progress.start_x,
            start_col: row_progress.start_col,
            end_x: row_progress.x,
            end_col: row_progress.col,
            start_buffer_pos: self.current_row_first_display_pos.take(),
            end_buffer_pos: self.current_row_last_display_pos.take(),
            end_source: std::mem::take(&mut self.current_row_end_source),
            truncated_end_buffer_pos: self.truncated_end_buffer_pos.take(),
            // Fringe bitmaps are stamped onto the matrix row after the walk
            // that pushes this snapshot row, so they are filled in later from
            // the finished matrix (`fringe_snapshot::publish_row_fringe_bitmaps`).
            fringe: Default::default(),
        });
        self.row_metrics.push(RowMetricsSnapshot::new(
            row_progress
                .display_row_index
                .expect("text row must have display row progress before recording metrics"),
            row_progress.row.max(0) as usize,
            row_y_start,
            row_height.max(1.0),
            row_ascent.max(0.0).min(row_height.max(1.0)),
        ));
    }

    fn push_chrome_row(&mut self, row: DisplayRowSnapshot) {
        self.rows.push(row);
    }

    /// Seed the chrome rows a SKIPPED chrome walk would have pushed. Same
    /// destination as [`Self::push_chrome_row`], so `finish_snapshot` (which
    /// sorts by row index) cannot tell a reused chrome row from a walked one.
    pub(super) fn push_reused_chrome(&mut self, rows: Vec<DisplayRowSnapshot>) {
        self.rows.extend(rows);
    }

    pub(super) fn push_chrome_row_progress(&mut self, progress: DisplayRowOutputProgress) {
        let row_progress = self
            .current_row_progress
            .take()
            .expect("chrome row must have live output progress before finishing");
        self.push_chrome_row(DisplayRowSnapshot {
            row: row_progress.row,
            y: row_progress.y,
            height: progress.height().round() as i64,
            start_x: row_progress.start_x,
            start_col: row_progress.start_col,
            end_x: row_progress.x,
            end_col: row_progress.col,
            start_buffer_pos: None,
            end_buffer_pos: None,
            end_source: DisplayRowEndSource::Buffer,
            truncated_end_buffer_pos: None,
            fringe: Default::default(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_output::DisplayRowTerminatorCell;

    #[test]
    fn unfinished_row_moves_to_worker_with_rewind_and_terminator_state() {
        fn require_send_sync<T: Send + Sync + 'static>() {}
        require_send_sync::<WindowRowGeometry>();
        let mut geometry = WindowRowGeometry::new(2, 8.0, 12.0);
        geometry.begin_current_row_progress(Some(0), 2, 0, 8, 0);
        geometry.note_row_walk_start(LispCharPos1::new(1));
        geometry.push_text_display_point(LispCharPos1::new(2), 16.0, 20.0, 8.0, 16.0, 0, 1);
        geometry.update_current_row_progress(2, 2, 8, 16);
        let checkpoint = geometry.display_point_len();
        let (first, last) = geometry.current_row_display_positions();
        // A rejected wrap candidate must not survive the handoff.
        geometry.push_text_display_point(LispCharPos1::new(99), 24.0, 20.0, 8.0, 16.0, 0, 2);
        geometry.truncate_display_points(checkpoint);
        geometry.restore_current_row_display_positions(first, last);
        geometry.note_row_terminator(DisplayRowTerminator::new(
            LispCharPos1::new(3),
            DisplayRowTerminatorCell::new(8.0, 16.0),
        ));
        geometry.note_display_string_wrap(LispCharPos1::new(3));
        let prepared = std::thread::spawn(move || {
            geometry.push_text_row(20.0, 16.0, 12.0);
            assert_eq!(geometry.row_metrics()[0].height(), 16.0);
            assert_eq!(geometry.row_metrics()[0].ascent(), 12.0);
            geometry.finish(8)
        })
        .join()
        .unwrap();
        assert_eq!(prepared.rows.len(), 1);
        let row = &prepared.rows[0];
        assert_eq!(row.start_buffer_pos, Some(LispCharPos1::new(1)));
        assert_eq!(row.end_buffer_pos, Some(LispCharPos1::new(3)));
        assert_eq!(row.end_source, DisplayRowEndSource::DisplayStringWrap);
        assert_eq!((row.row, row.y, row.end_x, row.end_col), (2, 8, 16, 2));
        let points = prepared
            .point_rows
            .as_ref()
            .map(|rows| rows.iter_points().collect::<Vec<_>>())
            .unwrap_or_else(|| prepared.points.clone());
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].buffer_pos, LispCharPos1::new(2));
        assert_eq!(points[1].buffer_pos, LispCharPos1::new(3));
        assert_eq!((points[1].x, points[1].width), (16, 8));
        assert_eq!(prepared.body_rows.len(), 1);
        assert_eq!(prepared.body_rows[0].body_row, 0);
        assert_eq!(prepared.body_rows[0].body_y, 0);
    }
}

#[cfg(test)]
#[path = "row_geometry/lookup_test.rs"]
mod lookup_test;
