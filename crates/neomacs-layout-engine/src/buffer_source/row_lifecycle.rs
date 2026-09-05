//! Buffer source special-row lifecycle rendering.
//!
//! This module owns the buffer source row lifecycle actions that sit between
//! source walking and generic row/source append rendering: hscroll skip,
//! selective display, invisible text, line breaks, and end-of-buffer tails.

use crate::buffer_source::loop_state::BufferSourceLoopMutableState;
use crate::buffer_source::walk::BufferSourceWalk;
use crate::coords::layout_i64_char_pos_to_lisp_char_pos;
use crate::display_cursor::{
    CapturedCursorInfo, CapturedCursorPlacement, CapturedCursorSlotWidth, CursorCaptureState,
    capture_cursor_approximation,
};
use crate::display_row::append_context::DisplayRowAppendSurface;
use crate::display_row::builder::DisplayRowPosition;
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::geometry::{
    DisplayRowExtendState, DisplayRowGeometryDefaults, DisplayRowGeometryState, DisplayRowLimit,
    DisplayRowYPositions,
};
use crate::display_row::line_end::{
    LineEndContext, LineEndExtend, LineEndFillGeometry, LineEndIndicator,
};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::overlay_string::{
    BufferOverlayStringTextRowRenderContext, OverlayStringRenderPositions,
};
use crate::display_row::replacement::DisplayReplacementStringLineBreak;
use crate::display_row::source_append::{
    BufferSyntheticTextRenderContext, SyntheticTextAppendRequest, SyntheticTextMarker,
};
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::transition::{
    DisplayRowLineBreakTransitionPlan, DisplayRowTextWindowEmitContext,
    DisplayRowTransitionContinuation,
};
use crate::display_row::walk_state::{
    BoxFaceRowState, DisplayRowSourceStart, FaceScanCheckpoint, HorizontalScrollDisplayItem,
    HorizontalScrollSkipState, HorizontalScrollTruncationTarget, HorizontalScrollVisibleRemainder,
    HscrollConsumedTextDisposition, InvisibleTextScanCheckpoint, LineNumberRenderState,
    TrailingWhitespaceRenderState, sync_position_after_row_transition,
};
use crate::display_source::DisplaySourceStepChar;
use crate::display_source::DisplaySourceTextPosition;
use crate::display_source_progress::{DisplaySourceProgressState, DisplaySourceRowProgressState};
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{LayoutBufferView, RustTextPropAccess};
use crate::types::LayoutCharPos0;
use crate::unicode::is_wide_char;
use crate::window_output::{
    DisplayRowTerminator, DisplayRowTerminatorCell, DisplayTextRowTransition, WindowOutputEmitter,
};
use neomacs_display_protocol::types::Color;
use neovm_core::buffer::{EmacsBytePos, LispCharPos1};

#[derive(Clone, Copy)]
pub(crate) struct BufferSourceEndOfBufferTailRenderContext<'a> {
    byte_idx: usize,
    charpos: i64,
    accessible_end: i64,
    point_charpos: i64,
    overlay_context: BufferOverlayStringTextRowRenderContext<'a>,
    active_face_state: &'a DisplayRowActiveFaceState,
}

pub(crate) struct BufferSourceEndOfBufferTailRenderOutcome {
    point_is_visible_eob: bool,
}

impl BufferSourceEndOfBufferTailRenderOutcome {
    pub(crate) fn point_is_visible_eob(self) -> bool {
        self.point_is_visible_eob
    }
}

