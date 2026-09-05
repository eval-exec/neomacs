//! DisplayOutputBuilder — records authoritative window matrices during layout.
//!
//! The builder observes layout emissions and writes them into the per-window
//! `GlyphMatrix` grids published through `FrameDisplayState`. Renderers then
//! materialize that immutable snapshot into runtime glyph buffers on the
//! consumer side; layout no longer treats `FrameGlyphBuffer` as the primary
//! output contract.

use crate::display_cursor::{
    CursorVisualColumnResolutionContext, CursorVisualColumnResolutionRequest,
};
#[cfg(test)]
use crate::display_row::face_state::resolved_display_row_face;
#[cfg(test)]
use crate::font::metrics::FontMetrics;
use crate::frame_face_arena::{FrameFaceArena, FrameFaceAttempt};
#[cfg(test)]
use crate::neovm_bridge::ResolvedFace;
use crate::output::frame_state::OutputFrameBuildState;
use crate::output::install_request::{
    OutputCursorInstallRequest, OutputFrameArtifactInstallRequest,
    OutputFrameIdentityInstallRequest, OutputFrameStateInstallRequest,
    OutputWindowMetadataInstallRequest,
};
#[cfg(test)]
use crate::output::row_request::OutputCurrentRowDecorationRequest;
use crate::output::row_request::{
    DisplayCurrentRowMutation, DisplayWindowRowMutation, DisplayWindowRowsMutation,
    OutputRowLifecycleRequest,
};
use crate::output::window_request::OutputWindowLifecycleRequest;
use crate::output::window_state::OutputWindowBuildState;
use neomacs_display_protocol::face::Face;
#[cfg(test)]
use neomacs_display_protocol::frame_glyphs::CursorStyle;
#[cfg(test)]
use neomacs_display_protocol::frame_glyphs::DisplaySlotId;
#[cfg(test)]
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
#[cfg(test)]
use neomacs_display_protocol::frame_glyphs::PhysCursor;
use neomacs_display_protocol::frame_glyphs::{ContentTransitionHint, WindowInfo};
use neomacs_display_protocol::glyph_matrix::*;
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{Color, DisplayFrameId, DisplayWindowId, Rect};

pub(crate) const FRAME_CHROME_WINDOW_ID: i64 = 0;

pub(crate) struct DisplayOutputBuilder {
    window_state: OutputWindowBuildState,
    frame_state: OutputFrameBuildState,
    face_attempt: FrameFaceAttempt,
}

impl DisplayOutputBuilder {
    pub(crate) fn new() -> Self {
        Self {
            window_state: OutputWindowBuildState::new(),
            frame_state: OutputFrameBuildState::new(),
            face_attempt: FrameFaceArena::default().begin_attempt(),
        }
    }

    pub(crate) fn set_face_attempt(&mut self, face_attempt: FrameFaceAttempt) {
        self.face_attempt = face_attempt;
    }

    pub(crate) fn reset(&mut self) {
        self.window_state.reset();
        self.frame_state.reset();
    }

    #[cfg(test)]
    pub(crate) fn begin_window(
        &mut self,
        window_id: u64,
        nrows: usize,
        ncols: usize,
        pixel_bounds: Rect,
        selected: bool,
    ) {
        self.begin_window_with_text_bounds(
            window_id,
            nrows,
            ncols,
            pixel_bounds,
            pixel_bounds,
            selected,
        );
    }

    #[cfg(test)]
    pub(crate) fn begin_window_with_text_bounds(
        &mut self,
        window_id: u64,
        nrows: usize,
        ncols: usize,
        pixel_bounds: Rect,
        text_pixel_bounds: Rect,
        selected: bool,
    ) {
        self.begin_output_window(
            window_id,
            nrows,
            ncols,
            pixel_bounds,
            text_pixel_bounds,
            selected,
        );
    }

    #[cfg(test)]
    pub(crate) fn end_window(&mut self) {
        self.end_output_window();
    }

    pub(crate) fn install_output_window_lifecycle(
        &mut self,
        request: OutputWindowLifecycleRequest,
    ) {
        self.window_state.install_window_lifecycle(request);
    }

