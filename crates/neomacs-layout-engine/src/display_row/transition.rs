use crate::display_row::builder::DisplayRowPosition;
use crate::display_row::geometry::{
    DisplayRowBoundaryTarget, DisplayRowFlagKind, DisplayRowFlags, DisplayRowGeometryDefaults,
    DisplayRowGeometryState, DisplayRowHitRange, DisplayRowLimit, DisplayRowVisibilityLimit,
    DisplayRowYPositions, DisplayRowYRecording,
};
use crate::display_row::lisp_string::DisplayRowPrefixRequest;
use crate::display_row::source_render::{TextRowOutputRenderState, TextRowSourceRenderState};
use crate::display_row::walk_state::{
    HorizontalScrollSkipState, LineNumberRenderState, TextRowTransitionStatePolicy,
    TrailingWhitespaceRenderState, WordWrapRenderState,
};
use crate::hit_test::HitRow;
use crate::window_output::{DisplayTextRowGeometryTransition, DisplayTextRowTransition};

pub(crate) struct DisplayRowBoundaryTransitionRequest<'a> {
    target: DisplayRowBoundaryTarget<'a>,
    max_rows: usize,
}

pub(crate) struct DisplayRowLineBreakTransitionRequest<'a> {
    hit_range: DisplayRowHitRange,
    defaults: DisplayRowGeometryDefaults,
    row_base: usize,
    col: usize,
    x: f32,
    line_spacing: f32,
    row_y_recording: DisplayRowYRecording<'a>,
    max_rows: usize,
}

pub(crate) struct DisplayRowTransitionRequestContext<'a> {
    defaults: DisplayRowGeometryDefaults,
    row_base: usize,
    row_y_recording: DisplayRowYRecording<'a>,
    max_rows: usize,
}

pub(crate) struct DisplayRowTextWindowEmitContext<'a, 'emit> {
    defaults: DisplayRowGeometryDefaults,
    row_base: usize,
    row_y_positions: &'a mut DisplayRowYPositions,
    max_rows: usize,
    row_geometry: &'emit mut DisplayRowGeometryState,
    row_flags: &'emit mut DisplayRowFlags,
    row_limit: DisplayRowLimit,
    hit_rows: &'emit mut Vec<HitRow>,
    output_render: TextRowOutputRenderState<'emit>,
}

pub(crate) struct DisplayRowTransitionRenderState<'a> {
    prefix_request: &'a mut DisplayRowPrefixRequest,
    has_prefix: bool,
    line_numbers: &'a mut LineNumberRenderState,
    hscroll_skip: &'a mut HorizontalScrollSkipState,
    word_wrap: &'a mut WordWrapRenderState,
    trailing_whitespace: &'a mut TrailingWhitespaceRenderState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayRowTransitionContinuation {
    Continue,
    Exhausted,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowLineBreakTransitionPlan {
    state_policy: TextRowTransitionStatePolicy,
}

/// How a visually wrapped row reached the right edge, in GNU's terms.
///
/// GNU's `display_line` takes two different branches when a line does not fit.
/// The word-wrap branch (`back_to_wrap`, src/xdisp.c:26360-26388) rewinds to
/// the recorded wrap point, marks `row->continued_p` and extends the face, but
/// never calls `produce_special_glyphs (it, IT_CONTINUATION, ...)`. Every other
/// overflow branch -- the element does not fit at all (src/xdisp.c:26336-26345),
/// an over-wide TAB (src/xdisp.c:26399-26403) and the general mid-element break
/// (src/xdisp.c:26421-26432) -- does produce that glyph on a TTY frame. The
/// distinction only affects the marker, never `continued_p` itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VisualWrapBreak {
    /// The row broke at a recorded word-wrap point: no continuation marker.
    AtWordBoundary,
    /// The row broke inside a display element: GNU produces IT_CONTINUATION.
    MidElement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayRowOverflowTransitionKind {
    Truncation,
    VisualWrap(VisualWrapBreak),
}

pub(crate) struct DisplayRowOverflowTransitionRequest<'a> {
    kind: DisplayRowOverflowTransitionKind,
    hit_range: DisplayRowHitRange,
    defaults: DisplayRowGeometryDefaults,
    row_base: usize,
    col: usize,
    x: f32,
    row_y_recording: DisplayRowYRecording<'a>,
    max_rows: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowOverflowTransitionPlan {
    kind: DisplayRowOverflowTransitionKind,
    state_policy: TextRowTransitionStatePolicy,
}

