//! Overlay string rendering — GNU `load_overlay_strings` / the
//! `it->overlay_strings` consumption (xdisp.c). Overlay strings render from the
//! single GNU-ordered before/after list exposed by the buffer bridge.

use crate::buffer_source::row_prelude::BufferSourceContinuationRowPreludeRequest;
use crate::coords::layout_char_pos_from_i64;
use crate::display_cursor::{
    CapturedCursorInfo, CapturedCursorPlacement, CapturedCursorSlotWidth,
    CapturedCursorVisualState, CursorCaptureState, DisplayStringCursorContext,
};
#[cfg(test)]
use crate::display_face_policy::BaseFacePolicy;
use crate::display_item::DisplayStringBoxBoundaries;
use crate::display_origin::{DisplayOrigin, OverlayStringKind};
use crate::display_row::append_context::DisplayRowAppendSurface;
use crate::display_row::builder::{DisplayRowGlyphSlot, DisplayRowPosition};
use crate::display_row::geometry::{
    DisplayRowGeometryDefaults, DisplayRowGeometryState, DisplayRowLimit, DisplayRowYPositions,
    DisplayRowYRecording,
};
use crate::display_row::lisp_string::{
    LispStringSourceAppendRequest, LispStringSourceAppendSessionRequest, LispStringSourceId,
    LispStringSourceRowAppendSession, LispStringSourceRowAppendSessionRequest,
};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::render_state::DisplayRowRenderStop;
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::transition::{
    DisplayRowLineBreakTransitionRequest, DisplayRowTransitionContinuation,
};
use crate::display_row::walk_state::{
    FaceScanCheckpoint, HitRowRangeTracker, LineNumberRenderState,
};
use crate::display_source::LispStringSourceOrigin;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::hit_test::HitRow;
use crate::neovm_bridge::{
    LayoutBufferView, OverlayDisplayString, ResolvedFace, RustTextPropAccess,
};
use neovm_core::buffer::CharPos0;
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::value::get_string_text_properties_table_for_value;

#[derive(Clone, Copy)]
pub(crate) struct OverlayStringRenderSource {
    value: Value,
    overlay_id: Value,
    overlay_start_charpos: CharPos0,
    anchor_charpos: CharPos0,
    kind: OverlayStringKind,
    box_boundaries: DisplayStringBoxBoundaries,
}

/// Buffer positions shared by every string rendered at one overlay anchor.
///
/// Both values use the layout engine's zero-based character coordinate.  The
/// named fields prevent the attachment position and point from being swapped
/// while they travel through the regular, routed-row, and EOB render paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OverlayStringRenderPositions {
    attachment: CharPos0,
    point: CharPos0,
}

impl OverlayStringRenderPositions {
    pub(crate) const fn new(attachment: CharPos0, point: CharPos0) -> Self {
        Self { attachment, point }
    }

    pub(crate) fn from_layout_i64(attachment: i64, point: i64) -> Self {
        Self::new(
            layout_char_pos_from_i64(attachment)
                .expect("overlay attachment must be a nonnegative layout position"),
            Self::layout_point(point),
        )
    }

    pub(crate) fn from_attachment_and_layout_point(attachment: CharPos0, point: i64) -> Self {
        Self::new(attachment, Self::layout_point(point))
    }

    fn layout_point(point: i64) -> CharPos0 {
        layout_char_pos_from_i64(point).expect("point must be a nonnegative layout position")
    }

    const fn attachment(self) -> CharPos0 {
        self.attachment
    }

    const fn point(self) -> CharPos0 {
        self.point
    }
}

pub(crate) struct OverlayStringRenderRequest<'a> {
    source: OverlayStringRenderSource,
    row_context: OverlayStringRenderRowContext<'a>,
}

impl OverlayStringRenderSource {
    pub(crate) fn new(
        overlay_string: OverlayDisplayString,
        anchor_charpos: CharPos0,
        kind: OverlayStringKind,
        box_boundaries: DisplayStringBoxBoundaries,
    ) -> Self {
        Self {
            value: overlay_string.string,
            overlay_id: overlay_string.overlay_id,
            overlay_start_charpos: overlay_string.overlay_start_charpos,
            anchor_charpos,
            kind,
            box_boundaries,
        }
    }