    #[cfg(test)]
    pub(crate) fn begin_output_window(
        &mut self,
        window_id: u64,
        nrows: usize,
        ncols: usize,
        pixel_bounds: Rect,
        text_pixel_bounds: Rect,
        selected: bool,
    ) {
        self.install_output_window_lifecycle(OutputWindowLifecycleRequest::begin(
            window_id,
            nrows,
            ncols,
            pixel_bounds,
            text_pixel_bounds,
            text_pixel_bounds,
            selected,
        ));
    }

    #[cfg(test)]
    pub(crate) fn end_output_window(&mut self) {
        self.install_output_window_lifecycle(OutputWindowLifecycleRequest::end());
    }

    pub(crate) fn install_window_metadata(
        &mut self,
        request: impl Into<OutputWindowMetadataInstallRequest>,
    ) {
        self.install_output_window_metadata(request.into());
    }

    fn install_output_window_metadata(&mut self, request: OutputWindowMetadataInstallRequest) {
        self.frame_state.install_window_metadata(request);
    }

    pub(crate) fn install_output_row_lifecycle(&mut self, request: OutputRowLifecycleRequest) {
        self.window_state
            .install_row_lifecycle(request, self.frame_state.phys_cursor_mut());
    }

    /// Install an already-finalized (visual-order) row into the current window
    /// grid verbatim, marking it finalized so it is not bidi-reordered again.
    /// Used by the Phase 1 cursor-only fast path to replay retained body rows.
    pub(crate) fn install_finalized_output_row(
        &mut self,
        row: usize,
        source: neomacs_display_protocol::glyph_matrix::MatrixRow,
    ) {
        self.window_state.install_finalized_window_row(row, source);
    }

    /// Re-resolve the active cursor after a post-layout decoration replaces
    /// glyph provenance in the current window row.
    ///
    /// GNU performs overlay-arrow replacement before `set_cursor_from_row`.
    /// Neomacs publishes the cursor with the body and then applies late row
    /// decorations, so this explicit seam restores the same ordering without
    /// giving decorations arbitrary mutable access to frame cursor state.
    pub(crate) fn reconcile_phys_cursor_after_row_decoration(&mut self, char_width: f32) {
        let Some(mut cursor) = self.frame_state.phys_cursor().cloned() else {
            return;
        };
        let Some(placement) = CursorVisualColumnResolutionRequest::from_cursor(&cursor)
            .resolve_after_row_decoration(
                self.cursor_visual_column_context(),
                self.window_state.current_window_text_pixel_bounds(),
                char_width,
            )
        else {
            return;
        };
        placement.apply_to(&mut cursor);
        let row = cursor.row;
        let col = cursor.col;
        let style = cursor.style;
        self.install_output_frame_artifact(OutputFrameArtifactInstallRequest::phys_cursor(cursor));
        self.install_output_row_lifecycle(OutputRowLifecycleRequest::cursor(row, col, style));
    }

    /// Find the current window's buffer-text row containing `charpos` (Phase 2
    /// scroll cursor re-decorate target).
    pub(crate) fn find_current_window_cursor_row(&self, charpos: usize) -> Option<usize> {
        self.window_state.find_current_window_cursor_row(charpos)
    }

    /// Read a row from the current window grid.
    pub(crate) fn current_window_row(&self, row: usize) -> Option<&GlyphRow> {
        self.window_state.current_window_row(row)
    }

    /// Strip cursor decoration from every row of the current window grid.
    pub(crate) fn clear_current_window_cursors(&mut self) {
        self.window_state.clear_current_window_cursors();
    }

    #[cfg(test)]
    pub(crate) fn begin_output_row(&mut self, row: usize, role: GlyphRowRole, mode_line: bool) {
        self.install_output_row_lifecycle(OutputRowLifecycleRequest::begin(row, role, mode_line));
    }

    #[cfg(test)]
    pub(crate) fn install_complete_output_row(
        &mut self,
        matrix_row: usize,
        role: GlyphRowRole,
        mode_line: bool,
        glyph_row: GlyphRow,
    ) {
        self.install_output_row_lifecycle(OutputRowLifecycleRequest::complete(
            matrix_row, role, mode_line, glyph_row,
        ));
    }

