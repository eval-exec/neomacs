use crate::display_item::RenderFaceRef;
use crate::display_pixel_calc::PixelCalcContext;
use crate::display_row::builder::{DisplayRowLayout, DisplayTabPolicy};
use crate::display_row::face_state::DisplayRowMeasurementMode;
use crate::hit_test::HitRow;
use crate::types::LayoutCharPos0;
use crate::window_output::{
    DisplayTextRowBegin, DisplayTextRowGeometryTransition, DisplayTextRowMetrics,
    RowMetricsSnapshot,
};
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;

/// Horizontal append limit for a display row.
///
/// Tabs are measured against an unbounded limit so that tab-stop alignment
/// survives past the right edge; every other kind clips at a concrete pixel
/// boundary.  Using an enum instead of `f32::INFINITY` makes the unbounded
/// case explicit at compile time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayRowMaxX {
    Unbounded,
    Bounded(f32),
}

impl DisplayRowMaxX {
    pub(crate) fn to_f32(self) -> f32 {
        match self {
            Self::Unbounded => f32::INFINITY,
            Self::Bounded(x) => x,
        }
    }
}

/// Where the window's text area starts, in the frame-absolute pixels the row
/// writer positions glyphs in.
///
/// GNU measures `it->current_x` and `it->last_visible_x` from this edge
/// (src/dispextern.h:2785-2791, emacs-31.0.90), so any rule ported from a
/// `produce_*` function that compares against `last_visible_x` itself, not
/// only against the difference of the two, has to subtract it first.  The
/// line-number prefix lies inside the text area and is not subtracted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowTextAreaOrigin {
    x_px: f32,
}

impl DisplayRowTextAreaOrigin {
    /// A row laid out in its own coordinates: the text area starts at 0.
    /// Chrome rows, mock frames and unit tests build rows this way.
    pub(crate) fn row_local() -> Self {
        Self { x_px: 0.0 }
    }

    /// A row of a window whose text area starts at frame-absolute `x_px`.
    pub(crate) fn at_frame_x(x_px: f32) -> Self {
        Self { x_px }
    }

    pub(crate) fn window_local(self, frame_x_px: f32) -> f32 {
        frame_x_px - self.x_px
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayRowGeometry {
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) char_width: f32,
    pub(crate) ascent: f32,
    pub(crate) tab_policy: DisplayTabPolicy,
}

impl DisplayRowGeometry {
    pub(crate) fn new(
        y: f32,
        width: f32,
        height: f32,
        char_width: f32,
        ascent: f32,
        tab_policy: DisplayTabPolicy,
    ) -> Self {
        Self {
            y,
            width,
            height,
            char_width,
            ascent,
            tab_policy,
        }
    }

    pub(crate) fn y(&self) -> f32 {
        self.y
    }

    pub(crate) fn width(&self) -> f32 {
        self.width
    }

    pub(crate) fn height(&self) -> f32 {
        self.height
    }

    pub(crate) fn char_width(&self) -> f32 {
        self.char_width
    }

    pub(crate) fn ascent(&self) -> f32 {
        self.ascent
    }

    pub(crate) fn tab_policy(&self) -> &DisplayTabPolicy {
        &self.tab_policy
    }

    pub(crate) fn with_char_width(mut self, char_width: f32) -> Self {
        self.char_width = char_width;
        self
    }