impl<'a> DisplayRowBoundaryTransitionRequest<'a> {
    pub(crate) fn new(target: DisplayRowBoundaryTarget<'a>, max_rows: usize) -> Self {
        Self { target, max_rows }
    }

    pub(crate) fn emit_with_output(
        self,
        row_geometry: &mut DisplayRowGeometryState,
        hit_rows: &mut Vec<HitRow>,
        output_render: TextRowOutputRenderState<'_>,
    ) -> DisplayTextRowTransition {
        let geometry_transition =
            row_geometry.finish_boundary_and_record_hit(self.target, hit_rows);
        output_render.transition_text_row_with_limit(geometry_transition, self.max_rows)
    }
}

impl<'a> DisplayRowTransitionRequestContext<'a> {
    pub(crate) fn new(
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        row_y_recording: DisplayRowYRecording<'a>,
        max_rows: usize,
    ) -> Self {
        Self {
            defaults,
            row_base,
            row_y_recording,
            max_rows,
        }
    }

    pub(crate) fn line_break(
        self,
        plan: DisplayRowLineBreakTransitionPlan,
        hit_range: DisplayRowHitRange,
        position: DisplayRowPosition,
        line_spacing: f32,
    ) -> DisplayRowLineBreakTransitionRequest<'a> {
        plan.request(
            hit_range,
            self.defaults,
            self.row_base,
            position,
            line_spacing,
            self.row_y_recording,
            self.max_rows,
        )
    }

    pub(crate) fn overflow(
        self,
        plan: DisplayRowOverflowTransitionPlan,
        hit_range: DisplayRowHitRange,
        position: DisplayRowPosition,
    ) -> DisplayRowOverflowTransitionRequest<'a> {
        plan.request(
            hit_range,
            self.defaults,
            self.row_base,
            position,
            self.row_y_recording,
            self.max_rows,
        )
    }
}

