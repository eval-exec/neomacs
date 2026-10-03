//! Mutable output window state owned while layout builds a frame snapshot.

use crate::display_cursor::{CursorVisualColumnResolutionContext, CursorVisualColumnRows};
use crate::display_row::finalizer::GlyphRowFinalizationContext;
use crate::output::row_request::{
    DisplayCurrentRowMutation, DisplayWindowRowMutation, DisplayWindowRowsMutation,
    OutputCompleteRowInstallRequest, OutputCurrentRowDecorationRequest, OutputRowBeginRequest,
    OutputRowLifecycleRequest, OutputRowMetricsRequest,
};
use crate::output::window_request::{OutputWindowLifecycleRequest, OutputWindowRowCapacity};
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::frame_glyphs::PhysCursor;
use neomacs_display_protocol::glyph_matrix::MatrixRow;
use neomacs_display_protocol::glyph_matrix::{GlyphMatrix, GlyphRow, WindowMatrixEntry};
use neomacs_display_protocol::types::{DisplayWindowId, Rect};

/// Owned by one exclusive frame-build attempt, never shared across mutators.
/// Numeric publication boundary and inactive-window geometry saved before an
/// attempt begins. An active grid belongs solely to the failed attempt.
pub(crate) struct OutputSourceWindowCheckpoint {
    windows_len: usize,
    current_window_id: u64,
    current_pixel_bounds: Rect,
    current_text_pixel_bounds: Rect,
    current_text_clip_bounds: Rect,
    current_selected: bool,
    current_row: usize,
}

pub(crate) struct OutputWindowBuildState {
    windows: Vec<OutputWindowGridEntry>,
    current_row_grid: Option<OutputWindowRowGrid>,
    current_window_id: u64,
    current_pixel_bounds: Rect,
    current_text_pixel_bounds: Rect,
    current_text_clip_bounds: Rect,
    current_selected: bool,
    current_row: usize,
}

impl OutputWindowBuildState {
    pub(crate) fn new() -> Self {
        Self {
            windows: Vec::new(),
            current_row_grid: None,
            current_window_id: 0,
            current_pixel_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            current_text_pixel_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            current_text_clip_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            current_selected: false,
            current_row: 0,
        }
    }

