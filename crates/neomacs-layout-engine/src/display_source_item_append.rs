use crate::display_cursor::{
    CapturedCursorInfo, CapturedCursorPlacement, CapturedCursorSlotWidth, CursorCaptureState,
    capture_cursor_info, update_cursor_info_for_main_char,
};
use crate::display_item::DisplayItem;
use crate::display_row::append_context::{DisplayRowAppendFrame, DisplayRowAppendKind};
use crate::display_row::builder::{DisplayRowAppendProgress, DisplayRowPosition};
use crate::display_row::face_state::{DisplayRowActiveFaceState, DisplayRowExtendFace};
use crate::display_row::geometry::{DisplayRowGeometryState, DisplayRowTextPosition};
use crate::display_row::source_append::SingleDisplayItemAppendContext;
use crate::display_row::source_render::{TextRowSourceMeasureState, TextRowSourceRenderState};
use crate::display_row::walk_state::{
    DisplayRowTextOverflowDecision, FaceScanCheckpoint, SpecialTextRowOverflowDecision,
    TrailingWhitespaceRenderState, WordWrapRenderState,
};
use crate::display_source::{
    DisplaySourceAppendContinuation, DisplaySourceAppendItem, DisplaySourceClusterState,
    DisplaySourceItemRequest, DisplaySourceNaturalMeasurementRequest,
    DisplaySourceRenderPlanRequest, DisplaySourceSpecialDisplayKind, DisplaySourceStepChar,
    DisplaySourceTextPosition, DisplaySourceTextRange, DisplaySourceTextRequest,
    DisplaySpecialSourceCharRequest,
};
use crate::display_source_append_plan::{
    DisplaySourceAppendMeasurementKind, DisplaySourceAppendRenderPlan,
    DisplaySourceAppendRenderPolicy,
};
use crate::display_source_overflow::{
    DisplaySourceSpecialCharOverflowAction, DisplaySourceTextCharOverflowAction,
};
use crate::display_source_progress::DisplaySourceProgressState;
use crate::display_text_run_measurement::ComplexTextRunAdvanceResolver;
#[cfg(test)]
use crate::frame_face_arena::FrameFaceArena;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::ResolvedFace;
use crate::types::{LineWrapMode, WindowParams};
use neomacs_display_protocol::types::FaceId;

pub(crate) trait DisplaySourceCharAppendContext {
    fn append_source_char_plan_to_text_row(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
        plan: DisplaySourceTextCharAppendPlan,
    ) -> Option<DisplayRowAppendProgress>;

    fn append_special_source_char_plan_to_text_row_and_emit(
        &self,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
        plan: DisplaySourceSpecialCharAppendPlan,
    ) -> Option<DisplayRowAppendProgress>;
}