    pub(crate) fn apply_current_output_row_mutation<M>(&mut self, mutation: M) -> Option<M::Output>
    where
        M: DisplayCurrentRowMutation,
    {
        self.window_state.apply_current_row_mutation(mutation)
    }

    #[cfg(test)]
    pub(crate) fn set_output_row_metrics(
        &mut self,
        row: usize,
        pixel_y: f32,
        height_px: f32,
        ascent_px: f32,
    ) {
        self.install_output_row_lifecycle(OutputRowLifecycleRequest::metrics(
            row, pixel_y, height_px, ascent_px,
        ));
    }

    #[cfg(test)]
    pub(crate) fn finalize_output_row_index(&mut self, row: usize) {
        self.install_output_row_lifecycle(OutputRowLifecycleRequest::finalize(row));
    }

    #[cfg(test)]
    pub(crate) fn set_output_row_cursor(&mut self, row: usize, col: u16, style: CursorStyle) {
        self.install_output_row_lifecycle(OutputRowLifecycleRequest::cursor(row, col, style));
    }

    #[cfg(test)]
    pub(crate) fn mark_current_output_row_truncated_left(&mut self) {
        self.install_output_row_lifecycle(OutputRowLifecycleRequest::current_decoration(
            OutputCurrentRowDecorationRequest::MarkTruncatedLeft,
        ));
    }

    pub(crate) fn apply_current_window_row_mutation<M>(
        &mut self,
        row_idx: usize,
        mutation: M,
    ) -> Option<M::Output>
    where
        M: DisplayWindowRowMutation,
    {
        self.window_state
            .apply_current_window_row_mutation(row_idx, mutation)
    }

    pub(crate) fn apply_last_window_rows_mutation<M>(&mut self, mutation: M)
    where
        M: DisplayWindowRowsMutation,
    {
        self.window_state.apply_last_window_rows_mutation(mutation);
    }

    #[cfg(test)]
    pub(crate) fn begin_row(&mut self, row: usize, role: GlyphRowRole) {
        self.begin_output_row(row, role, matches!(role, GlyphRowRole::ModeLine));
    }

    #[cfg(test)]
    pub(crate) fn end_row(&mut self) {
        let current_row = self.window_state.current_row_index();
        self.finalize_output_row_index(current_row);
    }

    /// Record stored geometry for the currently open row.
    #[cfg(test)]
    pub(crate) fn set_current_row_metrics(&mut self, pixel_y: f32, height_px: f32, ascent_px: f32) {
        let current_row = self.window_state.current_row_index();
        self.set_output_row_metrics(current_row, pixel_y, height_px, ascent_px);
    }

    #[cfg(test)]
    pub(crate) fn edit_current_row_for_test<R>(
        &mut self,
        f: impl FnOnce(&mut GlyphRow) -> R,
    ) -> Option<R> {
        self.apply_current_output_row_mutation(EditCurrentRowForTestMutation { f })
    }

    pub(crate) fn current_row_for_render(&self) -> Option<&GlyphRow> {
        self.window_state.current_row_for_render()
    }

    #[cfg(test)]
    pub(crate) fn current_row_for_test(&self) -> Option<&GlyphRow> {
        self.current_row_for_render()
    }

    #[cfg(test)]
    fn install_display_row(&mut self, row_index: usize, source: &GlyphRow) {
        self.install_output_row_lifecycle(OutputRowLifecycleRequest::complete_window_absolute_row(
            row_index,
            source,
            self.current_window_pixel_bounds(),
        ));
    }

    #[cfg(test)]
    pub(crate) fn set_cursor_at_row(
        &mut self,
        row: usize,
        col: u16,
        style: neomacs_display_protocol::frame_glyphs::CursorStyle,
    ) {
        self.set_output_row_cursor(row, col, style);
    }

    // -----------------------------------------------------------------------
    // Non-grid item installation
    // -----------------------------------------------------------------------

    pub(crate) fn install_output_frame_artifact(
        &mut self,
        request: OutputFrameArtifactInstallRequest,
    ) {
        self.frame_state.install_artifact(request);
    }