    #[inline]
    pub(crate) fn capture_source_attempt_checkpoint(&self) -> OutputSourceWindowCheckpoint {
        assert!(
            self.current_row_grid.is_none(),
            "a source attempt must begin between completed windows"
        );
        OutputSourceWindowCheckpoint {
            windows_len: self.windows.len(),
            current_window_id: self.current_window_id,
            current_pixel_bounds: self.current_pixel_bounds,
            current_text_pixel_bounds: self.current_text_pixel_bounds,
            current_text_clip_bounds: self.current_text_clip_bounds,
            current_selected: self.current_selected,
            current_row: self.current_row,
        }
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn restore_source_attempt_checkpoint(
        &mut self,
        checkpoint: OutputSourceWindowCheckpoint,
    ) {
        assert_eq!(
            self.windows.len(),
            checkpoint.windows_len,
            "source horizon retry must precede completed window publication"
        );
        self.current_row_grid = None;
        self.current_window_id = checkpoint.current_window_id;
        self.current_pixel_bounds = checkpoint.current_pixel_bounds;
        self.current_text_pixel_bounds = checkpoint.current_text_pixel_bounds;
        self.current_text_clip_bounds = checkpoint.current_text_clip_bounds;
        self.current_selected = checkpoint.current_selected;
        self.current_row = checkpoint.current_row;
    }

    pub(crate) fn reset(&mut self) {
        self.windows.clear();
        self.current_row_grid = None;
        self.current_window_id = 0;
        self.current_selected = false;
        self.current_row = 0;
    }

    pub(crate) fn install_window_lifecycle(&mut self, request: OutputWindowLifecycleRequest) {
        match request {
            OutputWindowLifecycleRequest::Begin(begin) => {
                self.current_row_grid = Some(
                    OutputWindowRowGrid::new(begin.nrows, begin.ncols)
                        .with_row_capacity(begin.row_capacity),
                );
                self.current_window_id = begin.window_id;
                self.current_pixel_bounds = begin.pixel_bounds;
                self.current_text_pixel_bounds = begin.text_pixel_bounds;
                self.current_text_clip_bounds = begin.text_clip_bounds;
                self.current_selected = begin.selected;
                self.current_row = 0;
            }
            OutputWindowLifecycleRequest::End => {
                if let Some(grid) = self.current_row_grid.take() {
                    self.windows.push(OutputWindowGridEntry::new(
                        self.current_window_id,
                        grid,
                        self.current_pixel_bounds,
                        self.current_text_pixel_bounds,
                        self.current_text_clip_bounds,
                        self.current_selected,
                    ));
                }
            }
        }
    }

    pub(crate) fn install_row_lifecycle(
        &mut self,
        request: OutputRowLifecycleRequest,
        phys_cursor: Option<&mut PhysCursor>,
    ) {
        match request {
            OutputRowLifecycleRequest::Begin(begin) => self.begin_current_row(begin),
            OutputRowLifecycleRequest::AbandonEmptyBegin { row } => {
                if let Some(grid) = self.current_row_grid.as_mut() {
                    grid.abandon_empty_begin(row);
                }
            }
            OutputRowLifecycleRequest::Complete(complete) => {
                self.install_complete_row(complete, phys_cursor);
            }
            OutputRowLifecycleRequest::Metrics { row, metrics } => {
                self.write_row_metrics_at(row, metrics);
            }
            OutputRowLifecycleRequest::Finalize { row } => {
                self.finalize_output_row(row, phys_cursor)
            }
            OutputRowLifecycleRequest::Cursor { row, col, style } => {
                self.write_row_cursor(row, col, style);
            }
            OutputRowLifecycleRequest::CurrentDecoration(decoration) => {
                self.decorate_current_row(decoration);
            }
        }
    }

    fn edit_current_row<R>(&mut self, f: impl FnOnce(&mut GlyphRow) -> R) -> Option<R> {
        let grid = self.current_row_grid.as_mut()?;
        let row = grid.row_mut(self.current_row)?;
        Some(f(row))
    }

    pub(crate) fn current_row_for_render(&self) -> Option<&GlyphRow> {
        self.current_row_grid.as_ref()?.row(self.current_row)
    }

    pub(crate) fn apply_current_row_mutation<M>(&mut self, mutation: M) -> Option<M::Output>
    where
        M: DisplayCurrentRowMutation,
    {
        self.edit_current_row(|row| mutation.apply(row))
    }

    pub(crate) fn apply_current_window_row_mutation<M>(
        &mut self,
        row_idx: usize,
        mutation: M,
    ) -> Option<M::Output>
    where
        M: DisplayWindowRowMutation,
    {
        self.current_row_grid
            .as_mut()?
            .edit_row_with_matrix_cols(row_idx, |row, matrix_cols| mutation.apply(row, matrix_cols))
    }

    pub(crate) fn apply_last_window_rows_mutation<M>(&mut self, mut mutation: M)
    where
        M: DisplayWindowRowsMutation,
    {
        if let Some(entry) = self.windows.last_mut() {
            entry.edit_rows_with_matrix_cols(|row, matrix_cols| {
                mutation.apply(row, matrix_cols);
            });
        }
    }

    #[cfg(test)]
    pub(crate) fn current_row_index(&self) -> usize {
        self.current_row
    }

    pub(crate) fn current_window_id_i64(&self) -> i64 {
        self.current_window_id as i64
    }

    pub(crate) fn current_window_pixel_bounds(&self) -> Rect {
        self.current_pixel_bounds
    }

    pub(crate) fn current_window_text_pixel_bounds(&self) -> Rect {
        self.current_text_pixel_bounds
    }

    pub(crate) fn cursor_visual_column_context(&self) -> CursorVisualColumnResolutionContext<'_> {
        CursorVisualColumnResolutionContext::new(
            self.current_window_id,
            self.current_row_grid
                .as_ref()
                .map(OutputWindowRowGrid::cursor_rows),
        )
    }

    fn write_row_cursor(
        &mut self,
        row: usize,
        col: u16,
        style: neomacs_display_protocol::frame_glyphs::CursorStyle,
    ) {
        if let Some(grid) = self.current_row_grid.as_mut() {
            grid.write_row_cursor(row, col, style);
        }
    }

    pub(crate) fn window_content_height_px(
        &self,
        window_id: i64,
        fallback_row_height: f32,
    ) -> Option<f32> {
        self.windows
            .iter()
            .find(|window| window.window_id == window_id as u64)
            .map(|window| window.grid.content_height_px(fallback_row_height))
    }