    pub(crate) fn anchor_i64(self) -> i64 {
        self.anchor_charpos.get() as i64
    }

    pub(crate) fn value(self) -> Value {
        self.value
    }

    fn cursor_context(self, point: CharPos0) -> DisplayStringCursorContext {
        DisplayStringCursorContext::for_overlay(
            self.overlay_start_charpos,
            self.anchor_charpos,
            point,
        )
    }

    pub(crate) fn origin(self) -> DisplayOrigin {
        DisplayOrigin::OverlayString {
            overlay_id: self.overlay_id,
            anchor_charpos: self.anchor_charpos,
            kind: self.kind,
        }
    }

    #[cfg(test)]
    pub(crate) fn base_face_policy(self) -> BaseFacePolicy {
        self.origin().default_base_face_policy()
    }

    pub(crate) fn append_request(
        self,
        position: DisplayRowPosition,
    ) -> LispStringSourceAppendRequest {
        LispStringSourceAppendRequest::new(position, LispStringSourceId::OVERLAY_STRING, self.value)
            .with_source_origin(LispStringSourceOrigin::OverlayString {
                overlay_id: self.overlay_id,
                kind: self.kind,
            })
            .with_box_boundaries(self.box_boundaries)
    }
}

impl<'a> OverlayStringRenderRequest<'a> {
    pub(crate) fn new(
        source: OverlayStringRenderSource,
        row_context: OverlayStringRenderRowContext<'a>,
    ) -> Self {
        Self {
            source,
            row_context,
        }
    }

    pub(crate) fn render<B: LayoutBufferView>(
        self,
        buffer: &B,
        point: CharPos0,
        state: &mut OverlayStringRenderState<'_>,
    ) -> DisplayRowTransitionContinuation {
        render_overlay_string(buffer, self.source, point, self.row_context, state)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct OverlayStringRenderRowContext<'a> {
    pub(crate) append_surface: &'a DisplayRowAppendSurface,
    pub(crate) metrics: DisplayRowFallbackMetrics,
    text_y: f32,
    pub(crate) row_base: usize,
    pub(crate) max_rows: usize,
    continuation_row_prelude: Option<BufferSourceContinuationRowPreludeRequest>,
}

impl<'a> OverlayStringRenderRowContext<'a> {
    pub(crate) fn new(
        append_surface: &'a DisplayRowAppendSurface,
        metrics: DisplayRowFallbackMetrics,
        text_y: f32,
        row_base: usize,
        max_rows: usize,
    ) -> Self {
        Self {
            append_surface,
            metrics,
            text_y,
            row_base,
            max_rows,
            continuation_row_prelude: None,
        }
    }

    fn with_continuation_row_prelude(
        mut self,
        request: Option<BufferSourceContinuationRowPreludeRequest>,
    ) -> Self {
        self.continuation_row_prelude = request;
        self
    }

    pub(crate) fn content_x(self) -> f32 {
        self.append_surface.content_x()
    }

    pub(crate) fn right_edge(self) -> f32 {
        self.append_surface.right_edge()
    }

    pub(crate) fn geometry_defaults(self) -> DisplayRowGeometryDefaults {
        DisplayRowGeometryDefaults::new(
            self.text_y,
            self.metrics.row_height(),
            self.metrics.ascent(),
        )
    }

    pub(crate) fn row_limit(self) -> DisplayRowLimit {
        DisplayRowLimit {
            max_rows: self.max_rows,
        }
    }