    pub(crate) fn install_output_cursor(&mut self, request: OutputCursorInstallRequest) {
        self.frame_state.install_cursor(request);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn set_output_frame_identity(
        &mut self,
        frame_id: u64,
        parent_id: u64,
        parent_x: f32,
        parent_y: f32,
        z_order: i32,
        undecorated: bool,
        border_width: f32,
        border_color: Color,
        outer_border_width: f32,
        outer_border_color: Color,
        background_alpha: f32,
        no_accept_focus: bool,
    ) {
        self.install_output_frame_state(OutputFrameStateInstallRequest::Identity(
            OutputFrameIdentityInstallRequest {
                frame_id: DisplayFrameId::new(frame_id),
                parent_id: DisplayFrameId::new(parent_id),
                parent_x,
                parent_y,
                z_order,
                undecorated,
                border_width,
                border_color,
                outer_border_width,
                outer_border_color,
                background_alpha,
                no_accept_focus,
            },
        ));
    }

    pub(crate) fn set_output_background_color(&mut self, color: Color) {
        self.install_output_frame_state(OutputFrameStateInstallRequest::BackgroundColor(color));
    }

    pub(crate) fn set_output_font_pixel_size(&mut self, size: f32) {
        self.install_output_frame_state(OutputFrameStateInstallRequest::FontPixelSize(size));
    }

    #[cfg(test)]
    pub(crate) fn install_output_face(&mut self, id: FaceId, face: Face) {
        self.publish_output_face(id, face);
    }

    #[cfg(test)]
    pub(crate) fn install_output_resolved_display_row_face(
        &mut self,
        face_id: FaceId,
        face: &ResolvedFace,
        metrics: Option<FontMetrics>,
    ) {
        let render_face = resolved_display_row_face(face_id, face, metrics);
        self.publish_output_face(render_face.face_id, render_face.render_face());
    }

    pub(crate) fn add_output_background(&mut self, bounds: Rect, color: Color) {
        self.install_output_frame_artifact(OutputFrameArtifactInstallRequest::Background {
            bounds,
            color,
        });
    }

    pub(crate) fn add_output_face_fill(&mut self, item: FaceFillItem) {
        self.install_output_frame_artifact(OutputFrameArtifactInstallRequest::FaceFill(item));
    }

    pub(crate) fn add_output_border(
        &mut self,
        window_id: i64,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    ) {
        self.install_output_frame_artifact(OutputFrameArtifactInstallRequest::Border {
            window_id: DisplayWindowId::new(window_id),
            x,
            y,
            width,
            height,
            color,
        });
    }

    pub(crate) fn add_output_scroll_bar(&mut self, item: ScrollBarItem) {
        self.install_output_frame_artifact(OutputFrameArtifactInstallRequest::ScrollBar(item));
    }

    pub(crate) fn add_output_window_info(&mut self, info: WindowInfo) {
        self.install_output_frame_artifact(OutputFrameArtifactInstallRequest::WindowInfo(info));
    }

    pub(crate) fn add_output_transition_hint(&mut self, hint: ContentTransitionHint) {
        self.install_output_frame_artifact(OutputFrameArtifactInstallRequest::TransitionHint(hint));
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_output_cursor(
        &mut self,
        window_id: i64,
        slot_id: DisplaySlotId,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        style: CursorStyle,
        color: Color,
    ) {
        self.install_output_cursor(OutputCursorInstallRequest::new(
            DisplayWindowId::new(window_id),
            neomacs_display_protocol::glyph_matrix::CursorItemRole::Decorative,
            slot_id,
            x,
            y,
            width,
            height,
            0.0,
            style,
            color,
            Color::BLACK,
        ));
    }

    pub(crate) fn current_window_id_i64(&self) -> i64 {
        self.window_state.current_window_id_i64()
    }

    pub(crate) fn current_window_pixel_bounds(&self) -> Rect {
        self.window_state.current_window_pixel_bounds()
    }

    pub(crate) fn cursor_visual_column_context(&self) -> CursorVisualColumnResolutionContext<'_> {
        self.window_state.cursor_visual_column_context()
    }

    #[cfg(test)]
    pub(crate) fn set_phys_cursor(&mut self, cursor: PhysCursor) {
        let mut cursor = cursor;
        let coordinates = CursorVisualColumnResolutionRequest::from_cursor(&cursor)
            .resolve_cursor_coordinates(self.cursor_visual_column_context());

        if let Some(coordinates) = coordinates {
            coordinates.apply_display_to(&mut cursor);
        }

        if let Some(coordinates) = coordinates {
            self.install_output_row_lifecycle(OutputRowLifecycleRequest::cursor(
                cursor.row,
                coordinates.display_col(),
                cursor.style,
            ));
        }

        // The active role is represented solely by the phys cursor: the shared
        // publication seam installs a CursorItem only for the inactive role,
        // so there is no second active-window artifact to keep in sync here.
        self.install_output_frame_artifact(OutputFrameArtifactInstallRequest::phys_cursor(cursor));
    }

    #[cfg(test)]
    pub(crate) fn set_glyph_row_resolved_phys_cursor(&mut self, cursor: PhysCursor) {
        self.install_output_row_lifecycle(OutputRowLifecycleRequest::cursor(
            cursor.row,
            cursor.col,
            cursor.style,
        ));

        self.install_output_frame_artifact(OutputFrameArtifactInstallRequest::phys_cursor(cursor));
    }

    pub(crate) fn install_output_frame_state(&mut self, request: OutputFrameStateInstallRequest) {
        self.frame_state.install_frame_state(request);
    }

    pub(crate) fn publish_output_face(&mut self, id: FaceId, mut face: Face) {
        face.id = id;
        self.face_attempt
            .publish(face)
            .expect("one frame face id must have one immutable rendering");
    }

    pub(crate) fn window_content_height_px(
        &self,
        window_id: i64,
        fallback_row_height: f32,
    ) -> Option<f32> {
        self.window_state
            .window_content_height_px(window_id, fallback_row_height)
    }

    #[cfg(test)]
    pub(crate) fn completed_window_count(&self) -> usize {
        self.window_state.completed_window_count()
    }

    #[cfg(test)]
    pub(crate) fn completed_window_id(&self, index: usize) -> Option<u64> {
        self.window_state.completed_window_id(index)
    }

    pub(crate) fn window_infos(&self) -> &[WindowInfo] {
        self.frame_state.window_infos()
    }

    #[cfg(test)]
    pub(crate) fn cursors(&self) -> &[CursorItem] {
        self.frame_state.cursors()
    }

    #[cfg(test)]
    pub(crate) fn phys_cursor(&self) -> Option<&PhysCursor> {
        self.frame_state.phys_cursor()
    }

    pub(crate) fn transition_hints(&self) -> &[ContentTransitionHint] {
        self.frame_state.transition_hints()
    }

    pub(crate) fn output_face(&self, face_id: FaceId) -> Option<Face> {
        self.face_attempt.face(face_id)
    }

    pub(crate) fn finish(
        self,
        frame_cols: usize,
        frame_rows: usize,
        char_width: f32,
        char_height: f32,
    ) -> FrameDisplayState {
        self.finish_with_pixel_size(
            frame_cols,
            frame_rows,
            char_width,
            char_height,
            frame_cols as f32 * char_width,
            frame_rows as f32 * char_height,
        )
    }

    pub(crate) fn finish_with_pixel_size(
        self,
        frame_cols: usize,
        frame_rows: usize,
        char_width: f32,
        char_height: f32,
        frame_pixel_width: f32,
        frame_pixel_height: f32,
    ) -> FrameDisplayState {
        let Self {
            window_state,
            frame_state,
            face_attempt,
        } = self;
        let mut state = FrameDisplayState::new(frame_cols, frame_rows, char_width, char_height);
        state.frame_pixel_width = frame_pixel_width;
        state.frame_pixel_height = frame_pixel_height;
        state.window_matrices = window_state.into_window_matrix_entries();
        frame_state.install_into(&mut state);
        state.faces = face_attempt.faces();
        state
    }
}

#[cfg(test)]
struct EditCurrentRowForTestMutation<F> {
    f: F,
}

#[cfg(test)]
impl<F, R> DisplayCurrentRowMutation for EditCurrentRowForTestMutation<F>
where
    F: FnOnce(&mut GlyphRow) -> R,
{
    type Output = R;

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        (self.f)(row)
    }
}

#[cfg(test)]
#[path = "builder_test.rs"]
mod tests;