    pub(crate) fn mini_measurement_height_px(
        &self,
        window_id: i64,
        fallback_row_height: f32,
    ) -> Option<f32> {
        self.windows
            .iter()
            .find(|window| window.window_id == window_id as u64)
            .map(|window| window.grid.mini_measurement_height_px(fallback_row_height))
    }

    #[cfg(test)]
    pub(crate) fn completed_window_count(&self) -> usize {
        self.windows.len()
    }

    #[cfg(test)]
    pub(crate) fn completed_window_id(&self, index: usize) -> Option<u64> {
        self.windows
            .get(index)
            .map(OutputWindowGridEntry::window_id)
    }

    pub(crate) fn into_window_matrix_entries(self) -> Vec<WindowMatrixEntry> {
        self.windows
            .into_iter()
            .map(OutputWindowGridEntry::into_window_matrix_entry)
            .collect()
    }

    fn decorate_current_row(&mut self, decoration: OutputCurrentRowDecorationRequest) {
        let _ = self.edit_current_row(|row| match decoration {
            OutputCurrentRowDecorationRequest::MarkTruncatedLeft => {
                row.truncated_left = true;
            }
        });
    }

    fn write_row_metrics_at(&mut self, row: usize, metrics: OutputRowMetricsRequest) {
        if let Some(grid) = self.current_row_grid.as_mut() {
            grid.write_row_metrics(row, metrics);
        }
    }

    fn replace_current_row(&mut self, source: GlyphRow) {
        let current_row = self.current_row;
        if let Some(grid) = self.current_row_grid.as_mut() {
            grid.replace_row(current_row, source);
        }
    }

    /// Install a complete, already bidi-finalized row into the current window
    /// grid verbatim (Phase 1 cursor-only replay). See
    /// [`OutputWindowRowGrid::install_finalized_row`].
    pub(crate) fn install_finalized_window_row(&mut self, row: usize, source: MatrixRow) {
        if let Some(grid) = self.current_row_grid.as_mut() {
            grid.install_finalized_row(row, source);
        }
    }

    /// Find the current window row that owns the cursor at `charpos` (Phase 2
    /// scroll/edit cursor re-decorate).
    pub(crate) fn find_current_window_cursor_row(&self, charpos: usize) -> Option<usize> {
        self.current_row_grid
            .as_ref()?
            .find_cursor_row_for_charpos(charpos)
    }

    /// Whether an enabled text row of the current window grid covers
    /// `charpos` and lies wholly above the window-relative `bottom_y`.
    pub(crate) fn current_window_shows_charpos(&self, charpos: usize, bottom_y: f32) -> bool {
        self.current_row_grid.as_ref().is_some_and(|grid| {
            grid.matrix.rows.iter().any(|row| {
                row.enabled
                    && row.role == GlyphRowRole::Text
                    && row.start_charpos <= charpos
                    && charpos <= row.end_charpos
                    && row.pixel_y + row.height_px <= bottom_y + 0.5
            })
        })
    }

    /// Read a row from the current window grid.
    pub(crate) fn current_window_row(&self, row: usize) -> Option<&GlyphRow> {
        self.current_row_grid.as_ref()?.row(row)
    }

    pub(crate) fn current_row_is_finalized(&self, row: usize) -> bool {
        self.current_row_grid
            .as_ref()
            .and_then(|grid| grid.finalized_rows.get(row))
            .copied()
            .unwrap_or(false)
    }

    fn begin_current_row(&mut self, begin: OutputRowBeginRequest) {
        self.current_row = begin.row;
        if let Some(grid) = self.current_row_grid.as_mut() {
            grid.begin_row(begin);
        }
    }

    fn install_complete_row(
        &mut self,
        request: OutputCompleteRowInstallRequest,
        phys_cursor: Option<&mut PhysCursor>,
    ) {
        let row = request.row_index();
        self.begin_current_row(request.begin_request());
        self.replace_current_row(request.into_glyph_row());
        self.finalize_output_row(row, phys_cursor);
    }

    fn finalize_output_row(&mut self, row: usize, phys_cursor: Option<&mut PhysCursor>) {
        if let Some(grid) = self.current_row_grid.as_mut() {
            grid.finalize_row(
                self.current_window_id,
                row,
                self.current_pixel_bounds,
                phys_cursor,
            );
        }
    }
}