impl<'a, 'emit> DisplayRowTextWindowEmitContext<'a, 'emit> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        row_y_positions: &'a mut DisplayRowYPositions,
        max_rows: usize,
        row_geometry: &'emit mut DisplayRowGeometryState,
        row_flags: &'emit mut DisplayRowFlags,
        row_limit: DisplayRowLimit,
        hit_rows: &'emit mut Vec<HitRow>,
        output_render: TextRowOutputRenderState<'emit>,
    ) -> Self {
        Self {
            defaults,
            row_base,
            row_y_positions,
            max_rows,
            row_geometry,
            row_flags,
            row_limit,
            hit_rows,
            output_render,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_source_render(
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        row_y_positions: &'a mut DisplayRowYPositions,
        max_rows: usize,
        row_geometry: &'emit mut DisplayRowGeometryState,
        row_flags: &'emit mut DisplayRowFlags,
        row_limit: DisplayRowLimit,
        hit_rows: &'emit mut Vec<HitRow>,
        source_render: &'emit mut TextRowSourceRenderState<'emit>,
    ) -> Self {
        Self::new(
            defaults,
            row_base,
            row_y_positions,
            max_rows,
            row_geometry,
            row_flags,
            row_limit,
            hit_rows,
            source_render.output_render(),
        )
    }

    pub(crate) fn emit_line_break(
        self,
        plan: DisplayRowLineBreakTransitionPlan,
        hit_range: DisplayRowHitRange,
        position: DisplayRowPosition,
        line_spacing: f32,
    ) -> DisplayTextRowTransition {
        DisplayRowTransitionRequestContext::new(
            self.defaults,
            self.row_base,
            self.row_y_positions.recording(),
            self.max_rows,
        )
        .line_break(plan, hit_range, position, line_spacing)
        .emit_with_output(self.row_geometry, self.hit_rows, self.output_render)
    }

    pub(crate) fn emit_line_break_then_row_start(
        self,
        plan: DisplayRowLineBreakTransitionPlan,
        hit_range: DisplayRowHitRange,
        position: DisplayRowPosition,
        line_spacing: f32,
        render_state: DisplayRowTransitionRenderState<'_>,
        col: &mut usize,
    ) -> DisplayTextRowTransition {
        let transition = self.emit_line_break(plan, hit_range, position, line_spacing);
        if !transition.is_exhausted() {
            render_state.apply_line_break_row_start(plan, col);
        }
        transition
    }

    pub(crate) fn emit_overflow(
        self,
        plan: DisplayRowOverflowTransitionPlan,
        hit_range: DisplayRowHitRange,
        position: DisplayRowPosition,
    ) -> DisplayTextRowTransition {
        DisplayRowTransitionRequestContext::new(
            self.defaults,
            self.row_base,
            self.row_y_positions.recording(),
            self.max_rows,
        )
        .overflow(plan, hit_range, position)
        .emit_with_output(
            self.row_geometry,
            self.row_flags,
            self.row_limit,
            self.hit_rows,
            self.output_render,
        )
    }

    pub(crate) fn emit_overflow_then_row_start(
        self,
        plan: DisplayRowOverflowTransitionPlan,
        hit_range: DisplayRowHitRange,
        position: DisplayRowPosition,
        render_state: DisplayRowTransitionRenderState<'_>,
        col: &mut usize,
    ) -> DisplayTextRowTransition {
        let transition = self.emit_overflow(plan, hit_range, position);
        if !transition.is_exhausted() {
            render_state.apply_overflow_row_start(plan, col);
        }
        transition
    }
}

impl<'a> DisplayRowTransitionRenderState<'a> {
    pub(crate) fn new(
        prefix_request: &'a mut DisplayRowPrefixRequest,
        has_prefix: bool,
        line_numbers: &'a mut LineNumberRenderState,
        hscroll_skip: &'a mut HorizontalScrollSkipState,
        word_wrap: &'a mut WordWrapRenderState,
        trailing_whitespace: &'a mut TrailingWhitespaceRenderState,
    ) -> Self {
        Self {
            prefix_request,
            has_prefix,
            line_numbers,
            hscroll_skip,
            word_wrap,
            trailing_whitespace,
        }
    }

    fn apply_state_policy(&mut self, policy: TextRowTransitionStatePolicy) {
        let prefix_action = policy.apply(
            self.line_numbers,
            self.hscroll_skip,
            self.word_wrap,
            self.trailing_whitespace,
        );
        self.prefix_request
            .apply_transition_prefix_action(self.has_prefix, prefix_action);
    }

    pub(crate) fn apply_line_break_row_start(
        self,
        plan: DisplayRowLineBreakTransitionPlan,
        col: &mut usize,
    ) {
        plan.apply_row_start_prefix_state(col, self);
    }

    pub(crate) fn apply_overflow_prefix(self, plan: DisplayRowOverflowTransitionPlan) {
        plan.apply_prefix_state(self);
    }

    pub(crate) fn apply_overflow_row_start(
        self,
        plan: DisplayRowOverflowTransitionPlan,
        col: &mut usize,
    ) {
        plan.apply_row_start_prefix_state(col, self);
    }
}

impl DisplayRowTransitionContinuation {
    pub(crate) fn after_visible_row_transition(
        row_transition: DisplayTextRowTransition,
        row_geometry: &DisplayRowGeometryState,
        row_visibility_limit: DisplayRowVisibilityLimit,
    ) -> Self {
        if row_transition.is_exhausted() {
            Self::Exhausted
        } else if row_geometry.current_row_is_visible(row_visibility_limit) {
            Self::Continue
        } else {
            Self::Hidden
        }
    }

    pub(crate) fn should_break(self) -> bool {
        !matches!(self, Self::Continue)
    }
}

impl DisplayRowLineBreakTransitionPlan {
    fn new(state_policy: TextRowTransitionStatePolicy) -> Self {
        Self { state_policy }
    }

    pub(crate) fn hscroll_line_break() -> Self {
        Self::new(TextRowTransitionStatePolicy::hscroll_line_break())
    }

    pub(crate) fn hidden_line_break() -> Self {
        Self::new(TextRowTransitionStatePolicy::hidden_line_break())
    }

    pub(crate) fn line_break() -> Self {
        Self::new(TextRowTransitionStatePolicy::line_break())
    }

    pub(crate) fn apply_prefix_state(self, mut state: DisplayRowTransitionRenderState<'_>) {
        state.apply_state_policy(self.state_policy);
    }

    /// The column the row this transition opens starts at.
    ///
    /// A line break returns the walk to the left edge of the text area, which
    /// is what GNU's `display_line` does when it starts the next glyph row
    /// (`it->current_x = it->first_visible_x`). It is ONE fact, and it used to
    /// be written in two places that could disagree: the walk's own column,
    /// set by [`Self::apply_row_start_prefix_state`], and the column the
    /// OUTPUT row is opened with, which [`Self::request`] took from the
    /// caller's pen -- the pen of the row that just ENDED. A row that draws a
    /// glyph immediately moves the output cursor and overwrites the second, so
    /// the disagreement was invisible until a row that draws NONE -- an empty
    /// line, whose only content is its own newline -- published the PREVIOUS
    /// row's end column as its own start and end.
    pub(crate) fn row_start_col(self) -> usize {
        0
    }

    pub(crate) fn apply_row_start_prefix_state(
        self,
        col: &mut usize,
        state: DisplayRowTransitionRenderState<'_>,
    ) {
        *col = self.row_start_col();
        self.apply_prefix_state(state);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn request<'a>(
        self,
        hit_range: DisplayRowHitRange,
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        position: DisplayRowPosition,
        line_spacing: f32,
        row_y_recording: DisplayRowYRecording<'a>,
        max_rows: usize,
    ) -> DisplayRowLineBreakTransitionRequest<'a> {
        DisplayRowLineBreakTransitionRequest::new(
            hit_range,
            defaults,
            row_base,
            self.row_start_col(),
            position.x_px(),
            line_spacing,
            row_y_recording,
            max_rows,
        )
    }
}

