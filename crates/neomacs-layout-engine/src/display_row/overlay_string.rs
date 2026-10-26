//! Overlay string rendering — GNU `load_overlay_strings` / the
//! `it->overlay_strings` consumption (xdisp.c). Overlay strings render from the
//! single GNU-ordered before/after list exposed by the buffer bridge.

use crate::buffer_source::loop_state::BufferSourceRowCarryoverState;
use crate::buffer_source::row_prelude::BufferSourceRowPreludeRequestContext;
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
use crate::display_row::face_state::{DisplayRowActiveFaceState, DisplayRowMeasurementMode};
use crate::display_row::geometry::{
    DisplayRowFlags, DisplayRowGeometryDefaults, DisplayRowGeometryState, DisplayRowLimit,
    DisplayRowYPositions, DisplayRowYRecording,
};
use crate::display_row::lisp_string::{
    LispStringSourceAppendRequest, LispStringSourceAppendSessionRequest, LispStringSourceId,
    LispStringSourceRowAppendSession, LispStringSourceRowAppendSessionRequest,
};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::render_state::DisplayRowRenderStop;
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::transition::{
    DisplayRowLineBreakTransitionRequest, DisplayRowOverflowTransitionPlan,
    DisplayRowTextWindowEmitContext, DisplayRowTransitionContinuation, VisualWrapBreak,
};
use crate::display_row::walk_state::{
    DisplayRowSourceStart, FaceScanCheckpoint, TextRowTransitionStatePolicy,
};
use crate::display_source::{LispStringPrefixProperties, LispStringSourceOrigin};
use crate::display_source_progress::{DisplaySourceContent, DisplaySourceVisualRowProgress};
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{
    LayoutBufferView, OverlayDisplayString, ResolvedFace, RustTextPropAccess,
};
use crate::types::{LayoutCharPos0, WindowParams};
use neomacs_display_protocol::types::Px;
use neovm_core::buffer::{CharPos0, DisplayColumn};
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

    pub(crate) fn render<B: LayoutBufferView, FS: crate::neovm_bridge::FaceServices>(
        self,
        buffer: &B,
        point: CharPos0,
        state: &mut OverlayStringRenderState<'_, FS>,
    ) -> DisplayRowTransitionContinuation {
        render_overlay_string(buffer, self.source, point, self.row_context, state)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayStringRowTransitions {
    Allow,
    Refuse,
}

#[derive(Clone, Copy)]
pub(crate) struct OverlayStringRenderRowContext<'a> {
    pub(crate) append_surface: &'a DisplayRowAppendSurface,
    pub(crate) metrics: DisplayRowFallbackMetrics,
    text_y: f32,
    pub(crate) row_base: usize,
    pub(crate) max_rows: usize,
    row_transitions: OverlayStringRowTransitions,
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
            row_transitions: OverlayStringRowTransitions::Allow,
        }
    }

    fn with_row_transitions(mut self, policy: OverlayStringRowTransitions) -> Self {
        self.row_transitions = policy;
        self
    }

    pub(crate) fn content_x(self) -> f32 {
        self.append_surface.content_x()
    }

    pub(crate) fn geometry_defaults(
        self,
        measurement_mode: DisplayRowMeasurementMode,
    ) -> DisplayRowGeometryDefaults {
        DisplayRowGeometryDefaults::new(
            self.text_y,
            self.metrics.row_height(),
            self.metrics.ascent(),
            measurement_mode,
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

pub(crate) struct OverlayStringRenderState<
    'a,
    FS: crate::neovm_bridge::FaceServices = crate::neovm_bridge::FaceResolver,
> {
    pub(crate) source_render: TextRowSourceRenderState<'a, FS>,
    progress: DisplaySourceVisualRowProgress<'a>,
    pub(crate) geometry: &'a mut DisplayRowGeometryState,
    row_flags: &'a mut DisplayRowFlags,
    pub(crate) cursor_info: &'a mut CursorCaptureState,
    pub(crate) row_source_start: &'a mut DisplayRowSourceStart,
    pub(crate) row_y_positions: &'a mut DisplayRowYPositions,
    pub(crate) face_ids: &'a mut FrameFaceAttempt,
    continuation: OverlayStringRowContinuation<'a>,
    source_completion: OverlaySourceCompletion,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OverlaySourceCompletion {
    Incomplete,
    Complete,
}

#[derive(Clone, Copy)]
struct OverlayStringContinuationRowContext<'a> {
    prelude: BufferSourceRowPreludeRequestContext,
    params: &'a WindowParams,
    surface: &'a DisplayRowAppendSurface,
}

struct BufferSourceContinuationRowPreludeState<'a> {
    prelude: Option<OverlayStringContinuationRowContext<'a>>,
    carryover: BufferSourceRowCarryoverState<'a>,
    face_scan: &'a mut FaceScanCheckpoint,
    active_face: &'a DisplayRowActiveFaceState,
}

/// Only the buffer variant owns the state needed to publish a new visual row.
/// A prelude cannot be supplied independently of that authority.
enum OverlayStringRowContinuation<'a> {
    Isolated,
    Buffer(BufferSourceContinuationRowPreludeState<'a>),
}

impl<'a, FS: crate::neovm_bridge::FaceServices> OverlayStringRenderState<'a, FS> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_source_render(
        source_render: TextRowSourceRenderState<'a, FS>,
        progress: DisplaySourceVisualRowProgress<'a>,
        geometry: &'a mut DisplayRowGeometryState,
        row_flags: &'a mut DisplayRowFlags,
        cursor_info: &'a mut CursorCaptureState,
        row_source_start: &'a mut DisplayRowSourceStart,
        row_y_positions: &'a mut DisplayRowYPositions,
        face_ids: &'a mut FrameFaceAttempt,
    ) -> Self {
        Self {
            source_render,
            progress,
            geometry,
            row_flags,
            cursor_info,
            row_source_start,
            row_y_positions,
            face_ids,
            continuation: OverlayStringRowContinuation::Isolated,
            source_completion: OverlaySourceCompletion::Incomplete,
        }
    }

    fn with_buffer_source_continuation_row_prelude(
        mut self,
        prelude: Option<OverlayStringContinuationRowContext<'a>>,
        carryover: BufferSourceRowCarryoverState<'a>,
        face_scan: &'a mut FaceScanCheckpoint,
        active_face: &'a DisplayRowActiveFaceState,
    ) -> Self {
        self.continuation =
            OverlayStringRowContinuation::Buffer(BufferSourceContinuationRowPreludeState {
                prelude,
                carryover,
                face_scan,
                active_face,
            });
        self
    }

    fn render_continuation_row_prelude<B: LayoutBufferView>(
        &mut self,
        buffer: &B,
        anchor: LayoutCharPos0,
        properties: LispStringPrefixProperties,
    ) {
        let state = match &mut self.continuation {
            OverlayStringRowContinuation::Isolated => return,
            OverlayStringRowContinuation::Buffer(state) => state,
        };
        let Some(context) = state.prelude else {
            return;
        };
        context
            .prelude
            .continuation_row_prelude()
            .render_with_source_state(
                state.carryover.line_numbers,
                &mut self.source_render,
                self.face_ids,
                self.geometry,
                state.face_scan,
            );
        let wrap_prefix = state.carryover.prefix_request.is_wrap();
        let position = if wrap_prefix {
            self.progress.row_position().on_screen_line_tab_grid()
        } else {
            self.progress.row_position()
        };
        let end = context
            .prelude
            .line_prefix_request(
                context.surface,
                self.geometry,
                state.active_face,
                0.0,
                position,
                context.params,
            )
            .with_source_properties(properties)
            .render_requested_to_text_row_and_emit(
                state.carryover.prefix_request,
                &mut self.source_render,
                buffer,
                anchor.get(),
                self.face_ids,
            );
        self.progress.apply_position(end);
        self.source_render
            .include_current_row_visible_content_metrics(
                self.face_ids,
                DisplayRowFallbackMetrics::from_default_face_extents(
                    context.params.char_width,
                    self.geometry.height(),
                    self.geometry.ascent(),
                ),
                self.geometry,
            );
        if wrap_prefix {
            self.progress
                .record_wrap_prefix_width(Px((end.x_px() - position.x_px()).max(0.0)));
        }
        self.progress.record_row_text_start();
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
    continuation_row_prelude: Option<OverlayStringContinuationRowContext<'a>>,
    row_transitions: OverlayStringRowTransitions,
}

impl<'a> BufferOverlayStringTextRowRenderContext<'a> {
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
            row_transitions: OverlayStringRowTransitions::Allow,
        }
    }

    pub(crate) fn with_continuation_row_prelude(
        mut self,
        prelude: BufferSourceRowPreludeRequestContext,
        params: &'a WindowParams,
    ) -> Self {
        self.continuation_row_prelude = Some(OverlayStringContinuationRowContext {
            prelude,
            params,
            surface: self.append_surface,
        });
        self
    }

    /// Routed rows may speculate within the current row, but must refuse
    /// before publishing a new row that their single-row checkpoint cannot undo.
    pub(crate) fn without_row_transitions(mut self) -> Self {
        self.row_transitions = OverlayStringRowTransitions::Refuse;
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
        .with_row_transitions(self.row_transitions)
    }

    /// Render the strings the PRODUCER collected for `anchor_charpos`.
    ///
    /// Collection and GNU ordering live in the producer since P4.6 (it surfaces
    /// them as a typed element with insertion semantics); this side owns only
    /// the append, which is a session per string because a string can break
    /// rows, clip, and carry its own `cursor` property.
    pub(crate) fn render_produced_strings<
        B: LayoutBufferView,
        FS: crate::neovm_bridge::FaceServices,
    >(
        self,
        buffer: &B,
        positions: OverlayStringRenderPositions,
        strings: &[OverlayDisplayString],
        box_boundaries: DisplayStringBoxBoundaries,
        state: &mut OverlayStringRenderState<'_, FS>,
    ) -> DisplayRowTransitionContinuation {
        if !self.enabled {
            return DisplayRowTransitionContinuation::Continue;
        }
        let row_context = self.row_context();
        let mut completion = OverlaySourceCompletion::Complete;
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
            if state.source_completion == OverlaySourceCompletion::Incomplete {
                completion = OverlaySourceCompletion::Incomplete;
            }
            if continuation.should_break() {
                return continuation;
            }
        }
        state.source_completion = completion;
        DisplayRowTransitionContinuation::Continue
    }

    /// The end-of-buffer anchor path: collect and render here, because the
    /// producer stops at point-max and cannot surface an element for it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_eob_anchor_strings_at_text_row<
        B: LayoutBufferView,
        FS: crate::neovm_bridge::FaceServices,
    >(
        self,
        buffer: &B,
        positions: OverlayStringRenderPositions,
        anchor_face_boxed: bool,
        mut source_render: TextRowSourceRenderState<'_, FS>,
        progress: DisplaySourceVisualRowProgress<'_>,
        geometry: &mut DisplayRowGeometryState,
        row_flags: &mut DisplayRowFlags,
        cursor_info: &mut CursorCaptureState,
        row_source_start: &mut DisplayRowSourceStart,
        row_y_positions: &mut DisplayRowYPositions,
        face_ids: &mut FrameFaceAttempt,
        carryover: BufferSourceRowCarryoverState<'_>,
        face_scan: &mut FaceScanCheckpoint,
        active_face: &DisplayRowActiveFaceState,
    ) -> DisplayRowTransitionContinuation {
        if !self.enabled {
            return DisplayRowTransitionContinuation::Continue;
        }
        let strings = RustTextPropAccess::new_for_window(buffer, self.window_id)
            .overlay_strings_at(positions.attachment().get() as i64);
        if strings.is_empty() {
            return DisplayRowTransitionContinuation::Continue;
        }
        let start = source_render
            .output_emitter()
            .begin_eob_overlay_motion(positions.attachment().to_lisp(), geometry.row());
        let mut state = OverlayStringRenderState::from_source_render(
            source_render,
            progress,
            geometry,
            row_flags,
            cursor_info,
            row_source_start,
            row_y_positions,
            face_ids,
        )
        .with_buffer_source_continuation_row_prelude(
            self.continuation_row_prelude,
            carryover,
            face_scan,
            active_face,
        );
        let continuation = self.render_produced_strings(
            buffer,
            positions,
            &strings,
            // Entry inherits the final buffer iterator face; leaving the
            // pushed string reaches end-of-source, which has no following
            // boxed glyph.
            DisplayStringBoxBoundaries::known(anchor_face_boxed, false),
            &mut state,
        );
        if continuation == DisplayRowTransitionContinuation::Continue
            && state.source_completion == OverlaySourceCompletion::Complete
        {
            let completion_row = state.geometry.row();
            state
                .source_render
                .output_emitter()
                .complete_eob_overlay_motion(start, completion_row);
        }
        continuation
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_produced_strings_at_text_row<
        B: LayoutBufferView,
        FS: crate::neovm_bridge::FaceServices,
    >(
        self,
        buffer: &B,
        positions: OverlayStringRenderPositions,
        strings: &[OverlayDisplayString],
        box_boundaries: DisplayStringBoxBoundaries,
        source_render: TextRowSourceRenderState<'_, FS>,
        progress: DisplaySourceVisualRowProgress<'_>,
        geometry: &mut DisplayRowGeometryState,
        row_flags: &mut DisplayRowFlags,
        cursor_info: &mut CursorCaptureState,
        row_source_start: &mut DisplayRowSourceStart,
        row_y_positions: &mut DisplayRowYPositions,
        face_ids: &mut FrameFaceAttempt,
        carryover: BufferSourceRowCarryoverState<'_>,
        face_scan: &mut FaceScanCheckpoint,
        active_face: &DisplayRowActiveFaceState,
    ) -> DisplayRowTransitionContinuation {
        let mut overlay_state = OverlayStringRenderState::from_source_render(
            source_render,
            progress,
            geometry,
            row_flags,
            cursor_info,
            row_source_start,
            row_y_positions,
            face_ids,
        )
        .with_buffer_source_continuation_row_prelude(
            self.continuation_row_prelude,
            carryover,
            face_scan,
            active_face,
        );
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

    #[inline]
    pub(crate) fn row_limit_for_eob_proof(self) -> DisplayRowLimit {
        self.row_context_row_limit()
    }

    fn row_context_row_limit(self) -> DisplayRowLimit {
        DisplayRowLimit {
            max_rows: self.max_rows,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct OverlayStringRowBreakRenderContext<'a> {
    anchor_charpos: LayoutCharPos0,
    row_context: OverlayStringRenderRowContext<'a>,
}

impl<'a> OverlayStringRowBreakRenderContext<'a> {
    pub(crate) fn new(anchor_charpos: i64, row_context: OverlayStringRenderRowContext<'a>) -> Self {
        Self {
            anchor_charpos: LayoutCharPos0::new(anchor_charpos),
            row_context,
        }
    }

    fn finish_visual_row<B: LayoutBufferView, FS: crate::neovm_bridge::FaceServices>(
        self,
        buffer: &B,
        properties: LispStringPrefixProperties,
        state: &mut OverlayStringRenderState<'_, FS>,
    ) -> DisplayRowTransitionContinuation {
        // An isolated row session has no authority to publish a buffer row
        // transition. Refuse before changing TAB progress or output rows.
        let context = match &mut state.continuation {
            OverlayStringRowContinuation::Isolated => {
                return DisplayRowTransitionContinuation::Exhausted;
            }
            OverlayStringRowContinuation::Buffer(context) => context,
        };
        let position = state.progress.row_position();
        state
            .progress
            .continue_after_visual_row(Px(self.row_context.content_x()));
        let plan = DisplayRowOverflowTransitionPlan::visual_wrap(
            VisualWrapBreak::MidElement,
            TextRowTransitionStatePolicy::visual_wrap(),
        );
        // The shared transition predates DisplayColumn; keep its raw mutable
        // coordinate confined to this call boundary.
        let mut column = state.progress.col().get();
        let transition = DisplayRowTextWindowEmitContext::from_source_render(
            self.row_context
                .geometry_defaults(state.source_render.measurement_mode()),
            self.row_context.row_base,
            state.row_y_positions,
            self.row_context.max_rows,
            state.geometry,
            state.row_flags,
            self.row_context.row_limit(),
            &mut state.source_render,
        )
        .emit_overflow_then_row_start(
            plan,
            self.anchor_charpos,
            position,
            context.carryover.render_state(true),
            &mut column,
        );
        state.progress.set_col(DisplayColumn::new(column));
        if transition.is_exhausted() {
            return DisplayRowTransitionContinuation::Exhausted;
        }
        context.face_scan.invalidate();
        state.row_source_start.advance_to(self.anchor_charpos.get());
        state
            .progress
            .apply_position(DisplayRowPosition::new(self.row_context.content_x(), 0));
        state.render_continuation_row_prelude(buffer, self.anchor_charpos, properties);
        state.progress.record_row_text_start();
        DisplayRowTransitionContinuation::Continue
    }

    pub(crate) fn finish_row<B: LayoutBufferView, FS: crate::neovm_bridge::FaceServices>(
        self,
        buffer: &B,
        properties: LispStringPrefixProperties,
        state: &mut OverlayStringRenderState<'_, FS>,
    ) -> DisplayRowTransitionContinuation {
        let content_x = self.row_context.content_x();
        let geometry_transition = DisplayRowLineBreakTransitionRequest::new(
            self.anchor_charpos,
            self.row_context
                .geometry_defaults(state.source_render.measurement_mode()),
            self.row_context.row_base,
            0,
            content_x,
            0.0,
            DisplayRowYRecording::None,
            self.row_context.max_rows,
        )
        .finish_geometry(state.geometry);

        state.row_source_start.advance_to(self.anchor_charpos.get());
        let row_transition = state
            .source_render
            .output_render()
            .transition_text_row_with_limit(geometry_transition, self.row_context.max_rows);
        if row_transition.is_exhausted() {
            state.geometry.exhaust_rows(self.row_context.max_rows);
            return DisplayRowTransitionContinuation::Exhausted;
        }

        state.geometry.record_current_row_y(state.row_y_positions);
        state.progress.reset_physical_line_tabs();
        state
            .progress
            .apply_position(DisplayRowPosition::new(content_x, 0));
        match &mut state.continuation {
            OverlayStringRowContinuation::Isolated => {}
            OverlayStringRowContinuation::Buffer(context) => {
                let mut column = state.progress.col().get();
                context.carryover.render_state(true).apply_line_break_row_start(
                    crate::display_row::transition::DisplayRowLineBreakTransitionPlan::display_string_line_break(),
                    &mut column,
                );
                state.progress.set_col(DisplayColumn::new(column));
                context.face_scan.invalidate();
            }
        }
        state.render_continuation_row_prelude(buffer, self.anchor_charpos, properties);
        state.progress.record_row_text_start();
        DisplayRowTransitionContinuation::Continue
    }
}

fn render_overlay_string<B: LayoutBufferView, FS: crate::neovm_bridge::FaceServices>(
    buffer: &B,
    source_request: OverlayStringRenderSource,
    point: CharPos0,
    row_context: OverlayStringRenderRowContext<'_>,
    state: &mut OverlayStringRenderState<'_, FS>,
) -> DisplayRowTransitionContinuation {
    state.source_completion = OverlaySourceCompletion::Incomplete;
    let anchor_charpos = source_request.anchor_i64();
    let text_value = source_request.value();
    if text_value.as_lisp_string().is_none() {
        state.source_completion = OverlaySourceCompletion::Complete;
        return DisplayRowTransitionContinuation::Continue;
    }
    let text_props = get_string_text_properties_table_for_value(text_value);
    let cursor_context = source_request.cursor_context(point);
    let base_face = state.source_render.default_display_string_base_face(
        buffer,
        source_request.origin(),
        state.face_ids,
    );
    let row_limit = row_context.row_limit();
    let row_break_context = OverlayStringRowBreakRenderContext::new(anchor_charpos, row_context);
    let append_request = source_request.append_request(state.progress.row_position());
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
        state.source_completion = OverlaySourceCompletion::Complete;
        return DisplayRowTransitionContinuation::Continue;
    };

    while state.geometry.is_within_row_limit(row_limit) {
        // Even at the right edge the pushed source must advance. Its next
        // element may be a newline; clipped text uses the bounded discard
        // path below, which can also prove that the stream is exhausted.
        let Some(outcome) = source_context.render_to_text_row_and_emit(
            &mut state.source_render,
            state.face_ids,
            state.geometry,
            state.progress.row_position(),
        ) else {
            state.source_completion = OverlaySourceCompletion::Complete;
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
        state.progress.apply_position(end);

        if matches!(stop, DisplayRowRenderStop::RowBreak(_)) {
            match row_context.row_transitions {
                OverlayStringRowTransitions::Allow => {}
                OverlayStringRowTransitions::Refuse => {
                    return DisplayRowTransitionContinuation::Exhausted;
                }
            }
            state
                .source_render
                .output_emitter()
                .note_overlay_string_row_end(
                    crate::coords::layout_i64_char_pos_to_lisp_char_pos(anchor_charpos),
                    source_request.kind,
                );
            let continuation = row_break_context.finish_row(
                buffer,
                source_context.pending_prefix_properties(),
                state,
            );
            if continuation.should_break() {
                return continuation;
            }
            continue;
        }
        match stop {
            DisplayRowRenderStop::SourceExhausted => {
                state.source_completion = OverlaySourceCompletion::Complete;
                break;
            }
            DisplayRowRenderStop::Clipped => {
                if !row_context.append_surface.line_wrap().wraps() {
                    // Numeric automatic-mini observation only: do not alter
                    // the public row flags, glyphs or ordinary transition.
                    state
                        .source_render
                        .output_emitter()
                        .note_automatic_mini_row_truncation();
                }
                match row_context.row_transitions {
                    OverlayStringRowTransitions::Allow => {}
                    OverlayStringRowTransitions::Refuse => {
                        return DisplayRowTransitionContinuation::Exhausted;
                    }
                }
                // GNU's pushed overlay-string iterator survives WINDOW_WRAP:
                // only TRUNCATE skips to a logical newline. Retain the pending
                // element so its full width/composition is replayed next row.
                // An element that cannot fit even an empty row keeps the old
                // bounded skip path rather than producing endless empty rows.
                if row_context.append_surface.line_wrap().wraps()
                    && match state.progress.source_content(Px(row_context.content_x())) {
                        DisplaySourceContent::Empty => false,
                        DisplaySourceContent::Present => true,
                    }
                {
                    // A visual row break consumes pushed string text, never
                    // the underlying buffer character at its anchor.
                    state
                        .source_render
                        .output_emitter()
                        .note_overlay_string_row_end(
                            crate::coords::layout_i64_char_pos_to_lisp_char_pos(anchor_charpos),
                            source_request.kind,
                        );
                    let continuation = row_break_context.finish_visual_row(
                        buffer,
                        source_context.pending_prefix_properties(),
                        state,
                    );
                    if continuation.should_break() {
                        return continuation;
                    }
                    continue;
                }
                if source_context.discard_pending_until_row_break(buffer.layout_display_target()) {
                    state
                        .source_render
                        .output_emitter()
                        .note_overlay_string_row_end(
                            crate::coords::layout_i64_char_pos_to_lisp_char_pos(anchor_charpos),
                            source_request.kind,
                        );
                    let continuation = row_break_context.finish_row(
                        buffer,
                        source_context.pending_prefix_properties(),
                        state,
                    );
                    if continuation.should_break() {
                        return continuation;
                    }
                    continue;
                }
                state.source_completion = OverlaySourceCompletion::Complete;
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