// Output row grid storage for text/window rows.
//
// This region owns the low-level `GlyphMatrix` row storage used by the output
// builder. Callers install already-built rows and metadata; the grid is
// responsible only for placing, finalizing, and exporting matrix rows.

pub(crate) struct OutputWindowRowGrid {
    matrix: GlyphMatrix,
    /// Per-row flag: has this matrix row already been bidi-finalized (reordered
    /// to visual order) at install? Bidi reordering must happen exactly once per
    /// row. Some buffer-text install flows issue a redundant `Finalize` for the
    /// last row (the row-end path finalizes it, then `finish_output_rows`
    /// finalizes the trailing row again); this flag makes that second
    /// `Finalize` a true no-op instead of a double-reorder. Cleared whenever a
    /// row's contents are (re)installed via `begin_row` / `replace_row`.
    finalized_rows: Vec<bool>,
    row_capacity: OutputWindowRowCapacity,
}

/// Why an enabled buffer row owns point.
///
/// GNU `row_containing_pos` scans rows in display order and a row owns CHARPOS
/// when `START_CHARPOS <= charpos < END_CHARPOS` (src/xdisp.c:22476-22483),
/// where `END_CHARPOS` is "the position after the last glyph on this row ... In
/// an up-to-date display, this should always be equal to the start position of
/// the next row" (src/dispextern.h:974-981).  A position exactly at that shared
/// end therefore belongs to the FOLLOWING row, unless `row_for_charpos_p` sees
/// this row ending at visible ZV.
///
/// Neomacs stamps a row's bounds from its buffer glyphs
/// (`apply_display_row_source_slot_bounds`), so only a row that WRAPS ends where
/// the next one starts.  A row that ends its line stops one position short:
/// its terminator newline is owned by the row's end-of-line slot rather than by
/// a glyph, so `end_charpos` IS the newline's own position -- the position the
/// cursor goes to at end of line -- and the next row starts one past it.  That
/// is what tells the two endpoint cases apart, and reading them as one dropped
/// every end-of-line cursor position out of every row: a scrolling replay's
/// cursor lookup then found no row at all, and the frame was sealed without the
/// physical cursor (`C-l` after an isearch).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CursorRowBufferMatch {
    DisplayedInterior,
    VisibleBufferEnd,
}

impl CursorRowBufferMatch {
    /// Whether ROW owns CHARPOS, given the row BELOW it in the display.
    fn classify(row: &GlyphRow, row_below_start: Option<usize>, charpos: usize) -> Option<Self> {
        if !row.enabled
            || row.role != GlyphRowRole::Text
            || charpos < row.start_charpos
            || charpos > row.end_charpos
        {
            return None;
        }
        if charpos < row.end_charpos {
            return row.displays_text.then_some(Self::DisplayedInterior);
        }
        // CHARPOS is the row's end position.  GNU keeps it here when the row
        // ends at visible ZV (`row_for_charpos_p`), hands it to the row below
        // when the two share it -- the up-to-date-display invariant above --
        // and otherwise it is this row's own end-of-line position.
        if row.ends_at_zv {
            return Some(Self::VisibleBufferEnd);
        }
        if row_below_start == Some(row.end_charpos) {
            return None;
        }
        row.displays_text.then_some(Self::DisplayedInterior)
    }
}

pub(crate) struct OutputWindowGridEntry {
    window_id: u64,
    grid: OutputWindowRowGrid,
    pixel_bounds: Rect,
    text_pixel_bounds: Rect,
    text_clip_bounds: Rect,
    selected: bool,
}

impl OutputWindowRowGrid {
    pub(crate) fn new(nrows: usize, ncols: usize) -> Self {
        Self {
            matrix: GlyphMatrix::new(nrows, ncols),
            finalized_rows: vec![false; nrows],
            row_capacity: OutputWindowRowCapacity::Fixed,
        }
    }

    fn with_row_capacity(mut self, capacity: OutputWindowRowCapacity) -> Self {
        self.row_capacity = capacity;
        self
    }

    fn clear_finalized(&mut self, row: usize) {
        if let Some(flag) = self.finalized_rows.get_mut(row) {
            *flag = false;
        }
    }

    /// Mark `row` finalized; returns true if it had NOT been finalized yet
    /// (i.e. this is the reorder that should run).
    fn take_unfinalized(&mut self, row: usize) -> bool {
        match self.finalized_rows.get_mut(row) {
            Some(flag) if !*flag => {
                *flag = true;
                true
            }
            // Out of range rows fall through to the finalizer (which itself
            // bounds-checks); never been tracked, so treat as unfinalized.
            None => true,
            _ => false,
        }
    }