    pub(crate) fn cursor_visual_state(self, base_face: &ResolvedFace) -> CapturedCursorVisualState {
        CapturedCursorVisualState {
            face_width: self.metrics.char_width(),
            face_height: self.metrics.row_height(),
            face_ascent: self.metrics.ascent(),
            foreground: neomacs_display_protocol::types::Color::from_pixel(base_face.fg),
            background: neomacs_display_protocol::types::Color::from_pixel(base_face.bg),
        }
    }
}

pub(crate) struct OverlayStringRenderState<'a> {
    pub(crate) source_render: TextRowSourceRenderState<'a>,
    pub(crate) x: &'a mut f32,
    pub(crate) col: &'a mut usize,
    pub(crate) geometry: &'a mut DisplayRowGeometryState,
    pub(crate) cursor_info: &'a mut CursorCaptureState,
    pub(crate) hit_rows: &'a mut Vec<HitRow>,
    pub(crate) hit_row_range: &'a mut HitRowRangeTracker,
    pub(crate) row_y_positions: &'a mut DisplayRowYPositions,
    pub(crate) face_ids: &'a mut FrameFaceAttempt,
    continuation_row_prelude_state: Option<BufferSourceContinuationRowPreludeState<'a>>,
}

struct BufferSourceContinuationRowPreludeState<'a> {
    line_numbers: &'a mut LineNumberRenderState,
    face_scan: &'a mut FaceScanCheckpoint,
}

impl<'a> OverlayStringRenderState<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_source_render(
        source_render: TextRowSourceRenderState<'a>,
        x: &'a mut f32,
        col: &'a mut usize,
        geometry: &'a mut DisplayRowGeometryState,
        cursor_info: &'a mut CursorCaptureState,
        hit_rows: &'a mut Vec<HitRow>,
        hit_row_range: &'a mut HitRowRangeTracker,
        row_y_positions: &'a mut DisplayRowYPositions,
        face_ids: &'a mut FrameFaceAttempt,
    ) -> Self {
        Self {
            source_render,
            x,
            col,
            geometry,
            cursor_info,
            hit_rows,
            hit_row_range,
            row_y_positions,
            face_ids,
            continuation_row_prelude_state: None,
        }
    }

    fn with_buffer_source_continuation_row_prelude(
        mut self,
        line_numbers: &'a mut LineNumberRenderState,
        face_scan: &'a mut FaceScanCheckpoint,
    ) -> Self {
        self.continuation_row_prelude_state = Some(BufferSourceContinuationRowPreludeState {
            line_numbers,
            face_scan,
        });
        self
    }

    fn render_continuation_row_prelude(
        &mut self,
        request: BufferSourceContinuationRowPreludeRequest,
    ) {
        let state = self
            .continuation_row_prelude_state
            .as_mut()
            .expect("buffer continuation-row prelude requires its render state");
        request.render_with_source_state(
            state.line_numbers,
            &mut self.source_render,
            self.face_ids,
            self.geometry,
            state.face_scan,
        );
    }
}

#[derive(Clone, Copy)]
pub(crate) struct BufferOverlayStringTextRowRenderContext<'a> {
    enabled: bool,
    window_id: u64,
    append_surface: &'a DisplayRowAppendSurface,
    metrics: DisplayRowFallbackMetrics,
    text_y: f32,
    row_base: usize,
    max_rows: usize,
    continuation_row_prelude: Option<BufferSourceContinuationRowPreludeRequest>,
}

