use crate::buffer_source::item_append::BufferSourceActiveFaceRowMetrics;
use crate::display_cursor::{CapturedCursorInfo, CursorCaptureState, capture_cursor_info};
use crate::display_item::{BufferDisplayPropertyReplacementItem, RenderFaceRef};
use crate::display_row::append_context::DisplayRowAppendSurface;
use crate::display_row::builder::DisplayRowPosition;
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::geometry::{DisplayRowGeometryState, DisplayRowTextPosition};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::replacement::{
    DisplayPropertyReplacementAppendOutcome, DisplayPropertyReplacementRowRender,
    DisplayPropertyReplacementRowRenderRequest, DisplayPropertyReplacementStringRender,
};
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_source::DisplaySourceTextPosition;
use crate::display_source::{DisplaySourceItem, DisplaySourceStepItem};
use crate::display_source_progress::DisplaySourceProgressState;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::LayoutBufferView;
use crate::types::WindowParams;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferDisplayPropertyTextReplacementOutcome {
    pub(crate) replacement: DisplayPropertyReplacementAppendOutcome,
    pub(crate) skip_to: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferDisplayPropertyTextReplacementWalkUpdate {
    row_position: DisplayRowPosition,
    source_position: DisplaySourceTextPosition,
}

// These short-lived walk outcomes carry complete display items by value;
// boxing would add an allocation on the text-layout hot path.
#[allow(clippy::large_enum_variant)]
pub(crate) enum BufferDisplayPropertyTextReplacementRenderOutcome {
    Rendered(BufferDisplayPropertyTextReplacementOutcome),
    String(DisplayPropertyReplacementStringRender),
    Fallback(DisplaySourceStepItem),
    Stop,
}

// Large payload variant; boxing is a perf hint deferred out of the lint gate.
#[allow(clippy::large_enum_variant)]
pub(crate) enum BufferDisplayPropertyTextReplacementApplyOutcome {
    Applied,
    String(DisplayPropertyReplacementStringRender),
    Fallback(DisplaySourceStepItem),
    Stop,
}

pub(crate) struct BufferDisplayPropertyTextReplacementRenderContext<'a, 'face> {
    request: BufferDisplayPropertyTextReplacementRenderRequest<'a, 'face>,
    current_x: f32,
    start_position: DisplayRowPosition,
    start_charpos: i64,
}

pub(crate) struct BufferDisplayPropertyTextReplacementRenderRequest<'a, 'face> {
    replacement: BufferDisplayPropertyReplacementItem,
    text_start_byte: usize,
    text: &'a [u8],
    content_x: f32,
    params: &'a WindowParams,
    glyph_y_offset: f32,
    fallback_metrics: DisplayRowFallbackMetrics,
    active_face_state: &'face DisplayRowActiveFaceState,
}

pub(crate) struct BufferDisplayPropertyTextReplacementRenderState<'emit> {
    source_render: TextRowSourceRenderState<'emit>,
    face_ids: &'emit mut FrameFaceAttempt,
    append_surface: &'emit DisplayRowAppendSurface,
    row_geometry: &'emit mut DisplayRowGeometryState,
    active_face_state: &'emit DisplayRowActiveFaceState,
}

impl<'emit> BufferDisplayPropertyTextReplacementRenderState<'emit> {
    pub(crate) fn new(
        source_render: TextRowSourceRenderState<'emit>,
        face_ids: &'emit mut FrameFaceAttempt,
        append_surface: &'emit DisplayRowAppendSurface,
        row_geometry: &'emit mut DisplayRowGeometryState,
        active_face_state: &'emit DisplayRowActiveFaceState,
    ) -> Self {
        Self {
            source_render,
            face_ids,
            append_surface,
            row_geometry,
            active_face_state,
        }
    }
}