fn sync_row_extend_to_active_face(
    row_extend: &mut DisplayRowExtendState,
    row_geometry: &DisplayRowGeometryState,
    active_face_state: &DisplayRowActiveFaceState,
) {
    if let Some(fill) = active_face_state.row_extend_fill() {
        row_extend.activate(row_geometry.current_row_marker(), fill);
    } else {
        row_extend.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferSourceHscrollSkipAction {
    LineBreak {
        source_char: DisplaySourceStepChar,
    },
    Text {
        source_char: DisplaySourceStepChar,
        disposition: HscrollConsumedTextDisposition,
    },
}

impl BufferSourceHscrollSkipAction {
    pub(crate) fn is_line_break(self) -> bool {
        matches!(self, Self::LineBreak { .. })
    }

    pub(crate) fn ch_start_byte_idx(self) -> usize {
        match self {
            Self::LineBreak { source_char } | Self::Text { source_char, .. } => {
                source_char.start_byte_idx()
            }
        }
    }

    pub(crate) fn end_charpos(self) -> i64 {
        match self {
            Self::LineBreak { source_char } | Self::Text { source_char, .. } => {
                source_char.start_charpos() + 1
            }
        }
    }

    fn left_truncation_effect(
        self,
    ) -> Option<(
        HorizontalScrollTruncationTarget,
        HorizontalScrollVisibleRemainder,
    )> {
        match self {
            Self::Text {
                disposition:
                    HscrollConsumedTextDisposition::InstallLeftTruncation {
                        target,
                        visible_remainder,
                    },
                ..
            } => Some((target, visible_remainder)),
            Self::LineBreak { .. }
            | Self::Text {
                disposition: HscrollConsumedTextDisposition::Hidden,
                ..
            } => None,
        }
    }

    pub(crate) fn apply_line_break_before_row_transition(
        self,
        row_extend: &mut DisplayRowExtendState,
        output_emitter: &mut WindowOutputEmitter,
        x: &mut f32,
        content_x: f32,
    ) {
        if self.is_line_break() {
            *x = content_x;
            output_emitter.note_display_buffer_pos(LispCharPos1::new(self.end_charpos()));
            row_extend.clear();
        }
    }

    /// The charpos the continuation row starts at, taken before the tracker
    /// is advanced onto it so the two can never disagree.
    pub(crate) fn line_break_next_row_start(
        self,
        row_source_start: &mut DisplayRowSourceStart,
    ) -> Option<LayoutCharPos0> {
        if !self.is_line_break() {
            return None;
        }
        row_source_start.advance_to(self.end_charpos());
        Some(LayoutCharPos0::new(self.end_charpos()))
    }

    pub(crate) fn capture_line_break_cursor_if_point(
        self,
        target: &mut CursorCaptureState,
        active_face_state: &DisplayRowActiveFaceState,
        row_geometry: &DisplayRowGeometryState,
        point_charpos: i64,
        x: f32,
        col: usize,
        char_h: f32,
    ) {
        if !target.is_missing() || point_charpos != self.end_charpos() {
            return;
        }
        capture_cursor_approximation(
            target,
            CapturedCursorInfo::line_break_from_active_face_state(
                active_face_state,
                CapturedCursorPlacement::from_row_text_position(
                    row_geometry.text_position(x, self.ch_start_byte_idx(), col),
                    CapturedCursorSlotWidth::FaceChar,
                    false,
                ),
                char_h,
            ),
        );
    }

    pub(crate) fn apply_after_line_break_row_transition(
        self,
        row_transition: DisplayTextRowTransition,
        target: &mut CursorCaptureState,
        active_face_state: &DisplayRowActiveFaceState,
        row_geometry: &DisplayRowGeometryState,
        point_charpos: i64,
        x: f32,
        col: usize,
        char_h: f32,
    ) -> DisplayRowTransitionContinuation {
        if row_transition.is_exhausted() {
            return DisplayRowTransitionContinuation::Exhausted;
        }
        self.capture_line_break_cursor_if_point(
            target,
            active_face_state,
            row_geometry,
            point_charpos,
            x,
            col,
            char_h,
        );
        DisplayRowTransitionContinuation::Continue
    }

    pub(crate) fn capture_text_cursor_if_point(
        self,
        target: &mut CursorCaptureState,
        active_face_state: &DisplayRowActiveFaceState,
        row_geometry: &DisplayRowGeometryState,
        point_charpos: i64,
        x: f32,
        col: usize,
    ) {
        let Self::Text {
            source_char,
            disposition,
        } = self
        else {
            return;
        };
        if !target.is_missing() || point_charpos != disposition.cursor_anchor_charpos(source_char) {
            return;
        }
        capture_cursor_approximation(
            target,
            CapturedCursorInfo::from_active_face_state(
                active_face_state,
                CapturedCursorPlacement::from_row_text_position(
                    row_geometry.text_position(x, source_char.start_byte_idx(), col),
                    CapturedCursorSlotWidth::FaceChar,
                    false,
                ),
            ),
        );
    }

    pub(crate) fn append_left_truncation_marker_to_text_row_and_apply<'ctx>(
        self,
        render_context: BufferSyntheticTextRenderContext<'ctx>,
        row_geometry: &'ctx DisplayRowGeometryState,
        source_render: &mut TextRowSourceRenderState<'_>,
        mut row_progress: DisplaySourceRowProgressState<'_>,
        content_x: f32,
    ) -> Option<DisplayRowPosition> {
        let Some((target, visible_remainder)) = self.left_truncation_effect() else {
            return None;
        };
        let body_position = row_progress.row_position();

        // GNU produces the marker with `CHARPOS (truncate_it.position) = -1`
        // and `object = Qnil` (src/xdisp.c:23858-23860), then overwrites an
        // already-laid-out glyph.  In the ordinary case that glyph represents
        // the first visible source character.  With line numbers it is the
        // first structural prefix glyph, while the source walk is already at
        // the first visible character (`maybe_produce_line_number`,
        // xdisp.c:10182-10188, 10628-10634).
        let metrics = render_context.metrics();
        let active_face_id = render_context.active_face().face_id();
        let marker_x = match target {
            HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph => row_progress.x(),
            HorizontalScrollTruncationTarget::LineNumberPrefix => render_context.text_area_left(),
        };
        source_render
            .output_emitter()
            .push_text_overlaid_marker_point(
                LispCharPos1::new(self.end_charpos()),
                marker_x,
                row_geometry.y(),
                metrics.char_width(),
                row_geometry.height(),
                row_geometry.row(),
                match target {
                    HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph => row_progress.col(),
                    HorizontalScrollTruncationTarget::LineNumberPrefix => 0,
                },
            );
        append_hscroll_truncation_marker_to_text_row(
            render_context,
            row_geometry,
            source_render,
            &mut row_progress,
            content_x,
        );
        if target == HorizontalScrollTruncationTarget::LineNumberPrefix {
            source_render.install_leading_hscroll_marker_from_tail();
            // The appended marker was only a vehicle for normal face/glyph
            // construction.  Moving it over the prefix must not consume a
            // body column; the next source character starts at `content_x`.
            row_progress.apply_position(body_position);
        }
        let cursor_anchor = row_progress.row_position();
        if visible_remainder != HorizontalScrollVisibleRemainder::None {
            let end = source_render.append_hscroll_visible_remainder(
                visible_remainder,
                active_face_id,
                match self {
                    Self::Text { source_char, .. } => source_char.start_charpos().max(0) as usize,
                    Self::LineBreak { .. } => unreachable!("line breaks have no visible remainder"),
                },
                metrics.char_width(),
                cursor_anchor,
            );
            row_progress.apply_position(end);
        }
        Some(cursor_anchor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceEndOfBufferCursorAction {
    byte_idx: usize,
    charpos: i64,
    accessible_end: i64,
    point_charpos: i64,
}

impl BufferSourceEndOfBufferCursorAction {
    pub(crate) fn new(
        byte_idx: usize,
        charpos: i64,
        accessible_end: i64,
        point_charpos: i64,
    ) -> Self {
        Self {
            byte_idx,
            charpos,
            accessible_end,
            point_charpos,
        }
    }

    fn is_at_accessible_end(self) -> bool {
        self.charpos == self.accessible_end
    }

    fn point_is_visible_eob(self) -> bool {
        self.point_charpos == self.accessible_end && self.is_at_accessible_end()
    }

    pub(crate) fn capture_cursor_if_point(
        self,
        target: &mut CursorCaptureState,
        active_face_state: &DisplayRowActiveFaceState,
        row_geometry: &DisplayRowGeometryState,
        x: f32,
        col: usize,
    ) {
        if !target.is_missing()
            || (self.charpos != self.point_charpos && !self.point_is_visible_eob())
        {
            return;
        }
        if self.point_is_visible_eob() {
            tracing::debug!(
                "layout_window_rust: capturing EOB cursor at x={:.1} y={:.1} point={} point-max={}",
                x,
                row_geometry.glyph_y(0.0),
                self.point_charpos,
                self.accessible_end
            );
        }
        capture_cursor_approximation(
            target,
            CapturedCursorInfo::from_active_face_state(
                active_face_state,
                CapturedCursorPlacement::from_row_text_position(
                    row_geometry.text_position(x, self.byte_idx, col),
                    CapturedCursorSlotWidth::FaceChar,
                    false,
                ),
            ),
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceEndOfBufferTailAction {
    cursor: BufferSourceEndOfBufferCursorAction,
}

impl BufferSourceEndOfBufferTailAction {
    pub(crate) fn new(
        byte_idx: usize,
        charpos: i64,
        accessible_end: i64,
        point_charpos: i64,
    ) -> Self {
        Self {
            cursor: BufferSourceEndOfBufferCursorAction::new(
                byte_idx,
                charpos,
                accessible_end,
                point_charpos,
            ),
        }
    }

    pub(crate) fn point_is_visible_eob(self) -> bool {
        self.cursor.point_is_visible_eob()
    }

    fn is_at_accessible_end(self) -> bool {
        self.cursor.is_at_accessible_end()
    }

    pub(crate) fn capture_cursor_if_point(
        self,
        target: &mut CursorCaptureState,
        active_face_state: &DisplayRowActiveFaceState,
        row_geometry: &DisplayRowGeometryState,
        x: f32,
        col: usize,
    ) {
        self.cursor
            .capture_cursor_if_point(target, active_face_state, row_geometry, x, col);
    }
}

impl<'a> BufferSourceEndOfBufferTailRenderContext<'a> {
    pub(crate) fn new(
        byte_idx: usize,
        charpos: i64,
        accessible_end: i64,
        point_charpos: i64,
        overlay_context: BufferOverlayStringTextRowRenderContext<'a>,
        active_face_state: &'a DisplayRowActiveFaceState,
    ) -> Self {
        Self {
            byte_idx,
            charpos,
            accessible_end,
            point_charpos,
            overlay_context,
            active_face_state,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_and_apply<B: LayoutBufferView>(
        self,
        buffer: &B,
        source_render: TextRowSourceRenderState<'_>,
        row_progress: DisplaySourceRowProgressState<'_>,
        row_geometry: &mut DisplayRowGeometryState,
        cursor_info: &mut CursorCaptureState,
        row_source_start: &mut DisplayRowSourceStart,
        row_y_positions: &mut DisplayRowYPositions,
        face_ids: &mut FrameFaceAttempt,
        line_numbers: &mut LineNumberRenderState,
        face_scan: &mut FaceScanCheckpoint,
    ) -> BufferSourceEndOfBufferTailRenderOutcome {
        let mut row_progress = row_progress;
        let mut source_render = source_render;

        let tail = BufferSourceEndOfBufferTailAction::new(
            self.byte_idx,
            self.charpos,
            self.accessible_end,
            self.point_charpos,
        );
        let point_is_visible_eob = tail.point_is_visible_eob();
        tail.capture_cursor_if_point(
            cursor_info,
            self.active_face_state,
            row_geometry,
            row_progress.x(),
            row_progress.col(),
        );

        if tail.is_at_accessible_end() {
            let face_metrics = self.active_face_state.metrics();
            source_render.output_emitter().push_text_insertion_boundary(
                layout_i64_char_pos_to_lisp_char_pos(self.charpos),
                row_progress.x(),
                row_geometry.glyph_y(0.0),
                face_metrics.char_width(),
                row_geometry.height().max(face_metrics.row_height()),
                row_geometry.row(),
                row_progress.col(),
            );
        }

        // The END-OF-BUFFER anchor keeps its own call: the producer stops at
        // point-max, so an overlay string anchored there (the shape completion
        // UIs use) has no element to ride on. Folding it in needs the producer
        // to emit at its end position, which is P4.7/P4.8 territory.
        if self.overlay_context.should_render(row_geometry) {
            let (x, col) = row_progress.coordinates_mut();
            self.overlay_context.render_eob_anchor_strings_at_text_row(
                buffer,
                OverlayStringRenderPositions::from_layout_i64(self.charpos, self.point_charpos),
                self.active_face_state.resolved_face().box_type != 0,
                source_render.reborrow(),
                x,
                col,
                row_geometry,
                cursor_info,
                row_source_start,
                row_y_positions,
                face_ids,
                line_numbers,
                face_scan,
            );
        }

        BufferSourceEndOfBufferTailRenderOutcome {
            point_is_visible_eob,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct BufferSourceHscrollSkipRenderContext<'a> {
    text: &'a [u8],
    tab_width: i32,
    content_x: f32,
    append_surface: &'a DisplayRowAppendSurface,
    active_face_state: &'a DisplayRowActiveFaceState,
    metrics: DisplayRowFallbackMetrics,
    point_charpos: i64,
    has_prefix: bool,
    row_geometry_defaults: DisplayRowGeometryDefaults,
    display_text_row_base: usize,
    max_rows: usize,
    row_limit: DisplayRowLimit,
}

pub(crate) fn consume_hscroll_skip_from_position(
    text: &[u8],
    position: &mut DisplaySourceTextPosition,
    hscroll_skip: &mut HorizontalScrollSkipState,
    tab_width: i32,
) -> Option<BufferSourceHscrollSkipAction> {
    let source_char = position.consume_step_char(text)?;
    Some(consume_source_char_for_hscroll(
        source_char,
        hscroll_skip,
        tab_width,
    ))
}

fn consume_source_char_for_hscroll(
    source_char: DisplaySourceStepChar,
    hscroll_skip: &mut HorizontalScrollSkipState,
    tab_width: i32,
) -> BufferSourceHscrollSkipAction {
    if source_char.ch() == '\n' {
        return BufferSourceHscrollSkipAction::LineBreak { source_char };
    }

    let columns =
        hscroll_skip_column_width(source_char, tab_width, hscroll_skip.consumed_columns());
    let display_item = if source_char.ch() == '\t' {
        HorizontalScrollDisplayItem::tab(columns)
    } else {
        HorizontalScrollDisplayItem::glyph(source_char.ch(), columns)
    };
    let disposition = hscroll_skip.consume_display_item(display_item);
    BufferSourceHscrollSkipAction::Text {
        source_char,
        disposition,
    }
}

fn hscroll_skip_column_width(
    source_char: DisplaySourceStepChar,
    tab_width: i32,
    consumed_columns: i32,
) -> i32 {
    if source_char.ch() == '\t' {
        let tab_width = tab_width.max(1);
        return ((consumed_columns / tab_width + 1) * tab_width) - consumed_columns;
    }

    if is_wide_char(source_char.ch()) { 2 } else { 1 }
}

impl<'a> BufferSourceHscrollSkipRenderContext<'a> {
    pub(crate) fn render_next_and_apply<B: LayoutBufferView>(
        self,
        source_walk: &mut BufferSourceWalk<'_, B>,
        state: BufferSourceLoopMutableState<'_, '_, '_>,
    ) -> DisplayRowTransitionContinuation {
        let BufferSourceLoopMutableState {
            progress,
            mut row_carryover,
            row_build,
            source_render,
            row_source_start,
            cursor_info,
            row_y_positions,
            face_ids,
            ..
        } = state;
        let mut progress = progress;
        let mut source_render = source_render;
        let context = self;

        let Some(hscroll_action) = source_walk
            .consume_hscroll_skip(
                context.text,
                progress.source_position(),
                row_carryover.hscroll_skip,
                context.tab_width,
            )
            .apply_to_progress(&mut progress)
        else {
            return DisplayRowTransitionContinuation::Exhausted;
        };

        // GNU records a row's start BEFORE it skips the hscrolled columns
        // (`row->start = it->start`, src/xdisp.c:25857; the skip is at
        // :25878-25890), and `it->start` is the previous row's end
        // (src/xdisp.c:26855).  A truncating row hscrolled by any amount
        // therefore starts at its LINE start, which is why GNU's
        // `vertical-motion 0' answers the same position at every hscroll.  The
        // characters this skip consumes draw nothing, so they are the row's
        // START and never its END; only the first one takes.
        source_render
            .output_emitter()
            .note_row_walk_start(LispCharPos1::new(hscroll_action.end_charpos()));

        if hscroll_action.is_line_break() {
            progress.reset_physical_line_tabs();
            hscroll_action.apply_line_break_before_row_transition(
                row_build.row_extend,
                source_render.output_emitter(),
                progress.row_progress_mut().x_mut(),
                context.content_x,
            );
            let row_position = progress.row_position();
            let line_break_transition = DisplayRowLineBreakTransitionPlan::hscroll_line_break();
            let next_row_start = hscroll_action
                .line_break_next_row_start(row_source_start)
                .expect("hscroll line break next row start");
            let row_transition = DisplayRowTextWindowEmitContext::from_source_render(
                context.row_geometry_defaults,
                context.display_text_row_base,
                row_y_positions,
                context.max_rows,
                row_build.row_geometry,
                row_build.row_flags,
                context.row_limit,
                &mut source_render,
            )
            .emit_line_break_then_row_start(
                line_break_transition,
                next_row_start,
                row_position,
                0.0,
                row_carryover.render_state(context.has_prefix),
                progress.row_progress_mut().coordinates_mut().1,
            );
            return hscroll_action.apply_after_line_break_row_transition(
                row_transition,
                cursor_info,
                context.active_face_state,
                row_build.row_geometry,
                context.point_charpos,
                row_position.x_px(),
                row_position.col(),
                context.metrics.row_height(),
            );
        }

        let cursor_position = hscroll_action
            .append_left_truncation_marker_to_text_row_and_apply(
                BufferSyntheticTextRenderContext::with_face_attempt(
                    context.append_surface,
                    context.active_face_state,
                    0.0,
                    context.metrics,
                    face_ids.clone(),
                ),
                row_build.row_geometry,
                &mut source_render.reborrow(),
                progress.row_progress_mut().reborrow(),
                context.content_x,
            )
            .unwrap_or_else(|| progress.row_position());
        hscroll_action.capture_text_cursor_if_point(
            cursor_info,
            context.active_face_state,
            row_build.row_geometry,
            context.point_charpos,
            cursor_position.x_px(),
            cursor_position.col(),
        );
        DisplayRowTransitionContinuation::Continue
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        text: &'a [u8],
        tab_width: i32,
        content_x: f32,
        append_surface: &'a DisplayRowAppendSurface,
        active_face_state: &'a DisplayRowActiveFaceState,
        metrics: DisplayRowFallbackMetrics,
        point_charpos: i64,
        has_prefix: bool,
        row_geometry_defaults: DisplayRowGeometryDefaults,
        display_text_row_base: usize,
        max_rows: usize,
        row_limit: DisplayRowLimit,
    ) -> Self {
        Self {
            text,
            tab_width,
            content_x,
            append_surface,
            active_face_state,
            metrics,
            point_charpos,
            has_prefix,
            row_geometry_defaults,
            display_text_row_base,
            max_rows,
            row_limit,
        }
    }
}

pub(crate) struct BufferSourceSelectiveDisplayTailRenderRequest<'a> {
    source_char: DisplaySourceStepChar,
    context: BufferSourceSelectiveDisplayTailRenderContext<'a>,
}

#[derive(Clone, Copy)]
pub(crate) struct BufferSourceSelectiveDisplayTailRenderContext<'a> {
    text: &'a [u8],
    text_start_byte: usize,
    selective_display: i32,
    tab_width: i32,
    append_surface: &'a DisplayRowAppendSurface,
    active_face_state: &'a DisplayRowActiveFaceState,
    glyph_y_offset: f32,
    metrics: DisplayRowFallbackMetrics,
    content_x: f32,
    has_prefix: bool,
    row_geometry_defaults: DisplayRowGeometryDefaults,
    display_text_row_base: usize,
    max_rows: usize,
    row_limit: DisplayRowLimit,
}

#[derive(Clone, Copy)]
pub(crate) struct BufferSourceInvisibleTextRenderContext<'a> {
    text: &'a [u8],
    accessible_end: i64,
    point_charpos: i64,
    append_surface: &'a DisplayRowAppendSurface,
    active_face_state: &'a DisplayRowActiveFaceState,
    glyph_y_offset: f32,
    metrics: DisplayRowFallbackMetrics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferSourceSelectiveDisplayTailRenderOutcome {
    NotHidden,
    ContinueBufferWalk,
    Stop,
}

/// The invisible checkpoint never ends the row: it either leaves the position
/// visible or folds a hidden span and hands the walk back. Ending a row from a
/// property is the selective-display tail's job, which is why only
/// [`BufferSourceSelectiveDisplayTailRenderOutcome`] carries a `Stop`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferSourceInvisibleTextRenderOutcome {
    /// No hidden text begins at the current source position.
    Visible,
    /// GNU preserves an overlay string anchored at the entry to a hidden text
    /// property.  The caller must render the producer's insertion element
    /// before asking this checkpoint to commit the skip on the next iteration.
    RenderBoundaryOverlayStrings,
    /// The hidden span (and optional ellipsis) was consumed; restart the walk
    /// at its first visible position.
    HiddenSpanApplied,
}

impl BufferSourceSelectiveDisplayTailRenderOutcome {
    pub(crate) fn should_break(self) -> bool {
        matches!(self, Self::Stop)
    }

    pub(crate) fn should_continue_buffer_walk(self) -> bool {
        matches!(self, Self::ContinueBufferWalk)
    }
}

impl BufferSourceInvisibleTextRenderOutcome {
    // Intentionally no boolean projection: the visible loop must exhaustively
    // handle all three ordering states, so adding a fourth cannot silently
    // acquire the behavior of an existing branch.
}

impl<'a> BufferSourceSelectiveDisplayTailRenderContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        text: &'a [u8],
        text_start_byte: usize,
        selective_display: i32,
        tab_width: i32,
        append_surface: &'a DisplayRowAppendSurface,
        active_face_state: &'a DisplayRowActiveFaceState,
        glyph_y_offset: f32,
        metrics: DisplayRowFallbackMetrics,
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
            selective_display,
            tab_width,
            append_surface,
            active_face_state,
            glyph_y_offset,
            metrics,
            content_x,
            has_prefix,
            row_geometry_defaults,
            display_text_row_base,
            max_rows,
            row_limit,
        }
    }
}

impl<'a> BufferSourceInvisibleTextRenderContext<'a> {
    pub(crate) fn new(
        text: &'a [u8],
        accessible_end: i64,
        point_charpos: i64,
        append_surface: &'a DisplayRowAppendSurface,
        active_face_state: &'a DisplayRowActiveFaceState,
        glyph_y_offset: f32,
        metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self {
            text,
            accessible_end,
            point_charpos,
            append_surface,
            active_face_state,
            glyph_y_offset,
            metrics,
        }
    }
}

impl<'a> BufferSourceSelectiveDisplayTailRenderRequest<'a> {
    pub(crate) fn new(
        source_char: DisplaySourceStepChar,
        context: BufferSourceSelectiveDisplayTailRenderContext<'a>,
    ) -> Self {
        Self {
            source_char,
            context,
        }
    }

    pub(crate) fn render_if_needed_and_apply<B: LayoutBufferView>(
        self,
        source_walk: &mut BufferSourceWalk<'_, B>,
        buffer: &B,
        state: BufferSourceLoopMutableState<'_, '_, '_>,
    ) -> BufferSourceSelectiveDisplayTailRenderOutcome {
        let context = self.context;
        let selective_display = BufferSourceSelectiveDisplayContext::new(
            context.text,
            context.selective_display,
            context.tab_width,
        );
        let Some(marker) = selective_display.carriage_return_tail_marker(self.source_char.ch())
        else {
            return BufferSourceSelectiveDisplayTailRenderOutcome::NotHidden;
        };

        let BufferSourceLoopMutableState {
            mut progress,
            source_render,
            row_build,
            mut row_carryover,
            row_source_start,
            row_y_positions,
            face_ids,
            ..
        } = state;
        let mut source_render = source_render;
        let ellipsis_text = crate::neovm_bridge::buffer_invisible_ellipsis_text(buffer);

        marker.append_to_text_row_and_apply(
            BufferSyntheticTextRenderContext::with_face_attempt(
                context.append_surface,
                context.active_face_state,
                context.glyph_y_offset,
                context.metrics,
                face_ids.clone(),
            ),
            row_build.row_geometry,
            &mut source_render.reborrow(),
            progress.row_progress_mut().reborrow(),
            ellipsis_text.as_deref(),
        );

        let tail_action = source_walk
            .consume_selective_display_tail(selective_display, progress.source_position())
            .apply_to_progress(&mut progress);
        if !tail_action.is_line_break() {
            return BufferSourceSelectiveDisplayTailRenderOutcome::ContinueBufferWalk;
        }

        progress.reset_physical_line_tabs();
        tail_action.apply_hidden_line_break_row_state(
            row_build.row_geometry,
            row_build.row_extend,
            row_build.box_face,
            context.content_x,
            progress.row_progress_mut().x_mut(),
        );
        let row_position = progress.row_position();
        let line_break_transition = DisplayRowLineBreakTransitionPlan::hidden_line_break();
        let row_transition = DisplayRowTextWindowEmitContext::from_source_render(
            context.row_geometry_defaults,
            context.display_text_row_base,
            row_y_positions,
            context.max_rows,
            row_build.row_geometry,
            row_build.row_flags,
            context.row_limit,
            &mut source_render,
        )
        .emit_line_break_then_row_start(
            line_break_transition,
            LayoutCharPos0::new(progress.charpos()),
            row_position,
            0.0,
            row_carryover.render_state(context.has_prefix),
            progress.row_progress_mut().coordinates_mut().1,
        );
        let synced_charpos = buffer
            .layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(
                context.text_start_byte + progress.source_position().byte_idx(),
            ))
            .get() as i64;
        let mut synced_source_position = progress.source_position();
        let continuation = tail_action.apply_after_hidden_line_break_transition(
            row_transition,
            synced_charpos,
            &mut synced_source_position,
            row_source_start,
        );
        source_walk
            .source_position_update(synced_source_position)
            .apply_to_progress(&mut progress);
        if continuation.should_break() {
            return BufferSourceSelectiveDisplayTailRenderOutcome::Stop;
        }

        BufferSourceSelectiveDisplayTailRenderOutcome::ContinueBufferWalk
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferSourceInvisibleTextScanAction {
    Unchecked,
    Visible { next_visible: i64 },
    Hidden(BufferSourceInvisibleTextSkip),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceInvisibleTextSkip {
    start_byte_idx: usize,
    start_charpos: i64,
    skip_to: i64,
    next_visible: i64,
    point_in_hidden_region: bool,
    ellipsis: bool,
    hidden_newline_count: usize,
}

impl BufferSourceInvisibleTextSkip {
    pub(crate) fn new(
        start_byte_idx: usize,
        start_charpos: i64,
        skip_to: i64,
        next_visible: i64,
        point_in_hidden_region: bool,
        ellipsis: bool,
        hidden_newline_count: usize,
    ) -> Self {
        Self {
            start_byte_idx,
            start_charpos,
            skip_to,
            next_visible,
            point_in_hidden_region,
            ellipsis,
            hidden_newline_count,
        }
    }

    #[cfg(test)]
    pub(crate) fn start_byte_idx(self) -> usize {
        self.start_byte_idx
    }

    #[cfg(test)]
    pub(crate) fn start_charpos(self) -> i64 {
        self.start_charpos
    }

    #[cfg(test)]
    pub(crate) fn skip_to(self) -> i64 {
        self.skip_to
    }

    #[cfg(test)]
    pub(crate) fn next_visible(self) -> i64 {
        self.next_visible
    }

    #[cfg(test)]
    pub(crate) fn point_in_hidden_region(self) -> bool {
        self.point_in_hidden_region
    }

    #[cfg(test)]
    pub(crate) fn ellipsis(self) -> bool {
        self.ellipsis
    }

    #[cfg(test)]
    pub(crate) fn hidden_newline_count(self) -> usize {
        self.hidden_newline_count
    }

    /// GNU `maybe_produce_line_number` derives the displayed line number from a
    /// raw BUFFER newline count, so newlines inside an invisible (folded) region
    /// are still counted. Advance the line number by the newlines this skip
    /// crossed so the next visible row shows its true buffer line — folding
    /// buffer lines 2..12 makes the next row's gutter read 13, not 4. No render
    /// is re-armed: the hidden span produces no row, only a line-number offset
    /// (mirrors the selective-display hidden-line path).
    pub(crate) fn apply_to_line_numbers(self, line_numbers: &mut LineNumberRenderState) {
        for _ in 0..self.hidden_newline_count {
            line_numbers.advance_hidden_line();
        }
    }

    pub(crate) fn capture_cursor_if_point(
        self,
        target: &mut CursorCaptureState,
        active_face_state: &DisplayRowActiveFaceState,
        row_geometry: &DisplayRowGeometryState,
        x: f32,
        col: usize,
    ) {
        if !self.point_in_hidden_region {
            return;
        }
        capture_cursor_approximation(
            target,
            CapturedCursorInfo::from_active_face_state(
                active_face_state,
                CapturedCursorPlacement::from_row_text_position(
                    row_geometry.text_position(x, self.start_byte_idx, col),
                    CapturedCursorSlotWidth::FaceChar,
                    false,
                ),
            ),
        );
    }

    pub(crate) fn ellipsis_append_request(
        self,
        position: DisplayRowPosition,
        ellipsis_text: Option<&str>,
    ) -> Option<SyntheticTextAppendRequest> {
        self.ellipsis.then(|| {
            SyntheticTextAppendRequest::active_marker_with_text(
                position,
                SyntheticTextMarker::InvisibleEllipsis,
                ellipsis_text,
            )
        })
    }

    pub(crate) fn append_to_text_row_and_apply<'ctx>(
        self,
        render_context: BufferSyntheticTextRenderContext<'ctx>,
        row_geometry: &'ctx DisplayRowGeometryState,
        cursor_info: &mut CursorCaptureState,
        source_render: &mut TextRowSourceRenderState<'_>,
        row_progress: &mut DisplaySourceRowProgressState<'_>,
        ellipsis_text: Option<&str>,
    ) {
        let position = row_progress.row_position();
        self.capture_cursor_if_point(
            cursor_info,
            render_context.active_face(),
            row_geometry,
            position.x_px(),
            position.col(),
        );

        let Some(request) = self.ellipsis_append_request(position, ellipsis_text) else {
            return;
        };
        append_synthetic_request_to_text_row(
            render_context,
            row_geometry,
            source_render,
            row_progress,
            request,
        );
    }
}

impl<'a> BufferSourceInvisibleTextRenderContext<'a> {
    pub(crate) fn render_at_checkpoint_and_apply<B: LayoutBufferView>(
        self,
        source_walk: &mut BufferSourceWalk<'_, B>,
        buffer: &B,
        state: BufferSourceLoopMutableState<'_, '_, '_>,
    ) -> BufferSourceInvisibleTextRenderOutcome {
        let BufferSourceLoopMutableState {
            invisible_text_checkpoint,
            mut progress,
            source_render,
            row_build,
            cursor_info,
            face_ids,
            row_carryover,
            ..
        } = state;
        let mut source_render = source_render;
        let context = self;

        let scan_context = BufferSourceInvisibleTextScanContext::new(
            context.text,
            context.accessible_end,
            context.point_charpos,
            cursor_info.is_missing(),
        );

        // GNU xdisp.c `handle_invisible_prop` advances the buffer iterator but,
        // for text-property invisibility, calls `get_overlay_strings` with the
        // OLD stop position first.  Keep the insertion in the producer: merely
        // tell the visible loop to consume its next typed element before this
        // checkpoint is entered again to commit the hidden-span skip.
        if source_walk.has_pending_overlay_strings_at(progress.source_position())
            && scan_context.will_skip_at_checkpoint(
                buffer,
                invisible_text_checkpoint,
                progress.source_position(),
            )
        {
            return BufferSourceInvisibleTextRenderOutcome::RenderBoundaryOverlayStrings;
        }

        let action = source_walk
            .consume_invisible_checkpoint(
                buffer,
                scan_context,
                invisible_text_checkpoint,
                progress.source_position(),
            )
            .apply_to_progress(&mut progress);
        let BufferSourceInvisibleTextScanAction::Hidden(hidden_text) = action else {
            return BufferSourceInvisibleTextRenderOutcome::Visible;
        };

        // Advance display-line-numbers past the buffer lines this fold hid, so
        // the next visible row's gutter shows its true buffer line number (GNU
        // counts every buffer newline, including ones inside invisible text).
        hidden_text.apply_to_line_numbers(row_carryover.line_numbers);

        let ellipsis_text = crate::neovm_bridge::buffer_invisible_ellipsis_text(buffer);
        let mut row_progress = progress.row_progress_mut().reborrow();
        hidden_text.append_to_text_row_and_apply(
            BufferSyntheticTextRenderContext::with_face_attempt(
                context.append_surface,
                context.active_face_state,
                context.glyph_y_offset,
                context.metrics,
                face_ids.clone(),
            ),
            row_build.row_geometry,
            cursor_info,
            &mut source_render.reborrow(),
            &mut row_progress,
            ellipsis_text.as_deref(),
        );

        // The strings anchored at the post-skip position are NOT rendered here
        // since P4.6: the producer surfaces them as an element when it produces
        // at that position, which is the very next step and appends nothing in
        // between.
        BufferSourceInvisibleTextRenderOutcome::HiddenSpanApplied
    }
}

pub(crate) struct BufferSourceInvisibleTextScanContext<'a> {
    text: &'a [u8],
    accessible_end: i64,
    point_charpos: i64,
    cursor_missing: bool,
}

impl<'a> BufferSourceInvisibleTextScanContext<'a> {
    pub(crate) fn new(
        text: &'a [u8],
        accessible_end: i64,
        point_charpos: i64,
        cursor_missing: bool,
    ) -> Self {
        Self {
            text,
            accessible_end,
            point_charpos,
            cursor_missing,
        }
    }

    pub(crate) fn consume_at_checkpoint<B: LayoutBufferView>(
        &self,
        buffer: &B,
        checkpoints: &mut InvisibleTextScanCheckpoint,
        position: &mut DisplaySourceTextPosition,
    ) -> BufferSourceInvisibleTextScanAction {
        if !checkpoints.should_check(position.charpos()) {
            return BufferSourceInvisibleTextScanAction::Unchecked;
        }

        let start_byte_idx = position.byte_idx();
        let start_charpos = position.charpos();
        let text_props = RustTextPropAccess::new(buffer);
        let (invisible, next_visible) = text_props.check_invisible(start_charpos);
        checkpoints.record_next_visible(next_visible);

        if !invisible.hidden() {
            return BufferSourceInvisibleTextScanAction::Visible { next_visible };
        }

        // GNU runs handle_display_prop BEFORE handle_invisible_prop, and a
        // REPLACING spec returns HANDLED_RETURN, so handle_stop never reaches
        // the invisible handler at this position: the display string or image
        // wins and the text it covers is replaced rather than hidden
        // (xdisp.c:1012-1021, :5974). The replacement consumes its own range,
        // and this checkpoint runs again at the position past it - where, if
        // the text there is still invisible, it hides from there.
        //
        // Asked only on the hidden branch: when the text is visible the answer
        // cannot change the outcome, and this keeps the check off the path
        // every ordinary character takes.
        if text_props.replacing_display_at(start_charpos) {
            return BufferSourceInvisibleTextScanAction::Visible { next_visible };
        }

        let skip_to = next_visible.min(self.accessible_end);
        let point_in_hidden_region = self.cursor_missing
            && self.point_charpos >= start_charpos
            && self.point_charpos < skip_to;
        position.skip_chars_until(self.text, skip_to);

        // Count the buffer newlines crossed by this fold so display-line-numbers
        // advance past the hidden lines (GNU counts every buffer '\n', visible or
        // not). Half-open range [start_byte_idx, end): the '\n' that begins the
        // first visible line after the fold sits at `skip_to` and is NOT in the
        // range — it is counted by the following real line-break transition, so
        // the count is exact (no off-by-one). Counting 0x0A in a UTF-8 slice is
        // safe: 0x0A never appears as a continuation byte.
        let end_byte_idx = position.byte_idx();
        let hidden_newline_count = self
            .text
            .get(start_byte_idx..end_byte_idx)
            .map_or(0, |slice| slice.iter().filter(|&&b| b == b'\n').count());

        BufferSourceInvisibleTextScanAction::Hidden(BufferSourceInvisibleTextSkip::new(
            start_byte_idx,
            start_charpos,
            skip_to,
            next_visible,
            point_in_hidden_region,
            invisible.ellipsis(),
            hidden_newline_count,
        ))
    }

    /// Whether the checkpoint would commit an invisible-text skip at this
    /// position.  This read-only classification exists so an anchored producer
    /// insertion can be rendered first without mutating either the checkpoint
    /// or source progress.
    fn will_skip_at_checkpoint<B: LayoutBufferView>(
        &self,
        buffer: &B,
        checkpoints: &InvisibleTextScanCheckpoint,
        position: DisplaySourceTextPosition,
    ) -> bool {
        if !checkpoints.should_check(position.charpos()) {
            return false;
        }
        let text_props = RustTextPropAccess::new(buffer);
        let (invisible, _) = text_props.check_invisible(position.charpos());
        invisible.preserves_boundary_overlay_strings()
            && !text_props.replacing_display_at(position.charpos())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferSourceSelectiveDisplayLineTailAction {
    Exhausted,
    LineBreak { charpos: i64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceSelectiveDisplayLineTailMarker;

impl BufferSourceSelectiveDisplayLineTailMarker {
    pub(crate) fn ellipsis_append_request(
        self,
        position: DisplayRowPosition,
        ellipsis_text: Option<&str>,
    ) -> SyntheticTextAppendRequest {
        SyntheticTextAppendRequest::active_marker_with_text(
            position,
            SyntheticTextMarker::SelectiveEllipsis,
            ellipsis_text,
        )
    }

    pub(crate) fn append_to_text_row_and_apply<'ctx>(
        self,
        render_context: BufferSyntheticTextRenderContext<'ctx>,
        row_geometry: &'ctx DisplayRowGeometryState,
        source_render: &mut TextRowSourceRenderState<'_>,
        mut row_progress: DisplaySourceRowProgressState<'_>,
        ellipsis_text: Option<&str>,
    ) {
        let request = self.ellipsis_append_request(row_progress.row_position(), ellipsis_text);
        append_synthetic_request_to_text_row(
            render_context,
            row_geometry,
            source_render,
            &mut row_progress,
            request,
        );
    }
}

impl BufferSourceSelectiveDisplayLineTailAction {
    pub(crate) fn is_line_break(self) -> bool {
        matches!(self, Self::LineBreak { .. })
    }

    pub(crate) fn apply_hidden_line_break_row_state(
        self,
        row_geometry: &DisplayRowGeometryState,
        row_extend: &mut DisplayRowExtendState,
        box_face: &mut BoxFaceRowState,
        content_x: f32,
        x: &mut f32,
    ) {
        if self.is_line_break() {
            *x = content_x;
            row_extend.clear();
            box_face.continue_on_row(row_geometry.next_row_marker(), content_x);
        }
    }

    pub(crate) fn apply_after_hidden_line_break_transition(
        self,
        row_transition: DisplayTextRowTransition,
        synced_charpos: i64,
        position: &mut DisplaySourceTextPosition,
        row_source_start: &mut DisplayRowSourceStart,
    ) -> DisplayRowTransitionContinuation {
        if row_transition.is_exhausted() {
            return DisplayRowTransitionContinuation::Exhausted;
        }
        sync_position_after_row_transition(synced_charpos, position, row_source_start);
        DisplayRowTransitionContinuation::Continue
    }

    #[cfg(test)]
    pub(crate) fn charpos(self) -> Option<i64> {
        match self {
            Self::LineBreak { charpos } => Some(charpos),
            Self::Exhausted => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceSelectiveDisplayHiddenLines {
    hidden_line_count: usize,
}

impl BufferSourceSelectiveDisplayHiddenLines {
    fn new(hidden_line_count: usize) -> Self {
        Self { hidden_line_count }
    }

    #[cfg(test)]
    pub(crate) fn hidden_line_count(self) -> usize {
        self.hidden_line_count
    }

    pub(crate) fn apply_to_line_numbers(self, line_numbers: &mut LineNumberRenderState) {
        for _ in 0..self.hidden_line_count {
            line_numbers.advance_hidden_line();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferSourceSelectiveDisplayContext<'a> {
    text: &'a [u8],
    selective_display: i32,
    tab_width: i32,
}

impl<'a> BufferSourceSelectiveDisplayContext<'a> {
    pub(crate) fn new(text: &'a [u8], selective_display: i32, tab_width: i32) -> Self {
        Self {
            text,
            selective_display,
            tab_width: tab_width.max(1),
        }
    }

    pub(crate) fn hides_carriage_return_tail(self, ch: char) -> bool {
        self.selective_display > 0 && ch == '\r'
    }

    pub(crate) fn carriage_return_tail_marker(
        self,
        ch: char,
    ) -> Option<BufferSourceSelectiveDisplayLineTailMarker> {
        self.hides_carriage_return_tail(ch)
            .then_some(BufferSourceSelectiveDisplayLineTailMarker)
    }

    pub(crate) fn hides_indented_lines_after_line_break(self, byte_idx: usize) -> bool {
        self.selective_display > 0
            && self.selective_display < i32::MAX
            && byte_idx < self.text.len()
    }

    pub(crate) fn skip_rest_of_line_after_carriage_return(
        self,
        position: &mut DisplaySourceTextPosition,
    ) -> BufferSourceSelectiveDisplayLineTailAction {
        position.advance_charpos_by_one();
        if position.consume_until_line_break(self.text) {
            return BufferSourceSelectiveDisplayLineTailAction::LineBreak {
                charpos: position.charpos(),
            };
        }

        BufferSourceSelectiveDisplayLineTailAction::Exhausted
    }

    pub(crate) fn skip_hidden_indented_lines_after_line_break(
        self,
        position: &mut DisplaySourceTextPosition,
    ) -> BufferSourceSelectiveDisplayHiddenLines {
        let mut hidden_line_count = 0;
        while position.byte_idx() < self.text.len() {
            let Some(indent) = self.indentation_columns_at(position.byte_idx()) else {
                break;
            };
            if indent <= self.selective_display {
                break;
            }

            if self.skip_line(position) {
                hidden_line_count += 1;
            }
        }

        BufferSourceSelectiveDisplayHiddenLines::new(hidden_line_count)
    }

    pub(crate) fn apply_hidden_indented_lines_after_line_break(
        self,
        position: &mut DisplaySourceTextPosition,
        line_numbers: &mut LineNumberRenderState,
    ) -> BufferSourceSelectiveDisplayHiddenLines {
        if !self.hides_indented_lines_after_line_break(position.byte_idx()) {
            return BufferSourceSelectiveDisplayHiddenLines::new(0);
        }
        let hidden_lines = self.skip_hidden_indented_lines_after_line_break(position);
        hidden_lines.apply_to_line_numbers(line_numbers);
        hidden_lines
    }

    fn indentation_columns_at(self, mut byte_idx: usize) -> Option<i32> {
        if byte_idx >= self.text.len() {
            return None;
        }

        let mut indent = 0i32;
        while byte_idx < self.text.len() {
            match self.text[byte_idx] {
                b' ' => {
                    indent += 1;
                    byte_idx += 1;
                }
                b'\t' => {
                    indent = ((indent / self.tab_width) + 1) * self.tab_width;
                    byte_idx += 1;
                }
                _ => break,
            }
        }
        Some(indent)
    }

    fn skip_line(self, position: &mut DisplaySourceTextPosition) -> bool {
        position.consume_until_line_break(self.text)
    }
}

/// NOTE(cursor capture, buffer vs item renderer): the point->cursor-info
/// COMPUTATION is already shared — every capture site builds a
/// `CapturedCursorInfo` through `display_cursor.rs`
/// (`CapturedCursorPlacement::from_row_text_position` +
/// `CapturedCursorInfo::from_active_face_state`, first-capture-wins via
/// `capture_once`), and the ordinary main-character capture goes through the
/// one shared seam
/// `DisplaySourceTextCharPreparedAppend::capture_cursor_info_for_main_char_if_point`
/// (display_source_item_append.rs) from `buffer_source/char_render.rs`. The
/// capture wrappers that remain in this file — line break at point (below),
/// end-of-buffer, invisible-region, and hscroll-truncated text — are gates on
/// BUFFER-POINT cases that a Lisp-string row cannot have (a string has no
/// point), mirroring GNU `set_cursor_from_row` operating on buffer positions
/// only. They are deliberately NOT merged into the item renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BufferSourceLineBreakSourceAction {
    ch_start_byte_idx: usize,
    charpos: i64,
    next_charpos: i64,
    line_spacing: f32,
}

/// What ended a display row, from the point of view of "which buffer position
/// owns every screen column past the row's last glyph".
///
/// GNU asks this question once, in `find_row_edges` (src/xdisp.c:25246-25360),
/// whose comment enumerates the cases and gives each one a different
/// `row->maxpos`. Only two of them reach this seam, and naming them is what
/// stops a third from being added without answering the question:
///
/// * a real buffer newline, which draws no glyph and therefore needs a slot
///   published for it (`row->maxpos = eol_pos + 1`);
/// * a newline that came from a `display` string, where GNU takes the
///   `ends_in_newline_from_string_p` branch and derives the row's end from the
///   string's own glyphs instead -- and where this port's `next_charpos`
///   deliberately does not advance over a buffer character at all.
///
/// Measured against GNU Emacs 31.0.90 in an 80-column pty: a row ending at a
/// buffer newline maps every trailing column to that newline
/// ("abcdef\nghijkl\n", row 0 by x: 1 2 3 4 5 6 7 7 7 ...), while a row that
/// ends by CONTINUATION or by TRUNCATION maps each of its columns to a
/// distinct position and repeats none -- so those rows correctly reach neither
/// arm of this enum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayRowEnd {
    /// The row ends at a newline that is in the buffer and on screen.
    BufferNewline { cell: DisplayRowTerminatorCell },
    /// The row ends where a `display` string's own newline ended the display
    /// line, consuming no buffer character.
    DisplayStringNewline,
}

pub(crate) struct BufferSourceLineBreakRenderRequest<'a> {
    source_char: DisplaySourceStepChar,
    context: BufferSourceLineBreakRenderContext<'a>,
    box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    line_spacing: crate::display_item::DisplayLineSpacingPolicy,
    display_string_line_break: Option<DisplayReplacementStringLineBreak>,
}

#[derive(Clone, Copy)]
pub(crate) struct BufferSourceLineBreakRenderContext<'a> {
    text: &'a [u8],
    text_start_byte: usize,
    selective_display: i32,
    tab_width: i32,
    active_face_state: &'a DisplayRowActiveFaceState,
    point_charpos: i64,
    char_h: f32,
    extra_line_spacing: f32,
    content_x: f32,
    has_prefix: bool,
    row_geometry_defaults: DisplayRowGeometryDefaults,
    display_text_row_base: usize,
    max_rows: usize,
    row_limit: DisplayRowLimit,
    append_surface: &'a DisplayRowAppendSurface,
    frame_background: Color,
    fill_column_indicator: i32,
    fill_column_indicator_char: char,
}

impl<'a> BufferSourceLineBreakRenderContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        text: &'a [u8],
        text_start_byte: usize,
        selective_display: i32,
        tab_width: i32,
        active_face_state: &'a DisplayRowActiveFaceState,
        point_charpos: i64,
        char_h: f32,
        extra_line_spacing: f32,
        content_x: f32,
        has_prefix: bool,
        row_geometry_defaults: DisplayRowGeometryDefaults,
        display_text_row_base: usize,
        max_rows: usize,
        row_limit: DisplayRowLimit,
        append_surface: &'a DisplayRowAppendSurface,
        frame_background: Color,
        fill_column_indicator: i32,
        fill_column_indicator_char: char,
    ) -> Self {
        Self {
            text,
            text_start_byte,
            selective_display,
            tab_width,
            active_face_state,
            point_charpos,
            char_h,
            extra_line_spacing,
            content_x,
            has_prefix,
            row_geometry_defaults,
            display_text_row_base,
            max_rows,
            row_limit,
            append_surface,
            frame_background,
            fill_column_indicator,
            fill_column_indicator_char,
        }
    }
}

impl<'a> BufferSourceLineBreakRenderRequest<'a> {
    pub(crate) fn new(
        source_char: DisplaySourceStepChar,
        context: BufferSourceLineBreakRenderContext<'a>,
    ) -> Self {
        debug_assert_eq!(source_char.ch(), '\n');
        Self {
            source_char,
            context,
            box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges::Neither,
            line_spacing: crate::display_item::DisplayLineSpacingPolicy::Inherit,
            display_string_line_break: None,
        }
    }

    pub(crate) fn with_box_vertical_edges(
        mut self,
        edges: neomacs_display_protocol::face::BoxVerticalEdges,
    ) -> Self {
        self.box_vertical_edges = edges;
        self
    }

    pub(crate) fn with_line_spacing(
        mut self,
        line_spacing: crate::display_item::DisplayLineSpacingPolicy,
    ) -> Self {
        self.line_spacing = line_spacing;
        self
    }

    pub(crate) fn with_display_string_line_break(
        mut self,
        line_break: DisplayReplacementStringLineBreak,
    ) -> Self {
        self.display_string_line_break = Some(line_break);
        self
    }

    pub(crate) fn render_and_apply<B: LayoutBufferView>(
        self,
        source_walk: &mut BufferSourceWalk<'_, B>,
        buffer: &B,
        state: BufferSourceLoopMutableState<'_, '_, '_>,
    ) -> DisplayRowTransitionContinuation {
        let BufferSourceLoopMutableState {
            mut progress,
            cursor_info,
            row_build,
            mut row_carryover,
            source_render,
            row_source_start,
            row_y_positions,
            face_ids,
            ..
        } = state;
        let mut source_render = source_render;
        let context = self.context;

        let line_break_action = BufferSourceLineBreakSourceAction::for_source_step_newline(
            self.source_char,
            context.char_h,
            context.extra_line_spacing,
            self.line_spacing,
        );
        // Strings anchored at the newline position are emitted by the producer's
        // element arm before this line-break step runs (P4.6), in the same order
        // this call used to produce them.
        let row_position = progress.row_position();
        line_break_action.capture_cursor_if_point(
            cursor_info,
            context.active_face_state,
            row_build.row_geometry,
            context.point_charpos,
            row_position.x_px(),
            row_position.col(),
        );
        // GNU order at a real newline (xdisp.c:26525-26533), decided by the
        // shared line-end seam: trailing-whitespace highlight first (a real
        // line end, so the run IS trailing, and it must precede the fills
        // that would otherwise be swept up and painted red past end-of-line),
        // then append_space_for_newline -- the appended glyph keeps the
        // newline's face on a terminal frame, or becomes the fill-column
        // indicator when the pen is exactly at the indicator column -- then
        // the indicator / `:extend` fill from the advanced pen.
        {
            let metrics = self
                .display_string_line_break
                .map(DisplayReplacementStringLineBreak::metrics)
                .unwrap_or_else(|| context.active_face_state.metrics());
            let line_end_ctx = LineEndContext {
                newline_face_id: context.active_face_state.face_id(),
                measurement_mode: source_render.measurement_mode(),
                pen_x: progress.row_progress().x(),
                pen_col: progress.row_progress().col() as i64,
                right_edge_x: context.append_surface.full_text_right_edge(),
                char_width: metrics.char_width(),
                indicator: (context.fill_column_indicator >= 0).then(|| LineEndIndicator {
                    col: context.fill_column_indicator,
                    ch: context.fill_column_indicator_char,
                }),
                // Handed over unfiltered: the frame-background visibility skip
                // is GNU's FRAME_WINDOW_P-guarded one (xdisp.c:24388) and now
                // belongs to line_end::extend_fill_runs, which never applies it
                // to a terminal row.
                extend: row_build
                    .row_extend
                    .value_on(row_build.row_geometry)
                    .copied()
                    .map(|face| LineEndExtend {
                        bg: face.background(),
                        face_id: face.face_id(),
                    }),
                frame_background: context.frame_background,
                trailing_whitespace_enabled: row_carryover.trailing_whitespace.is_enabled(),
                box_vertical_edges: self.box_vertical_edges,
                box_run_membership: self
                    .display_string_line_break
                    .map(DisplayReplacementStringLineBreak::box_run_membership)
                    .unwrap_or_else(|| {
                        neomacs_display_protocol::face::BoxRunMembership::from_boxed(
                            context.active_face_state.resolved_face().box_type > 0,
                        )
                    }),
            };
            let line_end_geometry = LineEndFillGeometry {
                content_x: context.content_x,
                height_px: metrics.row_height(),
                ascent_px: metrics.ascent(),
                fill_char_width: metrics.char_width(),
            };
            source_render.render_line_end(&line_end_ctx, line_end_geometry, face_ids);
        }
        // The row ends at a real buffer newline, so it owns the columns past
        // its last glyph. The cell is the one GNU's `append_space_for_newline`
        // would have appended, which is the face active at the line end.
        let terminator_cell = {
            let metrics = context.active_face_state.metrics();
            DisplayRowTerminatorCell::new(metrics.char_width(), metrics.row_height())
        };
        line_break_action.apply_before_row_transition(
            row_build.row_geometry,
            row_carryover.trailing_whitespace,
            row_build.row_extend,
            row_build.box_face,
            source_render.output_emitter(),
            DisplayRowEnd::BufferNewline {
                cell: terminator_cell,
            },
            context.content_x,
            &mut progress,
        );

        let line_break_transition = DisplayRowLineBreakTransitionPlan::line_break();
        let row_transition = DisplayRowTextWindowEmitContext::from_source_render(
            context.row_geometry_defaults,
            context.display_text_row_base,
            row_y_positions,
            context.max_rows,
            row_build.row_geometry,
            row_build.row_flags,
            context.row_limit,
            &mut source_render,
        )
        .emit_line_break_then_row_start(
            line_break_transition,
            LayoutCharPos0::new(progress.charpos()),
            progress.row_position(),
            line_break_action.line_spacing(),
            row_carryover.render_state(context.has_prefix),
            progress.row_progress_mut().col_mut(),
        );

        let synced_charpos = buffer
            .layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(
                context.text_start_byte + progress.source_position().byte_idx(),
            ))
            .get() as i64;
        let mut synced_source_position = progress.source_position();
        let continuation = line_break_action.apply_after_line_break_row_transition(
            row_transition,
            synced_charpos,
            &mut synced_source_position,
            row_source_start,
            row_build.row_geometry,
            row_build.row_extend,
            context.active_face_state,
            row_build.box_face,
            context.content_x,
        );
        source_walk
            .source_position_update(synced_source_position)
            .apply_to_progress(&mut progress);
        if continuation.should_break() {
            return continuation;
        }

        source_walk
            .consume_hidden_indented_lines_after_line_break(
                BufferSourceSelectiveDisplayContext::new(
                    context.text,
                    context.selective_display,
                    context.tab_width,
                ),
                progress.source_position(),
                row_carryover.line_numbers,
            )
            .apply_to_progress(&mut progress);
        DisplayRowTransitionContinuation::Continue
    }

    /// Terminate the current display row because a `display` string ended a
    /// display line with a newline. Unlike `render_and_apply`, this consumes NO
    /// buffer character: the buffer position (already advanced past the covered
    /// region by the replacement) is unchanged, so the following buffer text —
    /// including a bare trailing newline that must still produce its own blank
    /// row — starts on the freshly opened row. Mirrors GNU xdisp.c, where a
    /// display line "ends in a newline from a display string" and the buffer
    /// text after the display property resumes on the next glyph row.
    pub(crate) fn render_display_string_break_and_apply<B: LayoutBufferView>(
        self,
        source_walk: &mut BufferSourceWalk<'_, B>,
        buffer: &B,
        state: BufferSourceLoopMutableState<'_, '_, '_>,
    ) -> DisplayRowTransitionContinuation {
        let BufferSourceLoopMutableState {
            mut progress,
            row_build,
            mut row_carryover,
            source_render,
            row_source_start,
            row_y_positions,
            face_ids,
            ..
        } = state;
        let mut source_render = source_render;
        let context = self.context;

        let line_break_action = BufferSourceLineBreakSourceAction::for_display_string_newline(
            progress.charpos(),
            progress.source_position().byte_idx(),
            self.display_string_line_break
                .map(|line_break| line_break.line_spacing(context.extra_line_spacing))
                .unwrap_or(context.extra_line_spacing),
        );

        {
            let metrics = self
                .display_string_line_break
                .map(DisplayReplacementStringLineBreak::metrics)
                .unwrap_or_else(|| context.active_face_state.metrics());
            // A pushed-string newline takes the same GNU line-end path as a
            // buffer newline: append_space_for_newline, optional fill-column
            // indicator, then extend_face_to_end_of_line. Only its semantic
            // face/source position differs from the buffer walk.
            let (newline_face_id, extend) = self.display_string_line_break.map_or_else(
                || {
                    (
                        context.active_face_state.face_id(),
                        row_build
                            .row_extend
                            .value_on(row_build.row_geometry)
                            .copied()
                            .map(|face| LineEndExtend {
                                bg: face.background(),
                                face_id: face.face_id(),
                            }),
                    )
                },
                |face| {
                    (
                        face.face_id(),
                        face.extend_face().map(|face| LineEndExtend {
                            bg: face.background(),
                            face_id: face.face_id(),
                        }),
                    )
                },
            );
            let line_end_ctx = LineEndContext {
                newline_face_id,
                measurement_mode: source_render.measurement_mode(),
                pen_x: progress.row_progress().x(),
                pen_col: progress.row_progress().col() as i64,
                right_edge_x: context.append_surface.full_text_right_edge(),
                char_width: metrics.char_width(),
                indicator: (context.fill_column_indicator >= 0).then(|| LineEndIndicator {
                    col: context.fill_column_indicator,
                    ch: context.fill_column_indicator_char,
                }),
                extend,
                frame_background: context.frame_background,
                trailing_whitespace_enabled: row_carryover.trailing_whitespace.is_enabled(),
                box_vertical_edges: self
                    .display_string_line_break
                    .map(DisplayReplacementStringLineBreak::box_vertical_edges)
                    .unwrap_or(self.box_vertical_edges),
                box_run_membership: self
                    .display_string_line_break
                    .map(DisplayReplacementStringLineBreak::box_run_membership)
                    .unwrap_or_else(|| {
                        neomacs_display_protocol::face::BoxRunMembership::from_boxed(
                            context.active_face_state.resolved_face().box_type > 0,
                        )
                    }),
            };
            source_render.render_line_end(
                &line_end_ctx,
                LineEndFillGeometry {
                    content_x: context.content_x,
                    height_px: metrics.row_height(),
                    ascent_px: metrics.ascent(),
                    fill_char_width: metrics.char_width(),
                },
                face_ids,
            );
        }
        line_break_action.apply_before_row_transition(
            row_build.row_geometry,
            row_carryover.trailing_whitespace,
            row_build.row_extend,
            row_build.box_face,
            source_render.output_emitter(),
            DisplayRowEnd::DisplayStringNewline,
            context.content_x,
            &mut progress,
        );

        let line_break_transition = DisplayRowLineBreakTransitionPlan::line_break();
        let row_transition = DisplayRowTextWindowEmitContext::from_source_render(
            context.row_geometry_defaults,
            context.display_text_row_base,
            row_y_positions,
            context.max_rows,
            row_build.row_geometry,
            row_build.row_flags,
            context.row_limit,
            &mut source_render,
        )
        .emit_line_break_then_row_start(
            line_break_transition,
            LayoutCharPos0::new(progress.charpos()),
            progress.row_position(),
            line_break_action.line_spacing(),
            row_carryover.render_state(context.has_prefix),
            progress.row_progress_mut().col_mut(),
        );

        let synced_charpos = buffer
            .layout_emacs_byte_pos_to_char_pos(EmacsBytePos::new(
                context.text_start_byte + progress.source_position().byte_idx(),
            ))
            .get() as i64;
        let mut synced_source_position = progress.source_position();
        let continuation = line_break_action.apply_after_line_break_row_transition(
            row_transition,
            synced_charpos,
            &mut synced_source_position,
            row_source_start,
            row_build.row_geometry,
            row_build.row_extend,
            context.active_face_state,
            row_build.box_face,
            context.content_x,
        );
        source_walk
            .source_position_update(synced_source_position)
            .apply_to_progress(&mut progress);
        continuation
    }
}

impl BufferSourceLineBreakSourceAction {
    pub(crate) fn for_newline(
        charpos: i64,
        ch_start_byte_idx: usize,
        char_h: f32,
        extra_line_spacing: f32,
        line_spacing: crate::display_item::DisplayLineSpacingPolicy,
    ) -> Self {
        let line_spacing = line_spacing.resolve(char_h, extra_line_spacing);
        Self {
            ch_start_byte_idx,
            charpos,
            next_charpos: charpos + 1,
            line_spacing,
        }
    }

    pub(crate) fn for_source_step_newline(
        source_char: DisplaySourceStepChar,
        char_h: f32,
        extra_line_spacing: f32,
        line_spacing: crate::display_item::DisplayLineSpacingPolicy,
    ) -> Self {
        Self::for_newline(
            source_char.start_charpos(),
            source_char.start_byte_idx(),
            char_h,
            extra_line_spacing,
            line_spacing,
        )
    }

    /// A row break produced by a newline embedded in a `display` string, which
    /// consumes NO buffer character: unlike a buffer newline, `next_charpos`
    /// stays at `charpos` so the following buffer text keeps its position (GNU
    /// xdisp.c `display_line` ends the row on a display-string '\n' without
    /// advancing over a buffer char).
    pub(crate) fn for_display_string_newline(
        charpos: i64,
        ch_start_byte_idx: usize,
        line_spacing: f32,
    ) -> Self {
        Self {
            ch_start_byte_idx,
            charpos,
            next_charpos: charpos,
            line_spacing,
        }
    }

    pub(crate) fn point_matches(self, point_charpos: i64) -> bool {
        point_charpos == self.charpos
    }

    pub(crate) fn next_charpos(self) -> i64 {
        self.next_charpos
    }

    pub(crate) fn line_spacing(self) -> f32 {
        self.line_spacing
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_before_row_transition(
        self,
        row_geometry: &DisplayRowGeometryState,
        trailing_whitespace: &mut TrailingWhitespaceRenderState,
        row_extend: &mut DisplayRowExtendState,
        box_face: &mut BoxFaceRowState,
        output_emitter: &mut WindowOutputEmitter,
        row_end: DisplayRowEnd,
        content_x: f32,
        progress: &mut DisplaySourceProgressState<'_>,
    ) {
        progress.reset_physical_line_tabs();
        trailing_whitespace.reset_after_row_transition();
        row_extend.clear();
        box_face.continue_on_row(row_geometry.current_row_marker(), content_x);
        progress.set_charpos(self.next_charpos());
        *progress.row_progress_mut().x_mut() = content_x;
        // `next_charpos` is the zero-based position AFTER the consumed newline,
        // which is the newline's own ONE-based Lisp position -- the row's last
        // display position and, for a buffer newline, the position that owns
        // every screen column past the row's last glyph.
        let row_end_pos = LispCharPos1::new(progress.charpos());
        match row_end {
            DisplayRowEnd::BufferNewline { cell } => {
                output_emitter.note_row_terminator(DisplayRowTerminator::new(row_end_pos, cell))
            }
            DisplayRowEnd::DisplayStringNewline => {
                output_emitter.note_display_buffer_pos(row_end_pos)
            }
        }
    }

    pub(crate) fn apply_after_row_transition(
        self,
        row_geometry: &DisplayRowGeometryState,
        box_face: &mut BoxFaceRowState,
        content_x: f32,
    ) {
        box_face.continue_on_row(row_geometry.current_row_marker(), content_x);
    }

    pub(crate) fn apply_after_line_break_row_transition(
        self,
        row_transition: DisplayTextRowTransition,
        synced_charpos: i64,
        position: &mut DisplaySourceTextPosition,
        row_source_start: &mut DisplayRowSourceStart,
        row_geometry: &DisplayRowGeometryState,
        row_extend: &mut DisplayRowExtendState,
        active_face_state: &DisplayRowActiveFaceState,
        box_face: &mut BoxFaceRowState,
        content_x: f32,
    ) -> DisplayRowTransitionContinuation {
        if row_transition.is_exhausted() {
            return DisplayRowTransitionContinuation::Exhausted;
        }
        sync_position_after_row_transition(synced_charpos, position, row_source_start);
        sync_row_extend_to_active_face(row_extend, row_geometry, active_face_state);
        self.apply_after_row_transition(row_geometry, box_face, content_x);
        DisplayRowTransitionContinuation::Continue
    }

    pub(crate) fn cursor_info(
        self,
        active_face_state: &DisplayRowActiveFaceState,
        row_geometry: &DisplayRowGeometryState,
        x: f32,
        col: usize,
    ) -> CapturedCursorInfo {
        CapturedCursorInfo::from_active_face_state(
            active_face_state,
            CapturedCursorPlacement::from_row_text_position(
                row_geometry.text_position(x, self.ch_start_byte_idx, col),
                CapturedCursorSlotWidth::FaceChar,
                false,
            ),
        )
    }

    pub(crate) fn capture_cursor_if_point(
        self,
        target: &mut CursorCaptureState,
        active_face_state: &DisplayRowActiveFaceState,
        row_geometry: &DisplayRowGeometryState,
        point_charpos: i64,
        x: f32,
        col: usize,
    ) {
        if !target.is_missing() || !self.point_matches(point_charpos) {
            return;
        }
        capture_cursor_approximation(
            target,
            self.cursor_info(active_face_state, row_geometry, x, col),
        );
    }
}

pub(crate) fn append_synthetic_request_to_text_row<'ctx>(
    render_context: BufferSyntheticTextRenderContext<'ctx>,
    row_geometry: &'ctx DisplayRowGeometryState,
    source_render: &mut TextRowSourceRenderState<'_>,
    row_progress: &mut DisplaySourceRowProgressState<'_>,
    request: SyntheticTextAppendRequest,
) {
    let Some(progress) =
        render_context.render_request_to_text_row(source_render, row_geometry, request)
    else {
        return;
    };
    row_progress.apply_position(progress.end());
}

pub(crate) fn append_hscroll_truncation_marker_to_text_row<'ctx>(
    render_context: BufferSyntheticTextRenderContext<'ctx>,
    row_geometry: &'ctx DisplayRowGeometryState,
    source_render: &mut TextRowSourceRenderState<'_>,
    row_progress: &mut DisplaySourceRowProgressState<'_>,
    content_x: f32,
) {
    let request =
        render_context.hscroll_truncation_request(source_render.default_face(), content_x);
    append_synthetic_request_to_text_row(
        render_context,
        row_geometry,
        source_render,
        row_progress,
        request,
    );
    source_render.mark_current_text_row_truncated_left();
}