impl DisplaySourceTextRequest {
    pub(crate) fn append_render_policy(self) -> DisplaySourceAppendRenderPolicy {
        self.render_plan().render_policy()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct DisplaySourceAppendRenderPlanResolver {
    complex_run: ComplexTextRunAdvanceResolver,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DisplaySourceRowAppendState {
    render_plan_resolver: DisplaySourceAppendRenderPlanResolver,
}

impl DisplaySourceRowAppendState {
    pub(crate) fn resolve_source_render_plan_to_text_row(
        &mut self,
        state: &mut TextRowSourceMeasureState<'_>,
        active_face_state: &DisplayRowActiveFaceState,
        frame: DisplayRowAppendFrame,
        source: DisplaySourceRenderPlanRequest<'_>,
        position: DisplayRowPosition,
        source_item: &DisplayItem,
    ) -> DisplaySourceAppendRenderPlan {
        let request =
            DisplaySourceTextPositionedRenderPlanRequest::new(source, position, source_item);
        self.render_plan_resolver
            .resolve_source_render_plan_request_to_text_row(
                state,
                active_face_state,
                frame,
                request,
            )
    }
}

impl DisplaySourceNaturalMeasurementRequest {
    fn measure_to_text_row(
        self,
        state: &mut TextRowSourceMeasureState<'_>,
        base_face: &ResolvedFace,
        face_id: FaceId,
        frame: DisplayRowAppendFrame,
        position: DisplayRowPosition,
        source_item: &DisplayItem,
    ) -> Option<f32> {
        let mut render_policy = DisplaySourceAppendRenderPolicy::natural();
        SingleDisplayItemAppendContext::new(base_face, face_id, frame).measure_width_with_policy(
            state,
            source_item.clone(),
            position,
            self.source_item().append_kind(),
            &mut render_policy,
        )
    }

    pub(crate) fn resolve_to_text_row(
        self,
        state: &mut TextRowSourceMeasureState<'_>,
        active_face_state: &DisplayRowActiveFaceState,
        frame: DisplayRowAppendFrame,
        position: DisplayRowPosition,
        source_item: &DisplayItem,
    ) -> f32 {
        if let Some(measured_width) = self.measure_to_text_row(
            state,
            active_face_state.resolved_face(),
            active_face_state.face_id(),
            frame.clone(),
            position,
            source_item,
        ) {
            return measured_width;
        }

        self.fallback().resolve_to_text_row(
            state.font_metrics(),
            active_face_state,
            &frame,
            position,
            self.source_item().source_char(),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplaySourcePreparedCharAppend {
    Special(DisplaySourceSpecialCharPreparedAppend),
    Text(DisplaySourceTextCharPreparedAppend),
}

impl DisplaySourcePreparedCharAppend {
    #[cfg(test)]
    pub(crate) fn into_text(self) -> Option<DisplaySourceTextCharPreparedAppend> {
        match self {
            Self::Text(prepared_append) => Some(prepared_append),
            Self::Special(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceTextCharPreparedAppend {
    plan: DisplaySourceTextCharAppendPlan,
}

impl DisplaySourceTextCharPreparedAppend {
    pub(crate) fn new(plan: DisplaySourceTextCharAppendPlan) -> Self {
        Self { plan }
    }

    fn advance_px(&self) -> f32 {
        self.plan.advance_px()
    }

    pub(crate) fn update_cursor_info_for_main_char(
        &self,
        target: &mut CursorCaptureState,
        byte_idx: usize,
    ) {
        update_cursor_info_for_main_char(target, byte_idx, self.advance_px());
    }

    pub(crate) fn apply_rendered_progress_to_walk_state(
        &self,
        append_progress: DisplayRowAppendProgress,
        ch: char,
        geometry: &DisplayRowGeometryState,
        trailing_whitespace: &mut TrailingWhitespaceRenderState,
        word_wrap: &mut WordWrapRenderState,
        progress: &mut DisplaySourceProgressState<'_>,
    ) {
        DisplaySourceTextCharAppendOutcome {
            progress: append_progress,
        }
        .apply_rendered_char_to_walk_state(
            trailing_whitespace,
            word_wrap,
            ch,
            geometry,
            progress,
        );
    }

    /// THE shared point->cursor capture for an ordinary text character: the
    /// buffer path (`buffer_source/char_render.rs`) and the item renderer
    /// both funnel through this method, with the row/col computation in
    /// `display_cursor.rs`. Buffer-only cursor cases with no item-renderer
    /// counterpart (line break at point, end-of-buffer, invisible region,
    /// hscroll truncation) keep their gates in
    /// `buffer_source/row_lifecycle.rs` — see the note on
    /// `BufferSourceLineBreakSourceAction`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn capture_cursor_info_for_main_char_if_point(
        &self,
        target: &mut CursorCaptureState,
        active_face_state: &DisplayRowActiveFaceState,
        geometry: &DisplayRowGeometryState,
        x_px: f32,
        byte_idx: usize,
        col: usize,
        is_tab: bool,
        charpos: i64,
        point_charpos: i64,
    ) {
        if target.should_capture_visible_glyph_at(charpos, point_charpos) {
            capture_cursor_info(
                target,
                self.cursor_info_for_main_char(
                    active_face_state,
                    geometry.text_position(x_px, byte_idx, col),
                    is_tab,
                ),
            );
        }
    }

    pub(crate) fn overflow_decision(
        &self,
        ch: char,
        right_edge_px: f32,
        wrap_mode: LineWrapMode,
        word_wrap: WordWrapRenderState,
    ) -> DisplayRowTextOverflowDecision {
        DisplayRowTextOverflowDecision::for_char(
            ch,
            self.plan.position.x_px(),
            self.advance_px(),
            right_edge_px,
            wrap_mode,
            word_wrap,
        )
    }

    pub(crate) fn overflow_action(
        &self,
        ch: char,
        right_edge_px: f32,
        wrap_mode: LineWrapMode,
        word_wrap: WordWrapRenderState,
    ) -> DisplaySourceTextCharOverflowAction {
        DisplaySourceTextCharOverflowAction::for_decision(self.overflow_decision(
            ch,
            right_edge_px,
            wrap_mode,
            word_wrap,
        ))
    }

    pub(crate) fn cursor_info_for_main_char(
        &self,
        active_face_state: &DisplayRowActiveFaceState,
        position: DisplayRowTextPosition,
        is_tab: bool,
    ) -> CapturedCursorInfo {
        self.cursor_info_for_main_char_with_slot_width(
            active_face_state,
            position,
            self.advance_px(),
            is_tab,
        )
    }

    pub(crate) fn cursor_info_for_main_char_with_slot_width(
        &self,
        active_face_state: &DisplayRowActiveFaceState,
        position: DisplayRowTextPosition,
        slot_width_px: f32,
        is_tab: bool,
    ) -> CapturedCursorInfo {
        CapturedCursorInfo::from_active_face_state(
            active_face_state,
            CapturedCursorPlacement::from_row_text_position(
                position,
                CapturedCursorSlotWidth::Explicit(slot_width_px),
                is_tab,
            ),
        )
    }

    pub(crate) fn append_to_text_row<C: DisplaySourceCharAppendContext + ?Sized>(
        self,
        context: &C,
        geometry: &DisplayRowGeometryState,
        state: &mut TextRowSourceRenderState<'_>,
    ) -> Option<DisplaySourceTextCharAppendOutcome> {
        let progress = context.append_source_char_plan_to_text_row(geometry, state, self.plan)?;
        Some(DisplaySourceTextCharAppendOutcome { progress })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_to_text_row_and_apply<C: DisplaySourceCharAppendContext + ?Sized>(
        self,
        context: &C,
        geometry: &DisplayRowGeometryState,
        ch: char,
        source_render: &mut TextRowSourceRenderState<'_>,
        trailing_whitespace: &mut TrailingWhitespaceRenderState,
        word_wrap: &mut WordWrapRenderState,
        progress: &mut DisplaySourceProgressState<'_>,
    ) -> DisplaySourceAppendContinuation {
        let Some(outcome) = self.append_to_text_row(context, geometry, source_render) else {
            return DisplaySourceAppendContinuation::Stopped;
        };
        outcome.apply_rendered_char_to_walk_state(
            trailing_whitespace,
            word_wrap,
            ch,
            geometry,
            progress,
        );
        DisplaySourceAppendContinuation::Rendered
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceTextCharAppendOutcome {
    progress: DisplayRowAppendProgress,
}

impl DisplaySourceTextCharAppendOutcome {
    pub(crate) fn apply_to_text_row_state(
        &self,
        trailing_whitespace: &mut TrailingWhitespaceRenderState,
        ch: char,
        geometry: &DisplayRowGeometryState,
        x: &mut f32,
        col: &mut usize,
    ) {
        trailing_whitespace
            .track_rendered_char(ch, geometry.start_marker_at_x(self.progress.start().x_px()));
        *x = self.progress.end().x_px();
        *col = self.progress.end().col();
    }

    pub(crate) fn apply_rendered_char_to_walk_state(
        &self,
        trailing_whitespace: &mut TrailingWhitespaceRenderState,
        word_wrap: &mut WordWrapRenderState,
        ch: char,
        geometry: &DisplayRowGeometryState,
        progress: &mut DisplaySourceProgressState<'_>,
    ) {
        let (x, col) = progress.row_progress_mut().coordinates_mut();
        self.apply_to_text_row_state(trailing_whitespace, ch, geometry, x, col);
        progress.advance_charpos_by_one();
        word_wrap.allow_after_current_char(ch);
    }
}

impl DisplaySourceAppendRenderPlanResolver {
    fn resolve_source_render_plan_request_to_text_row(
        &mut self,
        state: &mut TextRowSourceMeasureState<'_>,
        active_face_state: &DisplayRowActiveFaceState,
        frame: DisplayRowAppendFrame,
        request: DisplaySourceTextPositionedRenderPlanRequest<'_, '_>,
    ) -> DisplaySourceAppendRenderPlan {
        let ch = request.cluster().ch();
        match request.measurement_kind() {
            DisplaySourceAppendMeasurementKind::ResolvedComplexRun => {
                let advance_px = active_face_state.complex_text_run_advance(
                    state.font_metrics(),
                    &mut self.complex_run,
                    request.text(),
                    request.byte_idx(),
                    ch,
                    request.cluster().is_cluster_continuation(),
                );
                DisplaySourceAppendRenderPlan::resolved_advance(advance_px)
            }
            DisplaySourceAppendMeasurementKind::NaturalRenderedSource => {
                let advance_px = DisplaySourceNaturalMeasurementRequest::for_range_and_cluster(
                    request.range(),
                    request.cluster(),
                )
                .resolve_to_text_row(
                    state,
                    active_face_state,
                    frame,
                    request.position(),
                    request.source_item(),
                );
                DisplaySourceAppendRenderPlan::natural(advance_px)
            }
        }
    }
}

impl DisplaySourceStepChar {
    #[cfg(test)]
    pub(crate) fn record_word_wrap_candidate(
        self,
        word_wrap: &mut WordWrapRenderState,
        source_render: &TextRowSourceRenderState<'_>,
    ) {
        self.record_word_wrap_candidate_at(
            word_wrap,
            source_render,
            DisplayRowPosition::default(),
            None,
        );
    }

    pub(crate) fn record_word_wrap_candidate_at(
        self,
        word_wrap: &mut WordWrapRenderState,
        source_render: &TextRowSourceRenderState<'_>,
        row_position: DisplayRowPosition,
        row_extend: Option<DisplayRowExtendFace>,
    ) {
        word_wrap.record_source_candidate(
            self.ch(),
            DisplaySourceTextPosition::new(self.start_byte_idx(), self.start_charpos()),
            source_render,
            row_position,
            row_extend,
        );
    }
}

impl WordWrapRenderState {
    /// Save one GNU-style word-wrap checkpoint before a source character is
    /// appended.  Glyph output, display-point metadata, and the authoritative
    /// buffer position are captured together so every source producer uses one
    /// atomic boundary.
    pub(crate) fn record_source_candidate(
        &mut self,
        ch: char,
        position: DisplaySourceTextPosition,
        source_render: &TextRowSourceRenderState<'_>,
        row_position: DisplayRowPosition,
        row_extend: Option<DisplayRowExtendFace>,
    ) {
        if self.can_record_candidate(ch) {
            // The candidate char has not yet been pushed to the row, so this
            // snapshots everything up to (and including) the preceding
            // whitespace.  GNU's SAVE_IT sits at this same point.
            let glyph_checkpoint = source_render.capture_glyph_checkpoint();
            let output_emitter = source_render.output_emitter_ref();
            self.record_candidate_at(
                ch,
                position,
                output_emitter.display_point_len(),
                output_emitter.current_row_display_positions(),
                glyph_checkpoint,
                row_position,
                row_extend,
            );
        }
    }
}

impl DisplaySpecialSourceCharRequest {
    pub(crate) fn append_plan_at(
        &self,
        position: DisplayRowPosition,
        display_item: DisplayItem,
    ) -> DisplaySourceSpecialCharAppendPlan {
        DisplaySourceSpecialCharAppendPlan::new(self.source_item_request(), position, display_item)
    }

    pub(crate) fn prepared_append_at(
        self,
        position: DisplayRowPosition,
        measured_width_px: Option<f32>,
        display_item: DisplayItem,
    ) -> DisplaySourceSpecialCharPreparedAppend {
        DisplaySourceSpecialCharPreparedAppend::new(
            self.kind(),
            self.append_plan_at(position, display_item),
            measured_width_px,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceSpecialCharPreparedAppend {
    kind: DisplaySourceSpecialDisplayKind,
    append_plan: DisplaySourceSpecialCharAppendPlan,
    measured_width_px: Option<f32>,
}

impl DisplaySourceSpecialCharPreparedAppend {
    pub(crate) fn new(
        kind: DisplaySourceSpecialDisplayKind,
        append_plan: DisplaySourceSpecialCharAppendPlan,
        measured_width_px: Option<f32>,
    ) -> Self {
        Self {
            kind,
            append_plan,
            measured_width_px,
        }
    }

    #[cfg(test)]
    pub(crate) fn kind(&self) -> DisplaySourceSpecialDisplayKind {
        self.kind
    }

    #[cfg(test)]
    pub(crate) fn display_item(&self) -> &DisplayItem {
        self.append_plan.display_item()
    }

    fn prepare_append_policy(&self) -> DisplaySourceSpecialCharAppendPolicy {
        DisplaySourceSpecialCharAppendPolicy {
            invalidate_face_after_append: self.kind.invalidates_face_after_append(),
        }
    }

    fn measured_width_px(&self) -> Option<f32> {
        self.measured_width_px
    }

    pub(crate) fn overflow_decision(
        &self,
        x_px: f32,
        right_edge_px: f32,
        wrap_mode: LineWrapMode,
    ) -> Option<SpecialTextRowOverflowDecision> {
        Some(SpecialTextRowOverflowDecision::for_width(
            x_px,
            self.measured_width_px()?,
            right_edge_px,
            wrap_mode,
        ))
    }

    pub(crate) fn overflow_action(
        &self,
        x_px: f32,
        right_edge_px: f32,
        wrap_mode: LineWrapMode,
    ) -> Option<DisplaySourceSpecialCharOverflowAction> {
        Some(DisplaySourceSpecialCharOverflowAction::for_decision(
            self.overflow_decision(x_px, right_edge_px, wrap_mode)?,
        ))
    }

    pub(crate) fn append_to_text_row<C: DisplaySourceCharAppendContext + ?Sized>(
        self,
        context: &C,
        geometry: &DisplayRowGeometryState,
        params: &WindowParams,
        face_ids: &mut FrameFaceAttempt,
        state: &mut TextRowSourceRenderState<'_>,
    ) -> Option<DisplaySourceSpecialCharAppendOutcome> {
        // The escape-glyph / nobreak face merge is realized earlier, in
        // `resolve_source_item_layout_for_active_face`, so the special-char
        // append no longer needs to allocate a policy face here.
        let _ = (params, face_ids);
        let append_policy = self.prepare_append_policy();
        let glyph_checkpoint = state.capture_glyph_checkpoint();
        let progress = context.append_special_source_char_plan_to_text_row_and_emit(
            geometry,
            state,
            self.append_plan,
        )?;
        let first_glyph = state.first_text_glyph_after_checkpoint(glyph_checkpoint);
        Some(DisplaySourceSpecialCharAppendOutcome {
            progress,
            append_policy,
            first_glyph,
        })
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn append_to_text_row_and_apply<C: DisplaySourceCharAppendContext + ?Sized>(
        self,
        context: &C,
        geometry: &DisplayRowGeometryState,
        params: &WindowParams,
        face_ids: &mut FrameFaceAttempt,
        source_render: &mut TextRowSourceRenderState<'_>,
        face_scan: &mut FaceScanCheckpoint,
        word_wrap: &mut WordWrapRenderState,
        progress: &mut DisplaySourceProgressState<'_>,
    ) -> DisplaySourceAppendContinuation {
        let Some(outcome) =
            self.append_to_text_row(context, geometry, params, face_ids, source_render)
        else {
            return DisplaySourceAppendContinuation::Stopped;
        };
        outcome.apply_rendered_special_char_to_walk_state(face_scan, word_wrap, progress);
        DisplaySourceAppendContinuation::Rendered
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DisplaySourceSpecialCharAppendPolicy {
    invalidate_face_after_append: bool,
}

impl DisplaySourceSpecialCharAppendPolicy {
    fn invalidates_face_after_append(self) -> bool {
        self.invalidate_face_after_append
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceSpecialCharAppendOutcome {
    progress: DisplayRowAppendProgress,
    append_policy: DisplaySourceSpecialCharAppendPolicy,
    first_glyph: Option<neomacs_display_protocol::glyph_matrix::Glyph>,
}

impl DisplaySourceSpecialCharAppendOutcome {
    pub(crate) fn capture_cursor_info_for_main_char_if_point(
        &self,
        target: &mut CursorCaptureState,
        geometry: &DisplayRowGeometryState,
        face_ids: &FrameFaceAttempt,
        byte_idx: usize,
        charpos: i64,
        point_charpos: i64,
    ) {
        if !target.should_capture_visible_glyph_at(charpos, point_charpos) {
            return;
        }
        let (Some(first_slot), Some(first_glyph)) =
            (self.progress.slots().first(), self.first_glyph.as_ref())
        else {
            return;
        };
        let Some(face) = face_ids.face(first_glyph.face_id) else {
            return;
        };
        capture_cursor_info(
            target,
            CapturedCursorInfo::from_rendered_glyph(
                first_glyph,
                &face,
                CapturedCursorPlacement::from_row_text_position(
                    geometry.text_position(first_slot.x_px(), byte_idx, first_slot.col()),
                    CapturedCursorSlotWidth::Explicit(first_glyph.pixel_width),
                    false,
                ),
            ),
        );
    }

    pub(crate) fn apply_to_text_row_state(
        &self,
        face_scan: &mut FaceScanCheckpoint,
        x: &mut f32,
        col: &mut usize,
    ) {
        if self.append_policy.invalidates_face_after_append() {
            face_scan.invalidate();
        }
        *x = self.progress.end().x_px();
        *col = self.progress.end().col();
    }

    pub(crate) fn apply_rendered_special_char_to_walk_state(
        &self,
        face_scan: &mut FaceScanCheckpoint,
        word_wrap: &mut WordWrapRenderState,
        progress: &mut DisplaySourceProgressState<'_>,
    ) {
        let (x, col) = progress.row_progress_mut().coordinates_mut();
        self.apply_to_text_row_state(face_scan, x, col);
        progress.advance_charpos_by_one();
        word_wrap.disallow_after_current_char();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceSpecialCharAppendPlan {
    source_item: DisplaySourceItemRequest,
    position: DisplayRowPosition,
    display_item: DisplayItem,
}

impl DisplaySourceSpecialCharAppendPlan {
    pub(crate) fn new(
        source_item: DisplaySourceItemRequest,
        position: DisplayRowPosition,
        display_item: DisplayItem,
    ) -> Self {
        Self {
            source_item,
            position,
            display_item,
        }
    }

    #[cfg(test)]
    pub(crate) fn display_item(&self) -> &DisplayItem {
        &self.display_item
    }

    pub(crate) fn into_append_request(
        self,
    ) -> (DisplayItem, DisplayRowPosition, DisplayRowAppendKind) {
        let fallback_kind = self.source_item.append_kind();
        (self.display_item, self.position, fallback_kind)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceTextCharAppendPlan {
    source_text: DisplaySourceTextRequest,
    position: DisplayRowPosition,
    source_item: DisplayItem,
}

impl DisplaySourceTextCharAppendPlan {
    pub(crate) fn new(
        source_text: DisplaySourceTextRequest,
        position: DisplayRowPosition,
        source_item: DisplayItem,
    ) -> Self {
        Self {
            source_text,
            position,
            source_item,
        }
    }

    pub(crate) fn from_render_plan(
        source: DisplaySourceRenderPlanRequest<'_>,
        position: DisplayRowPosition,
        source_item: &DisplayItem,
        render_plan: DisplaySourceAppendRenderPlan,
    ) -> Self {
        Self::new(
            source.into_text_request(render_plan),
            position,
            source_item.clone(),
        )
    }

    fn advance_px(&self) -> f32 {
        self.source_text.advance_px()
    }

    pub(crate) fn into_render_request(
        self,
    ) -> (
        DisplayItem,
        DisplayRowPosition,
        DisplayRowAppendKind,
        DisplaySourceAppendRenderPolicy,
    ) {
        let fallback_kind = self.source_text.source_item().append_kind();
        let render_policy = self.source_text.append_render_policy();
        (
            self.source_item,
            self.position,
            fallback_kind,
            render_policy,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DisplaySourceTextPositionedRenderPlanRequest<'text, 'item> {
    source: DisplaySourceRenderPlanRequest<'text>,
    position: DisplayRowPosition,
    source_item: &'item DisplayItem,
}

impl<'text, 'item> DisplaySourceTextPositionedRenderPlanRequest<'text, 'item> {
    fn new(
        source: DisplaySourceRenderPlanRequest<'text>,
        position: DisplayRowPosition,
        source_item: &'item DisplayItem,
    ) -> Self {
        Self {
            source,
            position,
            source_item,
        }
    }

    fn text(self) -> &'text [u8] {
        self.source.text()
    }

    fn byte_idx(self) -> usize {
        self.source.byte_idx()
    }

    fn range(self) -> DisplaySourceTextRange {
        self.source.range()
    }

    fn position(self) -> DisplayRowPosition {
        self.position
    }

    fn cluster(self) -> DisplaySourceClusterState {
        self.source.cluster()
    }

    fn measurement_kind(self) -> DisplaySourceAppendMeasurementKind {
        self.source.measurement_kind()
    }

    fn source_item(self) -> &'item DisplayItem {
        self.source_item
    }
}

impl DisplaySourceAppendItem {
    pub(crate) fn append_kind(&self) -> DisplayRowAppendKind {
        match self {
            Self::ControlChar { .. } => DisplayRowAppendKind::ControlChar,
            Self::SourceMappedText { .. } => DisplayRowAppendKind::SourceMappedText,
            Self::Glyphless { .. } => DisplayRowAppendKind::Glyphless,
        }
    }
}

impl crate::display_source::DisplaySourceTextItemRequest {
    pub(crate) fn append_kind(self) -> DisplayRowAppendKind {
        if self.source_char() == '\t' {
            DisplayRowAppendKind::Tab
        } else {
            DisplayRowAppendKind::SourceText
        }
    }
}

impl DisplaySourceItemRequest {
    pub(crate) fn append_kind(&self) -> DisplayRowAppendKind {
        self.item().append_kind()
    }
}

pub(crate) struct DisplaySourceItemAppendContext<'a> {
    single_item: SingleDisplayItemAppendContext<'a>,
    face_attempt: FrameFaceAttempt,
}

impl<'a> DisplaySourceItemAppendContext<'a> {
    pub(crate) fn with_face_attempt(
        face_id: FaceId,
        base_face: &'a ResolvedFace,
        frame: DisplayRowAppendFrame,
        face_attempt: FrameFaceAttempt,
    ) -> Self {
        Self {
            single_item: SingleDisplayItemAppendContext::new(base_face, face_id, frame),
            face_attempt,
        }
    }

    #[cfg(test)]
    pub(crate) fn new(
        face_id: FaceId,
        base_face: &'a ResolvedFace,
        frame: DisplayRowAppendFrame,
    ) -> Self {
        Self::with_face_attempt(
            face_id,
            base_face,
            frame,
            FrameFaceArena::default().begin_attempt(),
        )
    }

    #[cfg(test)]
    pub(crate) fn face_id(&self) -> FaceId {
        self.single_item.face_id()
    }

    #[cfg(test)]
    pub(crate) fn frame(&self) -> &DisplayRowAppendFrame {
        self.single_item.frame()
    }

    pub(crate) fn append_display_item_to_text_row_and_emit(
        &self,
        state: &mut TextRowSourceRenderState<'_>,
        item: DisplayItem,
        position: DisplayRowPosition,
        fallback_kind: DisplayRowAppendKind,
    ) -> Option<DisplayRowAppendProgress> {
        let mut face_ids = self.face_attempt.clone();
        self.single_item
            .render_naturally(state, &mut face_ids, item, position, fallback_kind)
    }

    #[cfg(test)]
    pub(crate) fn measure_display_item_width_naturally(
        &self,
        state: &mut TextRowSourceMeasureState<'_>,
        item: &DisplayItem,
        position: DisplayRowPosition,
        fallback_kind: DisplayRowAppendKind,
    ) -> Option<f32> {
        self.single_item
            .measure_width_naturally(state, item.clone(), position, fallback_kind)
    }

    pub(crate) fn measure_source_display_item_width_to_text_row(
        &self,
        state: &mut TextRowSourceMeasureState<'_>,
        item: &DisplayItem,
        source_item: DisplaySourceItemRequest,
        position: DisplayRowPosition,
    ) -> f32 {
        self.single_item.measure_width_with_source_fallback(
            state,
            item.clone(),
            position,
            source_item.append_kind(),
            source_item.fallback_width(),
        )
    }
}