impl<'a> DisplayRowLineBreakTransitionRequest<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        hit_range: DisplayRowHitRange,
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        col: usize,
        x: f32,
        line_spacing: f32,
        row_y_recording: DisplayRowYRecording<'a>,
        max_rows: usize,
    ) -> Self {
        Self {
            hit_range,
            defaults,
            row_base,
            col,
            x,
            line_spacing,
            row_y_recording,
            max_rows,
        }
    }

    fn boundary_target(self) -> DisplayRowBoundaryTarget<'a> {
        DisplayRowBoundaryTarget::line_break(
            self.hit_range,
            self.defaults,
            self.row_base,
            self.col,
            self.x,
            self.line_spacing,
            self.row_y_recording,
        )
    }

    pub(crate) fn finish_geometry(
        self,
        row_geometry: &mut DisplayRowGeometryState,
        hit_rows: &mut Vec<HitRow>,
    ) -> DisplayTextRowGeometryTransition {
        row_geometry.finish_boundary_and_record_hit(self.boundary_target(), hit_rows)
    }

    pub(crate) fn emit_with_output(
        self,
        row_geometry: &mut DisplayRowGeometryState,
        hit_rows: &mut Vec<HitRow>,
        output_render: TextRowOutputRenderState<'_>,
    ) -> DisplayTextRowTransition {
        let max_rows = self.max_rows;
        DisplayRowBoundaryTransitionRequest::new(self.boundary_target(), max_rows).emit_with_output(
            row_geometry,
            hit_rows,
            output_render,
        )
    }
}

impl DisplayRowOverflowTransitionPlan {
    fn new(
        kind: DisplayRowOverflowTransitionKind,
        state_policy: TextRowTransitionStatePolicy,
    ) -> Self {
        Self { kind, state_policy }
    }

    pub(crate) fn truncation(state_policy: TextRowTransitionStatePolicy) -> Self {
        Self::new(DisplayRowOverflowTransitionKind::Truncation, state_policy)
    }

    pub(crate) fn visual_wrap(
        break_kind: VisualWrapBreak,
        state_policy: TextRowTransitionStatePolicy,
    ) -> Self {
        Self::new(
            DisplayRowOverflowTransitionKind::VisualWrap(break_kind),
            state_policy,
        )
    }

    pub(crate) fn apply_prefix_state(self, mut state: DisplayRowTransitionRenderState<'_>) {
        state.apply_state_policy(self.state_policy);
    }

    /// The column the row this transition opens starts at; see
    /// [`DisplayRowLineBreakTransitionPlan::row_start_col`], which says the
    /// same thing for the other half of the row transitions.
    pub(crate) fn row_start_col(self) -> usize {
        0
    }

