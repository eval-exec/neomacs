//! Buffer source overflow rendering.
//!
//! This module owns the buffer source overflow lifecycle for main text and
//! special display items while delegating actual item appends to the shared row
//! source append pipeline.

use crate::buffer_source::loop_state::BufferSourceLoopMutableState;
use crate::buffer_source::walk::{BufferSourceRewind, BufferSourceWalk};
use crate::display_row::append_context::RightEdgeMarkerColumn;
use crate::display_row::builder::DisplayRowGlyphCheckpoint;
use crate::display_row::builder::DisplayRowPosition;
use crate::display_row::geometry::{
    DisplayRowExtendState, DisplayRowGeometryDefaults, DisplayRowGeometryState, DisplayRowHitRange,
    DisplayRowLimit, DisplayRowVisibilityLimit,
};
use crate::display_row::metrics::DisplayRowMeasuredFaceMetrics;
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::transition::{
    DisplayRowOverflowTransitionPlan, DisplayRowTextWindowEmitContext,
    DisplayRowTransitionContinuation, DisplayRowTransitionRenderState,
};
use crate::display_row::walk_state::{
    FaceScanCheckpoint, HitRowRangeTracker, LineNumberRenderState, WordWrapBreakCandidate,
    WordWrapRenderState, sync_position_after_row_transition,
};
use crate::display_source::{DisplaySourceStepChar, DisplaySourceTextPosition};
use crate::display_source_item_append::{
    DisplaySourceSpecialCharPreparedAppend, DisplaySourceTextCharPreparedAppend,
};
use crate::display_source_overflow::{
    DisplaySourceSpecialCharOverflowAction, DisplaySourceTextCharOverflowAction,
};
use crate::neovm_bridge::LayoutBufferView;
use crate::types::LineWrapMode;
use crate::window_output::{DisplayTextRowTransition, WindowOutputEmitter};
use neomacs_display_protocol::types::Color;
use neovm_core::buffer::EmacsBytePos;
use neovm_core::buffer::LispCharPos1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferSourceOverflowRenderRequest<'a> {
    prepared_append: &'a DisplaySourceTextCharPreparedAppend,
    source_step_char: DisplaySourceStepChar,
    context: BufferSourceOverflowRenderContext,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferSourceOverflowRenderContext {
    ch: char,
    right_edge_px: f32,
    right_edge_marker_column: RightEdgeMarkerColumn,
    wrap_mode: LineWrapMode,
    word_wrap: WordWrapRenderState,
    row_visibility_limit: DisplayRowVisibilityLimit,
    content_x: f32,
    has_prefix: bool,
    row_geometry_defaults: DisplayRowGeometryDefaults,
    display_text_row_base: usize,
    max_rows: usize,
    row_limit: DisplayRowLimit,
    active_face_metrics: DisplayRowMeasuredFaceMetrics,
    frame_background: Color,
}

impl BufferSourceOverflowRenderContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        ch: char,
        right_edge_px: f32,
        right_edge_marker_column: RightEdgeMarkerColumn,
        wrap_mode: LineWrapMode,
        word_wrap: WordWrapRenderState,
        row_visibility_limit: DisplayRowVisibilityLimit,
        content_x: f32,
        has_prefix: bool,
        row_geometry_defaults: DisplayRowGeometryDefaults,
        display_text_row_base: usize,
        max_rows: usize,
        row_limit: DisplayRowLimit,
        active_face_metrics: DisplayRowMeasuredFaceMetrics,
        frame_background: Color,
    ) -> Self {
        Self {
            ch,
            right_edge_px,
            right_edge_marker_column,
            wrap_mode,
            word_wrap,
            row_visibility_limit,
            content_x,
            has_prefix,
            row_geometry_defaults,
            display_text_row_base,
            max_rows,
            row_limit,
            active_face_metrics,
            frame_background,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferSourceOverflowRenderOutcome {
    Fits,
    Transition(DisplayRowTransitionContinuation),
}

impl BufferSourceOverflowRenderOutcome {
    pub(crate) fn should_break(self) -> bool {
        matches!(
            self,
            Self::Transition(
                DisplayRowTransitionContinuation::Exhausted
                    | DisplayRowTransitionContinuation::Hidden
            )
        )
    }

    pub(crate) fn should_continue_buffer_walk(self) -> bool {
        matches!(
            self,
            Self::Transition(DisplayRowTransitionContinuation::Continue)
        )
    }
}

impl<'a> BufferSourceOverflowRenderRequest<'a> {
    /// Publish the slot of the right-edge truncation `$` / continuation `\\`
    /// this transition is about to hand to the marker installer.
    ///
    /// GNU's marker OVERWRITES the last glyph the row produced and reports
    /// nothing of its own (`produce_special_glyphs` zeroes `temp_it.current`
    /// and nils `temp_it.object`, src/xdisp.c:32989-32991); the column is
    /// answered by the WALK, which `buffer_posn_from_coords` re-runs
    /// (src/dispnew.c:6327).  Measured, GNU Emacs 31.0.90, 80x24 pty: a
    /// truncating row and a continuing row BOTH answer 80 for a click on their
    /// column 79, which is the character this port stopped before and GNU drew
    /// and then covered.
    ///
    /// Only when a column was RESERVED for the marker: without a reservation
    /// the last column holds a real glyph that already answers for itself, and
    /// a slot here would sit one column past the window.
    fn publish_right_edge_marker_slot(
        &self,
        source_render: &mut TextRowSourceRenderState<'_>,
        row_geometry: &DisplayRowGeometryState,
        marker_position: DisplayRowPosition,
    ) {
        if !self.context.right_edge_marker_column.is_reserved() {
            return;
        }
        let metrics = self.context.active_face_metrics;
        source_render
            .output_emitter()
            .push_text_overlaid_marker_point(
                LispCharPos1::new(self.source_step_char.start_charpos() + 1),
                marker_position.x_px(),
                row_geometry.y(),
                metrics.char_width(),
                row_geometry.height(),
                row_geometry.row(),
                marker_position.col(),
            );
    }
}

impl<'a> BufferSourceOverflowRenderRequest<'a> {
    pub(crate) fn new(
        prepared_append: &'a DisplaySourceTextCharPreparedAppend,
        source_step_char: DisplaySourceStepChar,
        context: BufferSourceOverflowRenderContext,
    ) -> Self {
        Self {
            prepared_append,
            source_step_char,
            context,
        }
    }

    pub(crate) fn render_if_needed_and_apply<B: LayoutBufferView>(
        self,
        source_walk: &mut BufferSourceWalk<'_, B>,
        text: &[u8],
        state: BufferSourceLoopMutableState<'_, '_, '_>,
    ) -> BufferSourceOverflowRenderOutcome {
        let BufferSourceLoopMutableState {
            mut progress,
            source_render,
            row_build,
            mut row_carryover,
            hit_capture,
            face_scan,
            row_y_positions,
            ..
        } = state;
        let mut source_render = source_render;
        let context = self.context;

        match self.prepared_append.overflow_action(
            context.ch,
            context.right_edge_px,
            context.wrap_mode,
            context.word_wrap,
        ) {
            DisplaySourceTextCharOverflowAction::Fits => BufferSourceOverflowRenderOutcome::Fits,
            DisplaySourceTextCharOverflowAction::Truncate { transition } => {
                self.publish_right_edge_marker_slot(
                    &mut source_render,
                    row_build.row_geometry,
                    progress.row_position(),
                );
                progress.reset_physical_line_tabs();
                let truncation_skip = source_walk
                    .consume_truncation_skip(text, progress.source_position())
                    .apply_to_progress(&mut progress);
                truncation_skip.apply_before_row_transition(
                    row_carryover.line_numbers,
                    row_build.row_extend,
                    progress.row_progress_mut().x_mut(),
                    context.content_x,
                );
                let row_position = progress.row_position();
                let row_transition = DisplayRowTextWindowEmitContext::from_source_render(
                    context.row_geometry_defaults,
                    context.display_text_row_base,
                    row_y_positions,
                    context.max_rows,
                    row_build.row_geometry,
                    row_build.row_flags,
                    context.row_limit,
                    hit_capture.hit_rows,
                    &mut source_render,
                )
                .emit_overflow_then_row_start(
                    transition,
                    hit_capture.hit_row_range.range_to(progress.charpos()),
                    row_position,
                    row_carryover.render_state(context.has_prefix),
                    progress.row_progress_mut().col_mut(),
                );
                BufferSourceOverflowRenderOutcome::Transition(
                    truncation_skip.transition_continuation(row_transition),
                )
            }
            DisplaySourceTextCharOverflowAction::WordWrap {
                break_candidate: wrap_break,
                transition,
            } => {
                let word_wrap_action = BufferSourceWordWrapAction::new(wrap_break);
                progress.continue_physical_line_after_visual_row(
                    word_wrap_action.row_position().x_px(),
                    context.content_x,
                );
                let mut source_position = progress.source_position();
                // Roll the current row's drawn glyphs back to the word-wrap
                // candidate (the word boundary) before extending the trailing
                // background and finishing the row. The chars between the
                // candidate and the overflow point already fit and were pushed to
                // the row; without this they would remain drawn, splitting the
                // word. GNU keeps whole words by rewinding its iterator to the
                // boundary; this is the glyph-side of that rewind. (The
                // display-point/hit metadata is rewound to the same boundary by
                // `apply_before_row_transition` below.)
                source_render.restore_glyph_checkpoint(word_wrap_action.glyph_checkpoint());
                word_wrap_action.restore_row_extend(row_build.row_extend, row_build.row_geometry);
                {
                    let box_vertical_edges = source_render.trailing_box_run_terminal();
                    // R2L (reversed_p) handled inside the mutation; pass `false`.
                    source_render.extend_face_to_end_of_line(
                        row_build.row_extend,
                        row_build.row_geometry,
                        word_wrap_action.row_position().x_px(),
                        context.right_edge_px,
                        context.frame_background,
                        box_vertical_edges,
                    );
                }
                let (x, col) = progress.row_progress_mut().coordinates_mut();
                word_wrap_action.apply_before_row_transition(
                    source_render.output_emitter(),
                    &mut source_position,
                    col,
                    row_build.row_extend,
                    x,
                    context.content_x,
                );
                // `apply_before_row_transition` rewound `source_position` (and,
                // above, the row's drawn glyphs + display points) back to the word
                // boundary; reseat the producer at the SAME boundary so the
                // candidate char is RE-produced on the continuation row. The
                // producer's position IS its resume state, so this is a plain
                // reseat — it used to also have to drop a stale queued run
                // remainder (candidate + 1) that would otherwise replay first and
                // drop the candidate char from the layout.
                source_walk
                    .rewind_source_consumption(BufferSourceRewind::WordWrap(source_position));
                source_walk
                    .source_position_update(source_position)
                    .apply_to_progress(&mut progress);
                let row_transition = DisplayRowTextWindowEmitContext::from_source_render(
                    context.row_geometry_defaults,
                    context.display_text_row_base,
                    row_y_positions,
                    context.max_rows,
                    row_build.row_geometry,
                    row_build.row_flags,
                    context.row_limit,
                    hit_capture.hit_rows,
                    &mut source_render,
                )
                .emit_overflow(
                    transition,
                    hit_capture.hit_row_range.range_to(progress.charpos()),
                    progress.row_position(),
                );
                let continuation = word_wrap_action.apply_after_row_transition_and_prefix(
                    row_transition,
                    transition,
                    &mut source_position,
                    hit_capture.hit_row_range,
                    face_scan,
                    row_build.row_geometry,
                    context.row_visibility_limit,
                    row_carryover.render_state(context.has_prefix),
                );
                // GNU `maybe_produce_line_number`: each wrapped continuation row
                // reserves a blank (no-number) line-number gutter so its text
                // aligns with the first row's text column. Re-arm the loop's
                // `row_carryover.line_numbers` so the next `render_row_prelude` emits it.
                row_carryover.line_numbers.mark_continuation_row();
                source_walk
                    .source_position_update(source_position)
                    .apply_to_progress(&mut progress);
                BufferSourceOverflowRenderOutcome::Transition(continuation)
            }
            DisplaySourceTextCharOverflowAction::CharacterWrap { transition } => {
                self.publish_right_edge_marker_slot(
                    &mut source_render,
                    row_build.row_geometry,
                    progress.row_position(),
                );
                let character_wrap_action =
                    BufferSourceCharacterWrapAction::from_source_step_char(self.source_step_char);
                let row_end_x = if context.ch == '\t' {
                    context.right_edge_px
                } else {
                    progress.row_progress().x()
                };
                progress.continue_physical_line_after_visual_row(row_end_x, context.content_x);
                {
                    let box_vertical_edges = source_render.trailing_box_run_terminal();
                    // R2L (reversed_p) handled inside the mutation; pass `false`.
                    source_render.extend_face_to_end_of_line(
                        row_build.row_extend,
                        row_build.row_geometry,
                        progress.row_progress().x(),
                        context.right_edge_px,
                        context.frame_background,
                        box_vertical_edges,
                    );
                }
                character_wrap_action.apply_before_row_transition(
                    row_build.row_extend,
                    progress.row_progress_mut().x_mut(),
                    context.content_x,
                );
                let mut source_position = progress.source_position();
                let row_position = progress.row_position();
                let row_transition = DisplayRowTextWindowEmitContext::from_source_render(
                    context.row_geometry_defaults,
                    context.display_text_row_base,
                    row_y_positions,
                    context.max_rows,
                    row_build.row_geometry,
                    row_build.row_flags,
                    context.row_limit,
                    hit_capture.hit_rows,
                    &mut source_render,
                )
                .emit_overflow_then_row_start(
                    transition,
                    hit_capture.hit_row_range.range_to(progress.charpos()),
                    row_position,
                    row_carryover.render_state(context.has_prefix),
                    progress.row_progress_mut().col_mut(),
                );
                let continuation = character_wrap_action.apply_after_visible_row_transition(
                    row_transition,
                    &mut source_position,
                    hit_capture.hit_row_range,
                    face_scan,
                    row_build.row_geometry,
                    context.row_visibility_limit,
                );
                if !matches!(continuation, DisplayRowTransitionContinuation::Exhausted) {
                    // `source_position` was just rewound to the overflowing
                    // char's start by the transition; reseat the producer to the
                    // same position so that char opens the continuation row.
                    source_walk.rewind_source_consumption(BufferSourceRewind::CharacterWrap(
                        source_position,
                    ));
                }
                // GNU `maybe_produce_line_number`: each wrapped continuation row
                // reserves a blank (no-number) line-number gutter so its text
                // aligns with the first row's text column. Re-arm the loop's
                // `row_carryover.line_numbers` so the next `render_row_prelude` emits it.
                row_carryover.line_numbers.mark_continuation_row();
                source_walk
                    .source_position_update(source_position)
                    .apply_to_progress(&mut progress);
                BufferSourceOverflowRenderOutcome::Transition(continuation)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceTruncationSkipAction {
    pub(crate) charpos: i64,
    pub(crate) reached_line_break: bool,
    pub(crate) source_position: DisplaySourceTextPosition,
}

impl BufferSourceTruncationSkipAction {
    pub(crate) fn consume_source_step_char_and_rest_of_line(
        text: &[u8],
        position: &mut DisplaySourceTextPosition,
    ) -> Self {
        let reached_line_break = position.consume_one_then_until_line_break(text);
        Self {
            charpos: position.charpos(),
            reached_line_break,
            source_position: *position,
        }
    }

    #[cfg(test)]
    pub(crate) fn charpos(self) -> i64 {
        self.charpos
    }

    pub(crate) fn source_position(self) -> DisplaySourceTextPosition {
        self.source_position
    }

    pub(crate) fn reached_line_break(self) -> bool {
        self.reached_line_break
    }

    pub(crate) fn apply_before_row_transition(
        self,
        line_numbers: &mut LineNumberRenderState,
        row_extend: &mut DisplayRowExtendState,
        x: &mut f32,
        content_x: f32,
    ) {
        if self.reached_line_break() {
            line_numbers.advance_line();
        }
        *x = content_x;
        row_extend.clear();
    }

    pub(crate) fn transition_continuation(
        self,
        row_transition: DisplayTextRowTransition,
    ) -> DisplayRowTransitionContinuation {
        if row_transition.is_exhausted() {
            DisplayRowTransitionContinuation::Exhausted
        } else {
            DisplayRowTransitionContinuation::Continue
        }
    }

    pub(crate) fn sync_after_row_transition_if_visible(
        self,
        row_transition: DisplayTextRowTransition,
        synced_charpos: i64,
        position: &mut DisplaySourceTextPosition,
        hit_row_range: &mut HitRowRangeTracker,
    ) -> DisplayRowTransitionContinuation {
        if row_transition.is_exhausted() {
            return DisplayRowTransitionContinuation::Exhausted;
        }
        sync_position_after_row_transition(synced_charpos, position, hit_row_range);
        DisplayRowTransitionContinuation::Continue
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferSourceWordWrapAction {
    break_candidate: WordWrapBreakCandidate,
}

impl BufferSourceWordWrapAction {
    pub(crate) fn new(break_candidate: WordWrapBreakCandidate) -> Self {
        Self { break_candidate }
    }

    pub(crate) fn glyph_checkpoint(self) -> DisplayRowGlyphCheckpoint {
        self.break_candidate.glyph_checkpoint()
    }

    pub(crate) fn row_position(self) -> crate::display_row::builder::DisplayRowPosition {
        self.break_candidate.row_position()
    }

    pub(crate) fn restore_row_extend(
        self,
        row_extend: &mut DisplayRowExtendState,
        row_geometry: &DisplayRowGeometryState,
    ) {
        if let Some(extend) = self.break_candidate.row_extend() {
            row_extend.activate(row_geometry.current_row_marker(), extend);
        } else {
            row_extend.clear();
        }
    }

    pub(crate) fn restore_row_output_progress(self, output_emitter: &mut WindowOutputEmitter) {
        output_emitter.truncate_display_points(self.break_candidate.display_point_count());
        let (row_first_display_pos, row_last_display_pos) =
            self.break_candidate.row_display_positions();
        output_emitter
            .restore_current_row_display_positions(row_first_display_pos, row_last_display_pos);
    }

    pub(crate) fn source_position(self) -> DisplaySourceTextPosition {
        self.break_candidate.source_position()
    }

    pub(crate) fn rewind_source_state(
        self,
        position: &mut DisplaySourceTextPosition,
        col: &mut usize,
    ) {
        *position = self.source_position();
        *col = 0;
    }

    pub(crate) fn apply_before_row_transition(
        self,
        output_emitter: &mut WindowOutputEmitter,
        position: &mut DisplaySourceTextPosition,
        col: &mut usize,
        row_extend: &mut DisplayRowExtendState,
        x: &mut f32,
        content_x: f32,
    ) {
        self.restore_row_output_progress(output_emitter);
        self.rewind_source_state(position, col);
        *x = content_x;
        row_extend.clear();
    }

    pub(crate) fn apply_after_row_transition(
        self,
        position: &mut DisplaySourceTextPosition,
        hit_row_range: &mut HitRowRangeTracker,
        face_scan: &mut FaceScanCheckpoint,
    ) {
        *position = self.source_position();
        hit_row_range.advance_to(position.charpos());
        face_scan.invalidate();
    }

    pub(crate) fn apply_after_row_transition_and_prefix(
        self,
        row_transition: DisplayTextRowTransition,
        transition: DisplayRowOverflowTransitionPlan,
        position: &mut DisplaySourceTextPosition,
        hit_row_range: &mut HitRowRangeTracker,
        face_scan: &mut FaceScanCheckpoint,
        row_geometry: &DisplayRowGeometryState,
        row_visibility_limit: DisplayRowVisibilityLimit,
        render_state: DisplayRowTransitionRenderState<'_>,
    ) -> DisplayRowTransitionContinuation {
        if row_transition.is_exhausted() {
            return DisplayRowTransitionContinuation::Exhausted;
        }
        self.apply_after_row_transition(position, hit_row_range, face_scan);
        render_state.apply_overflow_prefix(transition);
        DisplayRowTransitionContinuation::after_visible_row_transition(
            row_transition,
            row_geometry,
            row_visibility_limit,
        )
    }

    #[cfg(test)]
    pub(crate) fn byte_idx(self) -> usize {
        self.break_candidate.byte_idx()
    }

    #[cfg(test)]
    pub(crate) fn charpos(self) -> i64 {
        self.break_candidate.charpos()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceSpecialWrapAction {
    charpos: i64,
}

impl BufferSourceSpecialWrapAction {
    pub(crate) fn new(charpos: i64) -> Self {
        Self { charpos }
    }

    pub(crate) fn apply_before_row_transition(
        self,
        row_extend: &mut DisplayRowExtendState,
        x: &mut f32,
        content_x: f32,
    ) {
        *x = content_x;
        row_extend.clear();
    }

    pub(crate) fn hit_range_and_advance(
        self,
        hit_row_range: &mut HitRowRangeTracker,
    ) -> DisplayRowHitRange {
        let hit_range = hit_row_range.range_to(self.charpos);
        hit_row_range.advance_to(self.charpos);
        hit_range
    }

    pub(crate) fn transition_continuation(
        self,
        row_transition: DisplayTextRowTransition,
        row_geometry: &DisplayRowGeometryState,
        row_visibility_limit: DisplayRowVisibilityLimit,
    ) -> DisplayRowTransitionContinuation {
        DisplayRowTransitionContinuation::after_visible_row_transition(
            row_transition,
            row_geometry,
            row_visibility_limit,
        )
    }

    #[cfg(test)]
    pub(crate) fn charpos(self) -> i64 {
        self.charpos
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceCharacterWrapAction {
    ch_start_byte_idx: usize,
    ch_start_charpos: i64,
}

impl BufferSourceCharacterWrapAction {
    pub(crate) fn new(ch_start_byte_idx: usize, ch_start_charpos: i64) -> Self {
        Self {
            ch_start_byte_idx,
            ch_start_charpos,
        }
    }

    pub(crate) fn from_source_step_char(source_char: DisplaySourceStepChar) -> Self {
        Self::new(source_char.start_byte_idx(), source_char.start_charpos())
    }

    pub(crate) fn source_position(self) -> DisplaySourceTextPosition {
        DisplaySourceTextPosition::new(self.ch_start_byte_idx, self.ch_start_charpos)
    }

    pub(crate) fn rewind_source_state(self, position: &mut DisplaySourceTextPosition) {
        *position = self.source_position();
    }

    pub(crate) fn apply_before_row_transition(
        self,
        row_extend: &mut DisplayRowExtendState,
        x: &mut f32,
        content_x: f32,
    ) {
        *x = content_x;
        row_extend.clear();
    }

    pub(crate) fn apply_after_row_transition(
        self,
        position: &mut DisplaySourceTextPosition,
        hit_row_range: &mut HitRowRangeTracker,
        face_scan: &mut FaceScanCheckpoint,
    ) {
        self.rewind_source_state(position);
        hit_row_range.advance_to(position.charpos());
        face_scan.invalidate();
    }

    pub(crate) fn apply_after_visible_row_transition(
        self,
        row_transition: DisplayTextRowTransition,
        position: &mut DisplaySourceTextPosition,
        hit_row_range: &mut HitRowRangeTracker,
        face_scan: &mut FaceScanCheckpoint,
        row_geometry: &DisplayRowGeometryState,
        row_visibility_limit: DisplayRowVisibilityLimit,
    ) -> DisplayRowTransitionContinuation {
        if row_transition.is_exhausted() {
            return DisplayRowTransitionContinuation::Exhausted;
        }
        self.apply_after_row_transition(position, hit_row_range, face_scan);
        DisplayRowTransitionContinuation::after_visible_row_transition(
            row_transition,
            row_geometry,
            row_visibility_limit,
        )
    }
}

pub(crate) struct BufferSourceSpecialOverflowRenderRequest<'a> {
    prepared_append: &'a DisplaySourceSpecialCharPreparedAppend,
    context: BufferSourceSpecialOverflowRenderContext<'a>,
}

#[derive(Clone, Copy)]
pub(crate) struct BufferSourceSpecialOverflowRenderContext<'a> {
    text: &'a [u8],
    text_start_byte: usize,
    x_px: f32,
    right_edge_px: f32,
    wrap_mode: LineWrapMode,
    row_visibility_limit: DisplayRowVisibilityLimit,
    content_x: f32,
    has_prefix: bool,
    row_geometry_defaults: DisplayRowGeometryDefaults,
    display_text_row_base: usize,
    max_rows: usize,
    row_limit: DisplayRowLimit,
}

impl<'a> BufferSourceSpecialOverflowRenderContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        text: &'a [u8],
        text_start_byte: usize,
        x_px: f32,
        right_edge_px: f32,
        wrap_mode: LineWrapMode,
        row_visibility_limit: DisplayRowVisibilityLimit,
        content_x: f32,
        has_prefix: bool,
        row_geometry_defaults: DisplayRowGeometryDefaults,
        display_text_row_base: usize,
        max_rows: usize,
        row_limit: DisplayRowLimit,
    ) -> Self {
        Self {
            text,
            text_start_byte,
            x_px,
            right_edge_px,
            wrap_mode,
            row_visibility_limit,
            content_x,
            has_prefix,
            row_geometry_defaults,
            display_text_row_base,
            max_rows,
            row_limit,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferSourceSpecialOverflowRenderOutcome {
    Fits,
    AppendPrepared(DisplayRowTransitionContinuation),
    ContinueBufferWalk(DisplayRowTransitionContinuation),
}

impl BufferSourceSpecialOverflowRenderOutcome {
    pub(crate) fn should_break(self) -> bool {
        matches!(
            self,
            Self::AppendPrepared(
                DisplayRowTransitionContinuation::Exhausted
                    | DisplayRowTransitionContinuation::Hidden
            ) | Self::ContinueBufferWalk(
                DisplayRowTransitionContinuation::Exhausted
                    | DisplayRowTransitionContinuation::Hidden
            )
        )
    }

    pub(crate) fn should_continue_buffer_walk(self) -> bool {
        matches!(
            self,
            Self::ContinueBufferWalk(DisplayRowTransitionContinuation::Continue)
        )
    }
}

impl<'a> BufferSourceSpecialOverflowRenderRequest<'a> {
    pub(crate) fn new(
        prepared_append: &'a DisplaySourceSpecialCharPreparedAppend,
        context: BufferSourceSpecialOverflowRenderContext<'a>,
    ) -> Self {
        Self {
            prepared_append,
            context,
        }
    }

    pub(crate) fn render_if_needed_and_apply<B: LayoutBufferView>(
        self,
        source_walk: &mut BufferSourceWalk<'_, B>,
        buffer: &B,
        state: BufferSourceLoopMutableState<'_, '_, '_>,
    ) -> BufferSourceSpecialOverflowRenderOutcome {
        let BufferSourceLoopMutableState {
            mut progress,
            source_render,
            row_build,
            mut row_carryover,
            hit_capture,
            row_y_positions,
            ..
        } = state;
        let mut source_render = source_render;
        let context = self.context;

        match self.prepared_append.overflow_action(
            context.x_px,
            context.right_edge_px,
            context.wrap_mode,
        ) {
            None | Some(DisplaySourceSpecialCharOverflowAction::Fits) => {
                BufferSourceSpecialOverflowRenderOutcome::Fits
            }
            Some(DisplaySourceSpecialCharOverflowAction::Truncate { transition }) => {
                progress.reset_physical_line_tabs();
                let truncation_skip = source_walk
                    .consume_truncation_skip(context.text, progress.source_position())
                    .apply_to_progress(&mut progress);
                let mut source_position = truncation_skip.source_position();
                truncation_skip.apply_before_row_transition(
                    row_carryover.line_numbers,
                    row_build.row_extend,
                    progress.row_progress_mut().x_mut(),
                    context.content_x,
                );
                let row_position = progress.row_position();
                let row_transition = DisplayRowTextWindowEmitContext::from_source_render(
                    context.row_geometry_defaults,
                    context.display_text_row_base,
                    row_y_positions,
                    context.max_rows,
                    row_build.row_geometry,
                    row_build.row_flags,
                    context.row_limit,
                    hit_capture.hit_rows,
                    &mut source_render,
                )
                .emit_overflow_then_row_start(
                    transition,
                    hit_capture.hit_row_range.range_to(progress.charpos()),
                    row_position,
                    row_carryover.render_state(context.has_prefix),
                    progress.row_progress_mut().col_mut(),
                );
                let synced_charpos = buffer
                    .layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(
                        context.text_start_byte + source_position.byte_idx(),
                    ))
                    .get() as i64;
                let continuation = truncation_skip.sync_after_row_transition_if_visible(
                    row_transition,
                    synced_charpos,
                    &mut source_position,
                    hit_capture.hit_row_range,
                );
                source_walk
                    .source_position_update(source_position)
                    .apply_to_progress(&mut progress);
                BufferSourceSpecialOverflowRenderOutcome::ContinueBufferWalk(continuation)
            }
            Some(DisplaySourceSpecialCharOverflowAction::Wrap { transition }) => {
                let special_wrap_action = BufferSourceSpecialWrapAction::new(progress.charpos());
                progress.continue_physical_line_after_visual_row(
                    progress.row_progress().x(),
                    context.content_x,
                );
                special_wrap_action.apply_before_row_transition(
                    row_build.row_extend,
                    progress.row_progress_mut().x_mut(),
                    context.content_x,
                );
                let hit_range =
                    special_wrap_action.hit_range_and_advance(hit_capture.hit_row_range);
                let row_position = progress.row_position();
                let row_transition = DisplayRowTextWindowEmitContext::from_source_render(
                    context.row_geometry_defaults,
                    context.display_text_row_base,
                    row_y_positions,
                    context.max_rows,
                    row_build.row_geometry,
                    row_build.row_flags,
                    context.row_limit,
                    hit_capture.hit_rows,
                    &mut source_render,
                )
                .emit_overflow_then_row_start(
                    transition,
                    hit_range,
                    row_position,
                    row_carryover.render_state(context.has_prefix),
                    progress.row_progress_mut().col_mut(),
                );
                BufferSourceSpecialOverflowRenderOutcome::AppendPrepared(
                    special_wrap_action.transition_continuation(
                        row_transition,
                        row_build.row_geometry,
                        context.row_visibility_limit,
                    ),
                )
            }
        }
    }
}