    pub(crate) fn to_layout(
        &self,
        role: GlyphRowRole,
        char_width_px: f32,
        ascent_px: f32,
        base_face: RenderFaceRef,
        pixel_calc: PixelCalcContext,
        space_image_params: Option<crate::display_pixel_calc::PixelCalcImageInputs>,
    ) -> DisplayRowLayout {
        DisplayRowLayout {
            role,
            y_px: self.y,
            height_px: self.height,
            ascent_px,
            char_width_px,
            tab_policy: self.tab_policy.clone(),
            base_face,
            pixel_calc,
            space_image_params,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CurrentDisplayRowMetrics {
    height: f32,
    ascent: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayRowAdvanceKind {
    LineBreak { line_spacing: f32 },
    Truncation,
    VisualWrap,
}

impl DisplayRowAdvanceKind {
    fn line_spacing(self) -> f32 {
        match self {
            Self::LineBreak { line_spacing } => line_spacing,
            Self::Truncation | Self::VisualWrap => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CurrentDisplayRowAdvance {
    pub(crate) y: f32,
    pub(crate) next_row: usize,
    pub(crate) text_y: f32,
    pub(crate) row_extra_y: f32,
    pub(crate) default_height: f32,
    pub(crate) default_ascent: f32,
    pub(crate) kind: DisplayRowAdvanceKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowAdvance {
    pub(crate) finished: DisplayTextRowMetrics,
    pub(crate) next_y: f32,
    pub(crate) row_extra_y: f32,
    pub(crate) next_height: f32,
    pub(crate) next_ascent: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowGeometryDefaults {
    pub(crate) text_y: f32,
    pub(crate) height: f32,
    pub(crate) ascent: f32,
    measurement_mode: DisplayRowMeasurementMode,
}

impl DisplayRowGeometryDefaults {
    pub(crate) fn new(
        text_y: f32,
        height: f32,
        ascent: f32,
        measurement_mode: DisplayRowMeasurementMode,
    ) -> Self {
        Self {
            text_y,
            height,
            ascent,
            measurement_mode,
        }
    }

    pub(crate) fn initial_state(self) -> DisplayRowGeometryState {
        DisplayRowGeometryState::for_measurement_mode(
            0,
            self.text_y,
            0.0,
            self.height,
            self.ascent,
            self.measurement_mode,
        )
    }

    #[cfg(test)]
    pub(crate) fn row_y_fallback(self, row_extra_y: f32) -> DisplayRowYFallback {
        DisplayRowYFallback {
            text_y: self.text_y,
            default_height: self.height,
            row_extra_y,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowGeometryCursor {
    row: usize,
    y: f32,
    row_extra_y: f32,
    metrics: CurrentDisplayRowMetrics,
    measurement_mode: DisplayRowMeasurementMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowVisibilityLimit {
    pub(crate) max_rows: usize,
    pub(crate) bottom_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowLimit {
    pub(crate) max_rows: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayRowFlagKind {
    /// GNU's `row->continued_p`: the row wrapped rather than ended a line.
    Continued,
    /// GNU's `row->truncated_on_right_p`.
    Truncated,
    /// This row continues the previous one (GNU's `row->continuation_lines_width`
    /// carry-over, drawn as the left fringe arrow).
    Continuation,
    /// The row's wrap broke a display element in the middle, so GNU produced the
    /// IT_CONTINUATION special glyph for it (src/xdisp.c:26336-26345,
    /// 26399-26403, 26421-26432). A row broken at a recorded word-wrap point
    /// (`back_to_wrap`, src/xdisp.c:26360-26388) is `Continued` but not this.
    ContinuedMidElement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayRowFlags {
    continued: Vec<bool>,
    truncated: Vec<bool>,
    continuation: Vec<bool>,
    continued_mid_element: Vec<bool>,
}

impl DisplayRowFlags {
    pub(crate) fn new(row_count: usize) -> Self {
        Self {
            continued: vec![false; row_count],
            truncated: vec![false; row_count],
            continuation: vec![false; row_count],
            continued_mid_element: vec![false; row_count],
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.truncated.len()
    }

    pub(crate) fn mark(&mut self, row: usize, kind: DisplayRowFlagKind) {
        if let Some(flag) = self.flags_mut(kind).get_mut(row) {
            *flag = true;
        }
    }

    pub(crate) fn is_set(&self, row: usize, kind: DisplayRowFlagKind) -> bool {
        self.flags(kind).get(row).copied().unwrap_or(false)
    }

    fn flags(&self, kind: DisplayRowFlagKind) -> &[bool] {
        match kind {
            DisplayRowFlagKind::Continued => &self.continued,
            DisplayRowFlagKind::Truncated => &self.truncated,
            DisplayRowFlagKind::Continuation => &self.continuation,
            DisplayRowFlagKind::ContinuedMidElement => &self.continued_mid_element,
        }
    }

    fn flags_mut(&mut self, kind: DisplayRowFlagKind) -> &mut [bool] {
        match kind {
            DisplayRowFlagKind::Continued => &mut self.continued,
            DisplayRowFlagKind::Truncated => &mut self.truncated,
            DisplayRowFlagKind::Continuation => &mut self.continuation,
            DisplayRowFlagKind::ContinuedMidElement => &mut self.continued_mid_element,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayRowMarker {
    #[cfg(test)]
    Inactive,
    Row(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DisplayRowScopedValue<T> {
    Inactive,
    Active { row: DisplayRowMarker, value: T },
}

/// Row-scoped realized state for GNU's `:extend` filler. The semantic alias
/// prevents callers from storing paint identity without the matching metrics.
pub(crate) type DisplayRowExtendState =
    DisplayRowScopedValue<crate::display_row::face_state::DisplayRowExtendFace>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayRowStartMarker {
    Inactive,
    Active { row: DisplayRowMarker, x: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowYFallback {
    pub(crate) text_y: f32,
    pub(crate) default_height: f32,
    pub(crate) row_extra_y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayRowYPositions {
    positions: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowTextPosition {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) byte_idx: usize,
    pub(crate) col: usize,
    pub(crate) row: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowGeometryState {
    row: usize,
    y: f32,
    row_extra_y: f32,
    height: f32,
    ascent: f32,
    measurement_mode: DisplayRowMeasurementMode,
}

pub(crate) enum DisplayRowYRecording<'a> {
    None,
    RowYPositions(&'a mut DisplayRowYPositions),
}

pub(crate) struct DisplayRowGeometryTransitionTarget<'a> {
    defaults: DisplayRowGeometryDefaults,
    kind: DisplayRowAdvanceKind,
    row_base: usize,
    col: usize,
    x: f32,
    row_y_recording: DisplayRowYRecording<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowHitRange {
    pub(crate) charpos_start: i64,
    pub(crate) charpos_end: i64,
}

pub(crate) struct DisplayRowBoundaryTarget<'a> {
    hit_range: DisplayRowHitRange,
    transition: DisplayRowGeometryTransitionTarget<'a>,
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayRowBoundaryTransition {
    pub(crate) hit_row: HitRow,
    pub(crate) transition: DisplayTextRowGeometryTransition,
}

impl DisplayRowBoundaryTransition {
    pub(crate) fn record_hit_row(
        self,
        hit_rows: &mut Vec<HitRow>,
    ) -> DisplayTextRowGeometryTransition {
        hit_rows.push(self.hit_row);
        self.transition
    }
}

impl DisplayRowYFallback {
    fn y_for_row(self, row: usize) -> f32 {
        self.text_y + row as f32 * self.default_height + self.row_extra_y
    }
}

impl DisplayRowYPositions {
    #[cfg(test)]
    pub(crate) fn with_first_row(first_row_y: f32, _default_height: f32) -> Self {
        Self {
            positions: vec![first_row_y],
        }
    }

    pub(crate) fn with_capacity_and_first_row(capacity: usize, first_row_y: f32) -> Self {
        let mut positions = Vec::with_capacity(capacity);
        positions.push(first_row_y);
        Self { positions }
    }

    #[cfg(test)]
    pub(crate) fn record(&mut self, row: usize, y: f32) {
        if row < self.positions.len() {
            self.positions[row] = y;
        } else {
            self.positions.push(y);
        }
    }

    pub(crate) fn push(&mut self, y: f32) {
        self.positions.push(y);
    }

    pub(crate) fn y_for_row(&self, row: usize, fallback: DisplayRowYFallback) -> f32 {
        self.positions
            .get(row)
            .copied()
            .unwrap_or_else(|| fallback.y_for_row(row))
    }

    pub(crate) fn recording(&mut self) -> DisplayRowYRecording<'_> {
        DisplayRowYRecording::RowYPositions(self)
    }

    #[cfg(test)]
    pub(crate) fn recorded(&self) -> &[f32] {
        &self.positions
    }
}

impl<'a> DisplayRowGeometryTransitionTarget<'a> {
    fn new(
        defaults: DisplayRowGeometryDefaults,
        kind: DisplayRowAdvanceKind,
        row_base: usize,
        col: usize,
        x: f32,
        row_y_recording: DisplayRowYRecording<'a>,
    ) -> Self {
        Self {
            defaults,
            kind,
            row_base,
            col,
            x,
            row_y_recording,
        }
    }

    pub(crate) fn line_break(
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        col: usize,
        x: f32,
        line_spacing: f32,
        row_y_recording: DisplayRowYRecording<'a>,
    ) -> Self {
        Self::new(
            defaults,
            DisplayRowAdvanceKind::LineBreak { line_spacing },
            row_base,
            col,
            x,
            row_y_recording,
        )
    }

    pub(crate) fn truncation(
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        col: usize,
        x: f32,
        row_y_recording: DisplayRowYRecording<'a>,
    ) -> Self {
        Self::new(
            defaults,
            DisplayRowAdvanceKind::Truncation,
            row_base,
            col,
            x,
            row_y_recording,
        )
    }

    pub(crate) fn visual_wrap(
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        col: usize,
        x: f32,
        row_y_recording: DisplayRowYRecording<'a>,
    ) -> Self {
        Self::new(
            defaults,
            DisplayRowAdvanceKind::VisualWrap,
            row_base,
            col,
            x,
            row_y_recording,
        )
    }
}

impl<'a> DisplayRowBoundaryTarget<'a> {
    pub(crate) fn new(
        hit_range: DisplayRowHitRange,
        transition: DisplayRowGeometryTransitionTarget<'a>,
    ) -> Self {
        Self {
            hit_range,
            transition,
        }
    }

    pub(crate) fn line_break(
        hit_range: DisplayRowHitRange,
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        col: usize,
        x: f32,
        line_spacing: f32,
        row_y_recording: DisplayRowYRecording<'a>,
    ) -> Self {
        Self::new(
            hit_range,
            DisplayRowGeometryTransitionTarget::line_break(
                defaults,
                row_base,
                col,
                x,
                line_spacing,
                row_y_recording,
            ),
        )
    }

    pub(crate) fn truncation(
        hit_range: DisplayRowHitRange,
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        col: usize,
        x: f32,
        row_y_recording: DisplayRowYRecording<'a>,
    ) -> Self {
        Self::new(
            hit_range,
            DisplayRowGeometryTransitionTarget::truncation(
                defaults,
                row_base,
                col,
                x,
                row_y_recording,
            ),
        )
    }

    pub(crate) fn visual_wrap(
        hit_range: DisplayRowHitRange,
        defaults: DisplayRowGeometryDefaults,
        row_base: usize,
        col: usize,
        x: f32,
        row_y_recording: DisplayRowYRecording<'a>,
    ) -> Self {
        Self::new(
            hit_range,
            DisplayRowGeometryTransitionTarget::visual_wrap(
                defaults,
                row_base,
                col,
                x,
                row_y_recording,
            ),
        )
    }
}

impl DisplayRowGeometryState {
    #[cfg(test)]
    pub(crate) fn new(row: usize, y: f32, row_extra_y: f32, height: f32, ascent: f32) -> Self {
        Self::for_measurement_mode(
            row,
            y,
            row_extra_y,
            height,
            ascent,
            DisplayRowMeasurementMode::ConcreteFont,
        )
    }

    pub(crate) fn for_measurement_mode(
        row: usize,
        y: f32,
        row_extra_y: f32,
        height: f32,
        ascent: f32,
        measurement_mode: DisplayRowMeasurementMode,
    ) -> Self {
        Self {
            row,
            y,
            row_extra_y,
            height,
            ascent,
            measurement_mode,
        }
    }

    pub(crate) fn row(&self) -> usize {
        self.row
    }

    pub(crate) fn y(&self) -> f32 {
        self.y
    }

    pub(crate) fn height(&self) -> f32 {
        self.height
    }

    pub(crate) fn ascent(&self) -> f32 {
        self.ascent
    }

    pub(crate) fn with_row_y(mut self, y: f32) -> Self {
        self.y = y;
        self
    }

    pub(crate) fn cursor(&self) -> DisplayRowGeometryCursor {
        DisplayRowGeometryCursor::from_state(*self)
    }

    pub(crate) fn current_row_is_visible(&self, limit: DisplayRowVisibilityLimit) -> bool {
        self.row < limit.max_rows && self.y + self.height <= limit.bottom_y
    }

    pub(crate) fn is_within_row_limit(&self, limit: DisplayRowLimit) -> bool {
        self.row < limit.max_rows
    }

    #[cfg(test)]
    pub(crate) fn rendered_row_count(&self, limit: DisplayRowLimit) -> usize {
        self.row.min(limit.max_rows)
    }

    #[cfg(test)]
    pub(crate) fn first_row_below_current(&self, limit: DisplayRowLimit) -> usize {
        self.row.saturating_add(1).min(limit.max_rows)
    }

    pub(crate) fn mark_current_row_flag_kind(
        &self,
        flags: &mut DisplayRowFlags,
        kind: DisplayRowFlagKind,
        limit: DisplayRowLimit,
    ) {
        if self.is_within_row_limit(limit) {
            flags.mark(self.row, kind);
        }
    }

    pub(crate) fn current_row_marker(&self) -> DisplayRowMarker {
        DisplayRowMarker::Row(self.row)
    }

    pub(crate) fn next_row_marker(&self) -> DisplayRowMarker {
        DisplayRowMarker::Row(self.row.saturating_add(1))
    }

    pub(crate) fn start_marker_at_x(&self, x: f32) -> DisplayRowStartMarker {
        DisplayRowStartMarker::Active {
            row: self.current_row_marker(),
            x,
        }
    }

    pub(crate) fn include_glyph_vertical_metrics(&mut self, glyph_height: f32, glyph_ascent: f32) {
        match self.measurement_mode {
            DisplayRowMeasurementMode::ConcreteFont => {}
            DisplayRowMeasurementMode::LogicalCells => return,
        }
        let mut metrics = CurrentDisplayRowMetrics::new(self.height, self.ascent);
        metrics.include_glyph(glyph_height, glyph_ascent);
        self.height = metrics.height();
        self.ascent = metrics.ascent();
    }

    pub(crate) fn include_row_extents(&mut self, height: f32, ascent: f32) {
        match self.measurement_mode {
            DisplayRowMeasurementMode::ConcreteFont => {}
            DisplayRowMeasurementMode::LogicalCells => return,
        }
        self.height = self.height.max(height);
        self.ascent = self.ascent.max(ascent);
    }

    /// Replace the current row's default-minimum geometry with authoritative
    /// visible-content geometry. Used for GNU `line-height t`, where the
    /// newline itself contributes no font height.
    pub(crate) fn replace_current_row_metrics(&mut self, height: f32, ascent: f32) {
        match self.measurement_mode {
            DisplayRowMeasurementMode::ConcreteFont => {}
            DisplayRowMeasurementMode::LogicalCells => return,
        }
        self.height = height.max(1.0);
        self.ascent = ascent.max(0.0).min(self.height);
    }

    pub(crate) fn record_current_row_y(&self, row_y_positions: &mut DisplayRowYPositions) {
        row_y_positions.push(self.y);
    }

    pub(crate) fn row_y_fallback(&self, text_y: f32, default_height: f32) -> DisplayRowYFallback {
        DisplayRowYFallback {
            text_y,
            default_height,
            row_extra_y: self.row_extra_y,
        }
    }

    pub(crate) fn row_y(
        &self,
        row: usize,
        row_y_positions: &DisplayRowYPositions,
        text_y: f32,
        default_height: f32,
    ) -> f32 {
        row_y_positions.y_for_row(row, self.row_y_fallback(text_y, default_height))
    }

    pub(crate) fn current_row_y(
        &self,
        row_y_positions: &DisplayRowYPositions,
        text_y: f32,
        default_height: f32,
    ) -> f32 {
        self.row_y(self.row, row_y_positions, text_y, default_height)
    }

    pub(crate) fn text_position(
        &self,
        x: f32,
        byte_idx: usize,
        col: usize,
    ) -> DisplayRowTextPosition {
        DisplayRowTextPosition {
            x,
            y: self.y,
            byte_idx,
            col,
            row: self.row,
        }
    }

    pub(crate) fn row_metrics_snapshot(&self, row_base: usize) -> RowMetricsSnapshot {
        let height = self.height.max(1.0);
        RowMetricsSnapshot::new(
            row_base + self.row,
            row_base + self.row,
            self.y,
            height,
            self.ascent.max(0.0).min(height),
        )
    }

    pub(crate) fn display_text_row_begin(
        &self,
        row_base: usize,
        col: usize,
        x: f32,
        start_charpos: LayoutCharPos0,
    ) -> DisplayTextRowBegin {
        DisplayRowGeometryCursor::from_state(*self).display_text_row_begin(
            row_base,
            col,
            x,
            start_charpos,
        )
    }

    pub(crate) fn glyph_y(&self, glyph_y_offset: f32) -> f32 {
        self.y + glyph_y_offset
    }

    pub(crate) fn finish_boundary_in_place(
        &mut self,
        target: DisplayRowBoundaryTarget<'_>,
    ) -> DisplayRowBoundaryTransition {
        let mut row_cursor = DisplayRowGeometryCursor::from_state(*self);
        let hit_row =
            row_cursor.hit_row(target.hit_range.charpos_start, target.hit_range.charpos_end);
        let transition = row_cursor.finish_and_begin_next_display_text_row(
            target.transition.defaults,
            target.transition.kind,
            target.transition.row_base,
            target.transition.col,
            target.transition.x,
            LayoutCharPos0::new(target.hit_range.charpos_end),
        );
        *self = row_cursor.state();
        match target.transition.row_y_recording {
            DisplayRowYRecording::None => {}
            DisplayRowYRecording::RowYPositions(row_y_positions) => {
                row_y_positions.push(self.y);
            }
        }
        DisplayRowBoundaryTransition {
            hit_row,
            transition,
        }
    }

    pub(crate) fn finish_boundary_and_record_hit(
        &mut self,
        target: DisplayRowBoundaryTarget<'_>,
        hit_rows: &mut Vec<HitRow>,
    ) -> DisplayTextRowGeometryTransition {
        self.finish_boundary_in_place(target)
            .record_hit_row(hit_rows)
    }
}

impl DisplayRowMarker {
    pub(crate) fn is_active_on(&self, geometry: &DisplayRowGeometryState) -> bool {
        matches!(self, Self::Row(row) if *row == geometry.row)
    }
}

impl<T> DisplayRowScopedValue<T> {
    pub(crate) fn inactive() -> Self {
        Self::Inactive
    }

    pub(crate) fn activate(&mut self, row: DisplayRowMarker, value: T) {
        *self = Self::Active { row, value };
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::Inactive;
    }

    #[cfg(test)]
    pub(crate) fn value(&self) -> Option<&T> {
        match self {
            Self::Active { value, .. } => Some(value),
            Self::Inactive => None,
        }
    }

    pub(crate) fn value_on(&self, geometry: &DisplayRowGeometryState) -> Option<&T> {
        match self {
            Self::Active { row, value } if row.is_active_on(geometry) => Some(value),
            Self::Active { .. } | Self::Inactive => None,
        }
    }
}

impl DisplayRowStartMarker {
    pub(crate) fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    #[cfg(test)]
    pub(crate) fn x_on(&self, geometry: &DisplayRowGeometryState) -> Option<f32> {
        match self {
            Self::Active { row, x } if row.is_active_on(geometry) => Some(*x),
            Self::Active { .. } | Self::Inactive => None,
        }
    }
}

impl CurrentDisplayRowMetrics {
    pub(crate) fn new(height: f32, ascent: f32) -> Self {
        Self { height, ascent }
    }

    pub(crate) fn height(&self) -> f32 {
        self.height
    }

    pub(crate) fn ascent(&self) -> f32 {
        self.ascent
    }

    pub(crate) fn include_glyph(&mut self, glyph_height: f32, glyph_ascent: f32) {
        let glyph_height = glyph_height.max(1.0);
        let glyph_ascent = glyph_ascent.max(0.0).min(glyph_height);
        let row_descent = (self.height - self.ascent).max(0.0);
        let glyph_descent = (glyph_height - glyph_ascent).max(0.0);
        self.ascent = self.ascent.max(glyph_ascent);
        self.height = (self.ascent + row_descent.max(glyph_descent)).max(glyph_height);
    }

    /// Signed correction from the nominal row grid to this finished row.
    ///
    /// Most rows begin at the default height and can only grow. GNU
    /// `line-height t` is the important exception: it constrains the newline
    /// to already-produced content, so a row can be shorter than the default.
    /// Keeping this signed prevents the next row from retaining an invisible
    /// default-height gap.
    pub(crate) fn height_delta_from_default(&self, default_height: f32) -> f32 {
        self.height - default_height
    }

    pub(crate) fn next_row_vertical_delta(&self, default_height: f32, line_spacing: f32) -> f32 {
        self.height_delta_from_default(default_height) + line_spacing.max(0.0)
    }

    pub(crate) fn finish_current_row(&self, y: f32) -> DisplayTextRowMetrics {
        DisplayTextRowMetrics {
            y,
            height: self.height,
            ascent: self.ascent,
        }
    }

    pub(crate) fn reset(&mut self, height: f32, ascent: f32) {
        self.height = height;
        self.ascent = ascent;
    }

    pub(crate) fn finish_and_reset(
        &mut self,
        y: f32,
        default_height: f32,
        default_ascent: f32,
    ) -> DisplayTextRowMetrics {
        let finished = self.finish_current_row(y);
        self.reset(default_height, default_ascent);
        finished
    }

    pub(crate) fn finish_and_advance_to_next_row(
        &mut self,
        advance: CurrentDisplayRowAdvance,
        measurement_mode: DisplayRowMeasurementMode,
    ) -> DisplayRowAdvance {
        // GNU `compute_line_metrics` makes terminal geometry categorical:
        // after producing the glyphs it overwrites ascent/height with one
        // logical cell and discards line spacing.  Keep that policy at this
        // shared row-transition boundary so buffer text, display strings,
        // overlays and wrapping cannot accidentally reintroduce pixel metrics.
        if measurement_mode == DisplayRowMeasurementMode::LogicalCells {
            self.reset(advance.default_height, advance.default_ascent);
        }
        let line_spacing = match measurement_mode {
            DisplayRowMeasurementMode::ConcreteFont => advance.kind.line_spacing(),
            DisplayRowMeasurementMode::LogicalCells => 0.0,
        };
        let row_extra_y = advance.row_extra_y
            + self.next_row_vertical_delta(advance.default_height, line_spacing);
        let finished =
            self.finish_and_reset(advance.y, advance.default_height, advance.default_ascent);
        DisplayRowAdvance {
            finished,
            next_y: advance.text_y + advance.next_row as f32 * advance.default_height + row_extra_y,
            row_extra_y,
            next_height: self.height(),
            next_ascent: self.ascent(),
        }
    }
}

impl DisplayRowGeometryCursor {
    pub(crate) fn from_state(state: DisplayRowGeometryState) -> Self {
        Self {
            row: state.row,
            y: state.y,
            row_extra_y: state.row_extra_y,
            metrics: CurrentDisplayRowMetrics::new(state.height, state.ascent),
            measurement_mode: state.measurement_mode,
        }
    }

    pub(crate) fn hit_row(&self, charpos_start: i64, charpos_end: i64) -> HitRow {
        HitRow {
            y_start: self.y,
            y_end: self.y + self.metrics.height(),
            charpos_start,
            charpos_end,
        }
    }

    pub(crate) fn finish_current_row(&self) -> DisplayTextRowMetrics {
        self.metrics.finish_current_row(self.y)
    }

    pub(crate) fn finish_and_advance_to_next_row(
        &mut self,
        defaults: DisplayRowGeometryDefaults,
        kind: DisplayRowAdvanceKind,
    ) -> DisplayTextRowMetrics {
        let row_advance = self.metrics.finish_and_advance_to_next_row(
            CurrentDisplayRowAdvance {
                y: self.y,
                next_row: self.row + 1,
                text_y: defaults.text_y,
                row_extra_y: self.row_extra_y,
                default_height: defaults.height,
                default_ascent: defaults.ascent,
                kind,
            },
            self.measurement_mode,
        );
        self.row += 1;
        self.y = row_advance.next_y;
        self.row_extra_y = row_advance.row_extra_y;
        self.metrics =
            CurrentDisplayRowMetrics::new(row_advance.next_height, row_advance.next_ascent);
        row_advance.finished
    }

    pub(crate) fn finish_and_begin_next_display_text_row(
        &mut self,
        defaults: DisplayRowGeometryDefaults,
        kind: DisplayRowAdvanceKind,
        row_base: usize,
        col: usize,
        x: f32,
        begin_row_start_charpos: LayoutCharPos0,
    ) -> DisplayTextRowGeometryTransition {
        let finished_row = self.finish_and_advance_to_next_row(defaults, kind);
        let begin_row = self.display_text_row_begin(row_base, col, x, begin_row_start_charpos);
        DisplayTextRowGeometryTransition {
            finished_row,
            begin_row,
        }
    }

    pub(crate) fn display_text_row_begin(
        &self,
        row_base: usize,
        col: usize,
        x: f32,
        start_charpos: LayoutCharPos0,
    ) -> DisplayTextRowBegin {
        DisplayTextRowBegin {
            display_row_index: row_base + self.row,
            row: self.row,
            col,
            y: self.y,
            x,
            start_charpos,
        }
    }

    pub(crate) fn state(&self) -> DisplayRowGeometryState {
        DisplayRowGeometryState {
            row: self.row,
            y: self.y,
            row_extra_y: self.row_extra_y,
            height: self.metrics.height(),
            ascent: self.metrics.ascent(),
            measurement_mode: self.measurement_mode,
        }
    }
}

#[cfg(test)]
#[path = "geometry_test.rs"]
mod tests;