    fn into_matrix(self) -> GlyphMatrix {
        self.matrix
    }

    fn ensure_hashes(&mut self) {
        self.matrix.ensure_hashes();
    }

    /// GNU removes the final row's spacing after move_it_to(ZV).
    fn mini_measurement_height_px(&self, fallback_row_height: f32) -> f32 {
        let last = self
            .matrix
            .rows
            .iter()
            .rev()
            .find(|row| row.enabled && row.height_px > 0.0);
        last.map_or_else(
            || self.content_height_px(fallback_row_height),
            |row| row.pixel_y + row.height_without_line_spacing(),
        )
    }

    pub(crate) fn content_height_px(&self, fallback_row_height: f32) -> f32 {
        let measured_bottom = self
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.height_px > 0.0)
            .map(|row| row.pixel_y + row.height_px)
            .fold(0.0_f32, f32::max);

        if measured_bottom > 0.0 {
            measured_bottom
        } else {
            self.matrix.rows.iter().filter(|row| row.enabled).count() as f32 * fallback_row_height
        }
    }

    pub(crate) fn cursor_rows(&self) -> CursorVisualColumnRows<'_> {
        CursorVisualColumnRows::new(&self.matrix.rows)
    }

    pub(crate) fn row(&self, row: usize) -> Option<&GlyphRow> {
        self.matrix.rows.get(row).map(|row| row.as_ref())
    }

    /// Find the enabled buffer row that owns `charpos` for cursor placement.
    ///
    /// Return the first enabled row that owns `charpos` in display order.
    ///
    /// This is GNU `row_containing_pos`'s decisive ordering: an earlier real
    /// span wins immediately, while an end position is admitted only when that
    /// row ends at visible ZV -- or is the row's own end-of-line position.  No
    /// second pass can let a later synthetic row steal an already-resolved
    /// point.
    pub(crate) fn find_cursor_row_for_charpos(&self, charpos: usize) -> Option<usize> {
        self.matrix
            .rows
            .iter()
            .enumerate()
            .find_map(|(row_index, row)| {
                let row_below_start = self
                    .matrix
                    .rows
                    .get(row_index + 1)
                    .map(|row_below| row_below.start_charpos);
                match CursorRowBufferMatch::classify(row, row_below_start, charpos) {
                    Some(
                        CursorRowBufferMatch::DisplayedInterior
                        | CursorRowBufferMatch::VisibleBufferEnd,
                    ) => Some(row_index),
                    None => None,
                }
            })
    }

    pub(crate) fn row_mut(&mut self, row: usize) -> Option<&mut GlyphRow> {
        // Copy-on-write: rows under construction are uniquely owned, so this
        // is an in-place borrow; a shared (reused) row is cloned on first
        // mutation only.
        self.matrix.rows.get_mut(row).map(MatrixRow::make_mut)
    }

    pub(crate) fn edit_row_with_matrix_cols<R>(
        &mut self,
        row: usize,
        f: impl FnOnce(&mut GlyphRow, usize) -> R,
    ) -> Option<R> {
        let ncols = self.matrix.ncols;
        let row = self.row_mut(row)?;
        Some(f(row, ncols))
    }

    fn edit_rows_with_matrix_cols(&mut self, mut f: impl FnMut(&mut GlyphRow, usize)) {
        let ncols = self.matrix.ncols;
        for row in &mut self.matrix.rows {
            f(MatrixRow::make_mut(row), ncols);
        }
    }

    pub(crate) fn write_row_metrics(&mut self, row: usize, metrics: OutputRowMetricsRequest) {
        let Some(row) = self.row_mut(row) else {
            return;
        };
        metrics.apply_to_row(row);
    }

    pub(crate) fn write_row_cursor(
        &mut self,
        row: usize,
        col: u16,
        style: neomacs_display_protocol::frame_glyphs::CursorStyle,
    ) {
        let Some(row) = self.row_mut(row) else {
            return;
        };
        row.cursor_col = Some(col);
        row.cursor_type = Some(style);
    }

    pub(crate) fn replace_row(&mut self, row: usize, source: GlyphRow) {
        self.clear_finalized(row);
        let Some(slot) = self.matrix.rows.get_mut(row) else {
            return;
        };
        // A freshly built row replaces the slot wholesale; assigning the Arc
        // avoids a make_mut copy of whatever shared row was there before.
        *slot = MatrixRow::new(source);
    }

    /// Install a complete, ALREADY bidi-finalized (visual-order) row verbatim
    /// and mark it finalized so a later [`Self::finalize_row`] is a true no-op.
    ///
    /// The cursor-only fast path (Phase 1) replays rows captured from a prior
    /// clean pass — those glyphs were reordered to visual order at install, so
    /// re-finalizing them would double-reverse an RTL row. Unlike `replace_row`
    /// (which clears the finalized flag for the normal install→finalize flow),
    /// this sets it.
    pub(crate) fn install_finalized_row(&mut self, row: usize, source: MatrixRow) {
        // Store the shared row directly — verbatim replay never copies.
        let Some(slot) = self.matrix.rows.get_mut(row) else {
            return;
        };
        *slot = source;
        if let Some(flag) = self.finalized_rows.get_mut(row) {
            *flag = true;
        }
    }

    pub(crate) fn begin_row(&mut self, begin: OutputRowBeginRequest) {
        if begin.row >= self.matrix.nrows && self.row_capacity == OutputWindowRowCapacity::Growing {
            let rows = begin.row.saturating_add(1);
            self.matrix.resize(rows, self.matrix.ncols);
            self.finalized_rows.resize(rows, false);
        }
        self.clear_finalized(begin.row);
        let Some(row) = self.row_mut(begin.row) else {
            return;
        };
        begin.apply_to_row(row);
    }

    fn abandon_empty_begin(&mut self, row: usize) {
        let Some(slot) = self.matrix.rows.get_mut(row) else {
            return;
        };
        let begun = slot.as_ref();
        let is_uncommitted = begun.enabled
            && begun.height_px == 0.0
            && begun.ascent_px == 0.0
            && begun.glyphs.iter().all(Vec::is_empty)
            && begun.cursor_col.is_none()
            && begun.cursor_type.is_none();
        if !is_uncommitted {
            return;
        }
        let mut disabled = GlyphRow::new(GlyphRowRole::Text);
        disabled.enabled = false;
        *slot = MatrixRow::new(disabled);
        self.clear_finalized(row);
    }

    pub(crate) fn finalize_row(
        &mut self,
        window_id: u64,
        row: usize,
        pixel_bounds: Rect,
        phys_cursor: Option<&mut PhysCursor>,
    ) {
        // Reorder exactly once per row. A redundant trailing-row `Finalize`
        // (see `finalized_rows`) is a true no-op, not a second reorder.
        if !self.take_unfinalized(row) {
            return;
        }
        let matrix_ncols = self.matrix.ncols;
        let Some(matrix_row) = self.row_mut(row) else {
            return;
        };
        GlyphRowFinalizationContext::new(window_id, row, pixel_bounds).finalize_row(
            matrix_row,
            matrix_ncols,
            phys_cursor,
        );
    }
}