impl<'a, 'face> BufferDisplayPropertyTextReplacementRenderContext<'a, 'face> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        replacement: BufferDisplayPropertyReplacementItem,
        text_start_byte: usize,
        text: &'a [u8],
        content_x: f32,
        params: &'a WindowParams,
        glyph_y_offset: f32,
        row_height_px: f32,
        active_face_state: &'face DisplayRowActiveFaceState,
        current_x: f32,
        start_position: DisplayRowPosition,
    ) -> Self {
        let start_charpos = replacement.start_charpos();
        let request = BufferDisplayPropertyTextReplacementRenderRequest::new(
            replacement,
            text_start_byte,
            text,
            content_x,
            params,
            glyph_y_offset,
            BufferSourceActiveFaceRowMetrics::from_active_face_row(
                active_face_state,
                row_height_px,
            )
            .fallback_metrics(),
            active_face_state,
        );
        Self {
            request,
            current_x,
            start_position,
            start_charpos,
        }
    }

    pub(crate) fn render_and_apply<B: LayoutBufferView>(
        &self,
        buffer: &B,
        mut state: BufferDisplayPropertyTextReplacementRenderState<'_>,
        progress: &mut DisplaySourceProgressState<'_>,
        cursor_info: &mut CursorCaptureState,
        point_charpos: i64,
    ) -> BufferDisplayPropertyTextReplacementApplyOutcome {
        match self.request.render_with_state(
            buffer,
            &mut state,
            self.current_x,
            self.start_position,
        ) {
            BufferDisplayPropertyTextReplacementRenderOutcome::Rendered(outcome) => {
                self.apply_rendered_outcome(
                    outcome,
                    progress,
                    cursor_info,
                    state.row_geometry,
                    point_charpos,
                );
                BufferDisplayPropertyTextReplacementApplyOutcome::Applied
            }
            BufferDisplayPropertyTextReplacementRenderOutcome::String(session) => {
                BufferDisplayPropertyTextReplacementOutcome {
                    replacement: session.opening_slot(),
                    skip_to: self.request.replacement.descriptor().resume_charpos(),
                }
                .capture_cursor_info_if_point(
                    cursor_info,
                    self.request.active_face_state,
                    state.row_geometry,
                    point_charpos,
                    self.start_charpos,
                    progress.byte_idx(),
                );
                BufferDisplayPropertyTextReplacementApplyOutcome::String(session)
            }
            BufferDisplayPropertyTextReplacementRenderOutcome::Fallback(source_item) => {
                BufferDisplayPropertyTextReplacementApplyOutcome::Fallback(source_item)
            }
            BufferDisplayPropertyTextReplacementRenderOutcome::Stop => {
                BufferDisplayPropertyTextReplacementApplyOutcome::Stop
            }
        }
    }

    fn apply_rendered_outcome(
        &self,
        outcome: BufferDisplayPropertyTextReplacementOutcome,
        progress: &mut DisplaySourceProgressState<'_>,
        cursor_info: &mut CursorCaptureState,
        row_geometry: &DisplayRowGeometryState,
        point_charpos: i64,
    ) {
        outcome.apply_to_progress_and_cursor(
            self.request.text,
            progress,
            cursor_info,
            self.request.active_face_state,
            row_geometry,
            point_charpos,
            self.start_charpos,
        );
    }

    pub(crate) fn apply_completed_string(
        &self,
        replacement: DisplayPropertyReplacementAppendOutcome,
        progress: &mut DisplaySourceProgressState<'_>,
    ) {
        BufferDisplayPropertyTextReplacementOutcome {
            replacement,
            skip_to: self.request.replacement.descriptor().resume_charpos(),
        }
        .apply_to_progress(self.request.text, progress);
    }
}