impl<'a> BufferOverlayStringTextRowRenderContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        enabled: bool,
        window_id: u64,
        append_surface: &'a DisplayRowAppendSurface,
        metrics: DisplayRowFallbackMetrics,
        text_y: f32,
        row_base: usize,
        max_rows: usize,
    ) -> Self {
        Self {
            enabled,
            window_id,
            append_surface,
            metrics,
            text_y,
            row_base,
            max_rows,
            continuation_row_prelude: None,
        }
    }

    pub(crate) fn with_continuation_row_prelude(
        mut self,
        request: BufferSourceContinuationRowPreludeRequest,
    ) -> Self {
        self.continuation_row_prelude = Some(request);
        self
    }

    /// The window overlay strings are collected FOR, or `None` when this row
    /// renders no overlay strings at all (the buffer carries no overlays).
    ///
    /// The routed row classifier needs both facts to decide a row: it collects
    /// through `RustTextPropAccess::new_for_window` with this window id, so the
    /// route and the pipeline can never disagree about which overlays apply
    /// (GNU's `window` overlay-property filter), and a `None` window means the
    /// append below is a no-op, so no position on the row is an anchor.
    pub(crate) fn string_window_id(self) -> Option<u64> {
        self.enabled.then_some(self.window_id)
    }

    fn row_context(self) -> OverlayStringRenderRowContext<'a> {
        OverlayStringRenderRowContext::new(
            self.append_surface,
            self.metrics,
            self.text_y,
            self.row_base,
            self.max_rows,
        )
        .with_continuation_row_prelude(self.continuation_row_prelude)
    }

    /// Render the strings the PRODUCER collected for `anchor_charpos`.
    ///
    /// Collection and GNU ordering live in the producer since P4.6 (it surfaces
    /// them as a typed element with insertion semantics); this side owns only
    /// the append, which is a session per string because a string can break
    /// rows, clip, and carry its own `cursor` property.
    pub(crate) fn render_produced_strings<B: LayoutBufferView>(
        self,
        buffer: &B,
        positions: OverlayStringRenderPositions,
        strings: &[OverlayDisplayString],
        box_boundaries: DisplayStringBoxBoundaries,
        state: &mut OverlayStringRenderState<'_>,
    ) -> DisplayRowTransitionContinuation {
        if !self.enabled {
            return DisplayRowTransitionContinuation::Continue;
        }
        let row_context = self.row_context();
        for (index, overlay_string) in strings.iter().copied().enumerate() {
            let kind = if overlay_string.after_string_p {
                OverlayStringKind::After
            } else {
                OverlayStringKind::Before
            };
            let continuation = OverlayStringRenderRequest::new(
                OverlayStringRenderSource::new(
                    overlay_string,
                    positions.attachment(),
                    kind,
                    box_boundaries.sequence_member(index, strings.len()),
                ),
                row_context,
            )
            .render(buffer, positions.point(), state);
            if continuation.should_break() {
                return continuation;
            }
        }
        DisplayRowTransitionContinuation::Continue
    }

    /// The end-of-buffer anchor path: collect and render here, because the
    /// producer stops at point-max and cannot surface an element for it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_eob_anchor_strings_at_text_row<B: LayoutBufferView>(
        self,
        buffer: &B,
        positions: OverlayStringRenderPositions,
        anchor_face_boxed: bool,
        source_render: TextRowSourceRenderState<'_>,
        x: &mut f32,
        col: &mut usize,
        geometry: &mut DisplayRowGeometryState,
        cursor_info: &mut CursorCaptureState,
        hit_rows: &mut Vec<HitRow>,
        hit_row_range: &mut HitRowRangeTracker,
        row_y_positions: &mut DisplayRowYPositions,
        face_ids: &mut FrameFaceAttempt,
        line_numbers: &mut LineNumberRenderState,
        face_scan: &mut FaceScanCheckpoint,
    ) -> DisplayRowTransitionContinuation {
        if !self.enabled {
            return DisplayRowTransitionContinuation::Continue;
        }
        let strings = RustTextPropAccess::new_for_window(buffer, self.window_id)
            .overlay_strings_at(positions.attachment().get() as i64);
        self.render_produced_strings_at_text_row(
            buffer,
            positions,
            &strings,
            // Entry inherits the final buffer iterator face; leaving the
            // pushed string reaches end-of-source, which has no following
            // boxed glyph.
            DisplayStringBoxBoundaries::known(anchor_face_boxed, false),
            source_render,
            x,
            col,
            geometry,
            cursor_info,
            hit_rows,
            hit_row_range,
            row_y_positions,
            face_ids,
            line_numbers,
            face_scan,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_produced_strings_at_text_row<B: LayoutBufferView>(
        self,
        buffer: &B,
        positions: OverlayStringRenderPositions,
        strings: &[OverlayDisplayString],
        box_boundaries: DisplayStringBoxBoundaries,
        source_render: TextRowSourceRenderState<'_>,
        x: &mut f32,
        col: &mut usize,
        geometry: &mut DisplayRowGeometryState,
        cursor_info: &mut CursorCaptureState,
        hit_rows: &mut Vec<HitRow>,
        hit_row_range: &mut HitRowRangeTracker,
        row_y_positions: &mut DisplayRowYPositions,
        face_ids: &mut FrameFaceAttempt,
        line_numbers: &mut LineNumberRenderState,
        face_scan: &mut FaceScanCheckpoint,
    ) -> DisplayRowTransitionContinuation {
        let mut overlay_state = OverlayStringRenderState::from_source_render(
            source_render,
            x,
            col,
            geometry,
            cursor_info,
            hit_rows,
            hit_row_range,
            row_y_positions,
            face_ids,
        )
        .with_buffer_source_continuation_row_prelude(line_numbers, face_scan);
        self.render_produced_strings(
            buffer,
            positions,
            strings,
            box_boundaries,
            &mut overlay_state,
        )
    }

    pub(crate) fn should_render(self, row_geometry: &DisplayRowGeometryState) -> bool {
        self.enabled && row_geometry.is_within_row_limit(self.row_context_row_limit())
    }

    fn row_context_row_limit(self) -> DisplayRowLimit {
        DisplayRowLimit {
            max_rows: self.max_rows,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct OverlayStringRowBreakRenderContext<'a> {
    anchor_charpos: i64,
    row_context: OverlayStringRenderRowContext<'a>,
}

impl<'a> OverlayStringRowBreakRenderContext<'a> {
    pub(crate) fn new(anchor_charpos: i64, row_context: OverlayStringRenderRowContext<'a>) -> Self {
        Self {
            anchor_charpos,
            row_context,
        }
    }

    pub(crate) fn finish_row(
        self,
        state: &mut OverlayStringRenderState<'_>,
    ) -> DisplayRowTransitionContinuation {
        let content_x = self.row_context.content_x();
        let geometry_transition = DisplayRowLineBreakTransitionRequest::new(
            state.hit_row_range.range_to(self.anchor_charpos),
            self.row_context.geometry_defaults(),
            self.row_context.row_base,
            0,
            content_x,
            0.0,
            DisplayRowYRecording::None,
            self.row_context.max_rows,
        )
        .finish_geometry(state.geometry, state.hit_rows);

        state.hit_row_range.advance_to(self.anchor_charpos);
        let row_transition = state
            .source_render
            .output_render()
            .transition_text_row_with_limit(geometry_transition, self.row_context.max_rows);
        if row_transition.is_exhausted() {
            return DisplayRowTransitionContinuation::Exhausted;
        }

        state.geometry.record_current_row_y(state.row_y_positions);
        *state.x = content_x;
        *state.col = 0;
        if let Some(prelude) = self.row_context.continuation_row_prelude {
            state.render_continuation_row_prelude(prelude);
        }
        DisplayRowTransitionContinuation::Continue
    }
}

fn render_overlay_string<B: LayoutBufferView>(
    buffer: &B,
    source_request: OverlayStringRenderSource,
    point: CharPos0,
    row_context: OverlayStringRenderRowContext<'_>,
    state: &mut OverlayStringRenderState<'_>,
) -> DisplayRowTransitionContinuation {
    let anchor_charpos = source_request.anchor_i64();
    let text_value = source_request.value();
    if text_value.as_lisp_string().is_none() {
        return DisplayRowTransitionContinuation::Continue;
    }
    let text_props = get_string_text_properties_table_for_value(text_value);
    let cursor_context = source_request.cursor_context(point);
    let base_face = state.source_render.default_display_string_base_face(
        buffer,
        source_request.origin(),
        state.face_ids,
    );
    let max_x = row_context.right_edge();
    let row_limit = row_context.row_limit();
    let row_break_context = OverlayStringRowBreakRenderContext::new(anchor_charpos, row_context);
    let append_request =
        source_request.append_request(DisplayRowPosition::new(*state.x, *state.col));
    let session_request = LispStringSourceAppendSessionRequest::for_buffer(
        buffer,
        append_request,
        base_face.face_id(),
        base_face.face(),
    );
    let row_session_request = LispStringSourceRowAppendSessionRequest::new(
        session_request,
        row_context.append_surface,
        0.0,
        row_context.metrics.row_height(),
        row_context.metrics.ascent(),
        row_context.metrics.char_width(),
        row_context.metrics,
    );
    let Some(mut source_context) = LispStringSourceRowAppendSession::new(row_session_request)
    else {
        return DisplayRowTransitionContinuation::Continue;
    };

    while state.geometry.is_within_row_limit(row_limit) {
        if *state.x >= max_x {
            break;
        }

        let Some(outcome) = source_context.render_to_text_row_and_emit(
            &mut state.source_render,
            state.face_ids,
            state.geometry,
            DisplayRowPosition::new(*state.x, *state.col),
        ) else {
            break;
        };
        let stop = outcome.stop();
        outcome.include_vertical_metrics(state.geometry);
        let overlay_cursor_visual_state = row_context.cursor_visual_state(base_face.face());
        for slot in outcome.source_slots() {
            capture_overlay_string_cursor_at_slot(
                text_props.as_ref(),
                slot,
                state.cursor_info,
                state.geometry.y(),
                state.geometry.row(),
                overlay_cursor_visual_state,
                cursor_context,
            );
        }
        let end = outcome.end_position();
        *state.x = end.x_px();
        *state.col = end.col();

        if matches!(stop, DisplayRowRenderStop::RowBreak(_)) {
            let continuation = row_break_context.finish_row(state);
            if continuation.should_break() {
                return continuation;
            }
            continue;
        }
        match stop {
            DisplayRowRenderStop::SourceExhausted => break,
            DisplayRowRenderStop::Clipped => {
                if source_context.discard_pending_until_row_break() {
                    let continuation = row_break_context.finish_row(state);
                    if continuation.should_break() {
                        return continuation;
                    }
                    continue;
                }
                break;
            }
            DisplayRowRenderStop::RowBreak(_) => unreachable!("row break handled above"),
        }
    }
    DisplayRowTransitionContinuation::Continue
}

fn root_lisp_position_char(source: &crate::display_item::DisplaySourcePosition) -> Option<usize> {
    match source {
        crate::display_item::DisplaySourcePosition::LispString {
            source_id,
            char_index,
            ..
        } if source_id.get() == LispStringSourceId::OVERLAY_STRING.raw() => Some(*char_index),
        _ => None,
    }
}

fn capture_overlay_string_cursor_at_slot(
    text_props: Option<&neovm_core::buffer::text_props::TextPropertyTable>,
    slot: &DisplayRowGlyphSlot,
    cursor_info: &mut CursorCaptureState,
    y: f32,
    display_row_offset: usize,
    visual_state: CapturedCursorVisualState,
    cursor_context: DisplayStringCursorContext,
) {
    let Some(char_idx) = root_lisp_position_char(&slot.source()) else {
        return;
    };
    capture_overlay_string_cursor(
        text_props,
        char_idx,
        cursor_info,
        slot.x_px(),
        y,
        slot.col(),
        display_row_offset,
        visual_state,
        CapturedCursorSlotWidth::Explicit(slot.width_px()),
        cursor_context,
    );
}

#[allow(clippy::too_many_arguments)]
fn capture_overlay_string_cursor(
    text_props: Option<&neovm_core::buffer::text_props::TextPropertyTable>,
    char_idx: usize,
    cursor_info: &mut CursorCaptureState,
    x: f32,
    y: f32,
    col: usize,
    display_row_offset: usize,
    visual_state: CapturedCursorVisualState,
    slot_width: CapturedCursorSlotWidth,
    cursor_context: DisplayStringCursorContext,
) {
    let Some(props) = text_props else {
        return;
    };
    let Some(cursor_prop) =
        props.get_property_at_char_pos(CharPos0::new(char_idx), Value::symbol("cursor"))
    else {
        return;
    };
    let info = CapturedCursorInfo::from_visual_state(
        visual_state,
        CapturedCursorPlacement {
            x,
            y,
            byte_idx: 0,
            col,
            display_row_offset,
            slot_width,
            stretch_like: false,
        },
    );
    if let Some(candidate) = cursor_context.resolve(cursor_prop, info) {
        cursor_info.capture_display_string_cursor(candidate);
    }
}