impl OutputWindowGridEntry {
    pub(crate) fn new(
        window_id: u64,
        grid: OutputWindowRowGrid,
        pixel_bounds: Rect,
        text_pixel_bounds: Rect,
        text_clip_bounds: Rect,
        selected: bool,
    ) -> Self {
        Self {
            window_id,
            grid,
            pixel_bounds,
            text_pixel_bounds,
            text_clip_bounds,
            selected,
        }
    }

    pub(crate) fn edit_rows_with_matrix_cols(&mut self, f: impl FnMut(&mut GlyphRow, usize)) {
        self.grid.edit_rows_with_matrix_cols(f);
    }

    #[cfg(test)]
    pub(crate) fn window_id(&self) -> u64 {
        self.window_id
    }

    pub(crate) fn into_window_matrix_entry(mut self) -> WindowMatrixEntry {
        self.grid.ensure_hashes();
        WindowMatrixEntry {
            // Single u64→DisplayWindowId conversion point for the published
            // matrix entry; the output pipeline's internal ids stay u64.
            window_id: DisplayWindowId::new(self.window_id as i64),
            matrix: self.grid.into_matrix(),
            pixel_bounds: self.pixel_bounds,
            text_pixel_bounds: self.text_pixel_bounds,
            text_clip_bounds: Some(self.text_clip_bounds),
            selected: self.selected,
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "window_state/tests/mini_preparation.rs"]
mod mini_preparation_tests;