impl<'a, 'face> BufferDisplayPropertyTextReplacementRenderRequest<'a, 'face> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        replacement: BufferDisplayPropertyReplacementItem,
        text_start_byte: usize,
        text: &'a [u8],
        content_x: f32,
        params: &'a WindowParams,
        glyph_y_offset: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
        active_face_state: &'face DisplayRowActiveFaceState,
    ) -> Self {
        Self {
            replacement,
            text_start_byte,
            text,
            content_x,
            params,
            glyph_y_offset,
            fallback_metrics,
            active_face_state,
        }
    }

    fn fallback_render_item(&self) -> Option<DisplaySourceStepItem> {
        let fallback = self.replacement.fallback_display_item(
            self.text_start_byte,
            self.text,
            RenderFaceRef::FaceId(self.active_face_state.face_id()),
        )?;
        let (item, start_byte_idx, start_charpos, source_char) = fallback.into_parts();
        let source_item = DisplaySourceItem::new(item, start_byte_idx, start_charpos, source_char);
        DisplaySourceStepItem::new(source_item, self.text_start_byte)
    }

    fn render_append_request<B: LayoutBufferView>(
        &self,
        append_request: DisplayPropertyReplacementRowRenderRequest,
        buffer: &B,
        state: &mut BufferDisplayPropertyTextReplacementRenderState<'_>,
    ) -> BufferDisplayPropertyTextReplacementRenderOutcome {
        match append_request.begin_render_to_text_rows(
            buffer,
            &mut state.source_render.reborrow(),
            state.face_ids,
            state.append_surface,
            state.row_geometry,
            state.active_face_state,
        ) {
            DisplayPropertyReplacementRowRender::Applied(outcome) => {
                BufferDisplayPropertyTextReplacementRenderOutcome::Rendered(
                    BufferDisplayPropertyTextReplacementOutcome {
                        replacement: outcome,
                        skip_to: self.replacement.descriptor().resume_charpos(),
                    },
                )
            }
            DisplayPropertyReplacementRowRender::String(session) => {
                BufferDisplayPropertyTextReplacementRenderOutcome::String(session)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn render<B: LayoutBufferView>(
        &self,
        buffer: &B,
        mut state: BufferDisplayPropertyTextReplacementRenderState<'_>,
        current_x: f32,
        start_position: DisplayRowPosition,
    ) -> BufferDisplayPropertyTextReplacementRenderOutcome {
        self.render_with_state(buffer, &mut state, current_x, start_position)
    }

    fn render_with_state<B: LayoutBufferView>(
        &self,
        buffer: &B,
        state: &mut BufferDisplayPropertyTextReplacementRenderState<'_>,
        current_x: f32,
        start_position: DisplayRowPosition,
    ) -> BufferDisplayPropertyTextReplacementRenderOutcome {
        let Some(source_text) = self
            .replacement
            .source_text(self.text_start_byte, self.text)
        else {
            return BufferDisplayPropertyTextReplacementRenderOutcome::Stop;
        };
        let descriptor = self.replacement.descriptor();
        // Attach the fringe bitmap (if this is a `(left-fringe …)` spec) to the
        // current row BEFORE rendering the empty inline replacement. The text
        // area still shows nothing; only the fringe column gains the bitmap.
        state.source_render.record_fringe_bitmap_for_descriptor(
            descriptor,
            state.face_ids,
            self.active_face_state,
        );
        let append_request = state
            .source_render
            .resolve_display_property_replacement_row_request(
                descriptor,
                source_text,
                self.active_face_state,
                current_x,
                self.content_x,
                self.params,
                self.glyph_y_offset,
                self.fallback_metrics,
                start_position,
            );
        match append_request {
            Some(request) => self.render_append_request(request, buffer, state),
            None => {
                let Some(source_item) = self.fallback_render_item() else {
                    return BufferDisplayPropertyTextReplacementRenderOutcome::Stop;
                };
                BufferDisplayPropertyTextReplacementRenderOutcome::Fallback(source_item)
            }
        }
    }
}

impl BufferDisplayPropertyTextReplacementOutcome {
    fn point_in_replacement(self, point_charpos: i64, start_charpos: i64) -> bool {
        point_charpos >= start_charpos && point_charpos < self.skip_to
    }

    fn start_position(self) -> DisplayRowPosition {
        self.replacement.start_position()
    }

    fn end_position(self) -> DisplayRowPosition {
        self.replacement.end_position()
    }

    fn skip_covered_buffer_text(self, text: &[u8], position: &mut DisplaySourceTextPosition) {
        position.skip_chars_until(text, self.skip_to);
    }

    fn capture_cursor_info_if_point(
        self,
        cursor_info: &mut CursorCaptureState,
        active_face_state: &DisplayRowActiveFaceState,
        row_geometry: &DisplayRowGeometryState,
        point_charpos: i64,
        start_charpos: i64,
        byte_idx: usize,
    ) {
        if cursor_info.is_missing() && self.point_in_replacement(point_charpos, start_charpos) {
            let start_position = self.start_position();
            // The cursor sits at the slot's left edge, i.e. the right edge of the
            // real glyph immediately before the replaced region (1-based buffer
            // position `start_charpos - 1`). Anchoring the cursor's grid x on that
            // glyph's already-rounded display point keeps it byte-identical to the
            // glyph edge for every font size (no round(x+w) vs round(x)+round(w)
            // drift). `None` when the replacement starts the buffer.
            let preceding_charpos = (start_charpos > 1).then_some(start_charpos - 1);
            capture_cursor_info(
                cursor_info,
                self.cursor_info(
                    active_face_state,
                    row_geometry.text_position(
                        start_position.x_px(),
                        byte_idx,
                        start_position.col(),
                    ),
                    preceding_charpos,
                ),
            );
        }
    }

    fn walk_update(
        self,
        text: &[u8],
        mut source_position: DisplaySourceTextPosition,
    ) -> BufferDisplayPropertyTextReplacementWalkUpdate {
        self.skip_covered_buffer_text(text, &mut source_position);
        BufferDisplayPropertyTextReplacementWalkUpdate::new(self.end_position(), source_position)
    }

    #[cfg(test)]
    pub(crate) fn skip_to(self) -> i64 {
        self.skip_to
    }

    pub(crate) fn cursor_info(
        self,
        active_face_state: &DisplayRowActiveFaceState,
        position: DisplayRowTextPosition,
        preceding_charpos: Option<i64>,
    ) -> CapturedCursorInfo {
        self.replacement
            .cursor_info(active_face_state, position, preceding_charpos)
    }

    pub(crate) fn apply_to_progress_and_cursor(
        self,
        text: &[u8],
        progress: &mut DisplaySourceProgressState<'_>,
        cursor_info: &mut CursorCaptureState,
        active_face_state: &DisplayRowActiveFaceState,
        row_geometry: &DisplayRowGeometryState,
        point_charpos: i64,
        start_charpos: i64,
    ) {
        self.capture_cursor_info_if_point(
            cursor_info,
            active_face_state,
            row_geometry,
            point_charpos,
            start_charpos,
            progress.byte_idx(),
        );
        self.apply_to_progress(text, progress);
    }

    fn apply_to_progress(self, text: &[u8], progress: &mut DisplaySourceProgressState<'_>) {
        let walk_update = self.walk_update(text, progress.source_position());
        progress.apply_row_position(walk_update.row_position());
        progress.apply_source_position(walk_update.source_position());
    }
}

impl BufferDisplayPropertyTextReplacementWalkUpdate {
    pub(crate) fn new(
        row_position: DisplayRowPosition,
        source_position: DisplaySourceTextPosition,
    ) -> Self {
        Self {
            row_position,
            source_position,
        }
    }

    pub(crate) fn row_position(self) -> DisplayRowPosition {
        self.row_position
    }

    pub(crate) fn source_position(self) -> DisplaySourceTextPosition {
        self.source_position
    }
}