    pub(crate) fn apply_row_start_prefix_state(
        self,
        col: &mut usize,
        state: DisplayRowTransitionRenderState<'_>,
    ) {
        *col = self.row_start_col();
        self.apply_prefix_state(state);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn request<'a>(
        self,
        hit_range: DisplayRowHitRange,
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        position: DisplayRowPosition,
        row_y_recording: DisplayRowYRecording<'a>,
        max_rows: usize,
    ) -> DisplayRowOverflowTransitionRequest<'a> {
        match self.kind {
            DisplayRowOverflowTransitionKind::Truncation => {
                DisplayRowOverflowTransitionRequest::truncation(
                    hit_range,
                    defaults,
                    row_base,
                    self.row_start_col(),
                    position.x_px(),
                    row_y_recording,
                    max_rows,
                )
            }
            DisplayRowOverflowTransitionKind::VisualWrap(break_kind) => {
                DisplayRowOverflowTransitionRequest::visual_wrap(
                    break_kind,
                    hit_range,
                    defaults,
                    row_base,
                    self.row_start_col(),
                    position.x_px(),
                    row_y_recording,
                    max_rows,
                )
            }
        }
    }
}

impl<'a> DisplayRowOverflowTransitionRequest<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn truncation(
        hit_range: DisplayRowHitRange,
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        col: usize,
        x: f32,
        row_y_recording: DisplayRowYRecording<'a>,
        max_rows: usize,
    ) -> Self {
        Self {
            kind: DisplayRowOverflowTransitionKind::Truncation,
            hit_range,
            defaults,
            row_base,
            col,
            x,
            row_y_recording,
            max_rows,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn visual_wrap(
        break_kind: VisualWrapBreak,
        hit_range: DisplayRowHitRange,
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        col: usize,
        x: f32,
        row_y_recording: DisplayRowYRecording<'a>,
        max_rows: usize,
    ) -> Self {
        Self {
            kind: DisplayRowOverflowTransitionKind::VisualWrap(break_kind),
            hit_range,
            defaults,
            row_base,
            col,
            x,
            row_y_recording,
            max_rows,
        }
    }

    fn boundary_target(self) -> DisplayRowBoundaryTarget<'a> {
        match self.kind {
            DisplayRowOverflowTransitionKind::Truncation => DisplayRowBoundaryTarget::truncation(
                self.hit_range,
                self.defaults,
                self.row_base,
                self.col,
                self.x,
                self.row_y_recording,
            ),
            DisplayRowOverflowTransitionKind::VisualWrap(_) => {
                DisplayRowBoundaryTarget::visual_wrap(
                    self.hit_range,
                    self.defaults,
                    self.row_base,
                    self.col,
                    self.x,
                    self.row_y_recording,
                )
            }
        }
    }

    pub(crate) fn emit_with_output(
        self,
        row_geometry: &mut DisplayRowGeometryState,
        row_flags: &mut DisplayRowFlags,
        row_limit: DisplayRowLimit,
        hit_rows: &mut Vec<HitRow>,
        output_render: TextRowOutputRenderState<'_>,
    ) -> DisplayTextRowTransition {
        match self.kind {
            DisplayRowOverflowTransitionKind::Truncation => {
                row_geometry.mark_current_row_flag_kind(
                    row_flags,
                    DisplayRowFlagKind::Truncated,
                    row_limit,
                );
            }
            DisplayRowOverflowTransitionKind::VisualWrap(break_kind) => {
                // GNU sets row->continued_p on every wrap branch; only the
                // mid-element branches also produce the IT_CONTINUATION glyph.
                row_geometry.mark_current_row_flag_kind(
                    row_flags,
                    DisplayRowFlagKind::Continued,
                    row_limit,
                );
                if break_kind == VisualWrapBreak::MidElement {
                    row_geometry.mark_current_row_flag_kind(
                        row_flags,
                        DisplayRowFlagKind::ContinuedMidElement,
                        row_limit,
                    );
                }
            }
        }
        let kind = self.kind;
        let max_rows = self.max_rows;
        let transition = DisplayRowBoundaryTransitionRequest::new(self.boundary_target(), max_rows)
            .emit_with_output(row_geometry, hit_rows, output_render);
        if matches!(kind, DisplayRowOverflowTransitionKind::VisualWrap(_))
            && !transition.is_exhausted()
        {
            row_geometry.mark_current_row_flag_kind(
                row_flags,
                DisplayRowFlagKind::Continuation,
                row_limit,
            );
        }
        transition
    }
}
