//! Live window-output emission helpers for Rust redisplay.
//!
//! This layer bridges Rust layout/status-line emission to GNU-like live window
//! output state. It advances live output through explicit output-cursor moves
//! while simultaneously recording immutable row snapshots for renderer
//! handoff.

use super::display_status_line::{
    ChromeRowRenderServices, DisplayRowOutputProgress, WindowChromeRowsRenderOutcome,
    WindowChromeRowsRenderRequest, WindowChromeRowsRenderState,
};
use crate::coords::layout_i64_char_pos_to_lisp_char_pos;
use crate::display_current_row_output::DisplayRowCurrentRowOutput;
use crate::display_cursor::{
    CursorSlotResolutionState, CursorVisualColumnResolutionRequest, ResolvedCursorCoordinatePair,
};
use crate::display_rendered_row_output_install::{
    install_measured_window_display_row, install_rendered_display_row_fragment_assets,
};
#[cfg(test)]
use crate::display_row::builder::DisplayRowAppendProgress;
use crate::display_row::builder::{DisplayRowGlyphCheckpoint, DisplayRowPosition};
use crate::display_row::geometry::{
    DisplayRowGeometryState, DisplayRowLimit, DisplayRowYPositions,
};
use crate::display_row::measured_state::MeasuredDisplayRow;
use crate::display_row::special_glyphs::{
    TextWindowRightEdgeMarkers, install_text_window_right_edge_markers,
};
use crate::display_row::text_output::{TextOutputSpan, TextRowOutput};
use crate::display_row::walk_state::HitRowRangeTracker;
use crate::display_text_output_install::{
    DisplayOutputRowStoredMetrics, DisplayOutputTextRowMetricsInstallRequest,
    DisplayOutputTextWindowBeginInstallRequest, TextWindowRowDecorationRequest,
    install_output_resolved_face,
};
use crate::hit_test::HitRow;
use crate::neovm_bridge::ResolvedFace;
use crate::output::builder::DisplayOutputBuilder;
use crate::output::install_request::{
    OutputCursorInstallRequest, OutputFrameArtifactInstallRequest, OutputFrameStateInstallRequest,
    OutputRetryCheckpointRestoreRequest, OutputTextWindowDisplayRangeInstallRequest,
};
use crate::output::row_request::{OutputCurrentRowDecorationRequest, OutputRowLifecycleRequest};
use crate::output::window_request::OutputWindowLifecycleRequest;
use crate::types::LayoutCharPos0;
use crate::window_layout::WindowChromeMetrics;
use neomacs_display_protocol::effect_config::EffectsConfig;
use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::frame_glyphs::{CursorStyle, DisplaySlotId, PhysCursor};
use neomacs_display_protocol::glyph_matrix::CursorItemRole;
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{Color, DisplayWindowId, Rect};
use neovm_core::buffer::{CharPos0, EmacsBytePos, LispCharPos1, TextPositionAnchor};
use neovm_core::emacs_core::Context;
use neovm_core::window::geometry::CellOrigin;
use neovm_core::window::{
    DisplayPointSnapshot, DisplayRowSnapshot, MatrixRow0, PresentedWindowChromeArea,
    PresentedWindowChromeString, PresentedWindowRegions, WindowCursorKind, WindowCursorPos,
    WindowCursorSnapshot, WindowDisplaySnapshot,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct RowMetricsSnapshot {
    display_row_index: usize,
    row: usize,
    pixel_y: f32,
    height: f32,
    ascent: f32,
}

impl RowMetricsSnapshot {
    pub(crate) fn new(
        display_row_index: usize,
        row: usize,
        pixel_y: f32,
        height: f32,
        ascent: f32,
    ) -> Self {
        Self {
            display_row_index,
            row,
            pixel_y,
            height,
            ascent,
        }
    }

    pub(crate) fn row(self) -> usize {
        self.row
    }

    pub(crate) fn pixel_y(self) -> f32 {
        self.pixel_y
    }

    pub(crate) fn height(self) -> f32 {
        self.height
    }

    pub(crate) fn ascent(self) -> f32 {
        self.ascent
    }
}

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

/// The cell a row's terminator slot is measured with.
///
/// The terminator draws no glyph of its own, so the width and height of the
/// posn it answers come from the face active at the line end -- GNU's
/// `append_space_for_newline` (src/xdisp.c:24122) appends a space in exactly
/// that face for exactly this reason, "so that there is always one glyph at
/// the end of a glyph row that the cursor can be set on".
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowTerminatorCell {
    width: f32,
    height: f32,
}

impl DisplayRowTerminatorCell {
    pub(crate) fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// A row's own end: the buffer position that owns every screen column past the
/// row's last glyph, and the cell its slot is measured with.
///
/// GNU records this inside `display_line` as `it->eol_pos = it->current.pos`
/// (src/xdisp.c:26541) at the moment `ITERATOR_AT_END_OF_LINE_P` becomes true,
/// and `find_row_edges` turns it into the row's `maxpos`
/// (src/xdisp.c:25342-25344). Measured against GNU Emacs 31.0.90, an
/// 80-column pty and the buffer "abcdef\nghijkl\n": every column from 6 to 79
/// of row 0 answers buffer position 7, the newline ending line 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowTerminator {
    pos: LispCharPos1,
    cell: DisplayRowTerminatorCell,
}

impl DisplayRowTerminator {
    pub(crate) fn new(pos: LispCharPos1, cell: DisplayRowTerminatorCell) -> Self {
        Self { pos, cell }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChromeRowOutput {
    row: i64,
    y: f32,
}

impl ChromeRowOutput {
    pub(crate) fn new(row: i64, y: f32) -> Self {
        Self { row, y }
    }

    pub(crate) fn row(self) -> i64 {
        self.row
    }

    pub(crate) fn y(self) -> f32 {
        self.y
    }

    /// Re-anchor this chrome row's Y once its measured height is known (the
    /// bottom-anchored mode line moves up when it measures taller than the
    /// reserved estimate).
    pub(crate) fn with_y(self, y: f32) -> Self {
        Self { y, ..self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChromeRowProgress {
    output: ChromeRowOutput,
    progress: DisplayRowOutputProgress,
}

impl ChromeRowProgress {
    pub(crate) fn new(output: ChromeRowOutput, progress: DisplayRowOutputProgress) -> Self {
        Self { output, progress }
    }

    pub(crate) fn output(self) -> ChromeRowOutput {
        self.output
    }

    pub(crate) fn progress(self) -> DisplayRowOutputProgress {
        self.progress
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayTextRowMetrics {
    pub(crate) y: f32,
    pub(crate) height: f32,
    pub(crate) ascent: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayTextRowBegin {
    pub(crate) display_row_index: usize,
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) y: f32,
    pub(crate) x: f32,
    /// Buffer position where this row's walk begins; stamped onto the row's
    /// start/end charpos at BEGIN so every buffer-text row carries real
    /// bounds from birth (GNU MATRIX_ROW_START_CHARPOS comes from the
    /// iterator at display_line entry). Empty lines and the EOB placeholder
    /// therefore never expose a (0, 0) sentinel.
    pub(crate) start_charpos: LayoutCharPos0,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextWindowBegin {
    pub(crate) window_id: u64,
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) bounds: Rect,
    pub(crate) text_bounds: Rect,
    pub(crate) text_clip_bounds: Rect,
    pub(crate) selected: bool,
    pub(crate) first_row: DisplayTextRowBegin,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextWindowOutputBegin {
    pub(crate) window_id: u64,
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) bounds: Rect,
    pub(crate) text_bounds: Rect,
    pub(crate) text_clip_bounds: Rect,
    pub(crate) selected: bool,
}

impl From<TextWindowBegin> for TextWindowOutputBegin {
    fn from(request: TextWindowBegin) -> Self {
        Self {
            window_id: request.window_id,
            rows: request.rows,
            cols: request.cols,
            bounds: request.bounds,
            text_bounds: request.text_bounds,
            text_clip_bounds: request.text_clip_bounds,
            selected: request.selected,
        }
    }
}

impl TextWindowOutputBegin {
    fn into_output_install_request(self) -> DisplayOutputTextWindowBeginInstallRequest {
        DisplayOutputTextWindowBeginInstallRequest::new(
            self.window_id,
            self.rows,
            self.cols,
            self.bounds,
            self.text_bounds,
            self.text_clip_bounds,
            self.selected,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextWindowDisplayRange {
    pub(crate) window_id: u64,
    pub(crate) window_start: LispCharPos1,
    pub(crate) window_end: LispCharPos1,
}

pub(crate) struct TextWindowPendingRowFinish<'a> {
    pub(crate) row_geometry: &'a DisplayRowGeometryState,
    /// True when the walk stopped because the buffer source is exhausted
    /// (charpos reached ZV) rather than because the window filled. The row
    /// that reached ZV gets `ends_at_zv` (GNU row->ends_at_zv_p), whether it
    /// displays trailing text or is the empty EOB placeholder.
    pub(crate) source_exhausted: bool,
    pub(crate) row_limit: DisplayRowLimit,
    pub(crate) row_y_positions: &'a DisplayRowYPositions,
    pub(crate) text_y: f32,
    pub(crate) char_height: f32,
    pub(crate) charpos: i64,
    pub(crate) hit_row_range: &'a mut HitRowRangeTracker,
    pub(crate) hit_rows: &'a mut Vec<HitRow>,
}

pub(crate) struct TextWindowOutputTarget<'a> {
    output_builder: &'a mut DisplayOutputBuilder,
}

impl<'a> TextWindowOutputTarget<'a> {
    pub(crate) fn from_builder(output_builder: &'a mut DisplayOutputBuilder) -> Self {
        Self { output_builder }
    }

    pub(crate) fn reborrow(&mut self) -> TextWindowOutputTarget<'_> {
        TextWindowOutputTarget {
            output_builder: self.output_builder,
        }
    }

    pub(crate) fn builder(&mut self) -> &mut DisplayOutputBuilder {
        self.output_builder
    }

    pub(crate) fn current_row_output(&mut self) -> DisplayRowCurrentRowOutput<'_> {
        DisplayRowCurrentRowOutput::from_output_builder(self.builder())
    }

    /// Capture the current output row's glyph counts so a later word-wrap break
    /// can truncate the row back to a word boundary. Returns the default
    /// (zero-length) checkpoint when no row is open; such a checkpoint is never
    /// applied (it belongs to an unavailable word-wrap candidate).
    pub(crate) fn capture_current_row_glyph_checkpoint(&self) -> DisplayRowGlyphCheckpoint {
        self.output_builder
            .current_row_for_render()
            .map(DisplayRowGlyphCheckpoint::capture)
            .unwrap_or_default()
    }

    pub(crate) fn install_resolved_face(
        &mut self,
        face_id: FaceId,
        face: &ResolvedFace,
        metrics: Option<crate::font::metrics::FontMetrics>,
    ) {
        install_output_resolved_face(self.builder(), face_id, face, metrics);
    }

    pub(crate) fn install_rendered_fragment_assets(&mut self, faces: &[Face]) {
        install_rendered_display_row_fragment_assets(self.builder(), faces);
    }

    pub(crate) fn install_measured_window_display_row(&mut self, measured: &MeasuredDisplayRow) {
        install_measured_window_display_row(self.builder(), measured);
    }
}

pub(crate) fn begin_text_window_output(
    mut output: TextWindowOutputTarget<'_>,
    request: TextWindowOutputBegin,
) {
    request
        .into_output_install_request()
        .install(output.builder());
}

pub(crate) fn record_text_window_display_range(
    mut output: TextWindowOutputTarget<'_>,
    range: TextWindowDisplayRange,
) {
    output
        .builder()
        .install_window_metadata(OutputTextWindowDisplayRangeInstallRequest::new(
            DisplayWindowId::new(range.window_id as i64),
            range.window_start.as_i64(),
            range.window_end.as_i64(),
        ));
}

fn record_text_window_redisplay_positions(
    output: TextWindowOutputTarget<'_>,
    window_id: u64,
    positions: TextWindowRedisplayPositions,
) {
    record_text_window_display_range(output, positions.display_range(window_id));
}

fn install_text_window_row_decoration(
    output_builder: &mut DisplayOutputBuilder,
    request: TextWindowRowDecorationRequest,
) {
    match request {
        TextWindowRowDecorationRequest::MarkCurrentTruncatedLeft => {
            output_builder.install_output_row_lifecycle(
                OutputRowLifecycleRequest::current_decoration(
                    OutputCurrentRowDecorationRequest::MarkTruncatedLeft,
                ),
            );
        }
    }
}

pub(crate) fn install_text_window_row_decoration_request(
    mut output: TextWindowOutputTarget<'_>,
    request: TextWindowRowDecorationRequest,
) {
    install_text_window_row_decoration(output.builder(), request);
}

fn begin_display_text_row(
    output_builder: &mut DisplayOutputBuilder,
    begin: DisplayTextRowBegin,
) -> usize {
    output_builder.install_output_row_lifecycle(OutputRowLifecycleRequest::begin_text_at(
        begin.display_row_index,
        begin.start_charpos,
    ));
    begin.display_row_index
}

fn finish_display_text_row(
    output_builder: &mut DisplayOutputBuilder,
    display_row_index: usize,
    metrics: DisplayTextRowMetrics,
) -> DisplayTextRowFinish {
    let matrix_metrics =
        display_text_row_metrics_request(display_row_index, metrics).install(output_builder);
    DisplayTextRowFinish {
        display_row_index,
        metrics: matrix_metrics,
    }
}

fn finalize_display_text_row(output_builder: &mut DisplayOutputBuilder, display_row_index: usize) {
    output_builder
        .install_output_row_lifecycle(OutputRowLifecycleRequest::finalize(display_row_index));
}

pub(crate) fn begin_text_window_row(
    mut output: TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    evaluator: &mut Context,
    begin: DisplayTextRowBegin,
) -> usize {
    let display_row_index = begin_display_text_row(output.builder(), begin);
    output_emitter.begin_display_text_row(
        evaluator,
        begin.display_row_index,
        begin.row,
        begin.col,
        begin.y,
        begin.x,
    );
    display_row_index
}

pub(crate) fn finish_text_window_row(
    mut output: TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    metrics: DisplayTextRowMetrics,
) -> DisplayTextRowFinish {
    let display_row_index = output_emitter.current_display_text_row_index();
    let finish = finish_display_text_row(output.builder(), display_row_index, metrics);
    output_emitter.push_text_row(metrics.y, metrics.height, metrics.ascent);
    finish
}

pub(crate) fn finish_and_end_text_window_row(
    mut output: TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    metrics: DisplayTextRowMetrics,
) -> DisplayTextRowFinish {
    let display_row_index = output_emitter.current_display_text_row_index();
    let request = display_text_row_metrics_request(display_row_index, metrics);
    let output_builder = output.builder();
    let matrix_metrics = request.install(output_builder);
    output_builder.install_output_row_lifecycle(OutputRowLifecycleRequest::finalize(
        request.display_row_index(),
    ));
    output_emitter.push_text_row(metrics.y, metrics.height, metrics.ascent);
    DisplayTextRowFinish {
        display_row_index,
        metrics: matrix_metrics,
    }
}

pub(crate) fn transition_text_window_row(
    mut output: TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    evaluator: &mut Context,
    transition: DisplayTextRowGeometryTransition,
) -> DisplayTextRowTransition {
    finish_and_end_text_window_row(output.reborrow(), output_emitter, transition.finished_row);
    begin_text_window_row(
        output.reborrow(),
        output_emitter,
        evaluator,
        transition.begin_row,
    );
    DisplayTextRowTransition::BeganNextRow
}

pub(crate) fn transition_text_window_row_with_limit(
    mut output: TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    evaluator: &mut Context,
    transition: DisplayTextRowGeometryTransition,
    max_rows: usize,
) -> DisplayTextRowTransition {
    if transition.begin_row.row >= max_rows {
        finish_and_end_text_window_row(output.reborrow(), output_emitter, transition.finished_row);
        return DisplayTextRowTransition::ExhaustedRows;
    }
    transition_text_window_row(output.reborrow(), output_emitter, evaluator, transition)
}

pub(crate) fn begin_text_window_output_and_row(
    mut output: TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    evaluator: &mut Context,
    request: TextWindowBegin,
) {
    let first_row = request.first_row;
    begin_text_window_output(output.reborrow(), request.into());
    begin_text_window_row(output.reborrow(), output_emitter, evaluator, first_row);
}

pub(crate) fn finish_pending_text_window_row(
    mut output: TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    request: TextWindowPendingRowFinish<'_>,
) -> bool {
    // The current row is the one whose walk hit the end of the source. When
    // the source is exhausted (ZV reached), GNU sets row->ends_at_zv_p on it —
    // both on a final text row and on the empty EOB placeholder, which the
    // guard below leaves unfinished yet enabled. Mark it before the guard.
    //
    // Only within the row limit: a row-BOUNDED walk (the below-reuse edit
    // replay relaying just the edited line) can consume the final newline and
    // reach ZV while the limit suppressed beginning the placeholder row — the
    // grid's current row is then the finalized CONTENT row, which a full
    // rebuild does not tag (the reused placeholder below it already carries
    // the flag). Same for a full walk whose window is exactly filled: the
    // ZV row was never begun, so no visible row reports it.
    if request.source_exhausted && request.row_geometry.is_within_row_limit(request.row_limit) {
        output.current_row_output().mark_text_row_ends_at_zv();
    }

    let has_pending_row_output = output_emitter.current_row_has_output();
    let within_row_limit = request.row_geometry.is_within_row_limit(request.row_limit);
    let should_finish = request
        .hit_row_range
        .should_finish_current_row(request.charpos, has_pending_row_output);
    if !within_row_limit || !should_finish {
        // Row begin precedes the visible-loop guard so a changed concrete font
        // height can leave one speculative row just below the pixel limit.
        // GNU never publishes that iterator scratch row. Preserve the special
        // enabled EOB placeholder, but roll back an ordinary empty begin. The
        // typed lifecycle request itself refuses to erase committed content.
        if within_row_limit && !request.source_exhausted && !has_pending_row_output {
            output.builder().install_output_row_lifecycle(
                OutputRowLifecycleRequest::abandon_empty_begin(
                    output_emitter.current_display_text_row_index(),
                ),
            );
        }
        return false;
    }

    let row_y_start = request.row_geometry.current_row_y(
        request.row_y_positions,
        request.text_y,
        request.char_height,
    );
    let row_cursor = request.row_geometry.with_row_y(row_y_start).cursor();
    request
        .hit_rows
        .push(row_cursor.hit_row(request.hit_row_range.start(), request.charpos));
    finish_text_window_row(output, output_emitter, row_cursor.finish_current_row());
    true
}

pub(crate) fn render_window_chrome_rows(
    output: TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    evaluator: &mut Context,
    request: WindowChromeRowsRenderRequest<'_, '_>,
    render_services: ChromeRowRenderServices<'_, '_>,
) -> WindowChromeRowsRenderOutcome {
    request.render(&mut WindowChromeRowsRenderState::new(
        output,
        output_emitter,
        evaluator,
        render_services,
    ))
}

/// Re-install a window's RETAINED chrome instead of walking it — the chrome
/// half of GNU's one-line optimization (xdisp.c:17572-17726, which reaches the
/// update phase without ever entering `redisplay_window` and so never calls
/// `display_mode_lines`).
///
/// GNU has nothing to re-install because its current matrix simply keeps the
/// mode line it already had; neomacs re-emits every row into a fresh frame, so
/// "not regenerated" has to be spelled as "installed from the retained
/// matrix". The output is the previous accepted frame's own chrome rows, which
/// is what makes this a skip rather than a cache: no key, no comparison, and
/// the miss path (walking) is untouched.
///
/// Mirrors the three effects of the chrome walk. The glyph rows go into the
/// grid already finalized; the row snapshots go back into the emitter, because
/// they are what populates `WindowDisplaySnapshot.rows`; and the retained
/// MEASURED heights are returned, so `window-mode-line-height` keeps reporting
/// the real height rather than the face-only estimate. Face publication is the
/// fourth effect and is handled upstream, by Phase A admitting the chrome rows'
/// face IDs alongside the body's (`CursorOnlyReplay::retained_face_ids`).
pub(crate) fn install_retained_window_chrome(
    mut output: TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    chrome: &crate::incremental_layout::RetainedChrome,
) -> WindowChromeMetrics {
    for (row_index, row) in &chrome.rows {
        // Verbatim install is a refcount bump, not a per-glyph deep copy.
        output.builder().install_finalized_output_row(
            *row_index,
            neomacs_display_protocol::glyph_matrix::MatrixRow::clone(row),
        );
    }
    output_emitter.push_reused_chrome(chrome.row_snapshots.clone());
    output_emitter.reuse_chrome_strings(chrome.chrome_strings.clone());
    chrome.metrics
}

pub(crate) fn close_text_window_output(mut output: TextWindowOutputTarget<'_>) {
    output
        .builder()
        .install_output_window_lifecycle(OutputWindowLifecycleRequest::end());
}

pub(crate) fn install_text_window_finished_rows(
    mut output: TextWindowOutputTarget<'_>,
    output_emitter: &WindowOutputEmitter,
) {
    finish_output_rows(output.builder(), output_emitter);
}

pub(crate) fn capture_text_window_retry_checkpoint(
    mut output: TextWindowOutputTarget<'_>,
) -> TextWindowOutputRetryCheckpoint {
    let output_builder = output.builder();
    TextWindowOutputRetryCheckpoint {
        transition_hints_len: output_builder.transition_hints().len(),
        effect_hints_len: output_builder.effect_hints().len(),
    }
}

pub(crate) fn restore_text_window_retry_checkpoint(
    mut output: TextWindowOutputTarget<'_>,
    checkpoint: TextWindowOutputRetryCheckpoint,
) {
    output
        .builder()
        .install_window_metadata(OutputRetryCheckpointRestoreRequest::new(
            checkpoint.transition_hints_len,
            checkpoint.effect_hints_len,
        ));
}

fn display_text_row_metrics_request(
    display_row_index: usize,
    metrics: DisplayTextRowMetrics,
) -> DisplayOutputTextRowMetricsInstallRequest {
    DisplayOutputTextRowMetricsInstallRequest::new(
        display_row_index,
        metrics.y,
        metrics.height,
        metrics.ascent,
    )
}

fn finish_output_rows(
    output_builder: &mut DisplayOutputBuilder,
    output_emitter: &WindowOutputEmitter,
) {
    if let Some(metric) = output_emitter.row_metrics().last() {
        finalize_display_text_row(output_builder, metric.display_row_index);
    }
}

pub(crate) struct TextWindowBodyOutputInstall<'a> {
    pub(crate) window_id: u64,
    pub(crate) window_start: i64,
    pub(crate) text_start_byte: usize,
    pub(crate) byte_idx: usize,
    pub(crate) right_edge_markers: Option<TextWindowRightEdgeMarkers<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextWindowRedisplayPositions {
    window_start: LispCharPos1,
    window_end: TextWindowEndPosition,
}

/// The char/byte/matrix-row identity of one row walk's terminal position.
///
/// Fast paths may replace this value only as a whole, preventing a corrected
/// character coordinate from retaining the byte companion of another walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextWindowEndPosition {
    anchor: TextPositionAnchor,
    matrix_row: MatrixRow0,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextWindowCursor {
    pub(crate) role: TextWindowCursorRole,
    pub(crate) window_id: i64,
    pub(crate) charpos: usize,
    pub(crate) slots: TextWindowCursorSlots,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) ascent: f32,
    pub(crate) style: CursorStyle,
    pub(crate) color: Color,
    pub(crate) cursor_fg: Color,
    pub(crate) text_area_left: f32,
    pub(crate) window_top: f32,
    /// Integer/grid x (relative to the text area) to publish instead of rounding
    /// the sub-pixel `x`. Set for a cursor at a `display`-replacement slot so the
    /// snapshot x is derived from the preceding glyph's already-rounded display
    /// point (`x + width`), staying byte-identical to the glyph edge across font
    /// sizes. `None` rounds `x` as before. Affects only the integer snapshot, not
    /// the sub-pixel `x` the GUI renderer draws the caret at.
    pub(crate) grid_x_override: Option<i64>,
}

/// The cursor's two column identities at the window-output boundary.
///
/// `output` is GNU's live-window/output coordinate. `display` is the
/// materialized glyph-matrix slot consumed by renderer artifacts.  Keeping a
/// resolved pair in one enum variant prevents a fast path from overwriting the
/// output identity while it maps through a line-number gutter or truncation
/// marker. An unresolved full-walk capture cannot pretend to have a display
/// slot until [`TextWindowCursor::resolve`] consults the completed row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TextWindowCursorSlots {
    Unresolved { output: DisplaySlotId },
    Resolved(ResolvedCursorCoordinatePair),
}

impl TextWindowCursorSlots {
    pub(crate) const fn from_capture(
        slot: DisplaySlotId,
        state: CursorSlotResolutionState,
    ) -> Self {
        match state {
            CursorSlotResolutionState::Unresolved => Self::Unresolved { output: slot },
            CursorSlotResolutionState::Resolved => {
                Self::Resolved(ResolvedCursorCoordinatePair::same(slot))
            }
        }
    }

    pub(crate) const fn resolved(coordinates: ResolvedCursorCoordinatePair) -> Self {
        Self::Resolved(coordinates)
    }
}

/// Whether a text-window cursor is the frame's active cursor or an inactive
/// window's cursor.
///
/// This role may choose presentation and storage, but never placement. GNU
/// redisplay first installs one `phys_cursor` position for every window in
/// `set_cursor_from_row`; `get_window_cursor_type` changes only how that
/// position is drawn for the selected or non-selected window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextWindowCursorRole {
    Active,
    Inactive,
}

impl TextWindowCursorRole {
    pub(crate) const fn from_selected(selected: bool) -> Self {
        if selected {
            Self::Active
        } else {
            Self::Inactive
        }
    }
}

/// A cursor whose semantic position has been mapped to the one materialized
/// glyph slot used by every renderer-facing artifact.
///
/// The captured cursor remains alongside that slot because the evaluator's live
/// window snapshot has a different contract: its x/column stay in the window's
/// output coordinate space. Horizontal truncation is the decisive case: the
/// caret is at output column 0 while point's first surviving buffer glyph is at
/// materialized slot 1. Naming both spaces prevents an implicit conversion.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ResolvedTextWindowCursor {
    captured: TextWindowCursor,
    coordinates: ResolvedCursorCoordinatePair,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextWindowDecorativeCursor {
    pub(crate) window_id: i64,
    pub(crate) slot_id: DisplaySlotId,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) style: CursorStyle,
    pub(crate) color: Color,
    pub(crate) cursor_fg: Color,
    pub(crate) effects: Option<EffectsConfig>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextWindowCursorEffects {
    pub(crate) window_id: i64,
    pub(crate) effects: EffectsConfig,
}

impl TextWindowCursor {
    fn resolve(self, output_builder: &DisplayOutputBuilder) -> ResolvedTextWindowCursor {
        let coordinates = match self.slots {
            TextWindowCursorSlots::Resolved(coordinates) => coordinates,
            TextWindowCursorSlots::Unresolved { output } => {
                let display = CursorVisualColumnResolutionRequest::new(
                    self.window_id,
                    output.row as usize,
                    self.charpos,
                )
                .resolve_cursor_coordinates(output_builder.cursor_visual_column_context())
                .map_or(output, ResolvedCursorCoordinatePair::display_slot_id);
                ResolvedCursorCoordinatePair::from_slots(output, display)
                    .unwrap_or_else(|| ResolvedCursorCoordinatePair::same(output))
            }
        };

        ResolvedTextWindowCursor {
            captured: self,
            coordinates,
        }
    }
}

impl ResolvedTextWindowCursor {
    fn row(self) -> usize {
        self.coordinates.display_slot_id().row as usize
    }

    fn col(self) -> u16 {
        self.coordinates.display_col()
    }

    fn window_snapshot(self) -> WindowCursorSnapshot {
        WindowCursorSnapshot {
            kind: window_cursor_kind(self.captured.style),
            x: self
                .captured
                .grid_x_override
                .unwrap_or_else(|| (self.captured.x - self.captured.text_area_left).round() as i64),
            y: (self.captured.y - self.captured.window_top).round() as i64,
            width: self.captured.width.round() as i64,
            height: self.captured.height.round() as i64,
            ascent: self.captured.ascent.round() as i64,
            row: self.coordinates.output_slot_id().row as i64,
            col: i64::from(self.coordinates.output_col()),
        }
    }

    fn phys_cursor(self) -> PhysCursor {
        PhysCursor {
            window_id: DisplayWindowId::new(self.captured.window_id),
            charpos: self.captured.charpos,
            row: self.row(),
            col: self.col(),
            slot_id: self.coordinates.display_slot_id(),
            x: self.captured.x,
            y: self.captured.y,
            width: self.captured.width,
            height: self.captured.height,
            ascent: self.captured.ascent,
            style: self.captured.style,
            color: self.captured.color,
            cursor_fg: self.captured.cursor_fg,
        }
    }

    fn artifact(self) -> TextWindowCursorArtifact {
        TextWindowCursorArtifact {
            window_id: self.captured.window_id,
            role: CursorItemRole::WindowCaret {
                charpos: self.captured.charpos,
            },
            slot_id: self.coordinates.display_slot_id(),
            x: self.captured.x,
            y: self.captured.y,
            width: self.captured.width,
            height: self.captured.height,
            ascent: self.captured.ascent,
            style: self.captured.style,
            color: self.captured.color,
            cursor_fg: self.captured.cursor_fg,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayTextRowGeometryTransition {
    pub(crate) finished_row: DisplayTextRowMetrics,
    pub(crate) begin_row: DisplayTextRowBegin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayTextRowTransition {
    BeganNextRow,
    ExhaustedRows,
}

impl DisplayTextRowTransition {
    pub(crate) fn is_exhausted(self) -> bool {
        matches!(self, Self::ExhaustedRows)
    }
}

fn window_cursor_kind(style: CursorStyle) -> WindowCursorKind {
    match style {
        CursorStyle::FilledBox => WindowCursorKind::FilledBox,
        CursorStyle::Hollow => WindowCursorKind::HollowBox,
        CursorStyle::Bar(_) => WindowCursorKind::Bar,
        CursorStyle::Hbar(_) => WindowCursorKind::Hbar,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayTextRowFinish {
    pub(crate) display_row_index: usize,
    pub(crate) metrics: DisplayTextRowStoredMetrics,
}

pub(crate) type DisplayTextRowStoredMetrics = DisplayOutputRowStoredMetrics;

impl TextWindowRedisplayPositions {
    pub(crate) fn from_output_rows(
        output_emitter: &WindowOutputEmitter,
        window_start: i64,
        text_start_byte: usize,
        byte_idx: usize,
    ) -> Self {
        let window_start = layout_i64_char_pos_to_lisp_char_pos(window_start);
        let window_end_char = output_emitter
            .rows()
            .iter()
            .rev()
            .find_map(|row| row.end_buffer_pos)
            .map(|pos| layout_i64_char_pos_to_lisp_char_pos(pos.as_i64()))
            .unwrap_or_else(|| LispCharPos1::from_one_based_usize(1));
        let window_end_row = output_emitter
            .rows()
            .last()
            .map(|row| row.row.max(0) as usize)
            .unwrap_or(0);

        Self {
            window_start,
            window_end: TextWindowEndPosition {
                anchor: TextPositionAnchor::new(
                    CharPos0::from_lisp(window_end_char),
                    EmacsBytePos::new(text_start_byte.saturating_add(byte_idx)),
                ),
                matrix_row: MatrixRow0::new(window_end_row),
            },
        }
    }

    pub(crate) const fn window_start(self) -> LispCharPos1 {
        self.window_start
    }

    pub(crate) fn window_end_lisp(self) -> LispCharPos1 {
        self.window_end.anchor.char_pos().to_lisp()
    }

    pub(crate) const fn window_end_position(self) -> TextWindowEndPosition {
        self.window_end
    }

    pub(crate) fn replace_window_end_anchor(&mut self, anchor: TextPositionAnchor) {
        self.window_end.anchor = anchor;
    }

    pub(crate) fn display_range(self, window_id: u64) -> TextWindowDisplayRange {
        TextWindowDisplayRange {
            window_id,
            window_start: self.window_start,
            window_end: self.window_end_lisp(),
        }
    }
}

impl TextWindowEndPosition {
    pub(crate) const fn anchor(self) -> TextPositionAnchor {
        self.anchor
    }

    pub(crate) const fn matrix_row(self) -> MatrixRow0 {
        self.matrix_row
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextWindowOutputRetryCheckpoint {
    pub(crate) transition_hints_len: usize,
    pub(crate) effect_hints_len: usize,
}

pub(crate) fn install_text_window_cursor_effects(
    mut output: TextWindowOutputTarget<'_>,
    request: TextWindowCursorEffects,
) {
    output
        .builder()
        .install_output_frame_state(OutputFrameStateInstallRequest::cursor_effects(
            DisplayWindowId::new(request.window_id),
            request.effects,
        ));
}

pub(crate) fn publish_text_window_decorative_cursor(
    mut output: TextWindowOutputTarget<'_>,
    cursor: TextWindowDecorativeCursor,
) {
    if let Some(effects) = cursor.effects {
        install_text_window_cursor_effects(
            output.reborrow(),
            TextWindowCursorEffects {
                window_id: cursor.window_id,
                effects,
            },
        );
    }
    install_text_window_cursor_artifact(
        output.builder(),
        TextWindowCursorArtifact {
            window_id: cursor.window_id,
            role: CursorItemRole::Decorative,
            slot_id: cursor.slot_id,
            x: cursor.x,
            y: cursor.y,
            width: cursor.width,
            height: cursor.height,
            // Decorative cursors are positioned directly by `y` (no text
            // baseline), so they keep ascent 0.
            ascent: 0.0,
            style: cursor.style,
            color: cursor.color,
            cursor_fg: cursor.cursor_fg,
        },
    );
}

fn install_text_window_cursor_artifact(
    output_builder: &mut DisplayOutputBuilder,
    cursor: TextWindowCursorArtifact,
) {
    output_builder.install_output_cursor(OutputCursorInstallRequest::new(
        DisplayWindowId::new(cursor.window_id),
        cursor.role,
        cursor.slot_id,
        cursor.x,
        cursor.y,
        cursor.width,
        cursor.height,
        cursor.ascent,
        cursor.style,
        cursor.color,
        cursor.cursor_fg,
    ));
}

fn store_text_window_phys_cursor(output_builder: &mut DisplayOutputBuilder, cursor: PhysCursor) {
    output_builder
        .install_output_frame_artifact(OutputFrameArtifactInstallRequest::phys_cursor(cursor));
}

fn install_text_window_row_cursor(
    output_builder: &mut DisplayOutputBuilder,
    row: usize,
    row_col: u16,
    style: CursorStyle,
) {
    output_builder
        .install_output_row_lifecycle(OutputRowLifecycleRequest::cursor(row, row_col, style));
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextWindowCursorPublication {
    presentation: TextWindowCursorPresentation,
    row: usize,
    row_col: u16,
    style: CursorStyle,
    live_cursor: WindowCursorSnapshot,
}

#[derive(Clone, Debug, PartialEq)]
enum TextWindowCursorPresentation {
    Active(PhysCursor),
    Inactive(TextWindowCursorArtifact),
}

#[derive(Clone, Debug, PartialEq)]
struct TextWindowCursorArtifact {
    window_id: i64,
    role: CursorItemRole,
    slot_id: DisplaySlotId,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    ascent: f32,
    style: CursorStyle,
    color: Color,
    cursor_fg: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextWindowCursorPublicationOutcome {
    pub(crate) installed_cursor_artifact: bool,
    pub(crate) stored_phys_cursor: bool,
    pub(crate) row: usize,
    pub(crate) row_col: u16,
    pub(crate) live_cursor: WindowCursorSnapshot,
}

impl TextWindowCursorPublication {
    fn resolve(output_builder: &DisplayOutputBuilder, cursor: TextWindowCursor) -> Self {
        let cursor = cursor.resolve(output_builder);
        let presentation = match cursor.captured.role {
            TextWindowCursorRole::Active => {
                TextWindowCursorPresentation::Active(cursor.phys_cursor())
            }
            TextWindowCursorRole::Inactive => {
                TextWindowCursorPresentation::Inactive(cursor.artifact())
            }
        };

        Self {
            presentation,
            row: cursor.row(),
            row_col: cursor.col(),
            style: cursor.captured.style,
            live_cursor: cursor.window_snapshot(),
        }
    }

    fn publish(
        self,
        mut output: TextWindowOutputTarget<'_>,
        output_emitter: &mut WindowOutputEmitter,
    ) -> TextWindowCursorPublicationOutcome {
        let (installed_cursor_artifact, stored_phys_cursor) = match self.presentation {
            TextWindowCursorPresentation::Active(cursor) => {
                store_text_window_phys_cursor(output.builder(), cursor);
                (false, true)
            }
            TextWindowCursorPresentation::Inactive(cursor) => {
                install_text_window_cursor_artifact(output.builder(), cursor);
                (true, false)
            }
        };
        install_text_window_row_cursor(output.builder(), self.row, self.row_col, self.style);
        output_emitter.set_phys_cursor(self.live_cursor.clone());
        TextWindowCursorPublicationOutcome {
            installed_cursor_artifact,
            stored_phys_cursor,
            row: self.row,
            row_col: self.row_col,
            live_cursor: self.live_cursor,
        }
    }
}

pub(crate) fn publish_text_window_cursor(
    mut output: TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    cursor: TextWindowCursor,
) -> TextWindowCursorPublicationOutcome {
    let publication = TextWindowCursorPublication::resolve(output.builder(), cursor);
    publication.publish(output, output_emitter)
}

pub(crate) fn install_text_window_body_output(
    mut output: TextWindowOutputTarget<'_>,
    output_emitter: &WindowOutputEmitter,
    request: TextWindowBodyOutputInstall<'_>,
    render_services: Option<ChromeRowRenderServices<'_, '_>>,
) -> TextWindowRedisplayPositions {
    let redisplay_positions = TextWindowRedisplayPositions::from_output_rows(
        output_emitter,
        request.window_start,
        request.text_start_byte,
        request.byte_idx,
    );
    record_text_window_redisplay_positions(
        output.reborrow(),
        request.window_id,
        redisplay_positions,
    );
    install_text_window_finished_rows(output.reborrow(), output_emitter);
    if let Some(markers) = request.right_edge_markers {
        let render_services =
            render_services.expect("right-edge markers require chrome render services");
        install_text_window_right_edge_markers(output.builder(), render_services, markers);
    }
    redisplay_positions
}

pub(crate) trait DisplayProgressSink {
    #[cfg(test)]
    fn emit_text_progress(
        &mut self,
        evaluator: &mut Context,
        output: TextRowOutput,
        progress: &DisplayRowAppendProgress,
    );

    fn emit_chrome_progress(&mut self, evaluator: &mut Context, progress: ChromeRowProgress);
}

pub(crate) struct WindowOutputEmitter {
    /// Whether output-cursor updates are mirrored into the live evaluator
    /// window while this emitter is being built. Production frame layout is
    /// speculative and keeps this false; focused lifecycle tests can use the
    /// live mode to exercise GNU-shaped output-cursor operations directly.
    publish_live: bool,
    frame_id: neovm_core::window::FrameId,
    window_id: neovm_core::window::WindowId,
    text_row_base: i64,
    text_x: f32,
    window_top: f32,
    logical_cursor: Option<WindowCursorPos>,
    phys_cursor: Option<WindowCursorSnapshot>,
    points: Vec<DisplayPointSnapshot>,
    rows: Vec<DisplayRowSnapshot>,
    chrome_strings: Vec<PresentedWindowChromeString>,
    row_metrics: Vec<RowMetricsSnapshot>,
    current_row_first_display_pos: Option<LispCharPos1>,
    current_row_last_display_pos: Option<LispCharPos1>,
    /// The end this row was closed at, when it is a position that draws no
    /// glyph of its own. Recorded rather than published on the spot so that
    /// closing the row is the only thing that can publish it.
    current_row_terminator: Option<DisplayRowTerminator>,
    current_row_progress: Option<CurrentRowProgress>,
}

impl DisplayProgressSink for WindowOutputEmitter {
    #[cfg(test)]
    fn emit_text_progress(
        &mut self,
        evaluator: &mut Context,
        output: TextRowOutput,
        progress: &DisplayRowAppendProgress,
    ) {
        self.emit_text_output_spans(
            evaluator,
            output,
            output.spans_for_source_slots(progress.slots()),
            progress.end(),
        );
    }

    fn emit_chrome_progress(&mut self, evaluator: &mut Context, progress: ChromeRowProgress) {
        let output = progress.output();
        self.begin_chrome_row(evaluator, output.row(), output.y());
        self.move_chrome_output_to(evaluator, output.row(), progress.progress());
        self.push_chrome_row_progress(progress.progress());
    }
}

impl WindowOutputEmitter {
    pub(crate) fn emit_text_output_spans(
        &mut self,
        evaluator: &mut Context,
        output: TextRowOutput,
        spans: Vec<TextOutputSpan>,
        end: DisplayRowPosition,
    ) {
        if spans.is_empty() {
            self.move_text_output_to(
                evaluator,
                output.row(),
                end.col(),
                output.row_y(),
                end.x_px(),
            );
            return;
        }
        for span in spans {
            self.emit_text_output_span(evaluator, span);
        }
    }

    #[cfg(test)]
    pub(crate) fn new(
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
        text_row_base: usize,
        text_x: f32,
        window_top: f32,
    ) -> Self {
        Self::new_with_publication_mode(
            frame_id,
            window_id,
            text_row_base,
            text_x,
            window_top,
            true,
        )
    }

    pub(crate) fn new_speculative(
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
        text_row_base: usize,
        text_x: f32,
        window_top: f32,
    ) -> Self {
        Self::new_with_publication_mode(
            frame_id,
            window_id,
            text_row_base,
            text_x,
            window_top,
            false,
        )
    }

    fn new_with_publication_mode(
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
        text_row_base: usize,
        text_x: f32,
        window_top: f32,
        publish_live: bool,
    ) -> Self {
        Self {
            publish_live,
            frame_id,
            window_id,
            text_row_base: text_row_base as i64,
            text_x,
            window_top,
            logical_cursor: None,
            phys_cursor: None,
            points: Vec::new(),
            rows: Vec::new(),
            chrome_strings: Vec::new(),
            row_metrics: Vec::new(),
            current_row_first_display_pos: None,
            current_row_last_display_pos: None,
            current_row_terminator: None,
            current_row_progress: None,
        }
    }

    /// Seed the body half of this emitter from a prior clean pass (Phase 1
    /// cursor-only replay), in place of walking the buffer. `rows` are the
    /// retained body [`DisplayRowSnapshot`]s and `points` the retained per-span
    /// display points — both are point-INDEPENDENT (they describe where glyphs
    /// render, not where the cursor is), so they replay verbatim. Chrome rows
    /// are appended afterward by the normal chrome path, and the cursor is set
    /// separately for the moved point.
    pub(crate) fn seed_cursor_only_body(
        &mut self,
        rows: Vec<DisplayRowSnapshot>,
        points: Vec<DisplayPointSnapshot>,
    ) {
        self.rows = rows;
        self.points = points;
    }

    /// Append reused (Phase 2 scroll) body rows + points to the emitter, on top
    /// of the newly-exposed rows the partial walk produced. `finish_snapshot`
    /// sorts rows by index and points by buffer position, so insertion order does
    /// not matter. No `row_metrics` are added (reused grid rows are installed
    /// already-finalized, so the exposed-row finalize pass must not touch them).
    pub(crate) fn push_reused_body(
        &mut self,
        rows: Vec<DisplayRowSnapshot>,
        points: Vec<DisplayPointSnapshot>,
    ) {
        self.rows.extend(rows);
        self.points.extend(points);
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
    pub(crate) fn normalize_body_start_cols(&mut self) {
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

    pub(crate) fn display_point_len(&self) -> usize {
        self.points.len()
    }

    pub(crate) fn truncate_display_points(&mut self, len: usize) {
        self.points.truncate(len);
    }

    pub(crate) fn rows(&self) -> &[DisplayRowSnapshot] {
        &self.rows
    }

    pub(crate) fn point_for_buffer_pos(&self, pos: LispCharPos1) -> Option<&DisplayPointSnapshot> {
        self.points.iter().find(|point| point.buffer_pos == pos)
    }

    pub(crate) fn point_for_lisp_buffer_pos(
        &self,
        pos: LispCharPos1,
    ) -> Option<&DisplayPointSnapshot> {
        self.point_for_buffer_pos(pos)
    }

    pub(crate) fn row_metrics(&self) -> &[RowMetricsSnapshot] {
        &self.row_metrics
    }

    pub(crate) fn current_row_display_positions(
        &self,
    ) -> (Option<LispCharPos1>, Option<LispCharPos1>) {
        (
            self.current_row_first_display_pos,
            self.current_row_last_display_pos,
        )
    }

    pub(crate) fn restore_current_row_display_positions(
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

    pub(crate) fn current_row_has_output(&self) -> bool {
        self.current_row_progress.as_ref().is_some_and(|progress| {
            progress.x != progress.start_x
                || progress.col != progress.start_col
                || self.current_row_first_display_pos.is_some()
                || self.current_row_last_display_pos.is_some()
        })
    }

    fn begin_current_row_progress(
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

    fn update_current_row_progress(&mut self, row: i64, col: i64, y: i64, x: i64) {
        match self.current_row_progress.as_mut() {
            Some(progress) if progress.row == row => {
                progress.y = y;
                progress.col = col;
                progress.x = x;
            }
            _ => self.begin_current_row_progress(None, row, col, y, x),
        }
    }

    fn with_live_update<T>(
        &self,
        evaluator: &mut Context,
        f: impl FnOnce(&mut neovm_core::window::WindowOutputUpdate<'_>) -> T,
    ) -> Option<T> {
        if !self.publish_live {
            return None;
        }
        let frame = evaluator.frame_manager_mut().get_mut(self.frame_id)?;
        let mut update = frame.window_output_update(self.window_id)?;
        Some(f(&mut update))
    }

    pub(crate) fn note_display_buffer_pos(&mut self, buffer_pos: LispCharPos1) {
        if self.current_row_first_display_pos.is_none() {
            self.current_row_first_display_pos = Some(buffer_pos);
        }
        self.current_row_last_display_pos = Some(buffer_pos);
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
    pub(crate) fn note_row_walk_start(&mut self, buffer_pos: LispCharPos1) {
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
    pub(crate) fn push_overlaid_marker_point(
        &mut self,
        buffer_pos: LispCharPos1,
        glyph_x: f32,
        glyph_y: f32,
        width: f32,
        height: f32,
        row: i64,
        col: usize,
    ) {
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
    pub(crate) fn note_row_terminator(&mut self, terminator: DisplayRowTerminator) {
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
            role: neovm_core::window::DisplayPointRole::Glyph,
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
    pub(crate) fn push_display_point(
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
    pub(crate) fn push_text_display_point(
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
    pub(crate) fn push_text_overlaid_marker_point(
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
    pub(crate) fn push_text_insertion_boundary(
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
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_text_span(
        &mut self,
        evaluator: &mut Context,
        buffer_pos: LispCharPos1,
        row: usize,
        row_y: f32,
        glyph_x: f32,
        glyph_y: f32,
        width: f32,
        height: f32,
        start_col: usize,
        end_col: usize,
    ) {
        self.push_text_display_point(buffer_pos, glyph_x, glyph_y, width, height, row, start_col);
        self.move_text_output_to(evaluator, row, end_col, row_y, glyph_x + width.max(0.0));
    }

    fn emit_text_output_span(&mut self, evaluator: &mut Context, span: TextOutputSpan) {
        let start = span.start();
        let end = span.end();
        self.emit_text_span(
            evaluator,
            span.buffer_pos(),
            span.row(),
            span.row_y(),
            start.x_px(),
            span.glyph_y(),
            end.x_px() - start.x_px(),
            span.height(),
            start.col(),
            end.col(),
        );
    }

    pub(crate) fn begin_row_output(
        &mut self,
        evaluator: &mut Context,
        row: i64,
        col: i64,
        y: i64,
        x: i64,
    ) {
        self.begin_current_row_progress(None, row, col, y, x);
        let _ = self.with_live_update(evaluator, |update| {
            update.output_cursor_to_coords(row, col, y, x)
        });
    }

    pub(crate) fn begin_display_text_row(
        &mut self,
        evaluator: &mut Context,
        display_row_index: usize,
        row: usize,
        col: usize,
        y: f32,
        x: f32,
    ) {
        let output_row = self.text_row_base + row as i64;
        let output_col = col as i64;
        let output_y = (y - self.window_top).round() as i64;
        let output_x = (x - self.text_x).round() as i64;
        self.begin_current_row_progress(
            Some(display_row_index),
            output_row,
            output_col,
            output_y,
            output_x,
        );
        let _ = self.with_live_update(evaluator, |update| {
            update.output_cursor_to_coords(output_row, output_col, output_y, output_x)
        });
    }

    pub(crate) fn current_display_text_row_index(&self) -> usize {
        self.current_row_progress
            .and_then(|progress| progress.display_row_index)
            .expect("text row must have display row progress before finishing")
    }

    #[cfg(test)]
    pub(crate) fn begin_text_row(
        &mut self,
        evaluator: &mut Context,
        row: usize,
        col: usize,
        y: f32,
        x: f32,
    ) {
        self.begin_display_text_row(evaluator, self.text_row_base as usize + row, row, col, y, x);
    }

    fn begin_chrome_row(&mut self, evaluator: &mut Context, row: i64, y: f32) {
        self.begin_row_output(evaluator, row, 0, (y - self.window_top).round() as i64, 0);
    }

    pub(crate) fn begin_update(&self, evaluator: &mut Context) {
        let _ = self.with_live_update(evaluator, |update| update.begin_update());
    }

    pub(crate) fn move_output_to(
        &mut self,
        evaluator: &mut Context,
        row: i64,
        col: i64,
        y: i64,
        x: i64,
    ) {
        self.update_current_row_progress(row, col, y, x);
        let _ = self.with_live_update(evaluator, |update| {
            update.output_cursor_to_coords(row, col, y, x)
        });
    }

    pub(crate) fn move_text_output_to(
        &mut self,
        evaluator: &mut Context,
        row: usize,
        col: usize,
        y: f32,
        x: f32,
    ) {
        self.move_output_to(
            evaluator,
            self.text_row_base + row as i64,
            col as i64,
            (y - self.window_top).round() as i64,
            (x - self.text_x).round() as i64,
        );
    }

    fn move_chrome_output_to(
        &mut self,
        evaluator: &mut Context,
        row: i64,
        progress: DisplayRowOutputProgress,
    ) {
        self.move_output_to(
            evaluator,
            row,
            progress.end_col(),
            (progress.y() - self.window_top).round() as i64,
            progress.end_x().round() as i64,
        );
    }

    pub(crate) fn push_text_row(&mut self, row_y_start: f32, row_height: f32, row_ascent: f32) {
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
    pub(crate) fn push_reused_chrome(&mut self, rows: Vec<DisplayRowSnapshot>) {
        self.rows.extend(rows);
    }

    pub(crate) fn replace_chrome_area_strings(
        &mut self,
        area: PresentedWindowChromeArea,
        sources: Vec<PresentedWindowChromeString>,
    ) {
        debug_assert!(sources.iter().all(|source| source.area() == area));
        self.chrome_strings.retain(|current| current.area() != area);
        self.chrome_strings.extend(sources);
    }

    pub(crate) fn reuse_chrome_strings(&mut self, sources: Vec<PresentedWindowChromeString>) {
        self.chrome_strings = sources;
    }

    fn push_chrome_row_progress(&mut self, progress: DisplayRowOutputProgress) {
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
            fringe: Default::default(),
        });
    }

    pub(crate) fn set_logical_cursor(&mut self, cursor: WindowCursorPos) {
        self.logical_cursor = Some(cursor);
    }

    pub(crate) fn set_phys_cursor(&mut self, cursor: WindowCursorSnapshot) {
        self.phys_cursor = Some(cursor);
    }

    pub(crate) fn finish_snapshot_with_geometry(
        mut self,
        evaluator: &mut Context,
        cell_origin: CellOrigin,
        regions: PresentedWindowRegions,
        mode_line_height: i64,
        header_line_height: i64,
        tab_line_height: i64,
    ) -> WindowDisplaySnapshot {
        let frame_id = self.frame_id;
        let window_id = self.window_id;
        let logical_cursor = self.logical_cursor.take();
        let phys_cursor = self.phys_cursor.take();
        self.points
            .sort_by_key(|point| (point.buffer_pos, point.row, point.col, point.x));
        self.rows.sort_by_key(|row| row.row);
        let body_origin_y = (regions.text_body.y - regions.outer.y).round() as i64;
        let mut body_rows: Vec<_> = self
            .points
            .iter()
            .map(|point| neovm_core::window::PresentedBodyRowSnapshot {
                output_row: point.row,
                body_row: point.row.saturating_sub(self.text_row_base),
                body_y: point.y.saturating_sub(body_origin_y),
            })
            .collect();
        body_rows.sort_by_key(|row| row.output_row);
        body_rows.dedup_by_key(|row| row.output_row);
        // Record the displayed buffer's modification tick so display primitives
        // that consult this snapshot (notably `vertical-motion` with a column
        // target) can reject it once the buffer is mutated without a fresh
        // redisplay — otherwise a stale snapshot returns positions that never
        // advance (e.g. `shr-fill-line` inserting text while rendering hangs at
        // 100% CPU).
        let buffer_id = evaluator
            .frame_manager()
            .get(frame_id)
            .and_then(|frame| frame.find_window(window_id))
            .and_then(|window| window.buffer_id());
        let buffer_modiff = buffer_id
            .and_then(|buffer_id| evaluator.buffer_manager().get(buffer_id))
            .map(|buffer| buffer.modified_tick());
        let layout_freshness = buffer_id.and_then(|buffer_id| {
            evaluator.window_display_snapshot_freshness(frame_id, window_id, buffer_id)
        });
        let snapshot = WindowDisplaySnapshot {
            window_id,
            cell_origin,
            regions,
            regions_materialized: true,
            body_rows,
            text_area_left_offset: (regions.text_body.x - regions.outer.x).round() as i64,
            mode_line_height,
            header_line_height,
            tab_line_height,
            chrome_strings: self.chrome_strings,
            logical_cursor,
            phys_cursor: phys_cursor.clone(),
            points: self.points,
            rows: self.rows,
            buffer_modiff,
            layout_freshness,
            window_end_record: None,
        };
        if self.publish_live
            && let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id)
            && let Some(mut update) = frame.window_output_update(window_id)
        {
            update.finalize_live_update(logical_cursor, phys_cursor);
        }
        snapshot
    }
}

#[cfg(test)]
#[path = "window_output_test.rs"]
mod tests;
